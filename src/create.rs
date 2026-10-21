//! `mandate create`: the guided setup the human runs in their own terminal.
//!
//! Flow: name, then per Account the Gmail preset or generic server settings,
//! the hidden Credential and a live login (retry or abort on failure), the
//! mailbox and operation checklists; then "add another Account?", a summary,
//! confirmation, sealing and storing, the Agent Token shown once, and an
//! offer to export the Accounts and Grants as a TOML Mandate file.
//!
//! With `--from`, a Mandate file pre-fills the name, each Account's settings
//! (each one kept or dropped), and the mailbox and operation checklists.
//! The file never supplies a Credential: each is typed and live-checked.
//!
//! Credentials come only from the Terminal's hidden input: no flag, no
//! environment variable, no pipe. The refusal checks run before any prompt
//! and before any store access.

use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::{Context, CoreError};
use crate::export::{FileAccount, InvalidFile, MandateFile};
use crate::imap::{self, ImapError};
use crate::mandate::{
    Account, Credential, Grant, MailboxPattern, Mandate, Operation, Policy, TlsMode,
};
use crate::seal::{self, AgentToken};
use crate::terminal::{Cancelled, Terminal, escape_controls};
use crate::utf7;
use zeroize::Zeroizing;

/// Environment variables whose presence means an agent sandbox or agent
/// session. Best effort: the real barrier is that every Credential must be
/// typed (ADR-0003).
pub const SANDBOX_INDICATORS: &[&str] = &[
    // Claude Code sessions and its sandbox runtime.
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "SANDBOX_RUNTIME",
    // OpenAI Codex CLI sandbox.
    "CODEX_SANDBOX",
    "CODEX_SANDBOX_NETWORK_DISABLED",
    // opencode sessions.
    "OPENCODE",
    // Gemini CLI sessions and its sandbox.
    "GEMINI_CLI",
    "GEMINI_SANDBOX",
    // Cursor agent terminals.
    "CURSOR_AGENT",
];

/// Account type choice: the Gmail preset.
pub const GMAIL: &str = "Gmail (imap.gmail.com, app password)";
/// Account type choice: any other IMAP server.
pub const GENERIC: &str = "Other IMAP server";
/// TLS mode choice: TLS from the first byte.
pub const TLS_IMPLICIT: &str = "Implicit TLS (usually port 993)";
/// TLS mode choice: STARTTLS upgrade, required before login.
pub const TLS_STARTTLS: &str = "STARTTLS (usually port 143)";
/// Login failure choice: type the Credential again.
pub const RETRY: &str = "Retry: type the credential again";
/// Login failure choice: stop without storing anything.
pub const ABORT: &str = "Abort: store nothing";
/// The export offer, asked after the Agent Token is shown.
pub const EXPORT_OFFER: &str = "Export this Mandate's Accounts and Grants to a TOML file?";
/// The export path prompt.
pub const EXPORT_PATH: &str = "Export file path:";
/// Shown whenever the flow ends without storing a Mandate.
pub const NOTHING_STORED: &str = "Nothing was stored.";

const GMAIL_HOST: &str = "imap.gmail.com";
const GMAIL_PORT: u16 = 993;

