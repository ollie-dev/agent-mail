//! The core: the single entry point every adapter calls.
//!
//! Given the presented Agent Token and a command, it opens the Mandate,
//! enforces the Policy (deny by default) and performs the mail operation.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::audit::{self, AuditLog, Decision};
use crate::draft::{self, DraftResult};
use crate::flag::{self, Flag, FlagAction, FlagResult};
use crate::imap::{self, ImapError, Network};
use crate::mandate::{Account, Grant, Mandate, Operation};
use crate::move_message::MoveResult;
use crate::read::{self, ReadResult};
use crate::seal::{self, AgentToken, ListedAccount};
use crate::search::{self, MessageRef, MessageSummary, Query, SearchResult, SearchText};
use crate::store::MandateStore;
use crate::terminal::{NoTerminal, Terminal};
use crate::utf7;

/// Environment variable holding the presented Agent Token.
pub const TOKEN_VAR: &str = "AGENT_MAIL_TOKEN";

/// What the core runs against: the Mandate store, the network, the
/// Terminal (used only by `mandate create`) and the audit log.
pub struct Context<'a> {
    pub(crate) store: &'a dyn MandateStore,
    pub(crate) network: Network,
    pub(crate) terminal: &'a dyn Terminal,
    pub(crate) audit: AuditLog,
}

impl<'a> Context<'a> {
    /// A context using `store`, the operating system's certificate verifier,
    /// no terminal and the audit log at the platform's per-user location.
    pub fn new(store: &'a dyn MandateStore) -> Self {
        Self {
            store,
            network: Network::platform(),
            terminal: &NoTerminal,
            audit: AuditLog::platform(),
        }
    }

    /// The same context, talking to the human through `terminal`.
    pub fn with_terminal(self, terminal: &'a dyn Terminal) -> Self {
        Self { terminal, ..self }
    }

    /// A test context trusting only `roots`, such as the fake server's CA,
    /// and writing its audit log to a fresh temporary file, never to the
    /// user's data directory.
    #[cfg(any(test, feature = "test-support"))]
    pub fn trusting_only(
        store: &'a dyn MandateStore,
        roots: Vec<rustls::pki_types::CertificateDer<'static>>,
    ) -> Self {
        Self {
            network: Network::trusting_only(roots),
            ..Self::new(store).with_audit_log(AuditLog::temporary())
        }
    }

    /// The same context, appending audit entries to `audit`. Test builds
    /// only.
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_audit_log(self, audit: AuditLog) -> Self {
        Self { audit, ..self }
    }

