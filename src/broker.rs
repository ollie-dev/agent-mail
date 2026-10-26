//! The Broker (ADR-0004): on Windows, a service under `NT SERVICE\agent-mail`
//! that holds the sealed Mandates and carries out each request itself.
//!
//! This module is the platform-neutral part, compiled on every OS: the
//! protocol types, the length-prefixed framing, the version handshake, the
//! [`Caller`], request handling over any `Read + Write` transport, the
//! [`SerializedStore`] wrapper and the access-list builders. The service
//! control manager entry point, the named-pipe server and impersonation
//! live in [`windows`].
//!
//! Every frame is untrusted: any authenticated local user can open the
//! pipe. The caller's identity comes from the pipe client's token, never
//! from a request. No reply carries a Credential or a sealed record; only
//! the reply to `create` carries the new Agent Token, once.

use std::collections::HashMap;
use std::fmt;
use std::io::{self, Read, Write};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use zeroize::{Zeroize, Zeroizing};

use crate::core::{self, Command, Context, CoreError, Warning};
use crate::mandate::{Account, Credential, Grant, Mandate, Operation, Policy, TlsMode};
use crate::seal::{self, AgentToken};
use crate::store::{Entry, MandateStore, StoreError};
use crate::terminal::NoTerminal;

#[cfg(windows)]
pub mod windows;

/// The Windows service name the Broker registers under.
pub const SERVICE_NAME: &str = "agent-mail";
/// The Broker's named pipe.
pub const PIPE_NAME: &str = r"\\.\pipe\agent-mail";
/// The largest frame body accepted, in bytes (1 MiB).
pub const MAX_FRAME: usize = 1 << 20;
/// The integrity RID of an elevated token (`SECURITY_MANDATORY_HIGH_RID`).
pub const HIGH_INTEGRITY: u32 = 0x3000;
/// The Broker's version, compared in the `hello` exchange.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The SID of the built-in Administrators group.
pub const ADMINISTRATORS_SID: &str = "S-1-5-32-544";

/// The rights authenticated users get on the pipe: `FILE_GENERIC_READ |
/// FILE_GENERIC_WRITE` without `FILE_CREATE_PIPE_INSTANCE` (0x4), so they
/// can talk to the Broker but cannot add a server instance of its pipe.
pub const PIPE_CLIENT_RIGHTS: u32 = 0x0012_019b;

/// The pipe's security descriptor: owned by the Broker, read and write,
/// but not creating instances, for authenticated users
/// ([`PIPE_CLIENT_RIGHTS`]), full access for the Broker, nothing
/// inherited. The explicit owner is what the clients check (see
/// [`windows::served_by`]); no other user can set it.
pub fn pipe_sddl(broker_sid: &str) -> String {
    format!("O:{broker_sid}{}", pipe_dacl_sddl(broker_sid))
}

/// The DACL part of [`pipe_sddl`].
pub fn pipe_dacl_sddl(broker_sid: &str) -> String {
    format!("D:P(A;;0x12019b;;;AU)(A;;GA;;;{broker_sid})")
}

/// The audit folder's security descriptor: owned by the Broker, full
/// access for the Broker and read access for Administrators, inherited by
/// the log file, nothing inherited from above.
pub fn audit_dir_sddl(broker_sid: &str) -> String {
    format!("O:{broker_sid}D:P(A;OICI;FA;;;{broker_sid})(A;OICI;FR;;;BA)")
}

/// Who sent a request: read from the pipe client's token while
/// impersonating it, never from the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    /// The client's user SID, as a string such as `S-1-5-21-...`.
    pub sid: String,
    /// The client token's integrity RID, e.g. `0x2000` (medium) or
    /// [`HIGH_INTEGRITY`].
    pub integrity: u32,
}

