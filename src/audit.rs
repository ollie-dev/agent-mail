//! The audit log: one metadata-only JSON line per Agent mail operation,
//! appended to a file in the mail owner's per-user data directory, or, for
//! the Windows Broker (ADR-0004), to `%ProgramData%\agent-mail\audit.log`.
//!
//! An [`Entry`] has only the fields below, none of which can hold a
//! Credential, an Agent Token, a token hash or a message body.
//! `agent-mail audit` reads it back: in-process the whole per-user log,
//! and through the Broker only the entries carrying the caller's own SID.

use std::ffi::OsStr;
use std::fs::{DirBuilder, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Directory name under the platform's per-user data directory.
const APP_DIR: &str = "agent-mail";
/// File name of the log.
const FILE_NAME: &str = "audit.log";

/// Whether the Policy granted the operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    /// The Policy granted the operation.
    Allowed,
    /// The Policy did not grant it, or no Mandate was opened.
    Denied,
}

/// One audit log entry. Keys serialise in field order; absent ones are
/// omitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// RFC 3339 UTC to the second.
    pub time: String,
    /// The Mandate short ID, if a Mandate was opened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mandate: Option<String>,
    /// The operation name, as in `Operation::as_str`.
    pub operation: &'static str,
    /// The Account's login, if one Account was chosen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// The mailbox involved, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailbox: Option<String>,
    /// The target mailbox, as the Agent named it, for `move` calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Message references involved, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<String>>,
    /// The recipients the Agent gave, for `draft` calls. Addresses only,
    /// never a subject or body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipients: Option<Vec<String>>,
    /// The flag change attempted, e.g. `+\Seen` or `-\Flagged`, for
    /// `flag` calls whose flag name was supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<String>,
    /// Number of results returned, if the operation returns results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<usize>,
    /// The Policy's decision.
    pub outcome: Decision,
    /// The stable error code if the call failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    /// The SID of the Broker's pipe client the call was made for. `None`
    /// in-process, and then left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sid: Option<String>,
}

impl Entry {
    /// The entry as one JSON line ending in `\n`. JSON escapes every control
    /// character, so no value can start a second line.
    pub fn to_line(&self) -> String {
        let mut line = serde_json::to_string(self).expect("entries always serialise");
        line.push('\n');
        line
    }
}

/// The entry could not be appended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditUnavailable;

/// Marker that a release build must not contain (checked by a test).
#[cfg(any(test, feature = "test-support"))]
pub const REDIRECT_MARKER: &str = "agent-mail-audit-redirect";

/// Keeps the marker bytes in every binary the redirect is compiled into.
#[cfg(any(test, feature = "test-support"))]
#[used]
static REDIRECT_MARKER_BYTES: [u8; REDIRECT_MARKER.len()] = *b"agent-mail-audit-redirect";

/// Environment variable naming the audit log of a test-support build of
/// the binary. A release build has no such variable.
#[cfg(any(test, feature = "test-support"))]
pub const TEST_LOG_VAR: &str = "AGENT_MAIL_TEST_AUDIT_LOG";

/// Where entries go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditLog {
    /// `None` when the platform location could not be determined.
    path: Option<PathBuf>,
    /// The Broker's own SID when this is the Broker's log, whose folder
    /// gets the Broker's access list and is checked before each use.
    #[cfg(windows)]
    broker_sid: Option<String>,
}

impl AuditLog {
    /// The log at the platform's per-user location; see [`platform_path`].
    pub fn platform() -> Self {
        Self::with_path(platform_path())
    }

    fn with_path(path: Option<PathBuf>) -> Self {
        Self {
            path,
            #[cfg(windows)]
            broker_sid: None,
        }
    }

    /// The Broker's log, `%ProgramData%\agent-mail\audit.log`, with
    /// `%ProgramData%` from the known folder (`FOLDERID_ProgramData`), not
    /// an environment variable. Its folder is created with
    /// [`crate::broker::audit_dir_sddl`] for `broker_sid`.
    #[cfg(windows)]
    pub fn broker(broker_sid: &str) -> Self {
        Self {
            path: program_data().map(|base| base.join(APP_DIR).join(FILE_NAME)),
            broker_sid: Some(broker_sid.to_string()),
        }
    }

