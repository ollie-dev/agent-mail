//! The Windows half of the Broker: the service control manager entry
//! point, the named-pipe server with impersonation of each client, the
//! access lists of the pipe and the audit folder, and the checks on the
//! audit log before each use.

use std::ffi::c_void;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::path::Path;
use std::ptr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use windows_sys::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_BROKEN_PIPE, ERROR_CALL_NOT_IMPLEMENTED,
    ERROR_FAILED_SERVICE_CONTROLLER_CONNECT, ERROR_FILE_NOT_FOUND, ERROR_IO_PENDING, ERROR_NO_DATA,
    ERROR_PATH_NOT_FOUND, ERROR_PIPE_CONNECTED, ERROR_SERVICE_SPECIFIC_ERROR, GENERIC_READ,
    GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE, LocalFree, NO_ERROR, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SDDL_REVISION_1, SE_FILE_OBJECT, SE_KERNEL_OBJECT, SE_OBJECT_TYPE,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
    DACL_SECURITY_INFORMATION, GetAce, GetAclInformation, GetSecurityDescriptorControl,
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, OWNER_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, RevertToSelf, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES,
    TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TOKEN_USER, TokenIntegrityLevel, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CreateDirectoryW, CreateFileW, FILE_APPEND_DATA,
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_FLAG_OVERLAPPED, FILE_GENERIC_READ, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TYPE_DISK, GetFileInformationByHandle, GetFileType,
    OPEN_ALWAYS, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, READ_CONTROL, ReadFile,
    SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT, SYNCHRONIZE, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, ImpersonateNamedPipeClient,
    PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES,
    PIPE_WAIT,
};
use windows_sys::Win32::System::Services::{
    RegisterServiceCtrlHandlerExW, SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP,
    SERVICE_CONTROL_INTERROGATE, SERVICE_CONTROL_SHUTDOWN, SERVICE_CONTROL_STOP, SERVICE_RUNNING,
    SERVICE_START_PENDING, SERVICE_STATUS, SERVICE_STATUS_CURRENT_STATE, SERVICE_STOP_PENDING,
    SERVICE_STOPPED, SERVICE_TABLE_ENTRYW, SERVICE_WIN32_OWN_PROCESS, SetServiceStatus,
    StartServiceCtrlDispatcherW,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, GetCurrentThread, INFINITE, OpenProcessToken, OpenThreadToken,
    SetEvent, WaitForMultipleObjects,
};
use windows_sys::core::PWSTR;

use super::{ADMINISTRATORS_SID, Caller, PIPE_NAME, SERVICE_NAME, SerializedStore};
use crate::audit::{AuditLog, AuditUnavailable};
use crate::core::{Context, CoreError};
use crate::imap::Network;
use crate::store::{CredentialManagerStore, MandateStore};
use crate::terminal::NoTerminal;

/// `ACCESS_ALLOWED_ACE_TYPE`.
pub const ACCESS_ALLOWED: u8 = 0;
/// `ACCESS_DENIED_ACE_TYPE`.
pub const ACCESS_DENIED: u8 = 1;
/// Pipe buffer size in each direction.
const PIPE_BUFFER: u32 = 64 * 1024;

/// `s` as a NUL-terminated UTF-16 string.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `path` as a NUL-terminated UTF-16 string.
fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn last_error() -> io::Error {
    io::Error::last_os_error()
}

fn raw(handle: &OwnedHandle) -> HANDLE {
    handle.as_raw_handle() as HANDLE
}

/// Takes ownership of a handle a Win32 call returned, failing on null or
/// `INVALID_HANDLE_VALUE`.
fn owned(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(last_error());
    }
    // SAFETY: a valid handle this process now owns alone.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) })
}

/// Memory a Win32 call allocated with `LocalAlloc`, freed on drop.
struct Local(*mut c_void);

impl Drop for Local {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: allocated by the system with LocalAlloc, freed once.
            unsafe { LocalFree(self.0) };
        }
    }
}