/// A string wiped from memory when dropped, with a redacted `Debug`.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    /// Wraps `text`.
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// The secret itself.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// A request frame.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    /// The version handshake; must be the first request.
    Hello {
        /// The client's version.
        version: String,
    },
    /// An Agent command, `mandate list` or `mandate delete`.
    Run(Box<RunRequest>),
    /// `mandate create`, from an elevated client.
    Create(CreateRequest),
    /// The caller's own audit entries.
    Audit {},
}

/// What `run` carries: the parsed command, the presented Agent Tokens and
/// the client's proxy setting.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRequest {
    /// The command. `mandate_create` is refused here.
    pub command: Command,
    /// The client's `AGENT_MAIL_TOKEN` value, if set.
    #[serde(default)]
    pub tokens: Option<Secret>,
    /// The client's proxy variables.
    #[serde(default)]
    pub proxy: ProxyVars,
    /// Whether the client prints `--text`: then `result` is a JSON string
    /// holding the CLI's text output instead of its JSON.
    #[serde(default)]
    pub text: bool,
}

/// The client's proxy variables; each may hold proxy credentials.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyVars {
    /// `HTTPS_PROXY`.
    #[serde(rename = "HTTPS_PROXY", default)]
    pub https_upper: Option<Secret>,
    /// `https_proxy`.
    #[serde(rename = "https_proxy", default)]
    pub https_lower: Option<Secret>,
    /// `ALL_PROXY`.
    #[serde(rename = "ALL_PROXY", default)]
    pub all_upper: Option<Secret>,
    /// `all_proxy`.
    #[serde(rename = "all_proxy", default)]
    pub all_lower: Option<Secret>,
}

/// What `create` carries: the Mandate the human set up in the elevated
/// client, with each Account's Credential.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRequest {
    /// The Mandate's name.
    pub name: String,
    /// The Accounts, each with a distinct login.
    pub accounts: Vec<CreateAccount>,
    /// The Grants, each on one of the Accounts.
    pub grants: Vec<Grant>,
}

/// One Account of a [`CreateRequest`].
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAccount {
    /// IMAP login name.
    pub login: String,
    /// IMAP server host name.
    pub host: String,
    /// IMAP server port.
    pub port: u16,
    /// How the connection is secured.
    pub tls: TlsMode,
    /// The Credential. Never echoed, logged or returned.
    pub credential: Credential,
}

/// The reply to `hello`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloReply {
    /// The Broker's version.
    pub version: String,
}

/// A reply frame: `ok` for `hello`, otherwise `result` or `error`, with an
/// optional `warning` holding the CLI's invalid-token warning. `result`
/// holds the JSON the CLI prints, byte for byte.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Reply {
    /// The handshake's answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<HelloReply>,
    /// The command's result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Box<RawValue>>,
    /// The error body, as the CLI prints it under `error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
    /// The invalid-token warning, as the CLI prints it under `warning`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<Box<RawValue>>,
}

impl Reply {
    fn ok() -> Self {
        Self {
            ok: Some(HelloReply {
                version: VERSION.to_string(),
            }),
            result: None,
            error: None,
            warning: None,
        }
    }

    fn error(error: &CoreError) -> Self {
        Self {
            ok: None,
            result: None,
            error: Some(serde_json::to_value(error.body()).expect("error bodies serialise")),
            warning: None,
        }
    }

    fn result(json: String) -> Self {
        Self {
            ok: None,
            result: Some(RawValue::from_string(json).expect("results are JSON")),
            error: None,
            warning: None,
        }
    }

    /// The reply as a frame body. Wiped when dropped, since a `create`
    /// reply holds the Agent Token.
    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(serde_json::to_vec(self).expect("replies serialise"))
    }
}

impl Drop for Reply {
    fn drop(&mut self) {
        if let Some(result) = self.result.take() {
            let text: Box<str> = result.into();
            text.into_string().zeroize();
        }
    }
}

/// A frame could not be read.
#[derive(Debug)]
pub enum FrameError {
    /// The transport failed or ended inside a frame.
    Io(io::Error),
    /// The announced body is longer than [`MAX_FRAME`].
    TooLarge,
}