/// Runs the guided setup and stores the Mandate. With `from`, the Mandate
/// file pre-fills the choices; it is read only after the refusal checks and
/// before any prompt.
pub(crate) fn run(
    env: &HashMap<String, String>,
    from: Option<&Path>,
    context: &Context<'_>,
) -> Result<(), CoreError> {
    let terminal = context.terminal;
    if !terminal.is_interactive() {
        return Err(CoreError::InteractiveTerminalRequired);
    }
    if sandbox_indicator(env).is_some() {
        return Err(CoreError::SandboxDetected);
    }
    let file = match from {
        None => None,
        Some(path) => Some(
            MandateFile::load(path)
                .map_err(|InvalidFile(problem)| CoreError::InvalidMandateFile { problem })?,
        ),
    };
    let mandate = match guided(context, file.as_ref()) {
        Ok(mandate) => mandate,
        Err(error) => {
            terminal.message(NOTHING_STORED);
            return Err(error);
        }
    };
    let token = AgentToken::mint();
    let record = seal::seal(&token, &mandate);
    if context.store.put(&record.key, &record.value).is_err() {
        terminal.message(&format!(
            "The Mandate could not be written to the credential store. {NOTHING_STORED}"
        ));
        return Err(CoreError::StoreUnavailable);
    }
    let reveal = Zeroizing::new(format!(
        "Mandate \"{}\" (ID {}) is sealed and stored.\n\n\
         Agent Token, shown only this once (it cannot be shown again):\n\n    {}\n\n\
         Give it to the Agent in the AGENT_MAIL_TOKEN environment variable.",
        escape_controls(&mandate.name),
        mandate.id,
        token.expose()
    ));
    if terminal.reveal(&reveal).is_err() {
        // A Mandate whose token nobody saw is unusable: take it back out.
        let _ = context.store.delete(&record.key);
        terminal.message(&format!(
            "The Agent Token could not be shown on the terminal. {NOTHING_STORED}"
        ));
        return Err(CoreError::Aborted);
    }
    offer_export(terminal, &mandate);
    Ok(())
}

/// Offers to save the stored Mandate's Accounts and Grants as TOML. The
/// Mandate is already stored, so nothing here can fail the run.
fn offer_export(terminal: &dyn Terminal, mandate: &Mandate) {
    if !terminal.confirm(EXPORT_OFFER, false).unwrap_or(false) {
        return;
    }
    let contents = MandateFile::of(mandate).to_toml();
    let default = format!("agent-mail-{}.toml", mandate.id);
    loop {
        let Ok(answer) = terminal.text(EXPORT_PATH, Some(&default)) else {
            terminal.message("Not exported. The Mandate is stored.");
            return;
        };
        let answer = answer.trim();
        if answer.is_empty() {
            terminal.message("Please enter a path.");
            continue;
        }
        let path = Path::new(answer);
        let shown = escape_controls(answer);
        match write_new(path, contents.as_bytes()) {
            Ok(()) => {
                terminal.message(&format!(
                    "Exported this Mandate's Accounts and Grants to {shown}."
                ));
                return;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                terminal.message(&format!(
                    "{shown} already exists and is never overwritten; choose another path."
                ));
            }
            Err(error) => {
                terminal.message(&format!(
                    "Could not write {shown}: {}. Not exported; the Mandate is stored.",
                    error.kind()
                ));
                return;
            }
        }
    }
}

/// Creates `path` exclusively (never overwriting a file or following a
/// symlink there) and writes `bytes`. A partly written file is removed.
fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    if written.is_err() {
        drop(file);
        let _ = std::fs::remove_file(path);
    }
    written
}

/// The first indicator variable present in `env`, if any.
pub fn sandbox_indicator(env: &HashMap<String, String>) -> Option<&'static str> {
    SANDBOX_INDICATORS
        .iter()
        .copied()
        .find(|name| env.contains_key(*name))
}

fn cancelled(_: Cancelled) -> CoreError {
    CoreError::Aborted
}

/// What the Mandate file pre-selects for one Account.
struct Preselection {
    /// Decoded mailbox names the file grants (non-`draft`).
    mailboxes: Vec<String>,
    /// Whether the file grants `*`.
    all: bool,
    /// Operations the file grants.
    operations: Vec<Operation>,
}

