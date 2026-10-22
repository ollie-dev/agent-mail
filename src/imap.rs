//! The IMAP connector. TLS is always required: implicit TLS, or a mandatory
//! STARTTLS upgrade. The Credential is sent only over an established TLS
//! session.

use std::sync::Arc;
use std::time::Duration;

use async_imap::imap_proto::rfc4315::UidSetMember;
use async_imap::imap_proto::{Capability, NameAttribute, Response, ResponseCode, Status};
use async_imap::types::UnsolicitedResponse;
use async_imap::types::{Fetch, Flag};
use async_imap::{Client, Session};
use futures_util::TryStreamExt;
use rustls::ClientConfig;
use rustls::pki_types::ServerName;
use rustls_platform_verifier::BuilderVerifierExt;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::flag::{Flag as MailFlag, FlagAction, store_item};
use crate::mandate::{Account, TlsMode};

const TIMEOUT: Duration = Duration::from_secs(30);

/// Which certificates the connector trusts.
#[derive(Clone, Default)]
pub enum TlsTrust {
    /// The operating system's certificate verifier.
    #[default]
    Platform,
    /// Only these roots. Test builds only: lets tests trust the fake server.
    #[cfg(any(test, feature = "test-support"))]
    OnlyRoots(Vec<rustls::pki_types::CertificateDer<'static>>),
}

impl std::fmt::Debug for TlsTrust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Platform => f.write_str("Platform"),
            #[cfg(any(test, feature = "test-support"))]
            Self::OnlyRoots(roots) => write!(f, "OnlyRoots({} roots)", roots.len()),
        }
    }
}

/// How the connector reaches servers: the TLS trust and, in test builds only,
/// host rewrites that send a preset host such as `imap.gmail.com` to a fake
/// server. TLS still verifies the Account's own host name.
#[derive(Clone, Default, Debug)]
pub struct Network {
    trust: TlsTrust,
    #[cfg(any(test, feature = "test-support"))]
    routes: Vec<Route>,
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Debug)]
struct Route {
    from: (String, u16),
    to: (String, u16),
}

/// Marker that a release build must not contain (checked by a test).
#[cfg(any(test, feature = "test-support"))]
pub const ROUTE_MARKER: &str = "agent-mail-connect-override";

/// Keeps the marker bytes in every binary the override is compiled into.
#[cfg(any(test, feature = "test-support"))]
#[used]
static ROUTE_MARKER_BYTES: [u8; ROUTE_MARKER.len()] = *b"agent-mail-connect-override";

impl Network {
    /// Direct connections, verified by the operating system.
    pub fn platform() -> Self {
        Self::default()
    }

    /// Direct connections trusting only `roots`. Test builds only.
    #[cfg(any(test, feature = "test-support"))]
    pub fn trusting_only(roots: Vec<rustls::pki_types::CertificateDer<'static>>) -> Self {
        Self {
            trust: TlsTrust::OnlyRoots(roots),
            routes: Vec::new(),
        }
    }

    /// Connects to `to_host:to_port` whenever an Account names
    /// `host:port`. Test builds only.
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_route(mut self, host: &str, port: u16, to_host: &str, to_port: u16) -> Self {
        self.routes.push(Route {
            from: (host.to_string(), port),
            to: (to_host.to_string(), to_port),
        });
        self
    }

    /// Where to open the TCP connection for `host:port`.
    fn target(&self, host: &str, port: u16) -> (String, u16) {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(route) = self
            .routes
            .iter()
            .find(|r| r.from.0.eq_ignore_ascii_case(host) && r.from.1 == port)
        {
            return route.to.clone();
        }
        (host.to_string(), port)
    }
}

/// Why talking to the IMAP server failed. Never carries a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImapError {
    /// The server offered no TLS: no handshake, or no STARTTLS.
    TlsRequired,
    /// The server's certificate was not trusted.
    Certificate,
    /// The host could not be reached, or the call timed out.
    Connection,
    /// The server refused the login.
    Login,
    /// The server sent something unexpected.
    Protocol,
    /// The server refused to open the mailbox (`EXAMINE` or `SELECT`
    /// answered `NO`).
    MailboxNotFound,
    /// The mailbox's UIDVALIDITY differs from the reference's, so its UID
    /// may now name another message. Nothing was fetched or changed.
    StaleReference,
    /// No message has the UID (the server returned no FETCH for it).
    MessageNotFound,
    /// The server advertises neither `MOVE` nor `UIDPLUS`. Nothing was
    /// selected, copied or changed.
    MoveUnsupported,
    /// The server refused to move or copy into the target mailbox (`NO`,
    /// e.g. `[TRYCREATE]`).
    TargetNotFound,
    /// No mailbox `LIST` returned carries the `\Drafts` special-use
    /// attribute. Nothing was appended.
    DraftsNotFound,
    /// A Gmail search was asked for, but the server does not advertise
    /// `X-GM-EXT-1`. No mailbox was opened and nothing was searched.
    GmailUnsupported,
}