/// Writes one frame: the body's length as 4 bytes big-endian, then the body.
pub fn write_frame(writer: &mut impl Write, body: &[u8]) -> io::Result<()> {
    let len = u32::try_from(body.len())
        .ok()
        .filter(|&n| n as usize <= MAX_FRAME)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "frame too large"))?;
    let mut frame = Zeroizing::new(Vec::with_capacity(4 + body.len()));
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(body);
    writer.write_all(&frame)?;
    writer.flush()
}

/// Reads one frame's body; `None` when the transport ends cleanly before
/// a frame starts. A body over [`MAX_FRAME`] is not read.
pub fn read_frame(reader: &mut impl Read) -> Result<Option<Zeroizing<Vec<u8>>>, FrameError> {
    let mut header = [0u8; 4];
    let mut filled = 0;
    while filled < header.len() {
        match reader.read(&mut header[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => return Err(FrameError::Io(io::ErrorKind::UnexpectedEof.into())),
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(FrameError::Io(e)),
        }
    }
    let len = u32::from_be_bytes(header) as usize;
    if len > MAX_FRAME {
        return Err(FrameError::TooLarge);
    }
    let mut body = Zeroizing::new(vec![0u8; len]);
    reader.read_exact(&mut body).map_err(FrameError::Io)?;
    Ok(Some(body))
}

/// Serves one client connection until it ends or a reply closes it.
///
/// `identify` is called once, after the first frame has been read, to learn
/// the [`Caller`] (on Windows by impersonating the pipe client, which needs
/// data read first). It must have reverted any impersonation when it
/// returns: every store, IMAP and audit call happens afterwards.
///
/// `context` provides the store, network and audit log; its owner is set to
/// the caller's SID for every request.
pub fn serve_session<T: Read + Write>(
    transport: &mut T,
    identify: impl FnOnce(&mut T) -> io::Result<Caller>,
    context: &Context<'_>,
) -> io::Result<()> {
    let mut identify = Some(identify);
    let mut caller: Option<Caller> = None;
    let mut greeted = false;
    loop {
        let frame = match read_frame(transport) {
            Ok(Some(frame)) => frame,
            Ok(None) => return Ok(()),
            Err(FrameError::TooLarge) => {
                let reply = Reply::error(&CoreError::InvalidRequest);
                return write_frame(transport, &reply.to_bytes());
            }
            Err(FrameError::Io(e)) => return Err(e),
        };
        let caller = match (&caller, identify.take()) {
            (Some(caller), _) => caller,
            (None, Some(identify)) => caller.insert(identify(transport)?),
            (None, None) => unreachable!("identify runs with the first frame"),
        };
        let request = serde_json::from_slice::<Request>(&frame);
        drop(frame);
        let (reply, close) = match request {
            Err(_) => (Reply::error(&CoreError::InvalidRequest), true),
            Ok(Request::Hello { version }) if !greeted => {
                if version == VERSION {
                    greeted = true;
                    (Reply::ok(), false)
                } else {
                    let error = CoreError::BrokerVersionMismatch {
                        broker: Some(VERSION.to_string()),
                    };
                    (Reply::error(&error), true)
                }
            }
            Ok(_) if !greeted => (Reply::error(&CoreError::InvalidRequest), true),
            Ok(Request::Hello { .. }) => (Reply::error(&CoreError::InvalidRequest), true),
            Ok(Request::Run(run)) => handle_run(run, caller, context),
            Ok(Request::Create(create)) => handle_create(create, caller, context),
            Ok(Request::Audit {}) => (handle_audit(caller, context), false),
        };
        write_frame(transport, &reply.to_bytes())?;
        if close {
            return Ok(());
        }
    }
}

/// The context `context` gives, serving `caller` only.
fn context_for<'a>(context: &Context<'a>, caller: &Caller) -> Context<'a> {
    Context {
        store: context.store,
        network: context.network.clone(),
        terminal: &NoTerminal,
        audit: context.audit.clone(),
        owner: None,
        broker: None,
    }
    .for_owner(&caller.sid)
}

