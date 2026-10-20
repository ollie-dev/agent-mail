//! The core: the single entry point every adapter calls.
//!
//! Given the presented Agent Token and a command, it opens the Mandate,
//! enforces the Policy (deny by default) and performs the mail operation.

use serde::Serialize;

use crate::imap::{self, ImapError, TlsTrust};
use crate::mandate::{Mandate, Operation};
use crate::seal::{self, AgentToken};
use crate::store::MandateStore;
use crate::utf7;

/// What the core runs against: the Mandate store and the TLS trust.
pub struct Context<'a> {
    store: &'a dyn MandateStore,
    trust: TlsTrust,
}

impl<'a> Context<'a> {
    /// A context using `store` and the operating system's certificate verifier.
    pub fn new(store: &'a dyn MandateStore) -> Self {
        Self {
            store,
            trust: TlsTrust::Platform,
        }
    }

    /// A test context trusting only `roots`, such as the fake server's CA.
    #[cfg(any(test, feature = "test-support"))]
    pub fn trusting_only(
        store: &'a dyn MandateStore,
        roots: Vec<rustls::pki_types::CertificateDer<'static>>,
    ) -> Self {
        Self {
            store,
            trust: TlsTrust::OnlyRoots(roots),
        }
    }
}

/// A command an Agent can run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// List the mailboxes the Policy grants.
    List,
}

/// Result of `list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MailboxList {
    /// Login name of the Account.
    pub account: String,
    /// Granted mailboxes, in server order.
    pub mailboxes: Vec<MailboxEntry>,
}

/// One mailbox in a [`MailboxList`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MailboxEntry {
    /// Decoded UTF-8 name.
    pub name: String,
}

/// A command's successful result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Result of `list`.
    Mailboxes(MailboxList),
}

/// A structured error. Never carries a token or a Credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    /// No Agent Token was presented.
    MissingToken,
    /// The token is malformed, unknown, or its record does not open.
    InvalidToken,
    /// The Policy has no Grant for the operation on the Account.
    Denied {
        /// The operation that was refused.
        operation: Operation,
        /// Login name of the Account.
        account: String,
    },
    /// The server offered no TLS.
    TlsRequired {
        /// Login name of the Account.
        account: String,
    },
    /// The server's TLS certificate was not trusted.
    TlsCertificate {
        /// Login name of the Account.
        account: String,
    },
    /// The server could not be reached.
    Connection {
        /// Login name of the Account.
        account: String,
    },
    /// The server refused the login.
    LoginFailed {
        /// Login name of the Account.
        account: String,
    },
    /// The server answered unexpectedly.
    Protocol {
        /// Login name of the Account.
        account: String,
    },
    /// The Mandate store could not be read.
    StoreUnavailable,
}

/// The JSON body of an error, in field order `code`, details, `message`.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    /// Stable machine-readable code.
    pub code: &'static str,
    /// The operation involved, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<&'static str>,
    /// Login name of the Account involved, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// Human-readable explanation.
    pub message: String,
}

impl CoreError {
    /// The stable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingToken => "missing_token",
            Self::InvalidToken => "invalid_token",
            Self::Denied { .. } => "denied",
            Self::TlsRequired { .. } => "tls_required",
            Self::TlsCertificate { .. } => "tls_certificate",
            Self::Connection { .. } => "connection_failed",
            Self::LoginFailed { .. } => "login_failed",
            Self::Protocol { .. } => "server_error",
            Self::StoreUnavailable => "store_unavailable",
        }
    }

    /// The error as a serialisable body.
    pub fn body(&self) -> ErrorBody {
        let (operation, account, message) = match self {
            Self::MissingToken => (
                None,
                None,
                "no agent token presented; set AGENT_MAIL_TOKEN".to_string(),
            ),
            Self::InvalidToken => (None, None, "the agent token is not valid".to_string()),
            Self::Denied { operation, account } => (
                Some(operation.as_str()),
                Some(account.clone()),
                format!(
                    "no grant allows `{}` on account {account}; ask the human to grant it",
                    operation.as_str()
                ),
            ),
            Self::TlsRequired { account } => (
                None,
                Some(account.clone()),
                "the server did not offer TLS; refusing to send the credential".to_string(),
            ),
            Self::TlsCertificate { account } => (
                None,
                Some(account.clone()),
                "the server's TLS certificate is not trusted".to_string(),
            ),
            Self::Connection { account } => (
                None,
                Some(account.clone()),
                "could not reach the mail server".to_string(),
            ),
            Self::LoginFailed { account } => (
                None,
                Some(account.clone()),
                "the mail server refused the login".to_string(),
            ),
            Self::Protocol { account } => (
                None,
                Some(account.clone()),
                "the mail server answered unexpectedly".to_string(),
            ),
            Self::StoreUnavailable => (
                None,
                None,
                "the credential store could not be read".to_string(),
            ),
        };
        ErrorBody {
            code: self.code(),
            operation,
            account,
            message,
        }
    }
}

/// Runs `command` for the Agent presenting `token`.
pub fn execute(
    token: Option<&str>,
    command: &Command,
    context: &Context<'_>,
) -> Result<Outcome, CoreError> {
    let mandate = open_mandate(token, context)?;
    match command {
        Command::List => list(&mandate, context).map(Outcome::Mailboxes),
    }
}

fn open_mandate(token: Option<&str>, context: &Context<'_>) -> Result<Mandate, CoreError> {
    let token = match token {
        None | Some("") => return Err(CoreError::MissingToken),
        Some(text) => AgentToken::parse(text).map_err(|_| CoreError::InvalidToken)?,
    };
    let record = context
        .store
        .get(&token.store_key())
        .map_err(|_| CoreError::StoreUnavailable)?
        .ok_or(CoreError::InvalidToken)?;
    seal::open(&token, &record).map_err(|_| CoreError::InvalidToken)
}

fn list(mandate: &Mandate, context: &Context<'_>) -> Result<MailboxList, CoreError> {
    let account = &mandate.account;
    let login = account.login.clone();
    // Deny by default, before any connection is made.
    if mandate
        .policy
        .grants_for(Operation::List, &login)
        .next()
        .is_none()
    {
        return Err(CoreError::Denied {
            operation: Operation::List,
            account: login,
        });
    }
    let raw = imap::list_mailboxes(account, &context.trust).map_err(|e| {
        let account = login.clone();
        match e {
            ImapError::TlsRequired => CoreError::TlsRequired { account },
            ImapError::Certificate => CoreError::TlsCertificate { account },
            ImapError::Connection => CoreError::Connection { account },
            ImapError::Login => CoreError::LoginFailed { account },
            ImapError::Protocol => CoreError::Protocol { account },
        }
    })?;
    let mailboxes = raw
        .into_iter()
        // Malformed modified UTF-7 falls back to the raw name, losslessly.
        .map(|name| utf7::decode(&name).unwrap_or(name))
        .filter(|name| mandate.policy.allows_mailbox(Operation::List, &login, name))
        .map(|name| MailboxEntry { name })
        .collect();
    Ok(MailboxList {
        account: login,
        mailboxes,
    })
}