/// A security descriptor built from SDDL.
fn security_descriptor(sddl: &str) -> io::Result<Local> {
    let text = wide(sddl);
    let mut sd: PSECURITY_DESCRIPTOR = ptr::null_mut();
    // SAFETY: `text` is NUL-terminated; `sd` receives a LocalAlloc buffer.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(last_error());
    }
    Ok(Local(sd))
}

/// The string form of `sid`.
///
/// # Safety
/// `sid` points to a valid SID.
unsafe fn sid_string(sid: PSID) -> io::Result<String> {
    let mut text: PWSTR = ptr::null_mut();
    // SAFETY: `sid` is valid; `text` receives a LocalAlloc string.
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(last_error());
    }
    let _free = Local(text.cast());
    let mut len = 0;
    // SAFETY: a NUL-terminated string.
    while unsafe { *text.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` units were just read.
    Ok(String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(text, len)
    }))
}

/// One token information class, in a buffer aligned for its structure.
fn token_information(token: HANDLE, class: i32) -> io::Result<Vec<u64>> {
    let mut needed = 0u32;
    // SAFETY: a size query with no buffer.
    unsafe { GetTokenInformation(token, class, ptr::null_mut(), 0, &mut needed) };
    if needed == 0 {
        return Err(last_error());
    }
    let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
    // SAFETY: `buf` holds at least `needed` bytes.
    let ok =
        unsafe { GetTokenInformation(token, class, buf.as_mut_ptr().cast(), needed, &mut needed) };
    if ok == 0 {
        return Err(last_error());
    }
    Ok(buf)
}

/// The user SID of `token`.
fn token_user(token: HANDLE) -> io::Result<String> {
    let buf = token_information(token, TokenUser)?;
    // SAFETY: the buffer holds a TOKEN_USER whose SID points inside it.
    unsafe {
        let user = &*buf.as_ptr().cast::<TOKEN_USER>();
        sid_string(user.User.Sid)
    }
}

/// The integrity RID of `token`.
fn token_integrity(token: HANDLE) -> io::Result<u32> {
    let buf = token_information(token, TokenIntegrityLevel)?;
    // SAFETY: the buffer holds a TOKEN_MANDATORY_LABEL whose SID points
    // inside it; its last sub-authority is the integrity RID.
    unsafe {
        let label = &*buf.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
        let sid = label.Label.Sid;
        let count = *GetSidSubAuthorityCount(sid);
        if count == 0 {
            return Err(io::Error::other("integrity SID has no RID"));
        }
        Ok(*GetSidSubAuthority(sid, u32::from(count) - 1))
    }
}

/// The SID string of the user this process runs as: the Broker's own SID
/// in the service.
pub fn current_user_sid() -> io::Result<String> {
    let mut token: HANDLE = ptr::null_mut();
    // SAFETY: the pseudo handle needs no closing; `token` receives a handle.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(last_error());
    }
    let token = owned(token)?;
    token_user(raw(&token))
}

/// The Win32 error `OpenThreadToken` gives on the calling thread, or `None`
/// if the thread has a token. `ERROR_NO_TOKEN` (1008) means no
/// impersonation is active.
pub fn thread_token_error() -> Option<u32> {
    let mut token: HANDLE = ptr::null_mut();
    // SAFETY: the pseudo handle needs no closing; `token` receives a handle.
    if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) } == 0 {
        // SAFETY: no preconditions.
        return Some(unsafe { GetLastError() });
    }
    drop(owned(token));
    None
}

/// One access control entry, read back from an object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ace {
    /// [`ACCESS_ALLOWED`], [`ACCESS_DENIED`] or another ACE type.
    pub kind: u8,
    /// The ACE flags, e.g. `OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE`.
    pub flags: u8,
    /// The access mask.
    pub mask: u32,
    /// The trustee's SID string; empty for ACE types without one here.
    pub sid: String,
}

/// An object's DACL, read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dacl {
    /// Whether the DACL is protected from inheritance (`SE_DACL_PROTECTED`).
    pub protected: bool,
    /// The entries, in order.
    pub aces: Vec<Ace>,
}

/// The security descriptor of `handle`, owner and DACL.
struct ObjectSecurity {
    descriptor: Local,
    owner: PSID,
    dacl: *mut ACL,
}

fn object_security(handle: HANDLE, kind: SE_OBJECT_TYPE) -> io::Result<ObjectSecurity> {
    let mut owner: PSID = ptr::null_mut();
    let mut dacl: *mut ACL = ptr::null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = ptr::null_mut();
    // SAFETY: valid out pointers; `sd` receives a LocalAlloc buffer holding
    // the owner and DACL.
    let rc = unsafe {
        GetSecurityInfo(
            handle,
            kind,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut dacl,
            ptr::null_mut(),
            &mut sd,
        )
    };
    if rc != NO_ERROR {
        return Err(io::Error::from_raw_os_error(rc as i32));
    }
    Ok(ObjectSecurity {
        descriptor: Local(sd),
        owner,
        dacl,
    })
}

impl ObjectSecurity {
    fn owner(&self) -> io::Result<String> {
        if self.owner.is_null() {
            return Err(io::Error::other("no owner"));
        }
        // SAFETY: the owner SID lives in `descriptor`.
        unsafe { sid_string(self.owner) }
    }

    fn dacl(&self) -> io::Result<Dacl> {
        let mut control = 0u16;
        let mut revision = 0u32;
        // SAFETY: `descriptor` is a valid security descriptor.
        if unsafe { GetSecurityDescriptorControl(self.descriptor.0, &mut control, &mut revision) }
            == 0
        {
            return Err(last_error());
        }
        let protected = control & SE_DACL_PROTECTED != 0;
        if self.dacl.is_null() {
            return Ok(Dacl {
                protected,
                aces: Vec::new(),
            });
        }
        let mut size = ACL_SIZE_INFORMATION::default();
        // SAFETY: `dacl` is a valid ACL; `size` is the right structure.
        let ok = unsafe {
            GetAclInformation(
                self.dacl,
                (&mut size as *mut ACL_SIZE_INFORMATION).cast(),
                size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            )
        };
        if ok == 0 {
            return Err(last_error());
        }
        let mut aces = Vec::new();
        for index in 0..size.AceCount {
            let mut ace: *mut c_void = ptr::null_mut();
            // SAFETY: `index` is below the ACE count.
            if unsafe { GetAce(self.dacl, index, &mut ace) } == 0 {
                return Err(last_error());
            }
            // SAFETY: every ACE starts with an ACE_HEADER; allowed and denied
            // ACEs share ACCESS_ALLOWED_ACE's layout, the SID at SidStart.
            let entry = unsafe {
                let header = &*ace.cast::<ACE_HEADER>();
                if header.AceType == ACCESS_ALLOWED || header.AceType == ACCESS_DENIED {
                    let ace = &*ace.cast::<ACCESS_ALLOWED_ACE>();
                    Ace {
                        kind: header.AceType,
                        flags: header.AceFlags,
                        mask: ace.Mask,
                        sid: sid_string((&ace.SidStart as *const u32).cast_mut().cast())?,
                    }
                } else {
                    Ace {
                        kind: header.AceType,
                        flags: header.AceFlags,
                        mask: 0,
                        sid: String::new(),
                    }
                }
            };
            aces.push(entry);
        }
        Ok(Dacl { protected, aces })
    }
}

/// The DACL of a pipe handle.
pub fn pipe_dacl(pipe: &OwnedHandle) -> io::Result<Dacl> {
    object_security(raw(pipe), SE_KERNEL_OBJECT)?.dacl()
}

/// Opens `path` itself, never a reparse point's target, for reading its
/// attributes and security.
fn open_no_follow(path: &Path) -> io::Result<OwnedHandle> {
    let name = wide_path(path);
    // SAFETY: `name` is NUL-terminated.
    owned(unsafe {
        CreateFileW(
            name.as_ptr(),
            READ_CONTROL | FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    })
}

/// The DACL of the file or folder at `path`, not following a reparse point.
pub fn path_dacl(path: &Path) -> io::Result<Dacl> {
    let handle = open_no_follow(path)?;
    object_security(raw(&handle), SE_FILE_OBJECT)?.dacl()
}

/// The owner SID of the file or folder at `path`, not following a reparse
/// point.
pub fn path_owner(path: &Path) -> io::Result<String> {
    let handle = open_no_follow(path)?;
    object_security(raw(&handle), SE_FILE_OBJECT)?.owner()
}

fn file_information(handle: &OwnedHandle) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: a valid handle and out pointer.
    if unsafe { GetFileInformationByHandle(raw(handle), &mut info) } == 0 {
        return Err(last_error());
    }
    Ok(info)
}

fn is_not_found(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error().map(|c| c as u32),
        Some(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND)
    )
}

/// Opens the Broker's audit log at `path` for appending (`append`) or for
/// reading, after checking it (F33). On append, the folder is created first
/// with [`super::audit_dir_sddl`] for `broker_sid` if it does not exist.
///
/// Refused, with [`AuditUnavailable`], when the folder is a reparse point,
/// not a folder, or owned by neither `broker_sid` nor Administrators, or
/// when `audit.log`, opened with `FILE_FLAG_OPEN_REPARSE_POINT`, is a
/// reparse point, not a regular file or has more than one hard link.
/// `Ok(None)` when reading and the folder or file does not exist yet.
pub fn open_audit_log(
    path: &Path,
    broker_sid: &str,
    append: bool,
) -> Result<Option<File>, AuditUnavailable> {
    let dir = path.parent().ok_or(AuditUnavailable)?;
    if append {
        let sd = security_descriptor(&super::audit_dir_sddl(broker_sid))
            .map_err(|_| AuditUnavailable)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: 0,
        };
        let name = wide_path(dir);
        // SAFETY: `name` is NUL-terminated and `attributes` valid.
        if unsafe { CreateDirectoryW(name.as_ptr(), &attributes) } == 0 {
            // SAFETY: no preconditions.
            if unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
                return Err(AuditUnavailable);
            }
        }
    }
    let dir_handle = match open_no_follow(dir) {
        Ok(handle) => handle,
        Err(e) if !append && is_not_found(&e) => return Ok(None),
        Err(_) => return Err(AuditUnavailable),
    };
    let info = file_information(&dir_handle).map_err(|_| AuditUnavailable)?;
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
    {
        return Err(AuditUnavailable);
    }
    let owner = object_security(raw(&dir_handle), SE_FILE_OBJECT)
        .and_then(|s| s.owner())
        .map_err(|_| AuditUnavailable)?;
    if owner != broker_sid && owner != ADMINISTRATORS_SID {
        return Err(AuditUnavailable);
    }
    let dacl = object_security(raw(&dir_handle), SE_FILE_OBJECT)
        .and_then(|s| s.dacl())
        .map_err(|_| AuditUnavailable)?;
    if !only_broker_writes(&dacl, broker_sid) {
        return Err(AuditUnavailable);
    }
    let (access, disposition) = if append {
        (
            FILE_APPEND_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            OPEN_ALWAYS,
        )
    } else {
        (FILE_GENERIC_READ, OPEN_EXISTING)
    };
    let name = wide_path(path);
    // SAFETY: `name` is NUL-terminated.
    let file = owned(unsafe {
        CreateFileW(
            name.as_ptr(),
            access,
            // Other Broker threads append and read at the same time. No
            // FILE_SHARE_DELETE: the open file cannot be renamed away.
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null(),
            disposition,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    });
    let file = match file {
        Ok(file) => file,
        Err(e) if !append && is_not_found(&e) => return Ok(None),
        Err(_) => return Err(AuditUnavailable),
    };
    // SAFETY: a valid handle.
    if unsafe { GetFileType(raw(&file)) } != FILE_TYPE_DISK {
        return Err(AuditUnavailable);
    }
    let info = file_information(&file).map_err(|_| AuditUnavailable)?;
    if info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0
        || info.nNumberOfLinks != 1
    {
        return Err(AuditUnavailable);
    }
    Ok(Some(File::from(file)))
}

/// Rights on a folder that let a trustee change what it holds or who may
/// use it: write or append data (add a file or subfolder), write extended
/// attributes or attributes, delete a child, delete, `WRITE_DAC`,
/// `WRITE_OWNER`, and the generic write and all rights.
const FOLDER_WRITE_RIGHTS: u32 = 0x0002
    | 0x0004
    | 0x0010
    | 0x0040
    | 0x0100
    | 0x0001_0000
    | 0x0004_0000
    | 0x0008_0000
    | 0x1000_0000
    | 0x4000_0000;

/// Whether `dacl` lets only `broker_sid` change the audit folder: it is
/// protected from inheritance, holds only allow and deny entries, and no
/// allow entry for another SID grants any of [`FOLDER_WRITE_RIGHTS`]. A
/// folder created under `%ProgramData%` by anyone else inherits entries
/// that let users add files, so it is refused.
fn only_broker_writes(dacl: &Dacl, broker_sid: &str) -> bool {
    dacl.protected
        && dacl.aces.iter().all(|ace| match ace.kind {
            ACCESS_DENIED => true,
            ACCESS_ALLOWED => ace.sid == broker_sid || ace.mask & FOLDER_WRITE_RIGHTS == 0,
            _ => false,
        })
}

/// A manual-reset event that tells the pipe server, and every connection,
/// to stop.
#[derive(Debug)]
pub struct StopSignal(OwnedHandle);

impl StopSignal {
    /// A signal that is not yet set.
    pub fn new() -> io::Result<Self> {
        new_event().map(Self)
    }

    /// Sets the signal.
    pub fn signal(&self) {
        // SAFETY: a valid event handle.
        unsafe { SetEvent(raw(&self.0)) };
    }

    fn handle(&self) -> HANDLE {
        raw(&self.0)
    }
}

fn new_event() -> io::Result<OwnedHandle> {
    // SAFETY: an unnamed manual-reset event, initially not set.
    owned(unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) })
}

/// What waiting for an overlapped operation ended with.
enum Waited {
    /// The operation completed; the byte count, or the error.
    Done(io::Result<u32>),
    /// The stop signal was set; the operation was cancelled.
    Stopped,
}

/// Waits for the overlapped operation `ov` on `handle`, begun with result
/// `started` and signalling `event`, or for `stop`, which cancels it.
///
/// # Safety
/// `ov` is the operation's OVERLAPPED and stays valid until this returns.
unsafe fn wait_overlapped(
    handle: HANDLE,
    ov: *const OVERLAPPED,
    event: HANDLE,
    started: i32,
    stop: &StopSignal,
) -> Waited {
    if started == 0 {
        // SAFETY: no preconditions.
        let code = unsafe { GetLastError() };
        if code != ERROR_IO_PENDING {
            return Waited::Done(Err(io::Error::from_raw_os_error(code as i32)));
        }
        let handles = [event, stop.handle()];
        // SAFETY: two valid handles.
        let woke = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) };
        if woke != WAIT_OBJECT_0 {
            // SAFETY: `ov` stays alive until the cancelled operation is
            // reaped by the waiting GetOverlappedResult below.
            unsafe {
                CancelIoEx(handle, ov);
                let mut n = 0;
                GetOverlappedResult(handle, ov, &mut n, 1);
            }
            return Waited::Stopped;
        }
    }
    let mut n = 0;
    // SAFETY: the operation has completed; `ov` is the one it used.
    if unsafe { GetOverlappedResult(handle, ov, &mut n, 1) } == 0 {
        return Waited::Done(Err(last_error()));
    }
    Waited::Done(Ok(n))
}

/// The server end of one connected pipe instance, read and written with
/// overlapped I/O so that the stop signal interrupts a waiting read.
struct PipeStream<'a> {
    pipe: OwnedHandle,
    event: OwnedHandle,
    stop: &'a StopSignal,
}

impl PipeStream<'_> {
    /// The pipe client's SID and integrity, read while impersonating it.
    /// Impersonation is reverted before this returns, on every path; if it
    /// cannot be reverted the process aborts rather than go on as the
    /// client.
    fn caller(&self) -> io::Result<Caller> {
        struct Revert;
        impl Drop for Revert {
            fn drop(&mut self) {
                // SAFETY: no preconditions.
                if unsafe { RevertToSelf() } == 0 {
                    std::process::abort();
                }
            }
        }
        // SAFETY: a connected pipe handle from which a frame was read.
        if unsafe { ImpersonateNamedPipeClient(raw(&self.pipe)) } == 0 {
            return Err(last_error());
        }
        let revert = Revert;
        let mut token: HANDLE = ptr::null_mut();
        // SAFETY: the pseudo handle needs no closing. OpenAsSelf checks the
        // access against the Broker's own token.
        let ok = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) };
        let caller = if ok == 0 {
            Err(last_error())
        } else {
            owned(token).and_then(|token| {
                Ok(Caller {
                    sid: token_user(raw(&token))?,
                    integrity: token_integrity(raw(&token))?,
                })
            })
        };
        drop(revert);
        caller
    }
}

impl Read for PipeStream<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        let event = raw(&self.event);
        let mut ov = OVERLAPPED {
            hEvent: event,
            ..Default::default()
        };
        let handle = raw(&self.pipe);
        // SAFETY: `buf` and `ov` outlive the operation, which
        // wait_overlapped completes or cancels and reaps.
        let waited = unsafe {
            let started = ReadFile(handle, buf.as_mut_ptr(), len, ptr::null_mut(), &mut ov);
            wait_overlapped(handle, &ov, event, started, self.stop)
        };
        match waited {
            Waited::Done(Ok(n)) => Ok(n as usize),
            Waited::Done(Err(e)) if e.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) => Ok(0),
            Waited::Done(Err(e)) => Err(e),
            // Not `Interrupted`, which readers retry.
            Waited::Stopped => Err(io::Error::other("the Broker is stopping")),
        }
    }
}

impl Write for PipeStream<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        let event = raw(&self.event);
        let mut ov = OVERLAPPED {
            hEvent: event,
            ..Default::default()
        };
        let handle = raw(&self.pipe);
        // SAFETY: as for `read`.
        let waited = unsafe {
            let started = WriteFile(handle, buf.as_ptr(), len, ptr::null_mut(), &mut ov);
            wait_overlapped(handle, &ov, event, started, self.stop)
        };
        match waited {
            Waited::Done(result) => result.map(|n| n as usize),
            Waited::Stopped => Err(io::Error::other("the Broker is stopping")),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Creates one instance of the pipe `name` with the Broker's access list
/// for `broker_sid`, overlapped, rejecting remote clients. The first
/// instance also has `FILE_FLAG_FIRST_PIPE_INSTANCE`, so it fails if the
/// pipe already exists: a second Broker or a squatter cannot create it.
fn create_pipe(name: &str, broker_sid: &str, first: bool) -> io::Result<OwnedHandle> {
    let sd = security_descriptor(&super::pipe_sddl(broker_sid))?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: 0,
    };
    let mut open_mode = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
    if first {
        open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    let name = wide(name);
    // SAFETY: `name` is NUL-terminated and `attributes` valid for the call.
    owned(unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            open_mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            PIPE_BUFFER,
            PIPE_BUFFER,
            0,
            &attributes,
        )
    })
}

/// Creates the first instance of the pipe `name` exactly as the Broker
/// does, with this process's user as the Broker. Test builds only.
#[cfg(any(test, feature = "test-support"))]
pub fn create_test_pipe(name: &str) -> io::Result<OwnedHandle> {
    create_pipe(name, &current_user_sid()?, true)
}

/// Creates an instance of the pipe `name` with the Broker's access list for
/// `broker_sid`, as the first instance or as a later one. Test builds only.
#[cfg(any(test, feature = "test-support"))]
pub fn create_test_pipe_for(name: &str, broker_sid: &str, first: bool) -> io::Result<OwnedHandle> {
    create_pipe(name, broker_sid, first)
}

/// Connects to the pipe `name` as a client, at identification level: the
/// server can read the client's identity but not act as it.
pub fn connect(name: &str) -> io::Result<File> {
    let name = wide(name);
    // SAFETY: `name` is NUL-terminated.
    let handle = owned(unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            ptr::null(),
            OPEN_EXISTING,
            SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
            ptr::null_mut(),
        )
    })?;
    Ok(File::from(handle))
}

/// Waits for a client on `pipe`. `Ok(false)` when `stop` was set first.
fn accept(pipe: &OwnedHandle, event: &OwnedHandle, stop: &StopSignal) -> io::Result<bool> {
    loop {
        let mut ov = OVERLAPPED {
            hEvent: raw(event),
            ..Default::default()
        };
        // SAFETY: `ov` outlives the operation, completed or cancelled and
        // reaped by wait_overlapped.
        let started = unsafe { ConnectNamedPipe(raw(pipe), &mut ov) };
        if started == 0 {
            // SAFETY: no preconditions.
            match unsafe { GetLastError() } {
                ERROR_PIPE_CONNECTED => return Ok(true),
                // A client came and went before the connect: start over.
                ERROR_NO_DATA => {
                    // SAFETY: a valid pipe handle.
                    unsafe { DisconnectNamedPipe(raw(pipe)) };
                    continue;
                }
                _ => {}
            }
        }
        // SAFETY: `ov` is the connect's OVERLAPPED, alive until this returns.
        let waited = unsafe { wait_overlapped(raw(pipe), &ov, raw(event), started, stop) };
        return match waited {
            Waited::Done(result) => result.map(|_| true),
            Waited::Stopped => Ok(false),
        };
    }
}

/// Serves one connected client: reads the first frame, reads the caller
/// from its token and reverts, then handles its requests.
fn serve_client(
    pipe: OwnedHandle,
    store: &dyn MandateStore,
    network: &Network,
    audit: &AuditLog,
    stop: &StopSignal,
) -> io::Result<()> {
    let mut stream = PipeStream {
        pipe,
        event: new_event()?,
        stop,
    };
    let context = Context {
        store,
        network: network.clone(),
        terminal: &NoTerminal,
        audit: audit.clone(),
        owner: None,
    };
    super::serve_session(&mut stream, |stream| stream.caller(), &context)
}

fn serve_pipe<S: MandateStore + Sync>(
    name: &str,
    store: &S,
    network: &Network,
    audit: &AuditLog,
    stop: &StopSignal,
) -> io::Result<()> {
    let broker_sid = current_user_sid()?;
    let event = new_event()?;
    let mut listening = create_pipe(name, &broker_sid, true)?;
    let result = std::thread::scope(|scope| {
        loop {
            match accept(&listening, &event, stop) {
                Ok(true) => {}
                Ok(false) => return Ok(()),
                Err(e) => return Err(e),
            }
            // A new instance listens before the connected one is handed
            // off, so the pipe never disappears while the Broker runs.
            let next = create_pipe(name, &broker_sid, false)?;
            let connected = std::mem::replace(&mut listening, next);
            scope.spawn(move || {
                let _ = serve_client(connected, store, network, audit, stop);
            });
        }
    });
    if result.is_err() {
        // Let every connection end, then report the failure.
        stop.signal();
    }
    result
}

/// Runs the pipe server on [`PIPE_NAME`] until `stop` is set: each client
/// is served on its own thread, impersonated only to read its SID and
/// integrity. Returns once every connection has ended.
pub fn serve<S: MandateStore + Sync>(
    store: &S,
    network: &Network,
    audit: &AuditLog,
    stop: &StopSignal,
) -> io::Result<()> {
    serve_pipe(PIPE_NAME, store, network, audit, stop)
}

/// Like [`serve`], on the pipe `name`. Test builds only.
#[cfg(any(test, feature = "test-support"))]
pub fn serve_on<S: MandateStore + Sync>(
    name: &str,
    store: &S,
    network: &Network,
    audit: &AuditLog,
    stop: &StopSignal,
) -> io::Result<()> {
    serve_pipe(name, store, network, audit, stop)
}

/// The signal the control handler sets on `SERVICE_CONTROL_STOP`.
static STOP: OnceLock<StopSignal> = OnceLock::new();
/// The service status handle, as an address.
static STATUS: AtomicUsize = AtomicUsize::new(0);

fn report(state: SERVICE_STATUS_CURRENT_STATE, exit_code: u32, specific: u32) {
    let handle = STATUS.load(Ordering::SeqCst);
    if handle == 0 {
        return;
    }
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: if state == SERVICE_RUNNING {
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN
        } else {
            0
        },
        dwWin32ExitCode: exit_code,
        dwServiceSpecificExitCode: specific,
        dwCheckPoint: 0,
        dwWaitHint: if state == SERVICE_RUNNING || state == SERVICE_STOPPED {
            0
        } else {
            10_000
        },
    };
    // SAFETY: the handle came from RegisterServiceCtrlHandlerExW.
    unsafe { SetServiceStatus(handle as *mut c_void, &status) };
}

unsafe extern "system" fn control_handler(
    control: u32,
    _event_type: u32,
    _event_data: *mut c_void,
    _context: *mut c_void,
) -> u32 {
    match control {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
            report(SERVICE_STOP_PENDING, NO_ERROR, 0);
            if let Some(stop) = STOP.get() {
                stop.signal();
            }
            NO_ERROR
        }
        SERVICE_CONTROL_INTERROGATE => NO_ERROR,
        _ => ERROR_CALL_NOT_IMPLEMENTED,
    }
}

/// Runs the Broker inside the service: its own Credential Manager store
/// with serialized writes, the `%ProgramData%` audit log and the platform
/// network, until the stop signal.
fn run_broker(stop: &StopSignal) -> io::Result<()> {
    let store = SerializedStore::new(CredentialManagerStore);
    let broker_sid = current_user_sid()?;
    let audit = AuditLog::broker(&broker_sid);
    let network = Network::platform();
    report(SERVICE_RUNNING, NO_ERROR, 0);
    serve(&store, &network, &audit, stop)
}

unsafe extern "system" fn service_main(_argc: u32, _argv: *mut PWSTR) {
    let name = wide(SERVICE_NAME);
    // SAFETY: `name` is NUL-terminated; the handler is a valid function.
    let handle =
        unsafe { RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(control_handler), ptr::null()) };
    if handle.is_null() {
        return;
    }
    STATUS.store(handle as usize, Ordering::SeqCst);
    report(SERVICE_START_PENDING, NO_ERROR, 0);
    let stop = match StopSignal::new() {
        Ok(signal) => STOP.get_or_init(|| signal),
        Err(_) => {
            report(SERVICE_STOPPED, ERROR_SERVICE_SPECIFIC_ERROR, 1);
            return;
        }
    };
    match run_broker(stop) {
        Ok(()) => report(SERVICE_STOPPED, NO_ERROR, 0),
        Err(_) => report(SERVICE_STOPPED, ERROR_SERVICE_SPECIFIC_ERROR, 1),
    }
}

/// Hands the process to the service control manager as the service
/// [`SERVICE_NAME`] and returns when the service has stopped. Fails with
/// `service_required` when the process was not started as a service.
pub fn run_service() -> Result<(), CoreError> {
    let mut name = wide(SERVICE_NAME);
    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: name.as_mut_ptr(),
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW {
            lpServiceName: ptr::null_mut(),
            lpServiceProc: None,
        },
    ];
    // SAFETY: a valid, null-terminated table; `name` outlives the call.
    if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
        // SAFETY: no preconditions.
        return match unsafe { GetLastError() } {
            // Not started by the service control manager.
            ERROR_FAILED_SERVICE_CONTROLLER_CONNECT => Err(CoreError::ServiceRequired),
            // Any other failure to reach it is reported the same way.
            _ => Err(CoreError::ServiceRequired),
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    #[test]
    fn wait_for_single_object_is_linked() {
        // The stop signal starts unset and is set once signalled.
        let stop = StopSignal::new().unwrap();
        // SAFETY: a valid handle; zero timeout.
        assert_ne!(
            unsafe { WaitForSingleObject(stop.handle(), 0) },
            WAIT_OBJECT_0
        );
        stop.signal();
        // SAFETY: as above.
        assert_eq!(
            unsafe { WaitForSingleObject(stop.handle(), 0) },
            WAIT_OBJECT_0
        );
    }
}