/// Logs in to the Account and lists every mailbox, raw (modified UTF-7) names
/// in server order. Doubles as the live login check of the guided setup.
pub fn list_mailboxes(account: &Account, network: &Network) -> Result<Vec<String>, ImapError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ImapError::Connection)?;
    runtime.block_on(async {
        tokio::time::timeout(TIMEOUT, list_async(account, network))
            .await
            .map_err(|_| ImapError::Connection)?
    })
}

fn tls_connector(trust: &TlsTrust) -> Result<TlsConnector, ImapError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| ImapError::Certificate)?;
    let config = match trust {
        TlsTrust::Platform => builder
            .with_platform_verifier()
            .map_err(|_| ImapError::Certificate)?
            .with_no_client_auth(),
        #[cfg(any(test, feature = "test-support"))]
        TlsTrust::OnlyRoots(roots) => {
            let mut store = rustls::RootCertStore::empty();
            for root in roots {
                store
                    .add(root.clone())
                    .map_err(|_| ImapError::Certificate)?;
            }
            builder.with_root_certificates(store).with_no_client_auth()
        }
    };
    Ok(TlsConnector::from(Arc::new(config)))
}

type TlsSession = Session<tokio_rustls::client::TlsStream<TcpStream>>;

/// Connects with TLS (implicit, or a mandatory STARTTLS upgrade) and logs
/// in. The Credential is sent only over the established TLS session.
async fn login(account: &Account, network: &Network) -> Result<TlsSession, ImapError> {
    let connector = tls_connector(&network.trust)?;
    let server_name =
        ServerName::try_from(account.host.clone()).map_err(|_| ImapError::Connection)?;
    let (host, port) = network.target(&account.host, account.port);
    let tcp = TcpStream::connect((host.as_str(), port))
        .await
        .map_err(|_| ImapError::Connection)?;

    let tls = match account.tls {
        TlsMode::Implicit => {
            let tls = connector
                .connect(server_name, tcp)
                .await
                .map_err(handshake_error)?;
            let mut client = Client::new(tls);
            expect_greeting(&mut client).await?;
            client.into_inner()
        }
        TlsMode::Starttls => {
            let mut client = Client::new(tcp);
            expect_greeting(&mut client).await?;
            if !offers_starttls(&mut client).await? {
                return Err(ImapError::TlsRequired);
            }
            client
                .run_command_and_check_ok("STARTTLS", None)
                .await
                .map_err(|_| ImapError::TlsRequired)?;
            // Anything the server sent before the handshake is discarded.
            connector
                .connect(server_name, client.into_inner())
                .await
                .map_err(handshake_error)?
        }
    };

    let client = Client::new(tls);
    client
        .login(&account.login, account.credential.expose())
        .await
        .map_err(|_| ImapError::Login)
}

async fn list_async(account: &Account, network: &Network) -> Result<Vec<String>, ImapError> {
    let mut session = login(account, network).await?;
    let names: Vec<String> = session
        .list(Some(""), Some("*"))
        .await
        .map_err(|_| ImapError::Protocol)?
        .map_ok(|name| name.name().to_string())
        .try_collect()
        .await
        .map_err(|_| ImapError::Protocol)?;
    let _ = session.logout().await;
    Ok(names)
}

/// What a search found: the mailbox's UIDVALIDITY, how many messages
/// matched, and the header data of the newest `limit` of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The mailbox's UIDVALIDITY.
    pub uidvalidity: u32,
    /// How many messages matched.
    pub total: usize,
    /// The fetched messages, UID descending.
    pub messages: Vec<FoundMessage>,
}

/// Header data of one message, as the server sent it. Untrusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundMessage {
    /// The UID.
    pub uid: u32,
    /// Internal date, Unix seconds.
    pub internal_date: i64,
    /// Display name of the first From address, raw (may be RFC 2047).
    pub from_name: Option<String>,
    /// `mailbox@host` of the first From address, `""` when absent.
    pub from_address: String,
    /// The Subject, raw (may be RFC 2047).
    pub subject: Option<String>,
    /// Flags, in server order.
    pub flags: Vec<String>,
}

