//! Agent Mail library target.
//!
//! Holds the CLI adapter, the core and its ports, so integration tests can
//! drive them in-process.

pub mod audit;
pub mod core;
pub mod create;
pub mod export;
pub mod imap;
pub mod mandate;
pub mod seal;
pub mod search;
pub mod store;
pub mod terminal;
pub mod utf7;

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

pub use crate::core::{Context, TOKEN_VAR};
use crate::core::{CoreError, MandateListing, Outcome};
use crate::search::{Date, Limit, Query, SearchResult, SearchText};
use crate::terminal::escape_controls;

/// Command-line interface of the `agent-mail` binary.
#[derive(Debug, Parser)]
#[command(name = "agent-mail", version, about, arg_required_else_help = true)]
pub struct Cli {
    /// Print human-readable text instead of JSON (for `search`, put it before the subcommand)
    #[arg(long)]
    pub text: bool,

    #[command(subcommand)]
    pub command: CliCommand,
}

/// Subcommands of the CLI.
#[derive(Debug, Subcommand)]
pub enum CliCommand {
    /// List the mailboxes this agent may access
    ///
    /// Reads the agent token from AGENT_MAIL_TOKEN. Prints JSON on stdout by
    /// default; errors go to stderr as JSON with a stable `code`.
    List {
        #[command(flatten)]
        output: TextOutput,
    },
    /// Search one mailbox this agent may search, newest first
    ///
    /// Reads the agent token from AGENT_MAIL_TOKEN. Criteria combine with
    /// AND; string criteria are case-insensitive substring matches. With no
    /// criterion every message matches. Prints one JSON object on stdout:
    /// account, mailbox, total (how many matched), more (whether more
    /// matched than are shown) and messages, each with ref, date, from,
    /// subject and flags. A ref is <account>/<mailbox>/<uidvalidity>/<uid>,
    /// with `%`, `/` and bytes outside printable ASCII in the account and
    /// mailbox percent-encoded. Here --text is the TEXT criterion; for
    /// human-readable output run `agent-mail --text search ...`.
    Search(SearchArgs),
    /// Create and manage Mandates (create is run by the human; agents may list and delete)
    Mandate {
        /// Print human-readable text instead of JSON
        #[arg(long, global = true)]
        text: bool,

        #[command(subcommand)]
        command: MandateCommand,
    },
}

/// The `--text` output flag, for subcommands that take it after their name.
#[derive(Debug, Args)]
pub struct TextOutput {
    /// Print human-readable text instead of JSON
    #[arg(long)]
    pub text: bool,
}

/// Options of `search`.
#[derive(Debug, Args)]
pub struct SearchArgs {
    /// Mailbox to search, by its name as `agent-mail list` prints it
    #[arg(long, value_name = "NAME", default_value = "INBOX", value_parser = search_text)]
    pub mailbox: SearchText,
    /// Messages whose From header contains S
    #[arg(long, value_name = "S", value_parser = search_text)]
    pub from: Option<SearchText>,
    /// Messages whose To header contains S
    #[arg(long, value_name = "S", value_parser = search_text)]
    pub to: Option<SearchText>,
    /// Messages whose Subject contains S
    #[arg(long, value_name = "S", value_parser = search_text)]
    pub subject: Option<SearchText>,
    /// Messages whose headers or body contain S
    #[arg(long, value_name = "S", value_parser = search_text)]
    pub text: Option<SearchText>,
    /// Messages received on or after this date (YYYY-MM-DD)
    #[arg(long, value_name = "YYYY-MM-DD", value_parser = date)]
    pub since: Option<Date>,
    /// Messages received before this date (YYYY-MM-DD)
    #[arg(long, value_name = "YYYY-MM-DD", value_parser = date)]
    pub before: Option<Date>,
    /// Only messages not yet seen
    #[arg(long)]
    pub unseen: bool,
    /// Only flagged messages
    #[arg(long)]
    pub flagged: bool,
    /// Return at most N messages, 1 to 1000
    #[arg(long, value_name = "N", default_value = "20", value_parser = limit)]
    pub limit: Limit,
}

fn search_text(value: &str) -> Result<SearchText, crate::search::InvalidValue> {
    SearchText::new(value)
}

fn date(value: &str) -> Result<Date, crate::search::InvalidValue> {
    Date::parse(value)
}

fn limit(value: &str) -> Result<Limit, crate::search::InvalidValue> {
    Limit::parse(value)
}

impl From<SearchArgs> for Query {
    fn from(args: SearchArgs) -> Self {
        Self {
            mailbox: args.mailbox,
            from: args.from,
            to: args.to,
            subject: args.subject,
            text: args.text,
            since: args.since,
            before: args.before,
            unseen: args.unseen,
            flagged: args.flagged,
            limit: args.limit,
        }
    }
}