    /// The Broker's log under `program_data` instead of the known folder.
    /// Test builds only.
    #[cfg(all(windows, any(test, feature = "test-support")))]
    pub fn broker_at(program_data: &Path, broker_sid: &str) -> Self {
        Self {
            path: Some(program_data.join(APP_DIR).join(FILE_NAME)),
            broker_sid: Some(broker_sid.to_string()),
        }
    }

    /// The log at `path`. Test builds only, so a release build can never be
    /// pointed elsewhere.
    #[cfg(any(test, feature = "test-support"))]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self::with_path(Some(path.into()))
    }

    /// A log at a fresh file in a new directory under the system temporary
    /// directory. Test builds only.
    #[cfg(any(test, feature = "test-support"))]
    pub fn temporary() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!(
            "agent-mail-test-{}-{nanos}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        Self::at(dir.join(FILE_NAME))
    }

    /// The log a test-support build of the binary writes to: the file named
    /// by [`TEST_LOG_VAR`], or else a fresh temporary file. Never the user's
    /// data directory, so binary tests leave it alone. Test builds only.
    #[cfg(any(test, feature = "test-support"))]
    pub fn for_test_binary() -> Self {
        match std::env::var_os(TEST_LOG_VAR) {
            Some(path) if !path.is_empty() => Self::at(path),
            _ => Self::temporary(),
        }
    }

    /// The file entries are appended to, if known.
    #[cfg(any(test, feature = "test-support"))]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Appends `entry` as one line. Creates the directory (mode `0700`) and
    /// the file (mode `0600`) on first write, and on Unix tightens an
    /// existing directory and file to those modes. Neither the log's own
    /// directory nor the file may be a symbolic link (`O_NOFOLLOW`), and the
    /// file must be a regular file. Existing content is never rewritten: the
    /// file is opened in append mode and the line goes out in one write.
    ///
    /// The Broker's log is checked instead as
    /// [`crate::broker::windows::open_audit_log`] describes.
    pub fn append(&self, entry: &Entry) -> Result<(), AuditUnavailable> {
        let path = self.path.as_deref().ok_or(AuditUnavailable)?;
        #[cfg(windows)]
        if let Some(sid) = &self.broker_sid {
            let mut file =
                crate::broker::windows::open_audit_log(path, sid, true)?.ok_or(AuditUnavailable)?;
            return file
                .write_all(entry.to_line().as_bytes())
                .and_then(|()| file.flush())
                .map_err(|_| AuditUnavailable);
        }
        let dir = path.parent().ok_or(AuditUnavailable)?;
        let mut builder = DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(dir).map_err(|_| AuditUnavailable)?;
        #[cfg(unix)]
        restrict_dir(dir)?;
        let mut options = OpenOptions::new();
        options.append(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // O_NONBLOCK keeps a planted FIFO from hanging the call; it has
            // no effect on a regular file.
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let mut file = options.open(path).map_err(|_| AuditUnavailable)?;
        let metadata = file.metadata().map_err(|_| AuditUnavailable)?;
        if !metadata.is_file() {
            return Err(AuditUnavailable);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o7777 != 0o600 {
                // fchmod on the open descriptor, not the path.
                file.set_permissions(std::fs::Permissions::from_mode(0o600))
                    .map_err(|_| AuditUnavailable)?;
            }
        }
        file.write_all(entry.to_line().as_bytes())
            .and_then(|()| file.flush())
            .map_err(|_| AuditUnavailable)
    }
}

impl AuditLog {
    /// Every line of the log, without its `\n`, in file order; empty when
    /// the log does not exist yet. The Broker's log is checked as for
    /// [`AuditLog::append`].
    pub fn lines(&self) -> Result<Vec<String>, AuditUnavailable> {
        use std::io::Read;
        let path = self.path.as_deref().ok_or(AuditUnavailable)?;
        #[cfg(windows)]
        let opened = match &self.broker_sid {
            Some(sid) => crate::broker::windows::open_audit_log(path, sid, false)?,
            None => open_existing(path)?,
        };
        #[cfg(not(windows))]
        let opened = open_existing(path)?;
        let Some(mut file) = opened else {
            return Ok(Vec::new());
        };
        let mut text = String::new();
        file.read_to_string(&mut text)
            .map_err(|_| AuditUnavailable)?;
        Ok(text.lines().map(str::to_string).collect())
    }
}