/// Logs in, opens `mailbox` (decoded UTF-8, sent as modified UTF-7) read-only
/// with `EXAMINE`, runs `UID SEARCH criteria`, fetches flags, internal date
/// and envelope of the `limit` highest matching UIDs only, and logs out.
/// Never changes mailbox state: no `SELECT`, no `STORE`, no body fetch.
///
/// With `gmail` (the criteria hold `X-GM-RAW`), the capabilities are read
/// after login first, and the search fails with
/// [`ImapError::GmailUnsupported`] before `EXAMINE` unless the server
/// advertises `X-GM-EXT-1`.
pub fn search(
    account: &Account,
    network: &Network,
    mailbox: &str,
    criteria: &str,
    gmail: bool,
    limit: usize,
) -> Result<Found, ImapError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ImapError::Connection)?;
    runtime.block_on(async {
        tokio::time::timeout(
            TIMEOUT,
            search_async(account, network, mailbox, criteria, gmail, limit),
        )
        .await
        .map_err(|_| ImapError::Connection)?
    })
}

async fn search_async(
    account: &Account,
    network: &Network,
    mailbox: &str,
    criteria: &str,
    gmail: bool,
    limit: usize,
) -> Result<Found, ImapError> {
    let mut session = login(account, network).await?;
    let result = async {
        // Before EXAMINE: a server without Gmail's extensions opens no
        // mailbox for a Gmail query.
        if gmail
            && !capability_atoms(&mut session)
                .await?
                .iter()
                .any(|c| c.eq_ignore_ascii_case("X-GM-EXT-1"))
        {
            return Err(ImapError::GmailUnsupported);
        }
        search_in(&mut session, mailbox, criteria, limit).await
    }
    .await;
    let _ = session.logout().await;
    result
}

async fn search_in(
    session: &mut TlsSession,
    mailbox: &str,
    criteria: &str,
    limit: usize,
) -> Result<Found, ImapError> {
    let opened = examine(session, mailbox).await?;
    let uidvalidity = opened.uid_validity.ok_or(ImapError::Protocol)?;
    let mut uids: Vec<u32> = session
        .uid_search(criteria)
        .await
        .map_err(|_| ImapError::Protocol)?
        .into_iter()
        .collect();
    uids.sort_unstable_by(|a, b| b.cmp(a));
    let total = uids.len();
    uids.truncate(limit);
    if uids.is_empty() {
        return Ok(Found {
            uidvalidity,
            total,
            messages: Vec::new(),
        });
    }
    let set: Vec<String> = uids.iter().map(u32::to_string).collect();
    let fetches: Vec<Fetch> = session
        .uid_fetch(set.join(","), "(UID FLAGS INTERNALDATE ENVELOPE)")
        .await
        .map_err(|_| ImapError::Protocol)?
        .try_collect()
        .await
        .map_err(|_| ImapError::Protocol)?;
    let mut messages = Vec::with_capacity(uids.len());
    for uid in uids {
        // A message expunged meanwhile has no fetch; it is left out. Any
        // unsolicited fetch for another UID is ignored.
        if let Some(fetch) = fetches.iter().find(|f| f.uid == Some(uid)) {
            messages.push(found_message(uid, fetch)?);
        }
    }
    Ok(Found {
        uidvalidity,
        total,
        messages,
    })
}

/// Logs in, opens `mailbox` (decoded UTF-8, sent as modified UTF-7)
/// read-only with `EXAMINE`, checks its UIDVALIDITY against `uidvalidity`,
/// fetches the whole raw message `uid` with `BODY.PEEK[]` and logs out.
/// Never changes mailbox state: no `SELECT`, no `STORE`, no non-peek fetch.
pub fn read(
    account: &Account,
    network: &Network,
    mailbox: &str,
    uidvalidity: u32,
    uid: u32,
) -> Result<Vec<u8>, ImapError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ImapError::Connection)?;
    runtime.block_on(async {
        tokio::time::timeout(
            TIMEOUT,
            read_async(account, network, mailbox, uidvalidity, uid),
        )
        .await
        .map_err(|_| ImapError::Connection)?
    })
}

