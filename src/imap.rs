//! The IMAP connector. TLS is always required: implicit TLS, or a mandatory
//! STARTTLS upgrade. The Credential is sent only over an established TLS
//! session.

use std::sync::Arc;
use std::time::Duration;

use async_imap::imap_proto::{Capability, Response, Status};
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
    /// The server refused to open the mailbox (`EXAMINE` answered `NO`).
    MailboxNotFound,
    /// The mailbox's UIDVALIDITY differs from the reference's, so its UID
    /// may now name another message. Nothing was fetched.
    StaleReference,
    /// No message has the UID (the server returned no FETCH for it).
    MessageNotFound,
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
pub fn search(
    account: &Account,
    network: &Network,
    mailbox: &str,
    criteria: &str,
    limit: usize,
) -> Result<Found, ImapError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ImapError::Connection)?;
    runtime.block_on(async {
        tokio::time::timeout(
            TIMEOUT,
            search_async(account, network, mailbox, criteria, limit),
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
    limit: usize,
) -> Result<Found, ImapError> {
    let mut session = login(account, network).await?;
    let result = search_in(&mut session, mailbox, criteria, limit).await;
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