fn guided(context: &Context<'_>, file: Option<&MandateFile>) -> Result<Mandate, CoreError> {
    let terminal = context.terminal;
    let name = ask_non_empty(terminal, "Mandate name:", file.map(|f| f.name.as_str()))?;
    let mut accounts: Vec<Account> = Vec::new();
    let mut grants: Vec<Grant> = Vec::new();
    let mut more = true;
    if let Some(file) = file {
        for saved in &file.accounts {
            let keep = format!(
                "Keep Account {} ({}:{}, TLS {})?",
                escape_controls(&saved.login),
                escape_controls(&saved.host),
                saved.port,
                saved.tls.as_str()
            );
            if !terminal.confirm(&keep, true).map_err(cancelled)? {
                continue;
            }
            let (all_mailboxes, all) = file.mailboxes_for(&saved.login);
            let pre = Preselection {
                mailboxes: all_mailboxes,
                all,
                operations: file.operations_for(&saved.login),
            };
            let (account, mailboxes) = add_account(context, &accounts, Some(saved))?;
            add_grants(
                terminal,
                account,
                &mailboxes,
                &pre,
                &mut accounts,
                &mut grants,
            )?;
        }
        if accounts.is_empty() {
            terminal.message("No Account from the file was kept; add one now.");
        } else {
            more = terminal
                .confirm("Add another Account?", false)
                .map_err(cancelled)?;
        }
    }
    while more {
        let (account, mailboxes) = add_account(context, &accounts, None)?;
        let pre = Preselection {
            mailboxes: Vec::new(),
            all: false,
            operations: Vec::new(),
        };
        add_grants(
            terminal,
            account,
            &mailboxes,
            &pre,
            &mut accounts,
            &mut grants,
        )?;
        more = terminal
            .confirm("Add another Account?", false)
            .map_err(cancelled)?;
    }
    let mandate = Mandate {
        name,
        id: seal::short_id(),
        created: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        accounts,
        policy: Policy { grants },
    };
    terminal.message(&summary(&mandate));
    if !terminal
        .confirm("Seal and store this Mandate?", true)
        .map_err(cancelled)?
    {
        return Err(CoreError::Aborted);
    }
    Ok(mandate)
}

/// Asks the mailbox and operation checklists for a checked `account` and
/// records it with its Grants.
fn add_grants(
    terminal: &dyn Terminal,
    account: Account,
    mailboxes: &[String],
    pre: &Preselection,
    accounts: &mut Vec<Account>,
    grants: &mut Vec<Grant>,
) -> Result<(), CoreError> {
    let login = account.login.clone();
    let ticked = ask_mailboxes(terminal, &login, mailboxes, pre)?;
    let operations = ask_operations(terminal, &login, &pre.operations)?;
    for operation in operations {
        let mailboxes = if operation == Operation::Draft {
            // Drafts always go to the \Drafts mailbox.
            Vec::new()
        } else {
            ticked
                .iter()
                .map(|m| MailboxPattern::Exact(m.clone()))
                .collect()
        };
        grants.push(Grant {
            operation,
            account: login.clone(),
            mailboxes,
        });
    }
    accounts.push(account);
    Ok(())
}

fn ask_non_empty(
    terminal: &dyn Terminal,
    prompt: &str,
    default: Option<&str>,
) -> Result<String, CoreError> {
    loop {
        let value = terminal.text(prompt, default).map_err(cancelled)?;
        let value = value.trim();
        if !value.is_empty() {
            return Ok(value.to_string());
        }
        terminal.message("Please enter a value.");
    }
}

fn ask_login(
    terminal: &dyn Terminal,
    prompt: &str,
    default: Option<&str>,
    existing: &[Account],
) -> Result<String, CoreError> {
    loop {
        let login = ask_non_empty(terminal, prompt, default)?;
        if existing.iter().any(|a| a.login == login) {
            terminal.message(&format!(
                "{} is already in this Mandate; each Account can be added once.",
                escape_controls(&login)
            ));
            continue;
        }
        return Ok(login);
    }
}

fn ask_port(terminal: &dyn Terminal, default: u16) -> Result<u16, CoreError> {
    let default = default.to_string();
    loop {
        let text = ask_non_empty(terminal, "IMAP server port:", Some(&default))?;
        match text.parse::<u16>() {
            Ok(port) if port != 0 => return Ok(port),
            _ => terminal.message("The port must be a number from 1 to 65535."),
        }
    }
}