/// The file at `path` opened for reading, or `None` if it or its
/// directory does not exist. On Unix, neither the log's own directory nor
/// the file may be a symbolic link, and the file must be a regular file.
fn open_existing(path: &Path) -> Result<Option<std::fs::File>, AuditUnavailable> {
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        match std::fs::symlink_metadata(dir) {
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
                return Err(AuditUnavailable);
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(AuditUnavailable),
        }
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(AuditUnavailable),
    };
    match file.metadata() {
        Ok(meta) if meta.is_file() => Ok(Some(file)),
        _ => Err(AuditUnavailable),
    }
}

/// Opens the log's directory without following a symbolic link and sets
/// its mode to `0700` through the open descriptor.
#[cfg(unix)]
fn restrict_dir(dir: &Path) -> Result<(), AuditUnavailable> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let handle = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
        .open(dir)
        .map_err(|_| AuditUnavailable)?;
    let mode = handle
        .metadata()
        .map_err(|_| AuditUnavailable)?
        .permissions()
        .mode();
    if mode & 0o7777 != 0o700 {
        handle
            .set_permissions(std::fs::Permissions::from_mode(0o700))
            .map_err(|_| AuditUnavailable)?;
    }
    Ok(())
}

/// Which per-user data directory convention applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// `~/Library/Application Support`.
    MacOs,
    /// The XDG base directory convention.
    OtherUnix,
    /// The Local AppData known folder; `home` is that folder.
    Windows,
}

/// The log path under `home` on `platform`. On other Unix,
/// `xdg_data_home` is used when it is set to an absolute path. On Windows
/// `home` is the Local AppData folder.
pub fn log_path(platform: Platform, home: &Path, xdg_data_home: Option<&OsStr>) -> PathBuf {
    let data = match platform {
        Platform::MacOs => home.join("Library").join("Application Support"),
        Platform::OtherUnix => match xdg_data_home.map(Path::new) {
            Some(dir) if dir.is_absolute() => dir.to_path_buf(),
            _ => home.join(".local").join("share"),
        },
        Platform::Windows => home.to_path_buf(),
    };
    data.join(APP_DIR).join(FILE_NAME)
}

/// The log path for the running user. On macOS the home directory comes
/// from the user database, never from `$HOME`, so it cannot be redirected
/// through the environment.
#[cfg(unix)]
pub fn platform_path() -> Option<PathBuf> {
    let home = home_from_user_database()?;
    if cfg!(target_os = "macos") {
        Some(log_path(Platform::MacOs, &home, None))
    } else {
        let xdg = std::env::var_os("XDG_DATA_HOME");
        Some(log_path(Platform::OtherUnix, &home, xdg.as_deref()))
    }
}

/// The log path for the running user, under the Local AppData known folder
/// (`SHGetKnownFolderPath`), not an environment variable.
#[cfg(windows)]
pub fn platform_path() -> Option<PathBuf> {
    let base = local_app_data()?;
    Some(log_path(Platform::Windows, &base, None))
}

/// No known per-user data directory on this OS.
#[cfg(not(any(unix, windows)))]
pub fn platform_path() -> Option<PathBuf> {
    None
}

/// The Local AppData known folder (`FOLDERID_LocalAppData`).
#[cfg(windows)]
fn local_app_data() -> Option<PathBuf> {
    use windows_sys::Win32::UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath};

    let mut raw: *mut u16 = std::ptr::null_mut();
    // SAFETY: valid pointers; the returned string is freed by
    // `known_folder`, as the API requires even on failure.
    let hr =
        unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, std::ptr::null_mut(), &mut raw) };
    // SAFETY: `raw` is null or was allocated by SHGetKnownFolderPath.
    unsafe { known_folder(hr, raw) }
}

/// The ProgramData known folder (`FOLDERID_ProgramData`), for the Broker's
/// audit log.
#[cfg(windows)]
fn program_data() -> Option<PathBuf> {
    use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramData, SHGetKnownFolderPath};

    let mut raw: *mut u16 = std::ptr::null_mut();
    // SAFETY: as in `local_app_data`.
    let hr =
        unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramData, 0, std::ptr::null_mut(), &mut raw) };
    // SAFETY: `raw` is null or was allocated by SHGetKnownFolderPath.
    unsafe { known_folder(hr, raw) }
}

