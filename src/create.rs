//! `mandate create`: the guided setup the human runs in their own terminal.
//!
//! Flow: name, then per Account the Gmail preset or generic server settings,
//! the hidden Credential and a live login (retry or abort on failure), the
//! mailbox and operation checklists; then "add another Account?", a summary,
//! confirmation, sealing and storing, and the Agent Token shown once.
//!
//! Credentials come only from the Terminal's hidden input: no flag, no
//! environment variable, no pipe. The refusal checks run before any prompt
//! and before any store access.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::{Context, CoreError};
use crate::imap::{self, ImapError};
use crate::mandate::{
    Account, Credential, Grant, MailboxPattern, Mandate, Operation, Policy, TlsMode,
};
use crate::seal::{self, AgentToken};
use crate::terminal::{Cancelled, Terminal, escape_controls};
use crate::utf7;

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
/// Shown whenever the flow ends without storing a Mandate.
pub const NOTHING_STORED: &str = "Nothing was stored.";

const GMAIL_HOST: &str = "imap.gmail.com";
const GMAIL_PORT: u16 = 993;

/// Runs the guided setup and stores the Mandate.
pub(crate) fn run(env: &HashMap<String, String>, context: &Context<'_>) -> Result<(), CoreError> {
    let terminal = context.terminal;
    if !terminal.is_interactive() {
        return Err(CoreError::InteractiveTerminalRequired);
    }
    if sandbox_indicator(env).is_some() {
        return Err(CoreError::SandboxDetected);
    }
    let mandate = match guided(context) {
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
    terminal.message(&format!(
        "Mandate \"{}\" (ID {}) is sealed and stored.\n\n\
         Agent Token, shown only this once (it cannot be shown again):\n\n    {}\n\n\
         Give it to the Agent in the AGENT_MAIL_TOKEN environment variable.",
        escape_controls(&mandate.name),
        mandate.id,
        token.expose()
    ));
    Ok(())
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

fn guided(context: &Context<'_>) -> Result<Mandate, CoreError> {
    let terminal = context.terminal;
    let name = ask_non_empty(terminal, "Mandate name:", None)?;
    let mut accounts: Vec<Account> = Vec::new();
    let mut grants: Vec<Grant> = Vec::new();
    loop {
        let (account, mailboxes) = add_account(context, &accounts)?;
        let login = account.login.clone();
        let ticked = ask_mailboxes(terminal, &login, &mailboxes)?;
        let operations = ask_operations(terminal, &login)?;
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
        if !terminal
            .confirm("Add another Account?", false)
            .map_err(cancelled)?
        {
            break;
        }
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
    existing: &[Account],
) -> Result<String, CoreError> {
    loop {
        let login = ask_non_empty(terminal, prompt, None)?;
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

fn ask_port(terminal: &dyn Terminal) -> Result<u16, CoreError> {
    loop {
        let text = ask_non_empty(terminal, "IMAP server port:", Some("993"))?;
        match text.parse::<u16>() {
            Ok(port) if port != 0 => return Ok(port),
            _ => terminal.message("The port must be a number from 1 to 65535."),
        }
    }
}

/// Asks for one Account's settings and Credential until the live login
/// succeeds. Returns the Account and its server's raw mailbox names.
fn add_account(
    context: &Context<'_>,
    existing: &[Account],
) -> Result<(Account, Vec<String>), CoreError> {
    let terminal = context.terminal;
    let kinds = [GMAIL.to_string(), GENERIC.to_string()];
    let gmail = terminal
        .choose("Account type:", &kinds)
        .map_err(cancelled)?
        == 0;
    let (host, port, tls, login, credential_prompt) = if gmail {
        let login = ask_login(terminal, "Gmail address:", existing)?;
        (
            GMAIL_HOST.to_string(),
            GMAIL_PORT,
            TlsMode::Implicit,
            login,
            "Gmail app password",
        )
    } else {
        let host = ask_non_empty(terminal, "IMAP server host:", None)?;
        let port = ask_port(terminal)?;
        let modes = [TLS_IMPLICIT.to_string(), TLS_STARTTLS.to_string()];
        let tls = match terminal.choose("TLS mode:", &modes).map_err(cancelled)? {
            0 => TlsMode::Implicit,
            _ => TlsMode::Starttls,
        };
        let login = ask_login(terminal, "Login name:", existing)?;
        (host, port, tls, login, "Credential (password)")
    };
    let shown = escape_controls(&login);
    loop {
        let credential: Credential = terminal
            .hidden(&format!("{credential_prompt} for {shown}:"))
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
                match terminal.choose("What now?", &choices).map_err(cancelled)? {
                    0 => continue,
                    _ => return Err(CoreError::Aborted),
                }
            }
        }
    }
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
        ImapError::Protocol => "the server answered unexpectedly; check the server settings",
    }
}

fn ask_mailboxes(
    terminal: &dyn Terminal,
    login: &str,
    raw: &[String],
) -> Result<Vec<String>, CoreError> {
    // Malformed modified UTF-7 falls back to the raw name, as `list` does.
    let names: Vec<String> = raw
        .iter()
        .map(|name| utf7::decode(name).unwrap_or_else(|_| name.clone()))
        .collect();
    let labels: Vec<String> = names.iter().map(|n| escape_controls(n)).collect();
    let prompt = format!("Mailboxes the Agent may use on {}:", escape_controls(login));
    loop {
        let ticked = terminal.checklist(&prompt, &labels).map_err(cancelled)?;
        if ticked.is_empty() && !names.is_empty() {
            terminal.message("Tick at least one mailbox.");
            continue;
        }
        return Ok(ticked.into_iter().map(|i| names[i].clone()).collect());
    }
}

fn ask_operations(terminal: &dyn Terminal, login: &str) -> Result<Vec<Operation>, CoreError> {
    let labels: Vec<String> = Operation::ALL
        .iter()
        .map(|op| op.as_str().to_string())
        .collect();
    let prompt = format!(
        "Operations the Agent may perform on {}:",
        escape_controls(login)
    );
    loop {
        let ticked = terminal.checklist(&prompt, &labels).map_err(cancelled)?;
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
    fn sandbox_indicator_detects_presence_even_when_empty() {
        let env = HashMap::from([("CLAUDECODE".to_string(), String::new())]);
        assert_eq!(sandbox_indicator(&env), Some("CLAUDECODE"));
        assert_eq!(sandbox_indicator(&HashMap::new()), None);
    }
}