/// Asks for one Account's settings and Credential until the live login
/// succeeds. With `saved`, the settings prompts default to its values and the
/// Account type is not asked. Returns the Account and its server's raw
/// mailbox names.
fn add_account(
    context: &Context<'_>,
    existing: &[Account],
    saved: Option<&FileAccount>,
) -> Result<(Account, Vec<String>), CoreError> {
    let terminal = context.terminal;
    let gmail = match saved {
        Some(_) => false,
        None => {
            let kinds = [GMAIL.to_string(), GENERIC.to_string()];
            terminal
                .choose("Account type:", &kinds, 0)
                .map_err(cancelled)?
                == 0
        }
    };
    let (host, port, tls, login, credential_prompt) = if gmail {
        let login = ask_login(terminal, "Gmail address:", None, existing)?;
        (
            GMAIL_HOST.to_string(),
            GMAIL_PORT,
            TlsMode::Implicit,
            login,
            "Gmail app password",
        )
    } else {
        let host = ask_non_empty(
            terminal,
            "IMAP server host:",
            saved.map(|s| s.host.as_str()),
        )?;
        let port = ask_port(terminal, saved.map_or(993, |s| s.port))?;
        let modes = [TLS_IMPLICIT.to_string(), TLS_STARTTLS.to_string()];
        let default = match saved.map(|s| s.tls) {
            Some(TlsMode::Starttls) => 1,
            _ => 0,
        };
        let tls = match terminal
            .choose("TLS mode:", &modes, default)
            .map_err(cancelled)?
        {
            0 => TlsMode::Implicit,
            _ => TlsMode::Starttls,
        };
        let login = ask_login(
            terminal,
            "Login name:",
            saved.map(|s| s.login.as_str()),
            existing,
        )?;
        (host, port, tls, login, "Credential (password)")
    };
    let shown = escape_controls(&login);
    let server = format!("{}:{port}", escape_controls(&host));
    if looks_like_gmail(&login) && !host.eq_ignore_ascii_case(GMAIL_HOST) {
        terminal.message(&format!(
            "Warning: {shown} looks like a Gmail address, but the server is {server}, not \
             {GMAIL_HOST}. The credential you type next goes to {server}; check the host \
             before typing it."
        ));
    }
    loop {
        let credential: Credential = terminal
            .hidden(&format!("{credential_prompt} for {shown} on {server}:"))
            .map_err(cancelled)?;
        let account = Account {
            login: login.clone(),
            host: host.clone(),
            port,
            tls,
            credential,
        };
        terminal.message(&format!("Checking the login for {shown}..."));
        match imap::list_mailboxes(&account, &context.network) {
            Ok(mailboxes) => return Ok((account, mailboxes)),
            Err(error) => {
                terminal.message(&format!(
                    "Login check for {shown} failed: {}",
                    failure_reason(&error)
                ));
                let choices = [RETRY.to_string(), ABORT.to_string()];
                match terminal
                    .choose("What now?", &choices, 0)
                    .map_err(cancelled)?
                {
                    0 => continue,
                    _ => return Err(CoreError::Aborted),
                }
            }
        }
    }
}

/// Whether `login` is a Gmail address (`@gmail.com` or `@googlemail.com`).
fn looks_like_gmail(login: &str) -> bool {
    let lower = login.trim().to_ascii_lowercase();
    lower.ends_with("@gmail.com") || lower.ends_with("@googlemail.com")
}

fn failure_reason(error: &ImapError) -> &'static str {
    match error {
        ImapError::Login => "the server refused the login; check the credential",
        ImapError::TlsRequired => {
            "the server did not offer TLS, so the credential was not sent; check the TLS mode"
        }
        ImapError::Certificate => {
            "the server's TLS certificate is not trusted; check the host name"
        }
        ImapError::Connection => "could not reach the server; check the host, port and network",
        ImapError::Protocol | ImapError::MailboxNotFound => {
            "the server answered unexpectedly; check the server settings"
        }
    }
}

fn ask_mailboxes(
    terminal: &dyn Terminal,
    login: &str,
    raw: &[String],
    pre: &Preselection,
) -> Result<Vec<String>, CoreError> {
    // Malformed modified UTF-7 falls back to the raw name, as `list` does.
    let names: Vec<String> = raw
        .iter()
        .map(|name| utf7::decode(name).unwrap_or_else(|_| name.clone()))
        .collect();
    let shown_login = escape_controls(login);
    for missing in pre.mailboxes.iter().filter(|m| !names.contains(m)) {
        terminal.message(&format!(
            "Mailbox {} from the file is unavailable on {shown_login}; it is left out.",
            escape_controls(missing)
        ));
    }
    let initial: Vec<usize> = names
        .iter()
        .enumerate()
        .filter(|(_, name)| pre.all || pre.mailboxes.contains(name))
        .map(|(i, _)| i)
        .collect();
    let labels: Vec<String> = names.iter().map(|n| escape_controls(n)).collect();
    let prompt = format!("Mailboxes the Agent may use on {shown_login}:");
    loop {
        let ticked = terminal
            .checklist(&prompt, &labels, &initial)
            .map_err(cancelled)?;
        if ticked.is_empty() && !names.is_empty() {
            terminal.message("Tick at least one mailbox.");
            continue;
        }
        return Ok(ticked.into_iter().map(|i| names[i].clone()).collect());
    }
}

