//! The core: the single entry point every adapter calls.
//!
//! Given the presented Agent Token and a command, it opens the Mandate,
//! enforces the Policy (deny by default) and performs the mail operation.

use std::collections::HashMap;

use serde::Serialize;

use crate::imap::{self, ImapError, Network};
use crate::mandate::{Mandate, Operation};
use crate::seal::{self, AgentToken};
use crate::store::MandateStore;
use crate::terminal::{NoTerminal, Terminal};
use crate::utf7;

/// Environment variable holding the presented Agent Token.
pub const TOKEN_VAR: &str = "AGENT_MAIL_TOKEN";

/// What the core runs against: the Mandate store, the network and the
/// Terminal (used only by `mandate create`).
pub struct Context<'a> {
    pub(crate) store: &'a dyn MandateStore,
    pub(crate) network: Network,
    pub(crate) terminal: &'a dyn Terminal,
}

impl<'a> Context<'a> {
    /// A context using `store`, the operating system's certificate verifier
    /// and no terminal.
    pub fn new(store: &'a dyn MandateStore) -> Self {
        Self {
            store,
            network: Network::platform(),
            terminal: &NoTerminal,
        }
    }

    /// The same context, talking to the human through `terminal`.
    pub fn with_terminal(self, terminal: &'a dyn Terminal) -> Self {
        Self { terminal, ..self }
    }

    /// A test context trusting only `roots`, such as the fake server's CA.
    #[cfg(any(test, feature = "test-support"))]
    pub fn trusting_only(
        store: &'a dyn MandateStore,
        roots: Vec<rustls::pki_types::CertificateDer<'static>>,
    ) -> Self {
        Self {
            network: Network::trusting_only(roots),
            ..Self::new(store)
        }
    }

    /// The same context, connecting to `to_host:to_port` whenever an Account
    /// names `host:port`. Test builds only; TLS still verifies `host`.
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_route(self, host: &str, port: u16, to_host: &str, to_port: u16) -> Self {
        Self {
            network: self.network.with_route(host, port, to_host, to_port),
            ..self
        }
    }
}

/// A command an Agent can run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// List the mailboxes the Policy grants.
    List,
    /// Guided creation of a Mandate by the human (`mandate create`).
    MandateCreate,
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
    /// A Mandate was sealed and stored. Its Agent Token went only to the
    /// Terminal, so nothing is printed.
    MandateCreated,
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
    /// The Mandate store could not be read or written.
    StoreUnavailable,
    /// The Mandate covers several Accounts and the command names none.
    AccountRequired {
        /// Login names of the covered Accounts.
        accounts: Vec<String>,
    },
    /// `mandate create` was run without a terminal a human can use.
    InteractiveTerminalRequired,
    /// `mandate create` was run inside an agent sandbox or session.
    SandboxDetected,
    /// The human aborted `mandate create`; nothing was stored.
    Aborted,
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
    /// Login names of the Accounts to choose from, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accounts: Option<Vec<String>>,
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
            Self::AccountRequired { .. } => "account_required",
            Self::InteractiveTerminalRequired => "interactive_terminal_required",
            Self::SandboxDetected => "sandbox_detected",
            Self::Aborted => "aborted",
        }
    }

    /// The error as a serialisable body.
    pub fn body(&self) -> ErrorBody {
        if let Self::AccountRequired { accounts } = self {
            return ErrorBody {
                code: self.code(),
                operation: None,
                account: None,
                accounts: Some(accounts.clone()),
                message: format!(
                    "this agent token covers several accounts ({}) and choosing one is not supported yet; ask the human for a token whose Mandate covers a single account",
                    accounts.join(", ")
                ),
            };
        }
        let (operation, account, message) = match self {
            Self::MissingToken => (
                None,
                None,
                "no agent token presented; set AGENT_MAIL_TOKEN".to_string(),
            ),
            Self::InvalidToken => (None, None, "the agent token is not valid; check AGENT_MAIL_TOKEN or ask the human for a new token"
                    .to_string()),
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
                "the server did not offer TLS, so the credential was not sent; ask the human to fix the account's server or TLS settings"
                    .to_string(),
            ),
            Self::TlsCertificate { account } => (
                None,
                Some(account.clone()),
                "the server's TLS certificate is not trusted; ask the human to check the account's host name and the server certificate"
                    .to_string(),
            ),
            Self::Connection { account } => (
                None,
                Some(account.clone()),
                "could not reach the mail server; check the network and retry, or ask the human to check the account's host and port"
                    .to_string(),
            ),
            Self::LoginFailed { account } => (
                None,
                Some(account.clone()),
                "the mail server refused the login; ask the human to check the account's credential"
                    .to_string(),
            ),
            Self::Protocol { account } => (
                None,
                Some(account.clone()),
                "the mail server answered unexpectedly; retry later, or ask the human to check the account's server settings"
                    .to_string(),
            ),
            Self::StoreUnavailable => (
                None,
                None,
                "the credential store could not be read or written; retry, or ask the human to check the credential store"
                    .to_string(),
            ),
            Self::AccountRequired { .. } => unreachable!("handled above"),
            Self::InteractiveTerminalRequired => (
                None,
                None,
                "mandate create must be run by a human in a terminal, so nothing was stored; ask the human to run `agent-mail mandate create` in their own terminal"
                    .to_string(),
            ),
            Self::SandboxDetected => (
                None,
                None,
                "mandate create cannot run inside an agent sandbox or session and must be run by the human in their own terminal, so nothing was stored; ask the human to run `agent-mail mandate create` in their own terminal"
                    .to_string(),
            ),
            Self::Aborted => (
                None,
                None,
                "mandate create was aborted and nothing was stored; run `agent-mail mandate create` again to start over"
                    .to_string(),
            ),
        };
        ErrorBody {
            code: self.code(),
            operation,
            account,
            accounts: None,
            message,
        }
    }
}