/// Subcommands of `mandate`.
#[derive(Debug, Subcommand)]
pub enum MandateCommand {
    /// Create a Mandate through a guided setup in your own terminal
    ///
    /// Asks for each Account's settings and Credential with hidden input,
    /// checks a live login, lets you tick mailboxes and operations, and shows
    /// the new Agent Token once, then offers to save the Mandate's Accounts
    /// and Grants as a TOML file (never its Credentials or Agent Token).
    /// With --from, a saved file pre-fills the choices; every Credential is
    /// still typed and live-checked. Refuses to run without a terminal or
    /// inside an agent sandbox. No option can supply a Credential.
    Create {
        /// Pre-fill the setup from a Mandate file saved by an earlier `mandate create`
        #[arg(long, value_name = "FILE")]
        from: Option<PathBuf>,
    },
    /// List every stored Mandate: ID, name, Accounts, Grants, creation date
    ///
    /// Needs no agent token. Never shows a Credential or an Agent Token.
    List,
    /// Delete a Mandate by its ID, revoking its Agent Token at once
    ///
    /// Needs no agent token or Credential and asks no confirmation. Deletes
    /// every stored record with this ID. Fails with `revocation_incomplete`
    /// if the store holds records it cannot read, since one may be this
    /// Mandate.
    Delete {
        /// The Mandate's ID, as shown by `agent-mail mandate list`
        id: String,
    },
}

/// What a run printed and how it exited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutput {
    /// Everything written to stdout.
    pub stdout: String,
    /// Everything written to stderr.
    pub stderr: String,
    /// The process exit code.
    pub code: i32,
}

/// Runs the CLI with `args` (including the program name) and `env`.
pub fn run<I, T>(args: I, env: &HashMap<String, String>, context: &Context<'_>) -> RunOutput
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(err) => {
            let text = err.render().to_string();
            let (stdout, stderr) = if err.use_stderr() {
                (String::new(), text)
            } else {
                (text, String::new())
            };
            return RunOutput {
                stdout,
                stderr,
                code: err.exit_code(),
            };
        }
    };
    let mut text = cli.text;
    let command = match cli.command {
        CliCommand::List { output } => {
            text |= output.text;
            crate::core::Command::List
        }
        CliCommand::Search(args) => crate::core::Command::Search(args.into()),
        CliCommand::Mandate {
            text: mandate_text,
            command,
        } => {
            text |= mandate_text;
            match command {
                MandateCommand::Create { from } => crate::core::Command::MandateCreate { from },
                MandateCommand::List => crate::core::Command::MandateList,
                MandateCommand::Delete { id } => crate::core::Command::MandateDelete { id },
            }
        }
    };
    match crate::core::execute(env, &command, context) {
        Ok(outcome) => RunOutput {
            stdout: render_outcome(&outcome, text),
            stderr: String::new(),
            code: 0,
        },
        Err(error) => RunOutput {
            stdout: String::new(),
            stderr: render_error(&error, text),
            code: 1,
        },
    }
}

fn render_outcome(outcome: &Outcome, text: bool) -> String {
    match outcome {
        Outcome::Mailboxes(list) if text => {
            let mut out = String::new();
            out.push_str(&escape_controls(&list.account));
            out.push('\n');
            for mailbox in &list.mailboxes {
                out.push_str("  ");
                out.push_str(&escape_controls(&mailbox.name));
                out.push('\n');
            }
            out
        }
        Outcome::Mailboxes(list) => json_line(list),
        Outcome::Messages(result) if text => render_messages(result),
        Outcome::Messages(result) => json_line(result),
        Outcome::MandateCreated => String::new(),
        Outcome::Mandates(listing) if text => render_listing(listing),
        Outcome::Mandates(listing) => json_line(listing),
        Outcome::Deleted(deleted) if text => match &deleted.name {
            Some(name) => format!(
                "Deleted Mandate \"{}\" (ID {}).\n",
                escape_controls(name),
                escape_controls(&deleted.id)
            ),
            None => format!(
                "Deleted Mandate (ID {}). The stored records with this ID had different names, so none is shown.\n",
                escape_controls(&deleted.id)
            ),
        },
        Outcome::Deleted(deleted) => json_line(&serde_json::json!({ "deleted": deleted })),
    }
}

fn json_line(value: &impl serde::Serialize) -> String {
    let mut out = serde_json::to_string(value).expect("results always serialise");
    out.push('\n');
    out
}