fn ask_operations(
    terminal: &dyn Terminal,
    login: &str,
    pre: &[Operation],
) -> Result<Vec<Operation>, CoreError> {
    let labels: Vec<String> = Operation::ALL
        .iter()
        .map(|op| op.as_str().to_string())
        .collect();
    let initial: Vec<usize> = Operation::ALL
        .iter()
        .enumerate()
        .filter(|(_, op)| pre.contains(op))
        .map(|(i, _)| i)
        .collect();
    let prompt = format!(
        "Operations the Agent may perform on {}:",
        escape_controls(login)
    );
    loop {
        let ticked = terminal
            .checklist(&prompt, &labels, &initial)
            .map_err(cancelled)?;
        if ticked.is_empty() {
            terminal.message("Tick at least one operation.");
            continue;
        }
        return Ok(ticked.into_iter().map(|i| Operation::ALL[i]).collect());
    }
}

/// The summary shown before confirming. Never contains a Credential.
fn summary(mandate: &Mandate) -> String {
    let mut out = format!(
        "Summary of Mandate \"{}\"\n",
        escape_controls(&mandate.name)
    );
    out.push_str("Accounts:\n");
    for account in &mandate.accounts {
        out.push_str(&format!(
            "  {}  {}:{}  TLS {}\n",
            escape_controls(&account.login),
            escape_controls(&account.host),
            account.port,
            account.tls.as_str()
        ));
    }
    out.push_str("Grants:\n");
    for grant in &mandate.policy.grants {
        let mailboxes = if grant.operation == Operation::Draft {
            "the Drafts mailbox".to_string()
        } else {
            grant
                .mailboxes
                .iter()
                .map(|m| escape_controls(&String::from(m.clone())))
                .collect::<Vec<_>>()
                .join(", ")
        };
        out.push_str(&format!(
            "  {} on {}: {}\n",
            grant.operation.as_str(),
            escape_controls(&grant.account),
            mailboxes
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_escapes_names_and_never_shows_the_credential() {
        let mandate = Mandate {
            name: "evil\nname".to_string(),
            id: "0123abcd".to_string(),
            created: 0,
            accounts: vec![Account {
                login: "agent@example.test".to_string(),
                host: "localhost".to_string(),
                port: 993,
                tls: TlsMode::Implicit,
                credential: Credential::new("fake-app-password-123"),
            }],
            policy: Policy {
                grants: vec![Grant {
                    operation: Operation::List,
                    account: "agent@example.test".to_string(),
                    mailboxes: vec![MailboxPattern::Exact("Evil\r\n\u{1b}[31mRed".to_string())],
                }],
            },
        };
        let text = summary(&mandate);
        assert!(!text.contains("fake-app-password-123"));
        assert!(!text.contains('\u{1b}') && !text.contains('\r'));
        assert!(text.contains("evil\\u{a}name"));
        assert!(text.contains("Evil\\u{d}\\u{a}\\u{1b}[31mRed"));
        assert!(text.contains("agent@example.test  localhost:993  TLS implicit"));
    }

    #[test]
    fn gmail_addresses_are_recognised() {
        assert!(looks_like_gmail("Someone@GMail.com"));
        assert!(looks_like_gmail("someone@googlemail.com"));
        assert!(!looks_like_gmail("agent@example.test"));
        assert!(!looks_like_gmail("someone@gmail.com.evil.test"));
    }

    #[test]
    fn sandbox_indicator_detects_presence_even_when_empty() {
        let env = HashMap::from([("CLAUDECODE".to_string(), String::new())]);
        assert_eq!(sandbox_indicator(&env), Some("CLAUDECODE"));
        assert_eq!(sandbox_indicator(&HashMap::new()), None);
    }
}