async fn read_async(
    account: &Account,
    network: &Network,
    mailbox: &str,
    uidvalidity: u32,
    uid: u32,
) -> Result<Vec<u8>, ImapError> {
    let mut session = login(account, network).await?;
    let result = read_in(&mut session, mailbox, uidvalidity, uid).await;
    let _ = session.logout().await;
    result
}

async fn read_in(
    session: &mut TlsSession,
    mailbox: &str,
    uidvalidity: u32,
    uid: u32,
) -> Result<Vec<u8>, ImapError> {
    let opened = examine(session, mailbox).await?;
    // Before any fetch: a renumbered mailbox must never yield another
    // message's content.
    if opened.uid_validity.ok_or(ImapError::Protocol)? != uidvalidity {
        return Err(ImapError::StaleReference);
    }
    let fetches: Vec<Fetch> = session
        .uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")
        .await
        .map_err(|_| ImapError::Protocol)?
        .try_collect()
        .await
        .map_err(|_| ImapError::Protocol)?;
    // Any unsolicited fetch for another UID is ignored.
    let fetch = fetches
        .iter()
        .find(|f| f.uid == Some(uid))
        .ok_or(ImapError::MessageNotFound)?;
    fetch.body().map(<[u8]>::to_vec).ok_or(ImapError::Protocol)
}

/// Logs in, opens `mailbox` (decoded UTF-8, sent as modified UTF-7)
/// read-write with `SELECT`, checks its UIDVALIDITY against `uidvalidity`,
/// confirms message `uid` exists with `UID FETCH <uid> (UID FLAGS)`, sets or
/// clears `flag` with `UID STORE`, and logs out. Returns the message's flags
/// after the change, in server order. Only the canonical `\Seen` or
/// `\Flagged` is sent. Never sends `CLOSE` (which would expunge messages
/// marked `\Deleted`), `EXPUNGE` or any body fetch.
pub fn flag(
    account: &Account,
    network: &Network,
    mailbox: &str,
    uidvalidity: u32,
    uid: u32,
    action: FlagAction,
    flag: MailFlag,
) -> Result<Vec<String>, ImapError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ImapError::Connection)?;
    runtime.block_on(async {
        tokio::time::timeout(TIMEOUT, async {
            let mut session = login(account, network).await?;
            let result = flag_in(&mut session, mailbox, uidvalidity, uid, action, flag).await;
            // LOGOUT only: never CLOSE, which silently expunges.
            let _ = session.logout().await;
            result
        })
        .await
        .map_err(|_| ImapError::Connection)?
    })
}

async fn flag_in(
    session: &mut TlsSession,
    mailbox: &str,
    uidvalidity: u32,
    uid: u32,
    action: FlagAction,
    flag: MailFlag,
) -> Result<Vec<String>, ImapError> {
    let opened = session
        .select(crate::utf7::encode(mailbox))
        .await
        .map_err(|e| match e {
            async_imap::error::Error::No(_) => ImapError::MailboxNotFound,
            _ => ImapError::Protocol,
        })?;
    // Before any fetch or store: a renumbered mailbox must never have the
    // wrong message changed.
    if opened.uid_validity.ok_or(ImapError::Protocol)? != uidvalidity {
        return Err(ImapError::StaleReference);
    }
    // A missing UID must not be reported as a successful change.
    if flags_of(session, uid).await?.is_none() {
        return Err(ImapError::MessageNotFound);
    }
    let stored: Vec<Fetch> = session
        .uid_store(uid.to_string(), store_item(action, flag))
        .await
        .map_err(|_| ImapError::Protocol)?
        .try_collect()
        .await
        .map_err(|_| ImapError::Protocol)?;
    if let Some(fetch) = stored.iter().find(|f| f.uid == Some(uid)) {
        return Ok(fetch.flags().map(|f| flag_name(&f)).collect());
    }
    // The server sent no FETCH for the message (for example because nothing
    // changed): ask for its flags.
    flags_of(session, uid)
        .await?
        .ok_or(ImapError::MessageNotFound)
}

