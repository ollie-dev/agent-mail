//! The IMAP connector. TLS is always required: implicit TLS, or a mandatory
//! STARTTLS upgrade. The Credential is sent only over an established TLS
//! session.

use std::sync::Arc;
use std::time::Duration;

use async_imap::Client;
use async_imap::imap_proto::{Capability, Response, Status};
use async_imap::types::UnsolicitedResponse;
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
}

/// Lists every mailbox on the Account, raw (modified UTF-7) names in server order.
pub fn list_mailboxes(account: &Account, trust: &TlsTrust) -> Result<Vec<String>, ImapError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ImapError::Connection)?;
    runtime.block_on(async {
        tokio::time::timeout(TIMEOUT, list_async(account, trust))
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

async fn list_async(account: &Account, trust: &TlsTrust) -> Result<Vec<String>, ImapError> {
    let connector = tls_connector(trust)?;
    let server_name =
        ServerName::try_from(account.host.clone()).map_err(|_| ImapError::Connection)?;
    let tcp = TcpStream::connect((account.host.as_str(), account.port))
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
    let mut session = client
        .login(&account.login, account.credential.expose())
        .await
        .map_err(|_| ImapError::Login)?;
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