    /// The audit log this context appends to. Test builds only.
    #[cfg(any(test, feature = "test-support"))]
    pub fn audit_log(&self) -> &AuditLog {
        &self.audit
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
    /// Search one granted mailbox.
    Search(Query),
    /// Read one message by its reference, without marking it seen.
    Read {
        /// The message, as `search` returned it. Untrusted input; checked
        /// again here with [`read::is_valid_reference`].
        reference: MessageRef,
        /// Return the whole body, untruncated.
        full: bool,
    },
    /// Set or clear `\Seen` or `\Flagged` on one message by its reference.
    Flag {
        /// The message, as `search` returned it. Untrusted input; checked
        /// again here with [`read::is_valid_reference`].
        reference: MessageRef,
        /// Whether the flag is set or cleared.
        action: FlagAction,
        /// The flag name as the Agent gave it. Untrusted input; parsed here
        /// with [`Flag::parse`] and never sent to the server.
        flag: String,
    },
    /// Move one message by its reference to another mailbox of the same
    /// Account.
    Move {
        /// The message, as `search` returned it. Untrusted input; checked
        /// again here with [`read::is_valid_reference`].
        reference: MessageRef,
        /// The target mailbox, decoded UTF-8 as `list` prints it. Untrusted
        /// input; compared exactly against the Grants and sent only as a
        /// quoted modified UTF-7 string.
        target: SearchText,
    },
    /// Create a draft in the Account's `\Drafts` mailbox, optionally as a
    /// reply.
    Draft {
        /// The recipients, each a bare `local@domain`. Untrusted input;
        /// checked again here with [`draft::is_valid_address`].
        to: Vec<String>,
        /// The subject; required without `reply_to` and refused with it.
        /// Untrusted input; checked again with [`draft::is_valid_subject`].
        subject: Option<String>,
        /// The body. Untrusted input; checked again with
        /// [`draft::is_valid_body`].
        body: String,
        /// The original message of a reply, as `search` returned it.
        /// Untrusted input; checked again with [`read::is_valid_reference`].
        reply_to: Option<MessageRef>,
    },
    /// Guided creation of a Mandate by the human (`mandate create`).
    MandateCreate {
        /// A Mandate file whose choices pre-fill the flow (`--from`).
        /// Untrusted input; it never supplies a Credential.
        from: Option<PathBuf>,
    },
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
    /// Its name, as every deleted record's unauthenticated listing gave it.
    /// `None` when the records with this ID named different names, since a
    /// forged record may have supplied any of them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
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
    /// Result of `search`.
    Messages(SearchResult),
    /// Result of `read`.
    Message(ReadResult),
    /// Result of `flag`.
    Flagged(FlagResult),
    /// Result of `move`.
    Moved(MoveResult),
    /// Result of `draft`.
    Drafted(DraftResult),
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
        /// The mailbox, for operations on one mailbox.
        mailbox: Option<String>,
    },
    /// `move` named the reference's own mailbox as the target. Nothing was
    /// moved and no connection was made.
    SameMailbox {
        /// Login name of the Account.
        account: String,
        /// The mailbox, decoded UTF-8.
        mailbox: String,
    },
    /// The server advertises neither `MOVE` nor `UIDPLUS`, so the message
    /// cannot be moved without risking other messages. Nothing was moved.
    MoveUnsupported {
        /// Login name of the Account.
        account: String,
        /// The source mailbox, decoded UTF-8.
        mailbox: String,
    },
    /// A Gmail search was asked for, but the server does not advertise
    /// Gmail's extensions (`X-GM-EXT-1`). No mailbox was opened and nothing
    /// was searched. The query is never echoed.
    Unsupported {
        /// Login name of the Account.
        account: String,
        /// The mailbox asked for, decoded UTF-8.
        mailbox: String,
    },
    /// The reference's UIDVALIDITY no longer matches the mailbox, so its
    /// UID may name another message. Nothing was read or changed.
    StaleReference {
        /// The operation that was attempted (`read`, `flag` or `move`).
        operation: Operation,
        /// Login name of the Account.
        account: String,
        /// The mailbox, decoded UTF-8.
        mailbox: String,
    },
    /// The mailbox has no message with the reference's UID.
    MessageNotFound {
        /// The operation that was attempted (`read`, `flag` or `move`).
        operation: Operation,
        /// Login name of the Account.
        account: String,
        /// The mailbox, decoded UTF-8.
        mailbox: String,
    },
    /// A message reference is malformed or holds CR, LF or NUL. The CLI
    /// refuses such references while parsing; other adapters may reach this.
    InvalidReference {
        /// The operation that was attempted (`read`, `flag` or `move`).
        operation: Operation,
    },
    /// No mailbox of the Account is marked with the `\Drafts` special-use
    /// attribute. No draft was created.
    DraftsNotFound {
        /// Login name of the Account.
        account: String,
    },
    /// A `draft` value is not acceptable. The CLI refuses such values while
    /// parsing; other adapters may reach this. The value is never echoed or
    /// logged.
    InvalidDraft {
        /// The field: `to`, `subject` or `body`.
        field: &'static str,
    },
    /// `flag` named a flag other than `\Seen` or `\Flagged`. The rejected
    /// name is never echoed or logged.
    UnsupportedFlag,
    /// The server refused to open the mailbox.
    MailboxNotFound {
        /// Login name of the Account.
        account: String,
        /// The mailbox asked for, decoded UTF-8.
        mailbox: String,
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
    /// The `--from` Mandate file cannot be read or is malformed. Nothing was
    /// asked and nothing was stored.
    InvalidMandateFile {
        /// What is wrong, by key path or line. Never quotes a file value.
        problem: String,
    },
    /// No stored Mandate has this short ID.
    NotFound {
        /// The ID asked for.
        id: String,
    },
    /// `mandate delete` removed what it could, but the store holds records
    /// it cannot read, and one of them may be the Mandate, still in force.
    RevocationIncomplete {
        /// The ID asked for.
        id: String,
        /// Readable records with this ID that were deleted.
        deleted: usize,
        /// Stored records that could not be read.
        unreadable: usize,
    },
    /// The audit entry could not be appended, so the result was withheld.
    AuditUnavailable,
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
    /// The mailbox involved, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailbox: Option<String>,
    /// Login names of the Accounts to choose from, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accounts: Option<Vec<String>>,
    /// The Mandate ID involved, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Records deleted, if any count is reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted: Option<usize>,
    /// Stored records that could not be read, if reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unreadable: Option<usize>,
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
            Self::MailboxNotFound { .. } => "mailbox_not_found",
            Self::StaleReference { .. } => "stale_reference",
            Self::MessageNotFound { .. } => "message_not_found",
            Self::InvalidReference { .. } => "invalid_reference",
            Self::UnsupportedFlag => "unsupported_flag",
            Self::DraftsNotFound { .. } => "drafts_not_found",
            Self::InvalidDraft { .. } => "invalid_draft",
            Self::SameMailbox { .. } => "same_mailbox",
            Self::MoveUnsupported { .. } => "move_unsupported",
            Self::Unsupported { .. } => "unsupported",
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
            Self::RevocationIncomplete { .. } => "revocation_incomplete",
            Self::InvalidMandateFile { .. } => "invalid_mandate_file",
            Self::AuditUnavailable => "audit_unavailable",
        }
    }

    /// The error as a serialisable body.
    pub fn body(&self) -> ErrorBody {
        if let Self::Denied {
            operation,
            account,
            mailbox: Some(mailbox),
        } = self
        {
            return ErrorBody {
                code: self.code(),
                operation: Some(operation.as_str()),
                account: Some(account.clone()),
                mailbox: Some(mailbox.clone()),
                accounts: None,
                id: None,
                deleted: None,
                unreadable: None,
                message: format!(
                    "no grant allows `{}` on mailbox {mailbox} of account {account}; ask the human to grant it, or run `agent-mail list` to see the mailboxes this token can use",
                    operation.as_str()
                ),
            };
        }
        if let Self::MailboxNotFound { account, mailbox } = self {
            return ErrorBody {
                code: self.code(),
                operation: None,
                account: Some(account.clone()),
                mailbox: Some(mailbox.clone()),
                accounts: None,
                id: None,
                deleted: None,
                unreadable: None,
                message: format!(
                    "mailbox {mailbox} does not exist on account {account}; run `agent-mail list` to see the mailbox names"
                ),
            };
        }
        if let Self::StaleReference {
            operation,
            account,
            mailbox,
        }
        | Self::MessageNotFound {
            operation,
            account,
            mailbox,
        } = self
        {
            let nothing_done = match operation {
                Operation::Read => "the message was not read",
                Operation::Move => "nothing was moved",
                Operation::Draft => "no draft was created",
                _ => "no flag was changed",
            };
            let message = if matches!(self, Self::StaleReference { .. }) {
                format!(
                    "mailbox {mailbox} of account {account} was renumbered since this reference was made (its UIDVALIDITY changed), so {nothing_done}; run `agent-mail search` on mailbox {mailbox} again for a fresh reference"
                )
            } else {
                format!(
                    "mailbox {mailbox} of account {account} has no message with this reference's UID, perhaps because it was moved or deleted, so {nothing_done}; run `agent-mail search` on mailbox {mailbox} again for a current reference"
                )
            };
            return ErrorBody {
                code: self.code(),
                operation: Some(operation.as_str()),
                account: Some(account.clone()),
                mailbox: Some(mailbox.clone()),
                accounts: None,
                id: None,
                deleted: None,
                unreadable: None,
                message,
            };
        }
        if let Self::SameMailbox { account, mailbox } | Self::MoveUnsupported { account, mailbox } =
            self
        {
            let message = if matches!(self, Self::SameMailbox { .. }) {
                format!(
                    "the message is already in mailbox {mailbox} of account {account}, so nothing was moved; run `agent-mail list` and pass another mailbox to --to"
                )
            } else {
                format!(
                    "the mail server of account {account} supports neither MOVE nor UIDPLUS, so the message cannot be moved without risking other messages in mailbox {mailbox}, and nothing was moved; ask the human to move the message in their mail client"
                )
            };
            return ErrorBody {
                code: self.code(),
                operation: Some(Operation::Move.as_str()),
                account: Some(account.clone()),
                mailbox: Some(mailbox.clone()),
                accounts: None,
                id: None,
                deleted: None,
                unreadable: None,
                message,
            };
        }
        if let Self::Unsupported { account, mailbox } = self {
            return ErrorBody {
                code: self.code(),
                operation: Some(Operation::Search.as_str()),
                account: Some(account.clone()),
                mailbox: Some(mailbox.clone()),
                accounts: None,
                id: None,
                deleted: None,
                unreadable: None,
                message: format!(
                    "the mail server of account {account} does not support Gmail search (it does not advertise X-GM-EXT-1), so mailbox {mailbox} was not searched; run `agent-mail search` with the standard criteria (--from, --to, --subject, --text, --since, --before, --unseen, --flagged) instead of --gmail"
                ),
            };
        }
        if let Self::AccountRequired { accounts } = self {
            return ErrorBody {
                code: self.code(),
                operation: None,
                account: None,
                mailbox: None,
                accounts: Some(accounts.clone()),
                id: None,
                deleted: None,
                unreadable: None,
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
                mailbox: None,
                accounts: None,
                id: Some(id.clone()),
                deleted: None,
                unreadable: None,
                message: format!(
                    "no Mandate has ID {id}; run `agent-mail mandate list` to see the IDs"
                ),
            };
        }
        if let Self::RevocationIncomplete {
            id,
            deleted,
            unreadable,
        } = self
        {
            let records = match unreadable {
                1 => "1 stored record could not be read".to_string(),
                n => format!("{n} stored records could not be read"),
            };
            let done = if *deleted == 0 {
                format!("no readable Mandate has ID {id}, but {records} and may hold it")
            } else {
                format!(
                    "deleted the readable records with ID {id}, but {records} and may still hold this Mandate"
                )
            };
            return ErrorBody {
                code: self.code(),
                operation: None,
                account: None,
                mailbox: None,
                accounts: None,
                id: Some(id.clone()),
                deleted: Some(*deleted),
                unreadable: Some(*unreadable),
                message: format!(
                    "{done}, so its agent token may still work; check Keychain Access for `agent-mail` items that `agent-mail mandate list` does not show and delete them"
                ),
            };
        }
        if let Self::InvalidMandateFile { problem } = self {
            return ErrorBody {
                code: self.code(),
                operation: None,
                account: None,
                mailbox: None,
                accounts: None,
                id: None,
                deleted: None,
                unreadable: None,
                message: format!(
                    "the Mandate file cannot be used ({problem}), so nothing was asked or stored; check the file, or run `agent-mail mandate create` without --from"
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
            Self::Denied {
                operation, account, ..
            } => (
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
            Self::InvalidReference { operation } => (
                Some(operation.as_str()),
                None,
                "the message reference is malformed; run `agent-mail search` and pass a `ref` exactly as it prints it"
                    .to_string(),
            ),
            Self::DraftsNotFound { account } => (
                Some(Operation::Draft.as_str()),
                Some(account.clone()),
                format!(
                    "no mailbox of account {account} is marked as the Drafts mailbox (the \\Drafts special-use attribute), so no draft was created; ask the human to create a Drafts folder, or mark one as the Drafts folder, in their mail client"
                ),
            ),
            Self::InvalidDraft { field } => (
                Some(Operation::Draft.as_str()),
                None,
                match *field {
                    "to" => "a --to value is not a bare address local@domain (printable ASCII, no spaces, display names or brackets, at most 254 characters), so no draft was created; check the recipients against `agent-mail draft --help` and retry".to_string(),
                    "subject" => "the subject is missing, empty, holds a line break or NUL, or was given together with a reply reference, so no draft was created; check the subject against `agent-mail draft --help` and retry".to_string(),
                    _ => "the body holds a NUL character, so no draft was created; check the body and retry".to_string(),
                },
            ),
            Self::UnsupportedFlag => (
                Some(Operation::Flag.as_str()),
                None,
                "only the flags `seen` (read) and `flagged` (starred) can be set or cleared, so nothing was changed; run `agent-mail flag <REF> --set seen` or `--clear seen`, or the same with `flagged`"
                    .to_string(),
            ),
            Self::AccountRequired { .. }
            | Self::MailboxNotFound { .. }
            | Self::StaleReference { .. }
            | Self::MessageNotFound { .. }
            | Self::SameMailbox { .. }
            | Self::MoveUnsupported { .. }
            | Self::Unsupported { .. }
            | Self::NotFound { .. }
            | Self::RevocationIncomplete { .. }
            | Self::InvalidMandateFile { .. } => {
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
            Self::AuditUnavailable => (
                None,
                None,
                "the call could not be recorded in the audit log, so no result is returned; ask the human to check that the agent-mail data directory is writable"
                    .to_string(),
            ),
        };
        ErrorBody {
            code: self.code(),
            operation,
            account,
            mailbox: None,
            accounts: None,
            id: None,
            deleted: None,
            unreadable: None,
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
            let time = rfc3339_utc(now_secs());
            let mut trail = Trail::default();
            let token = env.get(TOKEN_VAR).map(String::as_str);
            let result = open_mandate(token, context).and_then(|mandate| {
                trail.mandate = Some(mandate.id.clone());
                list(&mandate, context, &mut trail)
            });
            let entry = audit::Entry {
                time,
                mandate: trail.mandate,
                operation: Operation::List.as_str(),
                account: trail.account,
                mailbox: None,
                target: None,
                messages: None,
                recipients: None,
                change: None,
                results: None,
                outcome: if trail.allowed {
                    Decision::Allowed
                } else {
                    Decision::Denied
                },
                reason: result.as_ref().err().map(CoreError::code),
            };
            // Fail closed: no result leaves without its audit entry.
            context
                .audit
                .append(&entry)
                .map_err(|_| CoreError::AuditUnavailable)?;
            result.map(Outcome::Mailboxes)
        }
        Command::Search(query) => {
            let time = rfc3339_utc(now_secs());
            let mut trail = Trail::default();
            let token = env.get(TOKEN_VAR).map(String::as_str);
            let result = open_mandate(token, context).and_then(|mandate| {
                trail.mandate = Some(mandate.id.clone());
                search(&mandate, query, context, &mut trail)
            });
            let found = result.as_ref().ok();
            let entry = audit::Entry {
                time,
                mandate: trail.mandate,
                operation: Operation::Search.as_str(),
                account: trail.account,
                mailbox: Some(query.mailbox.as_str().to_string()),
                target: None,
                // References and a count only: never subjects, addresses
                // or `total`.
                messages: found.map(|r| r.messages.iter().map(|m| m.reference.clone()).collect()),
                recipients: None,
                change: None,
                results: found.map(|r| r.messages.len()),
                outcome: if trail.allowed {
                    Decision::Allowed
                } else {
                    Decision::Denied
                },
                reason: result.as_ref().err().map(CoreError::code),
            };
            // Fail closed: no result leaves without its audit entry.
            context
                .audit
                .append(&entry)
                .map_err(|_| CoreError::AuditUnavailable)?;
            result.map(Outcome::Messages)
        }
        Command::Read { reference, full } => {
            let time = rfc3339_utc(now_secs());
            let mut trail = Trail::default();
            let token = env.get(TOKEN_VAR).map(String::as_str);
            let result = open_mandate(token, context).and_then(|mandate| {
                trail.mandate = Some(mandate.id.clone());
                read(&mandate, reference, *full, context, &mut trail)
            });
            let entry = audit::Entry {
                time,
                mandate: trail.mandate,
                operation: Operation::Read.as_str(),
                account: trail.account,
                mailbox: Some(reference.mailbox.clone()),
                target: None,
                // The reference only: never a subject, address, body or
                // attachment name.
                messages: Some(vec![reference.format()]),
                recipients: None,
                change: None,
                results: None,
                outcome: if trail.allowed {
                    Decision::Allowed
                } else {
                    Decision::Denied
                },
                reason: result.as_ref().err().map(CoreError::code),
            };
            // Fail closed: no result leaves without its audit entry.
            context
                .audit
                .append(&entry)
                .map_err(|_| CoreError::AuditUnavailable)?;
            result.map(Outcome::Message)
        }
        Command::Flag {
            reference,
            action,
            flag: name,
        } => {
            let time = rfc3339_utc(now_secs());
            let mut trail = Trail::default();
            let token = env.get(TOKEN_VAR).map(String::as_str);
            let result = open_mandate(token, context).and_then(|mandate| {
                trail.mandate = Some(mandate.id.clone());
                flag(&mandate, reference, *action, name, context, &mut trail)
            });
            let entry = audit::Entry {
                time,
                mandate: trail.mandate,
                operation: Operation::Flag.as_str(),
                account: trail.account,
                mailbox: Some(reference.mailbox.clone()),
                target: None,
                // The reference and the change only: never a subject or
                // address. An unsupported flag name is never logged.
                messages: Some(vec![reference.format()]),
                recipients: None,
                change: Flag::parse(name).map(|f| flag::change(*action, f)),
                results: None,
                outcome: if trail.allowed {
                    Decision::Allowed
                } else {
                    Decision::Denied
                },
                reason: result.as_ref().err().map(CoreError::code),
            };
            // Fail closed: no result leaves without its audit entry.
            context
                .audit
                .append(&entry)
                .map_err(|_| CoreError::AuditUnavailable)?;
            result.map(Outcome::Flagged)
        }
        Command::Move { reference, target } => {
            let time = rfc3339_utc(now_secs());
            let mut trail = Trail::default();
            let token = env.get(TOKEN_VAR).map(String::as_str);
            let result = open_mandate(token, context).and_then(|mandate| {
                trail.mandate = Some(mandate.id.clone());
                move_message(&mandate, reference, target, context, &mut trail)
            });
            // The reference, and on success the new reference the server
            // reported: never a subject or address.
            let mut messages = vec![reference.format()];
            if let Some(new_ref) = result.as_ref().ok().and_then(|r| r.new_ref.clone()) {
                messages.push(new_ref);
            }
            let entry = audit::Entry {
                time,
                mandate: trail.mandate,
                operation: Operation::Move.as_str(),
                account: trail.account,
                mailbox: Some(reference.mailbox.clone()),
                target: Some(target.as_str().to_string()),
                messages: Some(messages),
                recipients: None,
                change: None,
                results: None,
                outcome: if trail.allowed {
                    Decision::Allowed
                } else {
                    Decision::Denied
                },
                reason: result.as_ref().err().map(CoreError::code),
            };
            // Fail closed: no result leaves without its audit entry.
            context
                .audit
                .append(&entry)
                .map_err(|_| CoreError::AuditUnavailable)?;
            result.map(Outcome::Moved)
        }
        Command::Draft {
            to,
            subject,
            body,
            reply_to,
        } => {
            let time = rfc3339_utc(now_secs());
            let mut trail = Trail::default();
            let token = env.get(TOKEN_VAR).map(String::as_str);
            let result = open_mandate(token, context).and_then(|mandate| {
                trail.mandate = Some(mandate.id.clone());
                draft(
                    &mandate,
                    to,
                    subject.as_deref(),
                    body,
                    reply_to.as_ref(),
                    context,
                    &mut trail,
                )
            });
            // References only: the original's, then the new draft's when
            // the server reported it. Never a subject, body or header.
            let mut messages: Vec<String> = reply_to.iter().map(MessageRef::format).collect();
            if let Some(new_ref) = result.as_ref().ok().and_then(|r| r.reference.clone()) {
                messages.push(new_ref);
            }
            // The recipients the Agent gave, unless one is invalid: a
            // rejected value is never logged.
            let recipients = if to.iter().all(|a| draft::is_valid_address(a)) {
                to.clone()
            } else {
                Vec::new()
            };
            let entry = audit::Entry {
                time,
                mandate: trail.mandate,
                operation: Operation::Draft.as_str(),
                account: trail.account,
                mailbox: trail.mailbox,
                target: None,
                messages: (!messages.is_empty()).then_some(messages),
                recipients: Some(recipients),
                change: None,
                results: None,
                outcome: if trail.allowed {
                    Decision::Allowed
                } else {
                    Decision::Denied
                },
                reason: result.as_ref().err().map(CoreError::code),
            };
            // Fail closed: no result leaves without its audit entry.
            context
                .audit
                .append(&entry)
                .map_err(|_| CoreError::AuditUnavailable)?;
            result.map(Outcome::Drafted)
        }
        Command::MandateCreate { from } => {
            crate::create::run(env, from.as_deref(), context).map(|()| Outcome::MandateCreated)
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

/// Deletes every record whose clear metadata carries ID `id` exactly.
///
/// The metadata is unauthenticated and any local process can add an item, so
/// a decoy may copy a real Mandate's ID. Removing only one match could leave
/// the real record, and its token, in place while reporting success; so all
/// matches go. A record that cannot be read cannot be matched, so if the
/// store holds any, success cannot be promised and the result is
/// [`CoreError::RevocationIncomplete`] (after deleting every readable match).
/// The name is reported only if every match agreed on it.
fn mandate_delete(id: &str, context: &Context<'_>) -> Result<DeletedMandate, CoreError> {
    let entries = context
        .store
        .entries()
        .map_err(|_| CoreError::StoreUnavailable)?;
    let mut matches = Vec::new();
    let mut unreadable = 0;
    for (key, value) in &entries {
        match seal::listing(value) {
            Ok(listing) if listing.id == id => matches.push((key, listing.name)),
            Ok(_) => {}
            Err(_) => unreadable += 1,
        }
    }
    let mut deleted = 0;
    let mut names = Vec::new();
    for (key, name) in matches {
        match context.store.delete(key) {
            Ok(true) => {
                deleted += 1;
                names.push(name);
            }
            // Removed by someone else since it was read.
            Ok(false) => {}
            Err(_) => return Err(CoreError::StoreUnavailable),
        }
    }
    if unreadable > 0 {
        return Err(CoreError::RevocationIncomplete {
            id: id.to_string(),
            deleted,
            unreadable,
        });
    }
    let Some(first) = names.first() else {
        return Err(CoreError::NotFound { id: id.to_string() });
    };
    let name = names.iter().all(|n| n == first).then(|| first.clone());
    Ok(DeletedMandate {
        id: id.to_string(),
        name,
    })
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

/// What an Agent call got as far as, for its audit entry. Holds no
/// Credential or token.
#[derive(Default)]
struct Trail {
    /// Short ID of the opened Mandate.
    mandate: Option<String>,
    /// Login of the chosen Account.
    account: Option<String>,
    /// Whether the Policy granted the operation.
    allowed: bool,
    /// The mailbox found during the call, decoded UTF-8: the Drafts
    /// mailbox of a `draft`.
    mailbox: Option<String>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
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

fn list(
    mandate: &Mandate,
    context: &Context<'_>,
    trail: &mut Trail,
) -> Result<MailboxList, CoreError> {
    let account = single_account(mandate)?;
    let login = account.login.clone();
    trail.account = Some(login.clone());
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
            mailbox: None,
        });
    }
    trail.allowed = true;
    let raw = imap::list_mailboxes(account, &context.network)
        .map_err(|e| connector_error(e, Operation::List, &login, None))?;
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

/// The Mandate's only Account. One Account per command until `--account`
/// arrives (ticket 13); never merge Accounts silently.
fn single_account(mandate: &Mandate) -> Result<&Account, CoreError> {
    match mandate.accounts.as_slice() {
        [account] => Ok(account),
        accounts => Err(CoreError::AccountRequired {
            accounts: accounts.iter().map(|a| a.login.clone()).collect(),
        }),
    }
}

/// The structured error for a connector failure of `operation` on
/// `account`, and on `mailbox` for operations on one mailbox.
fn connector_error(
    error: ImapError,
    operation: Operation,
    account: &str,
    mailbox: Option<&str>,
) -> CoreError {
    let account = account.to_string();
    match error {
        ImapError::TlsRequired => CoreError::TlsRequired { account },
        ImapError::Certificate => CoreError::TlsCertificate { account },
        ImapError::Connection => CoreError::Connection { account },
        ImapError::Login => CoreError::LoginFailed { account },
        ImapError::Protocol => CoreError::Protocol { account },
        ImapError::MailboxNotFound => match mailbox {
            Some(mailbox) => CoreError::MailboxNotFound {
                account,
                mailbox: mailbox.to_string(),
            },
            None => CoreError::Protocol { account },
        },
        ImapError::StaleReference => match mailbox {
            Some(mailbox) => CoreError::StaleReference {
                operation,
                account,
                mailbox: mailbox.to_string(),
            },
            None => CoreError::Protocol { account },
        },
        ImapError::MessageNotFound => match mailbox {
            Some(mailbox) => CoreError::MessageNotFound {
                operation,
                account,
                mailbox: mailbox.to_string(),
            },
            None => CoreError::Protocol { account },
        },
        ImapError::DraftsNotFound => CoreError::DraftsNotFound { account },
        ImapError::GmailUnsupported => match mailbox {
            Some(mailbox) => CoreError::Unsupported {
                account,
                mailbox: mailbox.to_string(),
            },
            None => CoreError::Protocol { account },
        },
        // Only a move reports these, and it maps them itself.
        ImapError::MoveUnsupported | ImapError::TargetNotFound => CoreError::Protocol { account },
    }
}

fn read(
    mandate: &Mandate,
    reference: &MessageRef,
    full: bool,
    context: &Context<'_>,
    trail: &mut Trail,
) -> Result<ReadResult, CoreError> {
    // Checked again here, not only by the CLI parser: no CR, LF or NUL may
    // reach the server, and the UID must be a real one.
    if !read::is_valid_reference(reference) {
        return Err(CoreError::InvalidReference {
            operation: Operation::Read,
        });
    }
    let login = reference.account.as_str();
    let mailbox = reference.mailbox.as_str();
    trail.account = Some(login.to_string());
    // Deny by default, before any connection is made. The Account is the
    // Mandate's Account with exactly this login, so a reference naming an
    // uncovered Account never uses another Account's Credential.
    let denied = || CoreError::Denied {
        operation: Operation::Read,
        account: login.to_string(),
        mailbox: Some(mailbox.to_string()),
    };
    let account = mandate
        .accounts
        .iter()
        .find(|a| a.login == login)
        .ok_or_else(denied)?;
    if !mandate
        .policy
        .allows_mailbox(Operation::Read, login, mailbox)
    {
        return Err(denied());
    }
    trail.allowed = true;
    let raw = imap::read(
        account,
        &context.network,
        mailbox,
        reference.uidvalidity,
        reference.uid,
    )
    .map_err(|e| connector_error(e, Operation::Read, login, Some(mailbox)))?;
    let parsed = read::parse_message(&raw).ok_or_else(|| CoreError::Protocol {
        account: login.to_string(),
    })?;
    let (body, truncated) = read::truncate(parsed.body, full);
    Ok(ReadResult {
        reference: reference.format(),
        account: login.to_string(),
        mailbox: mailbox.to_string(),
        date: parsed
            .date
            .and_then(|secs| u64::try_from(secs).ok())
            .map(rfc3339_utc)
            .unwrap_or_default(),
        from: parsed.from,
        to: parsed.to,
        cc: parsed.cc,
        subject: parsed.subject,
        message_id: parsed.message_id,
        body,
        truncated,
        attachments: parsed.attachments,
    })
}

fn flag(
    mandate: &Mandate,
    reference: &MessageRef,
    action: FlagAction,
    name: &str,
    context: &Context<'_>,
    trail: &mut Trail,
) -> Result<FlagResult, CoreError> {
    // Checked again here, not only by the CLI parser: no CR, LF or NUL may
    // reach the server, and the UID must be a real one.
    if !read::is_valid_reference(reference) {
        return Err(CoreError::InvalidReference {
            operation: Operation::Flag,
        });
    }
    let login = reference.account.as_str();
    let mailbox = reference.mailbox.as_str();
    trail.account = Some(login.to_string());
    // The Agent's flag name maps to one of two fixed values; anything else
    // is refused before any connection, and never sent or echoed.
    let flag = Flag::parse(name).ok_or(CoreError::UnsupportedFlag)?;
    // Deny by default, before any connection is made. The Account is the
    // Mandate's Account with exactly this login, so a reference naming an
    // uncovered Account never uses another Account's Credential.
    let denied = || CoreError::Denied {
        operation: Operation::Flag,
        account: login.to_string(),
        mailbox: Some(mailbox.to_string()),
    };
    let account = mandate
        .accounts
        .iter()
        .find(|a| a.login == login)
        .ok_or_else(denied)?;
    if !mandate
        .policy
        .allows_mailbox(Operation::Flag, login, mailbox)
    {
        return Err(denied());
    }
    trail.allowed = true;
    let flags = imap::flag(
        account,
        &context.network,
        mailbox,
        reference.uidvalidity,
        reference.uid,
        action,
        flag,
    )
    .map_err(|e| connector_error(e, Operation::Flag, login, Some(mailbox)))?;
    Ok(FlagResult {
        reference: reference.format(),
        account: login.to_string(),
        mailbox: mailbox.to_string(),
        flags,
    })
}

fn move_message(
    mandate: &Mandate,
    reference: &MessageRef,
    target: &SearchText,
    context: &Context<'_>,
    trail: &mut Trail,
) -> Result<MoveResult, CoreError> {
    // Checked again here, not only by the CLI parser: no CR, LF or NUL may
    // reach the server, and the UID must be a real one.
    if !read::is_valid_reference(reference) {
        return Err(CoreError::InvalidReference {
            operation: Operation::Move,
        });
    }
    let login = reference.account.as_str();
    let mailbox = reference.mailbox.as_str();
    let target = target.as_str();
    trail.account = Some(login.to_string());
    if target == mailbox {
        return Err(CoreError::SameMailbox {
            account: login.to_string(),
            mailbox: mailbox.to_string(),
        });
    }
    // Deny by default, before any connection is made: `move` must be
    // granted on both the source and the target. The Account is the
    // Mandate's Account with exactly this login, so a reference naming an
    // uncovered Account never uses another Account's Credential.
    let denied = |mailbox: &str| CoreError::Denied {
        operation: Operation::Move,
        account: login.to_string(),
        mailbox: Some(mailbox.to_string()),
    };
    let account = mandate
        .accounts
        .iter()
        .find(|a| a.login == login)
        .ok_or_else(|| denied(mailbox))?;
    for name in [mailbox, target] {
        if !mandate.policy.allows_mailbox(Operation::Move, login, name) {
            return Err(denied(name));
        }
    }
    trail.allowed = true;
    let moved = imap::move_message(
        account,
        &context.network,
        mailbox,
        reference.uidvalidity,
        reference.uid,
        target,
    )
    .map_err(|e| match e {
        ImapError::MoveUnsupported => CoreError::MoveUnsupported {
            account: login.to_string(),
            mailbox: mailbox.to_string(),
        },
        ImapError::TargetNotFound => CoreError::MailboxNotFound {
            account: login.to_string(),
            mailbox: target.to_string(),
        },
        e => connector_error(e, Operation::Move, login, Some(mailbox)),
    })?;
    Ok(MoveResult {
        reference: reference.format(),
        account: login.to_string(),
        mailbox: mailbox.to_string(),
        target: target.to_string(),
        new_ref: moved.map(|(uidvalidity, uid)| {
            MessageRef {
                account: login.to_string(),
                mailbox: target.to_string(),
                uidvalidity,
                uid,
            }
            .format()
        }),
    })
}

fn draft(
    mandate: &Mandate,
    to: &[String],
    subject: Option<&str>,
    body: &str,
    reply_to: Option<&MessageRef>,
    context: &Context<'_>,
    trail: &mut Trail,
) -> Result<DraftResult, CoreError> {
    // Checked again here, not only by the CLI parser: these are the
    // header-injection surface. The values are never echoed.
    if to.is_empty() || !to.iter().all(|a| draft::is_valid_address(a)) {
        return Err(CoreError::InvalidDraft { field: "to" });
    }
    let subject_ok = match (subject, reply_to) {
        (Some(subject), None) => draft::is_valid_subject(subject),
        (None, Some(_)) => true,
        _ => false,
    };
    if !subject_ok {
        return Err(CoreError::InvalidDraft { field: "subject" });
    }
    if !draft::is_valid_body(body) {
        return Err(CoreError::InvalidDraft { field: "body" });
    }
    // Deny by default, before any connection is made.
    let account = match reply_to {
        Some(reference) => {
            if !read::is_valid_reference(reference) {
                return Err(CoreError::InvalidReference {
                    operation: Operation::Draft,
                });
            }
            let login = reference.account.as_str();
            trail.account = Some(login.to_string());
            // A reply reads the original: `read` on its mailbox, of the
            // Mandate's Account with exactly this login, so a reference
            // naming an uncovered Account never uses another Account's
            // Credential.
            let read_denied = || CoreError::Denied {
                operation: Operation::Read,
                account: login.to_string(),
                mailbox: Some(reference.mailbox.clone()),
            };
            let account = mandate.account(login).ok_or_else(read_denied)?;
            require_draft_grant(mandate, login)?;
            if !mandate
                .policy
                .allows_mailbox(Operation::Read, login, &reference.mailbox)
            {
                return Err(read_denied());
            }
            account
        }
        None => {
            let account = single_account(mandate)?;
            trail.account = Some(account.login.clone());
            require_draft_grant(mandate, &account.login)?;
            account
        }
    };
    trail.allowed = true;
    let login = account.login.as_str();
    let message_id = draft::new_message_id(login);
    let date = draft::rfc5322_date(now_secs());
    let mut sent_subject = subject.unwrap_or_default().to_string();
    let mut raw_drafts = None;
    let original = reply_to.map(|r| imap::ReplyTo {
        mailbox: &r.mailbox,
        uidvalidity: r.uidvalidity,
        uid: r.uid,
    });
    let appended = imap::draft(
        account,
        &context.network,
        original,
        &mut raw_drafts,
        |headers| {
            let (in_reply_to, references) = match headers {
                Some(raw) => {
                    let (in_reply_to, references, reply_subject) =
                        draft::reply_headers(&draft::parse_original(raw));
                    sent_subject = reply_subject;
                    (in_reply_to, references)
                }
                None => (None, Vec::new()),
            };
            draft::Draft {
                login,
                to,
                subject: &sent_subject,
                body,
                message_id: &message_id,
                date: &date,
                in_reply_to: in_reply_to.as_deref(),
                references: &references,
            }
            .build()
        },
    );
    // Malformed modified UTF-7 falls back to the raw name, losslessly.
    let mailbox = raw_drafts.map(|raw| utf7::decode(&raw).unwrap_or(raw));
    trail.mailbox.clone_from(&mailbox);
    let appended = appended.map_err(|e| {
        connector_error(
            e,
            Operation::Draft,
            login,
            reply_to.map(|r| r.mailbox.as_str()),
        )
    })?;
    let mailbox = mailbox.ok_or_else(|| CoreError::Protocol {
        account: login.to_string(),
    })?;
    Ok(DraftResult {
        reference: appended.map(|(uidvalidity, uid)| {
            MessageRef {
                account: login.to_string(),
                mailbox: mailbox.clone(),
                uidvalidity,
                uid,
            }
            .format()
        }),
        account: login.to_string(),
        mailbox,
        to: to.to_vec(),
        subject: sent_subject,
        original: reply_to.map(MessageRef::format),
    })
}

/// Fails with `denied` for `draft` unless a `draft` Grant covers the
/// Account. `draft` Grants take no mailboxes.
fn require_draft_grant(mandate: &Mandate, login: &str) -> Result<(), CoreError> {
    if mandate
        .policy
        .grants_for(Operation::Draft, login)
        .next()
        .is_none()
    {
        return Err(CoreError::Denied {
            operation: Operation::Draft,
            account: login.to_string(),
            mailbox: None,
        });
    }
    Ok(())
}

fn search(
    mandate: &Mandate,
    query: &Query,
    context: &Context<'_>,
    trail: &mut Trail,
) -> Result<SearchResult, CoreError> {
    let account = single_account(mandate)?;
    let login = account.login.clone();
    trail.account = Some(login.clone());
    // The query's values were checked when it was built (see
    // `crate::search`); the mailbox is the Agent's decoded name, compared
    // exactly. Deny by default, before any connection is made.
    let mailbox = query.mailbox.as_str();
    if !mandate
        .policy
        .allows_mailbox(Operation::Search, &login, mailbox)
    {
        return Err(CoreError::Denied {
            operation: Operation::Search,
            account: login,
            mailbox: Some(mailbox.to_string()),
        });
    }
    trail.allowed = true;
    let limit = usize::try_from(query.limit.get()).unwrap_or(usize::MAX);
    let found = imap::search(
        account,
        &context.network,
        mailbox,
        &query.imap_criteria(),
        query.gmail.is_some(),
        limit,
    )
    .map_err(|e| connector_error(e, Operation::Search, &login, Some(mailbox)))?;
    let messages: Vec<MessageSummary> = found
        .messages
        .into_iter()
        .map(|m| MessageSummary {
            reference: MessageRef {
                account: login.clone(),
                mailbox: mailbox.to_string(),
                uidvalidity: found.uidvalidity,
                uid: m.uid,
            }
            .format(),
            date: rfc3339_utc(u64::try_from(m.internal_date).unwrap_or(0)),
            from: if m.from_address.is_empty() {
                String::new()
            } else {
                search::format_address(m.from_name.as_deref(), &m.from_address)
            },
            subject: m
                .subject
                .as_deref()
                .map(search::decode_rfc2047)
                .unwrap_or_default(),
            flags: m.flags,
        })
        .collect();
    Ok(SearchResult {
        account: login,
        mailbox: mailbox.to_string(),
        total: found.total,
        more: found.total > messages.len(),
        messages,
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
                mailbox: None,
            },
            CoreError::Denied {
                operation: Operation::Search,
                account: account.clone(),
                mailbox: Some("Archive".to_string()),
            },
            CoreError::MailboxNotFound {
                account: account.clone(),
                mailbox: "Missing".to_string(),
            },
            CoreError::Denied {
                operation: Operation::Read,
                account: account.clone(),
                mailbox: Some("Read".to_string()),
            },
            CoreError::StaleReference {
                operation: Operation::Read,
                account: account.clone(),
                mailbox: "Read".to_string(),
            },
            CoreError::MessageNotFound {
                operation: Operation::Read,
                account: account.clone(),
                mailbox: "Read".to_string(),
            },
            CoreError::Denied {
                operation: Operation::Flag,
                account: account.clone(),
                mailbox: Some("Triage".to_string()),
            },
            CoreError::StaleReference {
                operation: Operation::Flag,
                account: account.clone(),
                mailbox: "Triage".to_string(),
            },
            CoreError::MessageNotFound {
                operation: Operation::Flag,
                account: account.clone(),
                mailbox: "Triage".to_string(),
            },
            CoreError::InvalidReference {
                operation: Operation::Read,
            },
            CoreError::InvalidReference {
                operation: Operation::Flag,
            },
            CoreError::UnsupportedFlag,
            CoreError::Denied {
                operation: Operation::Move,
                account: account.clone(),
                mailbox: Some("Archive".to_string()),
            },
            CoreError::StaleReference {
                operation: Operation::Move,
                account: account.clone(),
                mailbox: "INBOX".to_string(),
            },
            CoreError::MessageNotFound {
                operation: Operation::Move,
                account: account.clone(),
                mailbox: "INBOX".to_string(),
            },
            CoreError::InvalidReference {
                operation: Operation::Move,
            },
            CoreError::SameMailbox {
                account: account.clone(),
                mailbox: "INBOX".to_string(),
            },
            CoreError::MoveUnsupported {
                account: account.clone(),
                mailbox: "Filing".to_string(),
            },
            CoreError::Unsupported {
                account: account.clone(),
                mailbox: "INBOX".to_string(),
            },
            CoreError::Denied {
                operation: Operation::Draft,
                account: account.clone(),
                mailbox: None,
            },
            CoreError::StaleReference {
                operation: Operation::Draft,
                account: account.clone(),
                mailbox: "Thread".to_string(),
            },
            CoreError::MessageNotFound {
                operation: Operation::Draft,
                account: account.clone(),
                mailbox: "Thread".to_string(),
            },
            CoreError::InvalidReference {
                operation: Operation::Draft,
            },
            CoreError::DraftsNotFound {
                account: account.clone(),
            },
            CoreError::InvalidDraft { field: "to" },
            CoreError::InvalidDraft { field: "subject" },
            CoreError::InvalidDraft { field: "body" },
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
            CoreError::InvalidMandateFile {
                problem: "line 8: unknown key in an [[accounts]] entry".to_string(),
            },
            CoreError::RevocationIncomplete {
                id: "aaaa1111".to_string(),
                deleted: 1,
                unreadable: 2,
            },
            CoreError::RevocationIncomplete {
                id: "aaaa1111".to_string(),
                deleted: 0,
                unreadable: 1,
            },
            CoreError::AuditUnavailable,
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