/// Logs in, reads the capabilities, and moves message `uid` of `mailbox` to
/// `target` (both decoded UTF-8, sent as modified UTF-7). Returns the
/// target's UIDVALIDITY and the message's new UID when the server reports
/// them in a `COPYUID` response code, else `None`.
///
/// Refuses with [`ImapError::MoveUnsupported`] before any `SELECT` if the
/// server advertises neither `MOVE` nor `UIDPLUS`. Otherwise `SELECT`s the
/// source, checks its UIDVALIDITY against `uidvalidity`, confirms the
/// message exists with `UID FETCH <uid> (UID FLAGS)`, and then sends
/// `UID MOVE`, or, without `MOVE`, `UID COPY`, `UID STORE <uid>
/// +FLAGS.SILENT (\Deleted)` and `UID EXPUNGE <uid>`, which removes that
/// one message only. Never sends `CLOSE`, plain `EXPUNGE` or any body
/// fetch.
pub fn move_message(
    account: &Account,
    network: &Network,
    mailbox: &str,
    uidvalidity: u32,
    uid: u32,
    target: &str,
) -> Result<Option<(u32, u32)>, ImapError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ImapError::Connection)?;
    runtime.block_on(async {
        tokio::time::timeout(TIMEOUT, async {
            let mut session = login(account, network).await?;
            let result = move_in(&mut session, mailbox, uidvalidity, uid, target).await;
            // LOGOUT only: never CLOSE, which silently expunges.
            let _ = session.logout().await;
            result
        })
        .await
        .map_err(|_| ImapError::Connection)?
    })
}

async fn move_in(
    session: &mut TlsSession,
    mailbox: &str,
    uidvalidity: u32,
    uid: u32,
    target: &str,
) -> Result<Option<(u32, u32)>, ImapError> {
    let capabilities = capability_atoms(session).await?;
    let has = |name: &str| capabilities.iter().any(|c| c.eq_ignore_ascii_case(name));
    let (can_move, uidplus) = (has("MOVE"), has("UIDPLUS"));
    // Before any change: without either, the only way would be a plain
    // EXPUNGE, which also removes messages the human marked \Deleted.
    if !can_move && !uidplus {
        return Err(ImapError::MoveUnsupported);
    }
    let opened = session
        .select(crate::utf7::encode(mailbox))
        .await
        .map_err(|e| match e {
            async_imap::error::Error::No(_) => ImapError::MailboxNotFound,
            _ => ImapError::Protocol,
        })?;
    // Before any fetch, move or copy: a renumbered mailbox must never have
    // the wrong message moved.
    if opened.uid_validity.ok_or(ImapError::Protocol)? != uidvalidity {
        return Err(ImapError::StaleReference);
    }
    // `UID MOVE` of a missing UID answers OK: it must not be reported as
    // moved.
    if flags_of(session, uid).await?.is_none() {
        return Err(ImapError::MessageNotFound);
    }
    let target = quoted(&crate::utf7::encode(target));
    if can_move {
        return copy_or_move(session, &format!("UID MOVE {uid} {target}"), uid).await;
    }
    let copied = copy_or_move(session, &format!("UID COPY {uid} {target}"), uid).await?;
    let _: Vec<Fetch> = session
        .uid_store(uid.to_string(), "+FLAGS.SILENT (\\Deleted)")
        .await
        .map_err(|_| ImapError::Protocol)?
        .try_collect()
        .await
        .map_err(|_| ImapError::Protocol)?;
    // UIDPLUS: expunge this one UID only, never other \Deleted messages.
    session
        .run_command_and_check_ok(format!("UID EXPUNGE {uid}"))
        .await
        .map_err(|_| ImapError::Protocol)?;
    Ok(copied)
}

/// The capability atoms the server advertises. Asked after login, since
/// servers such as Gmail advertise more once authenticated.
async fn capability_atoms(session: &mut TlsSession) -> Result<Vec<String>, ImapError> {
    let capabilities = session
        .capabilities()
        .await
        .map_err(|_| ImapError::Protocol)?;
    Ok(capabilities
        .iter()
        .filter_map(|c| match c {
            async_imap::types::Capability::Atom(a) => Some(a.to_string()),
            _ => None,
        })
        .collect())
}

/// Runs a `UID MOVE` or `UID COPY` of `uid` and reads its responses
/// itself, since async-imap drops response codes. Returns the target's
/// UIDVALIDITY and the new UID from a `COPYUID` code, untagged or tagged.
/// A `NO` is [`ImapError::TargetNotFound`].
async fn copy_or_move(
    session: &mut TlsSession,
    command: &str,
    uid: u32,
) -> Result<Option<(u32, u32)>, ImapError> {
    let id = session
        .run_command(command)
        .await
        .map_err(|_| ImapError::Protocol)?;
    let mut reported = None;
    loop {
        let response = session
            .read_response()
            .await
            .map_err(|_| ImapError::Protocol)?
            .ok_or(ImapError::Protocol)?;
        match response.parsed() {
            Response::Data {
                status: Status::Ok,
                outcome,
            } if reported.is_none() => {
                reported = outcome.code.as_ref().and_then(|c| copy_uid(c, uid));
            }
            Response::Done {
                tag,
                status,
                outcome,
            } if *tag == id => {
                return match status {
                    Status::Ok => {
                        Ok(reported
                            .or_else(|| outcome.code.as_ref().and_then(|c| copy_uid(c, uid))))
                    }
                    Status::No => Err(ImapError::TargetNotFound),
                    _ => Err(ImapError::Protocol),
                };
            }
            // EXPUNGE, EXISTS and other untagged data are not needed.
            _ => {}
        }
    }
}