fn handle_run(run: Box<RunRequest>, caller: &Caller, context: &Context<'_>) -> (Reply, bool) {
    if matches!(run.command, Command::MandateCreate { .. }) {
        return (Reply::error(&CoreError::InvalidRequest), true);
    }
    let mut env: HashMap<String, String> = HashMap::new();
    let proxy = &run.proxy;
    for (name, value) in [
        (core::TOKEN_VAR, &run.tokens),
        ("HTTPS_PROXY", &proxy.https_upper),
        ("https_proxy", &proxy.https_lower),
        ("ALL_PROXY", &proxy.all_upper),
        ("all_proxy", &proxy.all_lower),
    ] {
        if let Some(value) = value {
            env.insert(name.to_string(), value.0.clone());
        }
    }
    let context = context_for(context, caller);
    let report = core::execute_reporting(&env, &run.command, &context);
    for (_, mut value) in env.drain() {
        value.zeroize();
    }
    let mut reply = match &report.result {
        Ok(outcome) if run.text => {
            let text = crate::render_outcome(outcome, true);
            Reply::result(serde_json::to_string(&text).expect("strings serialise"))
        }
        Ok(outcome) => {
            let line = crate::render_outcome(outcome, false);
            Reply::result(line.trim_end().to_string())
        }
        Err(error) => Reply::error(error),
    };
    reply.warning = Warning::invalid_tokens(&report.invalid_tokens).map(|warning| {
        RawValue::from_string(serde_json::to_string(&warning).expect("warnings serialise"))
            .expect("warnings are JSON")
    });
    (reply, false)
}

/// The `create` result: the new Mandate's ID and name and its Agent Token.
#[derive(Serialize)]
struct Created<'a> {
    id: &'a str,
    name: &'a str,
    token: &'a str,
}

fn handle_create(create: CreateRequest, caller: &Caller, context: &Context<'_>) -> (Reply, bool) {
    // Refused before anything else: no login, no store access.
    if caller.integrity < HIGH_INTEGRITY {
        return (Reply::error(&CoreError::ElevationRequired), false);
    }
    let Some(mandate) = checked_mandate(create) else {
        return (Reply::error(&CoreError::InvalidRequest), true);
    };
    // The live login of each Account, as the guided setup does it.
    for account in &mandate.accounts {
        if let Err(error) = crate::imap::list_mailboxes(account, &context.network) {
            let error = core::connector_error(error, Operation::List, &account.login, None);
            return (Reply::error(&error), false);
        }
    }
    let token = AgentToken::mint();
    let record = seal::seal_for(&token, &mandate, Some(&caller.sid));
    if let Err(error) = context.store.put(&record.key, &record.value) {
        return (Reply::error(&error.into()), false);
    }
    let created = Created {
        id: &mandate.id,
        name: &mandate.name,
        token: token.expose(),
    };
    let json = Zeroizing::new(serde_json::to_string(&created).expect("results serialise"));
    (Reply::result(json.as_str().to_string()), false)
}