fn render_messages(result: &SearchResult) -> String {
    let mut out = format!(
        "{} {}: showing {} of {} messages\n",
        escape_controls(&result.account),
        escape_controls(&result.mailbox),
        result.messages.len(),
        result.total
    );
    for m in &result.messages {
        let flags: Vec<String> = m.flags.iter().map(|f| escape_controls(f)).collect();
        out.push_str(&format!(
            "{}  {}  {}  [{}]  {}\n",
            escape_controls(&m.date),
            escape_controls(&m.from),
            escape_controls(&m.subject),
            flags.join(" "),
            escape_controls(&m.reference)
        ));
    }
    if result.more {
        out.push_str("more messages match; narrow the search or raise --limit\n");
    }
    out
}

fn render_listing(listing: &MandateListing) -> String {
    let mut out = String::new();
    if listing.mandates.is_empty() {
        out.push_str("No Mandates stored.\n");
    }
    for m in &listing.mandates {
        out.push_str(&format!(
            "{}  {}  created {}\n",
            escape_controls(&m.id),
            escape_controls(&m.name),
            escape_controls(&m.created)
        ));
        for a in &m.accounts {
            out.push_str(&format!(
                "  account {} ({}:{}, {})\n",
                escape_controls(&a.login),
                escape_controls(&a.host),
                a.port,
                a.tls.as_str()
            ));
        }
        for g in &m.grants {
            let mailboxes: Vec<String> = g
                .mailboxes
                .iter()
                .map(|p| escape_controls(&String::from(p.clone())))
                .collect();
            out.push_str(&format!(
                "  grant {} on {}: {}\n",
                g.operation.as_str(),
                escape_controls(&g.account),
                mailboxes.join(", ")
            ));
        }
    }
    match listing.unreadable {
        0 => {}
        1 => out.push_str("1 stored record could not be read and is not listed.\n"),
        n => out.push_str(&format!(
            "{n} stored records could not be read and are not listed.\n"
        )),
    }
    out
}

fn render_error(error: &CoreError, text: bool) -> String {
    let body = error.body();
    if text {
        // The message may echo untrusted input, such as a Mandate ID.
        return format!("error: {}: {}\n", body.code, escape_controls(&body.message));
    }
    let mut out = serde_json::to_string(&serde_json::json!({ "error": body }))
        .expect("errors always serialise");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{MailboxEntry, MailboxList};
    use crate::search::MessageSummary;

    #[test]
    fn text_output_escapes_control_characters_from_the_server() {
        let outcome = Outcome::Mailboxes(MailboxList {
            account: "agent@example.test".to_string(),
            mailboxes: vec![MailboxEntry {
                name: "Evil\nother@example.test\r\u{1b}[31mRed Entwürfe".to_string(),
            }],
        });
        let text = render_outcome(&outcome, true);
        assert_eq!(
            text,
            "agent@example.test\n  Evil\\u{a}other@example.test\\u{d}\\u{1b}[31mRed Entwürfe\n"
        );
        assert_eq!(text.lines().count(), 2);
    }

    #[test]
    fn search_text_output_keeps_each_message_on_one_escaped_line() {
        let outcome = Outcome::Messages(SearchResult {
            account: "agent@example.test".to_string(),
            mailbox: "INBOX".to_string(),
            total: 3,
            more: true,
            messages: vec![MessageSummary {
                reference: "agent@example.test/INBOX/1700000001/3".to_string(),
                date: "2026-10-10T09:00:00Z".to_string(),
                from: "Eve\r <eve@example.com>".to_string(),
                subject: "Hi\n\u{1b}[31mred".to_string(),
                flags: vec!["\\Seen".to_string(), "\\Flagged".to_string()],
            }],
        });
        let text = render_outcome(&outcome, true);
        assert_eq!(
            text,
            "agent@example.test INBOX: showing 1 of 3 messages\n\
             2026-10-10T09:00:00Z  Eve\\u{d} <eve@example.com>  Hi\\u{a}\\u{1b}[31mred  [\\Seen \\Flagged]  agent@example.test/INBOX/1700000001/3\n\
             more messages match; narrow the search or raise --limit\n"
        );
        assert_eq!(text.lines().count(), 3);
    }

    #[test]
    fn search_options_refuse_line_breaks_bad_dates_and_limits() {
        for args in [
            &["search", "--subject", "a\nb"][..],
            &["search", "--from", "a\rb"],
            &["search", "--mailbox", "IN\0BOX"],
            &["search", "--text", "x\ny"],
            &["search", "--to", "x\r\n"],
            &["search", "--since", "2026-13-01"],
            &["search", "--before", "10/05/2026"],
            &["search", "--limit", "0"],
            &["search", "--limit", "1001"],
            &["search", "--limit", "x"],
        ] {
            let argv = std::iter::once("agent-mail").chain(args.iter().copied());
            let err = Cli::try_parse_from(argv).expect_err(&format!("{args:?} parsed"));
            assert_eq!(err.exit_code(), 2, "{args:?}");
        }
    }
}