/// The target UIDVALIDITY and new UID from a `COPYUID` code, if it reports
/// exactly the one message `uid`.
fn copy_uid(code: &ResponseCode<'_>, uid: u32) -> Option<(u32, u32)> {
    let ResponseCode::CopyUid(uidvalidity, source, destination) = code else {
        return None;
    };
    (single_uid(source)? == uid).then_some(())?;
    Some((*uidvalidity, single_uid(destination)?))
}

/// The UID of a set holding exactly one, without expanding ranges.
fn single_uid(set: &[UidSetMember]) -> Option<u32> {
    match set {
        [UidSetMember::Uid(uid)] => Some(*uid),
        [UidSetMember::UidRange(range)] if range.start() == range.end() => Some(*range.start()),
        _ => None,
    }
}

/// `text` as an IMAP quoted string, `"` and `\` escaped. `text` holds no
/// CR, LF or NUL (checked by the core).
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// The original message of a reply draft: its mailbox (decoded UTF-8),
/// the reference's UIDVALIDITY and its UID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplyTo<'a> {
    /// The mailbox, decoded UTF-8.
    pub mailbox: &'a str,
    /// The reference's UIDVALIDITY.
    pub uidvalidity: u32,
    /// The UID.
    pub uid: u32,
}

/// Logs in and appends a draft, in one session:
///
/// 1. With `reply_to`, opens its mailbox read-only with `EXAMINE`, checks
///    its UIDVALIDITY, and fetches the original's header section with
///    `UID FETCH <uid> (UID BODY.PEEK[HEADER])` only, so it is not marked
///    `\Seen`.
/// 2. Calls `build` with those headers (`None` without `reply_to`) for the
///    message to append.
/// 3. Sends `LIST "" "*"`; the Drafts mailbox is the first entry, in server
///    order, whose attributes include `\Drafts`. Its raw name goes into
///    `drafts` as soon as it is found. Without one, fails with
///    [`ImapError::DraftsNotFound`] before any `APPEND`.
/// 4. Sends `APPEND <raw name> (\Draft) {n}` and the message, the raw name
///    the server listed sent back unchanged as a quoted string.
/// 5. Logs out.
///
/// Returns the Drafts mailbox's UIDVALIDITY and the new UID when the server
/// reports them in an `APPENDUID` response code (RFC 4315), else `None`.
/// Never sends `SELECT`, `STORE`, `COPY`, `MOVE`, `EXPUNGE` or `CLOSE`.
pub fn draft(
    account: &Account,
    network: &Network,
    reply_to: Option<ReplyTo<'_>>,
    drafts: &mut Option<String>,
    build: impl FnOnce(Option<&[u8]>) -> String,
) -> Result<Option<(u32, u32)>, ImapError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ImapError::Connection)?;
    runtime.block_on(async {
        tokio::time::timeout(TIMEOUT, async {
            let mut session = login(account, network).await?;
            let result = draft_in(&mut session, reply_to, drafts, build).await;
            let _ = session.logout().await;
            result
        })
        .await
        .map_err(|_| ImapError::Connection)?
    })
}

async fn draft_in(
    session: &mut TlsSession,
    reply_to: Option<ReplyTo<'_>>,
    drafts: &mut Option<String>,
    build: impl FnOnce(Option<&[u8]>) -> String,
) -> Result<Option<(u32, u32)>, ImapError> {
    let headers = match reply_to {
        Some(original) => Some(original_headers(session, original).await?),
        None => None,
    };
    let message = build(headers.as_deref());
    let raw = find_drafts(session).await?;
    *drafts = Some(raw.clone());
    append(session, &raw, &message).await
}