/// The path `SHGetKnownFolderPath` returned in `raw` with result `hr`, if
/// it succeeded and is absolute. Frees `raw`.
///
/// # Safety
/// `raw` is null or was allocated by `SHGetKnownFolderPath`.
#[cfg(windows)]
unsafe fn known_folder(hr: i32, raw: *mut u16) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;

    let path = if hr >= 0 && !raw.is_null() {
        let mut len = 0;
        // SAFETY: on success `raw` is a NUL-terminated UTF-16 string.
        while unsafe { *raw.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: `len` units were just read.
        let wide = unsafe { std::slice::from_raw_parts(raw, len) };
        Some(PathBuf::from(std::ffi::OsString::from_wide(wide)))
    } else {
        None
    };
    // SAFETY: `raw` is null or was allocated by SHGetKnownFolderPath.
    unsafe { CoTaskMemFree(raw.cast_const().cast()) };
    path.filter(|p| p.is_absolute())
}

/// The real user's home directory from the user database (`getpwuid_r`).
#[cfg(unix)]
fn home_from_user_database() -> Option<PathBuf> {
    use std::ffi::CStr;
    use std::os::unix::ffi::OsStrExt;

    let mut buf: Vec<libc::c_char> = vec![0; 4096];
    loop {
        // SAFETY: `passwd` is plain data; all-zero is a valid value that
        // getpwuid_r overwrites.
        let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        // SAFETY: every pointer is valid for the call and `buf.len()` is the
        // buffer's true size.
        let rc = unsafe {
            libc::getpwuid_r(
                libc::getuid(),
                &mut pwd,
                buf.as_mut_ptr(),
                buf.len(),
                &mut result,
            )
        };
        if rc == libc::ERANGE && buf.len() < 1 << 20 {
            buf.resize(buf.len() * 2, 0);
            continue;
        }
        if rc != 0 || result.is_null() || pwd.pw_dir.is_null() {
            return None;
        }
        // SAFETY: on success `pw_dir` points to a NUL-terminated string
        // inside `buf`, which outlives this borrow.
        let dir = unsafe { CStr::from_ptr(pwd.pw_dir) };
        let home = PathBuf::from(OsStr::from_bytes(dir.to_bytes()));
        return home.is_absolute().then_some(home);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_path_is_under_application_support() {
        assert_eq!(
            log_path(Platform::MacOs, Path::new("/Users/alice"), None),
            PathBuf::from("/Users/alice/Library/Application Support/agent-mail/audit.log")
        );
        // XDG_DATA_HOME plays no part on macOS.
        assert_eq!(
            log_path(
                Platform::MacOs,
                Path::new("/Users/alice"),
                Some(OsStr::new("/x/data"))
            ),
            PathBuf::from("/Users/alice/Library/Application Support/agent-mail/audit.log")
        );
    }

    #[cfg(unix)]
    #[test]
    fn other_unix_path_follows_xdg_data_home() {
        let home = Path::new("/home/alice");
        assert_eq!(
            log_path(Platform::OtherUnix, home, Some(OsStr::new("/x/data"))),
            PathBuf::from("/x/data/agent-mail/audit.log")
        );
        for unset in [None, Some(OsStr::new("")), Some(OsStr::new("relative"))] {
            assert_eq!(
                log_path(Platform::OtherUnix, home, unset),
                PathBuf::from("/home/alice/.local/share/agent-mail/audit.log"),
                "XDG_DATA_HOME = {unset:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn platform_path_comes_from_the_user_database() {
        let path = platform_path().expect("the user database names a home directory");
        assert!(path.is_absolute());
        assert!(path.ends_with("agent-mail/audit.log"));
    }

    #[cfg(unix)]
    fn sample() -> Entry {
        Entry {
            time: "2026-10-21T09:30:15Z".to_string(),
            mandate: None,
            operation: "list",
            account: None,
            mailbox: None,
            target: None,
            messages: None,
            recipients: None,
            change: None,
            results: None,
            outcome: Decision::Denied,
            reason: Some("missing_token"),
            sid: None,
        }
    }

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::symlink_metadata(path)
            .unwrap()
            .permissions()
            .mode()
            & 0o7777
    }

    #[cfg(unix)]
    #[test]
    fn existing_loose_directory_and_file_are_tightened() {
        use std::os::unix::fs::PermissionsExt;
        let log = AuditLog::temporary();
        let path = log.path().unwrap().to_path_buf();
        let dir = path.parent().unwrap();
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(&path, "earlier\n").unwrap();
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        log.append(&sample()).unwrap();
        assert_eq!(mode_of(dir), 0o700);
        assert_eq!(mode_of(&path), 0o600);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!("earlier\n{}", sample().to_line())
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_log_file_is_refused_and_its_target_untouched() {
        use std::os::unix::fs::PermissionsExt;
        let log = AuditLog::temporary();
        let path = log.path().unwrap().to_path_buf();
        let dir = path.parent().unwrap();
        std::fs::create_dir_all(dir).unwrap();
        let target = dir.join("elsewhere");
        std::fs::write(&target, "victim\n").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();

        assert_eq!(log.append(&sample()), Err(AuditUnavailable));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "victim\n");
        assert_eq!(mode_of(&target), 0o644);
        assert!(
            std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_log_directory_is_refused_and_its_target_untouched() {
        use std::os::unix::fs::PermissionsExt;
        let log = AuditLog::temporary();
        let path = log.path().unwrap().to_path_buf();
        let dir = path.parent().unwrap();
        let real = dir.with_extension("real");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(&real, dir).unwrap();

        assert_eq!(log.append(&sample()), Err(AuditUnavailable));
        assert_eq!(mode_of(&real), 0o755);
        assert!(!real.join("audit.log").exists());
        std::fs::remove_file(dir).unwrap();
        std::fs::remove_dir_all(&real).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn planted_fifo_is_refused_without_hanging() {
        let log = AuditLog::temporary();
        let path = log.path().unwrap().to_path_buf();
        let dir = path.parent().unwrap();
        std::fs::create_dir_all(dir).unwrap();
        let c_path = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        // SAFETY: a valid NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
        assert_eq!(log.append(&sample()), Err(AuditUnavailable));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn entry_keys_are_in_order_and_control_characters_are_escaped() {
        let entry = Entry {
            time: "2026-10-21T09:30:15Z".to_string(),
            mandate: Some("aaaa1111".to_string()),
            operation: "move",
            account: Some("a\r\nb@example.test".to_string()),
            mailbox: Some("Evil\n{\"outcome\":\"allowed\"}".to_string()),
            target: None,
            messages: Some(vec!["1\n2".to_string()]),
            recipients: None,
            change: None,
            results: Some(2),
            outcome: Decision::Denied,
            reason: Some("denied"),
            sid: None,
        };
        let line = entry.to_line();
        assert_eq!(line.matches('\n').count(), 1);
        assert!(line.ends_with('\n'));
        assert_eq!(
            line,
            "{\"time\":\"2026-10-21T09:30:15Z\",\"mandate\":\"aaaa1111\",\"operation\":\"move\",\"account\":\"a\\r\\nb@example.test\",\"mailbox\":\"Evil\\n{\\\"outcome\\\":\\\"allowed\\\"}\",\"messages\":[\"1\\n2\"],\"results\":2,\"outcome\":\"denied\",\"reason\":\"denied\"}\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn first_write_creates_private_directory_and_file() {
        use std::os::unix::fs::PermissionsExt;
        let log = AuditLog::temporary();
        let path = log.path().unwrap().to_path_buf();
        let entry = Entry {
            time: "2026-10-21T09:30:15Z".to_string(),
            mandate: None,
            operation: "list",
            account: None,
            mailbox: None,
            target: None,
            messages: None,
            recipients: None,
            change: None,
            results: None,
            outcome: Decision::Denied,
            reason: Some("missing_token"),
            sid: None,
        };
        log.append(&entry).unwrap();
        log.append(&entry).unwrap();
        let dir_mode = std::fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode();
        let file_mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(dir_mode & 0o777, 0o700);
        assert_eq!(file_mode & 0o777, 0o600);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            entry.to_line().repeat(2)
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