/// The Mandate a `create` request describes, or `None` if any value is
/// unacceptable: an empty name, no Account, a repeated or invalid login, an
/// empty host, port 0, a Grant on an Account not in the request, or a value
/// holding CR, LF or NUL.
fn checked_mandate(create: CreateRequest) -> Option<Mandate> {
    let clean = |s: &str| !s.contains(['\r', '\n', '\0']);
    let name = create.name.trim();
    if name.is_empty() || !clean(name) || create.accounts.is_empty() {
        return None;
    }
    let mut accounts: Vec<Account> = Vec::with_capacity(create.accounts.len());
    for account in create.accounts {
        let fine = core::is_valid_account(&account.login)
            && !account.host.is_empty()
            && clean(&account.host)
            && account.port != 0
            && !accounts.iter().any(|a| a.login == account.login);
        if !fine {
            return None;
        }
        accounts.push(Account {
            login: account.login,
            host: account.host,
            port: account.port,
            tls: account.tls,
            credential: account.credential,
        });
    }
    for grant in &create.grants {
        let names_ok = grant
            .mailboxes
            .iter()
            .all(|m| clean(&String::from(m.clone())));
        if !accounts.iter().any(|a| a.login == grant.account) || !names_ok {
            return None;
        }
    }
    Some(Mandate {
        name: name.to_string(),
        id: seal::short_id(),
        created: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        accounts,
        policy: Policy {
            grants: create.grants,
        },
    })
}

/// The caller's own audit entries, in file order, as they are in the log.
fn handle_audit(caller: &Caller, context: &Context<'_>) -> Reply {
    let Ok(lines) = context.audit.lines() else {
        return Reply::error(&CoreError::AuditUnreadable);
    };
    let own: Vec<&str> = lines
        .iter()
        .map(String::as_str)
        .filter(|line| entry_sid(line).as_deref() == Some(caller.sid.as_str()))
        .collect();
    Reply::result(format!("{{\"entries\":[{}]}}", own.join(",")))
}

/// The service SID of the Windows service `name`, as `NT SERVICE\<name>`
/// has it: `S-1-5-80-` and the SHA-1 of the upper-cased name in UTF-16LE,
/// read as five little-endian 32-bit numbers.
pub fn service_sid(name: &str) -> String {
    let bytes: Vec<u8> = name
        .to_uppercase()
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let digest = ring::digest::digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, &bytes);
    let parts: Vec<String> = digest
        .as_ref()
        .chunks(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]).to_string())
        .collect();
    format!("S-1-5-80-{}", parts.join("-"))
}

/// The `sid` of one audit log line, if it is a JSON object with one.
pub fn entry_sid(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    value.as_object()?.get("sid")?.as_str().map(str::to_string)
}

/// A store whose writes are serialized: one mutex is held around every
/// `put` and `delete`, so the inner store's read-then-write add-only check
/// cannot race another write in the same process (F31).
#[derive(Debug, Default)]
pub struct SerializedStore<S> {
    inner: S,
    writes: Mutex<()>,
}

impl<S> SerializedStore<S> {
    /// Serializes the writes to `inner`.
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            writes: Mutex::new(()),
        }
    }

    /// The wrapped store.
    pub fn inner(&self) -> &S {
        &self.inner
    }
}

impl<S: MandateStore> MandateStore for SerializedStore<S> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        self.inner.get(key)
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
        // A panic in an earlier write leaves no partial state here.
        let _guard = self.writes.lock().unwrap_or_else(|e| e.into_inner());
        self.inner.put(key, value)
    }

    fn delete(&self, key: &[u8]) -> Result<bool, StoreError> {
        let _guard = self.writes.lock().unwrap_or_else(|e| e.into_inner());
        self.inner.delete(key)
    }

    fn entries(&self) -> Result<Vec<Entry>, StoreError> {
        self.inner.entries()
    }

    fn check(&self) -> Result<(), StoreError> {
        self.inner.check()
    }
}

/// The one service start argument that makes the Broker purge instead of
/// serving: `sc.exe start agent-mail purge`, as `uninstall.ps1` runs it.
pub const PURGE_ARG: &str = "purge";

/// Whether the service start arguments `args`, after the service name, ask
/// for the purge: exactly one argument, exactly `purge`. Anything else
/// runs the normal Broker. Starting the service at all needs
/// `SERVICE_START`, which only SYSTEM and Administrators hold.
pub fn is_purge_start<S: AsRef<str>>(args: &[S]) -> bool {
    matches!(args, [only] if only.as_ref() == PURGE_ARG)
}

