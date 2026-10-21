//! The core: the single entry point every adapter calls.
//!
//! Given the presented Agent Token and a command, it opens the Mandate,
//! enforces the Policy (deny by default) and performs the mail operation.

use std::collections::HashMap;

use serde::Serialize;

use crate::imap::{self, ImapError, Network};
use crate::mandate::{Grant, Mandate, Operation};
use crate::seal::{self, AgentToken, ListedAccount};
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
    /// List every stored Mandate's metadata. Reads no token.
    MandateList,
    /// Delete the Mandate with short ID `id`. Reads no token and asks for no
    /// Credential (ADR-0003).
    MandateDelete {
        /// The short ID, matched exactly. Untrusted input.
        id: String,
    },
}

/// Result of `mandate list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MandateListing {
    /// The readable Mandates, by `created` ascending, then by ID.
    pub mandates: Vec<MandateSummary>,
    /// Stored records whose metadata could not be read, so not listed.
    pub unreadable: usize,
}

/// One Mandate in a [`MandateListing`]. Never holds a Credential, token,
/// token hash or store key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MandateSummary {
    /// Short ID.
    pub id: String,
    /// Name.
    pub name: String,
    /// Creation time, RFC 3339 UTC.
    pub created: String,
    /// The covered Accounts.
    pub accounts: Vec<ListedAccount>,
    /// The Grants.
    pub grants: Vec<Grant>,
}

/// Result of `mandate delete`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeletedMandate {
    /// Short ID of the deleted Mandate.
    pub id: String,
    /// Its name.
    pub name: String,
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
    /// Result of `mandate list`.
    Mandates(MandateListing),
    /// Result of `mandate delete`.
    Deleted(DeletedMandate),
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
    /// No stored Mandate has this short ID.
    NotFound {
        /// The ID asked for.
        id: String,
    },
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
    /// The Mandate ID involved, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
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
            Self::NotFound { .. } => "not_found",
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
                id: None,
                message: format!(
                    "this agent token covers several accounts ({}) and choosing one is not supported yet; ask the human for a token whose Mandate covers a single account",
                    accounts.join(", ")
                ),
            };
        }
        if let Self::NotFound { id } = self {
            return ErrorBody {
                code: self.code(),
                operation: None,
                account: None,
                accounts: None,
                id: Some(id.clone()),
                message: format!(
                    "no Mandate has ID {id}; run `agent-mail mandate list` to see the IDs"
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
            Self::AccountRequired { .. } | Self::NotFound { .. } => {
                unreachable!("handled above")
            }
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
            id: None,
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
        Command::MandateList => mandate_list(context).map(Outcome::Mandates),
        Command::MandateDelete { id } => mandate_delete(id, context).map(Outcome::Deleted),
    }
}

/// Lists what the store holds from each record's clear metadata. No token
/// is read, so this shows only what the store claims; trust comes from
/// [`seal::open`] when a token is presented.
fn mandate_list(context: &Context<'_>) -> Result<MandateListing, CoreError> {
    let entries = context
        .store
        .entries()
        .map_err(|_| CoreError::StoreUnavailable)?;
    let mut listings = Vec::with_capacity(entries.len());
    let mut unreadable = 0;
    for (_, value) in &entries {
        match seal::listing(value) {
            Ok(listing) => listings.push(listing),
            Err(_) => unreadable += 1,
        }
    }
    listings.sort_by(|a, b| (a.created, &a.id).cmp(&(b.created, &b.id)));
    let mandates = listings
        .into_iter()
        .map(|l| MandateSummary {
            id: l.id,
            name: l.name,
            created: rfc3339_utc(l.created),
            accounts: l.accounts,
            grants: l.grants,
        })
        .collect();
    Ok(MandateListing {
        mandates,
        unreadable,
    })
}

/// Deletes the first record whose clear metadata carries ID `id` exactly.
fn mandate_delete(id: &str, context: &Context<'_>) -> Result<DeletedMandate, CoreError> {
    let entries = context
        .store
        .entries()
        .map_err(|_| CoreError::StoreUnavailable)?;
    let found = entries.iter().find_map(|(key, value)| {
        seal::listing(value)
            .ok()
            .filter(|l| l.id == id)
            .map(|l| (key, l.name))
    });
    let Some((key, name)) = found else {
        return Err(CoreError::NotFound { id: id.to_string() });
    };
    match context.store.delete(key) {
        Ok(true) => Ok(DeletedMandate {
            id: id.to_string(),
            name,
        }),
        // Removed by someone else since it was read.
        Ok(false) => Err(CoreError::NotFound { id: id.to_string() }),
        Err(_) => Err(CoreError::StoreUnavailable),
    }
}

/// Formats Unix seconds as RFC 3339 UTC, e.g. `2026-10-20T08:00:00Z`.
pub fn rfc3339_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
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
    fn rfc3339_formats_utc_seconds() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(1_792_483_200), "2026-10-20T08:00:00Z");
        assert_eq!(rfc3339_utc(1_792_575_015), "2026-10-21T09:30:15Z");
        assert_eq!(rfc3339_utc(951_782_400), "2000-02-29T00:00:00Z");
    }

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
            CoreError::NotFound {
                id: "ffffffff".to_string(),
            },
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