/// Logs in, sends `LIST "" "*"` and logs out. Returns the raw name of the
/// first mailbox, in server order, whose attributes include the `\Drafts`
/// special-use attribute (RFC 6154), or [`ImapError::DraftsNotFound`].
/// `draft` finds its target the same way.
pub fn drafts_mailbox(account: &Account, network: &Network) -> Result<String, ImapError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ImapError::Connection)?;
    runtime.block_on(async {
        tokio::time::timeout(TIMEOUT, async {
            let mut session = login(account, network).await?;
            let result = find_drafts(&mut session).await;
            let _ = session.logout().await;
            result
        })
        .await
        .map_err(|_| ImapError::Connection)?
    })
}

/// The raw name of the first listed mailbox marked `\Drafts`. The name
/// came from the server, so one that would not fit a quoted string is
/// [`ImapError::Protocol`].
async fn find_drafts(session: &mut TlsSession) -> Result<String, ImapError> {
    let listed: Vec<(String, bool)> = session
        .list(Some(""), Some("*"))
        .await
        .map_err(|_| ImapError::Protocol)?
        .map_ok(|name| {
            let is_drafts = name.attributes().iter().any(|a| match a {
                NameAttribute::Drafts => true,
                NameAttribute::Extension(e) => e.eq_ignore_ascii_case("\\Drafts"),
                _ => false,
            });
            (name.name().to_string(), is_drafts)
        })
        .try_collect()
        .await
        .map_err(|_| ImapError::Protocol)?;
    let raw = listed
        .into_iter()
        .find_map(|(name, is_drafts)| is_drafts.then_some(name))
        .ok_or(ImapError::DraftsNotFound)?;
    // The name came from the server: it must still fit a quoted string.
    if raw.is_empty() || raw.contains(['\r', '\n', '\0']) {
        return Err(ImapError::Protocol);
    }
    Ok(raw)
}

/// The header section of the reply's original message, fetched with a
/// `.PEEK` fetch after `EXAMINE` and the UIDVALIDITY check.
async fn original_headers(
    session: &mut TlsSession,
    original: ReplyTo<'_>,
) -> Result<Vec<u8>, ImapError> {
    let opened = examine(session, original.mailbox).await?;
    // Before any fetch: a renumbered mailbox must never yield another
    // message's headers.
    if opened.uid_validity.ok_or(ImapError::Protocol)? != original.uidvalidity {
        return Err(ImapError::StaleReference);
    }
    let uid = original.uid;
    let fetches: Vec<Fetch> = session
        .uid_fetch(uid.to_string(), "(UID BODY.PEEK[HEADER])")
        .await
        .map_err(|_| ImapError::Protocol)?
        .try_collect()
        .await
        .map_err(|_| ImapError::Protocol)?;
    // Any unsolicited fetch for another UID is ignored.
    let fetch = fetches
        .iter()
        .find(|f| f.uid == Some(uid))
        .ok_or(ImapError::MessageNotFound)?;
    fetch
        .header()
        .or_else(|| fetch.body())
        .map(<[u8]>::to_vec)
        .ok_or(ImapError::Protocol)
}

/// Sends `APPEND <raw> (\Draft) {n}`, waits for the continuation, sends
/// the message, and reads the responses itself, since async-imap drops
/// response codes. Returns the UIDVALIDITY and UID of an `APPENDUID` code
/// naming exactly one message. A refusal is [`ImapError::Protocol`].
async fn append(
    session: &mut TlsSession,
    raw: &str,
    message: &str,
) -> Result<Option<(u32, u32)>, ImapError> {
    let id = session
        .run_command(format!(
            "APPEND {} (\\Draft) {{{}}}",
            quoted(raw),
            message.len()
        ))
        .await
        .map_err(|_| ImapError::Protocol)?;
    loop {
        let response = session
            .read_response()
            .await
            .map_err(|_| ImapError::Protocol)?
            .ok_or(ImapError::Protocol)?;
        match response.parsed() {
            Response::Continue { .. } => break,
            Response::Done { tag, .. } if *tag == id => return Err(ImapError::Protocol),
            _ => {}
        }
    }
    // The literal, followed by the CRLF that ends the command.
    session
        .run_command_untagged(message)
        .await
        .map_err(|_| ImapError::Protocol)?;
    loop {
        let response = session
            .read_response()
            .await
            .map_err(|_| ImapError::Protocol)?
            .ok_or(ImapError::Protocol)?;
        if let Response::Done {
            tag,
            status,
            outcome,
        } = response.parsed()
            && *tag == id
        {
            return match status {
                Status::Ok => Ok(outcome.code.as_ref().and_then(|code| match code {
                    ResponseCode::AppendUid(uidvalidity, set) => {
                        single_uid(set).map(|uid| (*uidvalidity, uid))
                    }
                    _ => None,
                })),
                _ => Err(ImapError::Protocol),
            };
        }
    }
}