/// Runs `command` with the process environment `env`. Agent commands read
/// the presented token from [`TOKEN_VAR`]; `mandate create` reads no token
/// and checks `env` only for agent sandbox indicators.
pub fn execute(
    env: &HashMap<String, String>,
    command: &Command,
    context: &Context<'_>,
) -> Result<Outcome, CoreError> {
    match command {
        Command::List => {
            let token = env.get(TOKEN_VAR).map(String::as_str);
            let mandate = open_mandate(token, context)?;
            list(&mandate, context).map(Outcome::Mailboxes)
        }
        Command::MandateCreate => {
            crate::create::run(env, context).map(|()| Outcome::MandateCreated)
        }
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
    // One Account per command until `--account` arrives (ticket 13); never
    // merge Accounts silently.
    let account = match mandate.accounts.as_slice() {
        [account] => account,
        accounts => {
            return Err(CoreError::AccountRequired {
                accounts: accounts.iter().map(|a| a.login.clone()).collect(),
            });
        }
    };
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
    let raw = imap::list_mailboxes(account, &context.network).map_err(|e| {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_error_message_names_a_next_step() {
        let account = "agent@example.test".to_string();
        let errors = [
            CoreError::MissingToken,
            CoreError::InvalidToken,
            CoreError::Denied {
                operation: Operation::List,
                account: account.clone(),
            },
            CoreError::TlsRequired {
                account: account.clone(),
            },
            CoreError::TlsCertificate {
                account: account.clone(),
            },
            CoreError::Connection {
                account: account.clone(),
            },
            CoreError::LoginFailed {
                account: account.clone(),
            },
            CoreError::Protocol {
                account: account.clone(),
            },
            CoreError::StoreUnavailable,
            CoreError::AccountRequired {
                accounts: vec![account.clone(), "other@example.test".to_string()],
            },
            CoreError::InteractiveTerminalRequired,
            CoreError::SandboxDetected,
            CoreError::Aborted,
        ];
        for error in errors {
            let message = error.body().message;
            let (_, next_step) = message
                .split_once("; ")
                .unwrap_or_else(|| panic!("{}: no next step in {message:?}", error.code()));
            assert!(
                ["set ", "check ", "ask ", "retry", "run "]
                    .iter()
                    .any(|verb| next_step.starts_with(verb)),
                "{}: next step is not an action: {message:?}",
                error.code()
            );
        }
    }
}