/// Why [`purge`] failed.
#[derive(Debug)]
pub enum PurgeError {
    /// The store could not be read or a record could not be deleted.
    Store(StoreError),
    /// Records were still there after every one was deleted.
    Remaining(usize),
    /// The audit log or its folder could not be deleted.
    Audit(io::Error),
}

/// Deletes every record in `store`, then the audit log `audit.log` in
/// `audit_dir` and the folder itself, as `uninstall.ps1` asks the Broker
/// to. Returns how many records were deleted. A folder that does not exist
/// is fine; a folder that is a link is unlinked, never followed.
pub fn purge(store: &dyn MandateStore, audit_dir: &std::path::Path) -> Result<usize, PurgeError> {
    let mut deleted = 0;
    for (key, _) in store.entries().map_err(PurgeError::Store)? {
        if store.delete(&key).map_err(PurgeError::Store)? {
            deleted += 1;
        }
    }
    let left = store.entries().map_err(PurgeError::Store)?.len();
    if left != 0 {
        return Err(PurgeError::Remaining(left));
    }
    remove_audit_dir(audit_dir).map_err(PurgeError::Audit)?;
    Ok(deleted)
}

fn remove_audit_dir(dir: &std::path::Path) -> io::Result<()> {
    let not_found = |e: io::Error| {
        if e.kind() == io::ErrorKind::NotFound {
            Ok(())
        } else {
            Err(e)
        }
    };
    let metadata = match std::fs::symlink_metadata(dir) {
        Ok(metadata) => metadata,
        Err(e) => return not_found(e),
    };
    if metadata.file_type().is_symlink() {
        // A junction or directory link is removed as a directory, a file
        // link as a file; neither touches the target.
        return std::fs::remove_dir(dir).or_else(|_| std::fs::remove_file(dir));
    }
    if !metadata.is_dir() {
        return std::fs::remove_file(dir);
    }
    std::fs::remove_file(dir.join(crate::audit::FILE_NAME)).or_else(not_found)?;
    std::fs::remove_dir_all(dir).or_else(not_found)
}

/// Runs `agent-mail broker`: on Windows, hands the process to the service
/// control manager until the service stops; elsewhere, or when the process
/// was not started as a service, prints one JSON error and exits 1.
pub fn run_service(text: bool) -> crate::RunOutput {
    #[cfg(windows)]
    let result = windows::run_service();
    #[cfg(not(windows))]
    let result: Result<(), CoreError> = Err(CoreError::BrokerUnsupported);
    match result {
        Ok(()) => crate::RunOutput {
            stdout: String::new(),
            stderr: String::new(),
            code: 0,
        },
        Err(error) => crate::RunOutput {
            stdout: String::new(),
            stderr: crate::render_error(&error, text),
            code: 1,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip_and_oversized_frames_are_not_read() {
        let mut buf = Vec::new();
        write_frame(&mut buf, b"{\"hello\":{}}").unwrap();
        assert_eq!(&buf[..4], &[0, 0, 0, 12]);
        let mut reader = &buf[..];
        assert_eq!(
            &**read_frame(&mut reader).unwrap().unwrap(),
            b"{\"hello\":{}}"
        );
        assert!(read_frame(&mut reader).unwrap().is_none());

        let too_large = ((MAX_FRAME + 1) as u32).to_be_bytes();
        assert!(matches!(
            read_frame(&mut &too_large[..]),
            Err(FrameError::TooLarge)
        ));
        assert!(matches!(
            read_frame(&mut &[0u8, 0][..]),
            Err(FrameError::Io(_))
        ));
    }

    #[test]
    fn service_sids_match_windows() {
        // As `sc.exe showsid TrustedInstaller` prints it.
        assert_eq!(
            service_sid("TrustedInstaller"),
            "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464"
        );
    }

    #[test]
    fn secrets_are_redacted_in_debug() {
        assert_eq!(format!("{:?}", Secret::new("amt_x")), "Secret(<redacted>)");
    }
}