/// The flags of message `uid` in the open mailbox, `None` if it has none.
async fn flags_of(session: &mut TlsSession, uid: u32) -> Result<Option<Vec<String>>, ImapError> {
    let fetches: Vec<Fetch> = session
        .uid_fetch(uid.to_string(), "(UID FLAGS)")
        .await
        .map_err(|_| ImapError::Protocol)?
        .try_collect()
        .await
        .map_err(|_| ImapError::Protocol)?;
    // Any unsolicited fetch for another UID is ignored.
    Ok(fetches
        .iter()
        .find(|f| f.uid == Some(uid))
        .map(|f| f.flags().map(|flag| flag_name(&flag)).collect()))
}

/// Opens `mailbox` read-only.
async fn examine(
    session: &mut TlsSession,
    mailbox: &str,
) -> Result<async_imap::types::Mailbox, ImapError> {
    session
        .examine(crate::utf7::encode(mailbox))
        .await
        .map_err(|e| match e {
            async_imap::error::Error::No(_) => ImapError::MailboxNotFound,
            _ => ImapError::Protocol,
        })
}

fn found_message(uid: u32, fetch: &Fetch) -> Result<FoundMessage, ImapError> {
    let internal_date = fetch
        .internal_date()
        .ok_or(ImapError::Protocol)?
        .timestamp();
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    let envelope = fetch.envelope();
    let from = envelope
        .and_then(|e| e.from.as_ref())
        .and_then(|list| list.iter().find(|a| a.mailbox.is_some()));
    let from_address = from.map_or_else(String::new, |a| {
        let mailbox = a.mailbox.as_deref().map(text).unwrap_or_default();
        match a.host.as_deref() {
            Some(host) => format!("{mailbox}@{}", text(host)),
            None => mailbox,
        }
    });
    Ok(FoundMessage {
        uid,
        internal_date,
        from_name: from.and_then(|a| a.name.as_deref()).map(text),
        from_address,
        subject: envelope.and_then(|e| e.subject.as_deref()).map(text),
        flags: fetch.flags().map(|f| flag_name(&f)).collect(),
    })
}

fn flag_name(flag: &Flag<'_>) -> String {
    match flag {
        Flag::Seen => "\\Seen".to_string(),
        Flag::Answered => "\\Answered".to_string(),
        Flag::Flagged => "\\Flagged".to_string(),
        Flag::Deleted => "\\Deleted".to_string(),
        Flag::Draft => "\\Draft".to_string(),
        Flag::Recent => "\\Recent".to_string(),
        Flag::MayCreate => "\\*".to_string(),
        Flag::Custom(name) => name.to_string(),
    }
}

fn handshake_error(error: std::io::Error) -> ImapError {
    let certificate = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
        .is_some_and(|e| matches!(e, rustls::Error::InvalidCertificate(_)));
    if certificate {
        ImapError::Certificate
    } else {
        ImapError::TlsRequired
    }
}

async fn expect_greeting<T>(client: &mut Client<T>) -> Result<(), ImapError>
where
    T: AsyncRead + AsyncWrite + Unpin + std::fmt::Debug + Send,
{
    let greeting = client
        .read_response()
        .await
        .map_err(|_| ImapError::Protocol)?
        .ok_or(ImapError::Protocol)?;
    match greeting.parsed() {
        Response::Data {
            status: Status::Ok, ..
        } => Ok(()),
        _ => Err(ImapError::Protocol),
    }
}

async fn offers_starttls<T>(client: &mut Client<T>) -> Result<bool, ImapError>
where
    T: AsyncRead + AsyncWrite + Unpin + std::fmt::Debug + Send,
{
    let (sender, receiver) = async_channel::bounded(64);
    client
        .run_command_and_check_ok("CAPABILITY", Some(sender))
        .await
        .map_err(|_| ImapError::Protocol)?;
    let mut starttls = false;
    while let Ok(response) = receiver.try_recv() {
        if let UnsolicitedResponse::Other(data) = response
            && let Response::Capabilities(caps) = data.parsed()
        {
            starttls |= caps
                .iter()
                .any(|c| matches!(c, Capability::Atom(a) if a.eq_ignore_ascii_case("STARTTLS")));
        }
    }
    Ok(starttls)
}
