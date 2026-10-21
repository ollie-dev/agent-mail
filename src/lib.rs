//! Agent Mail library target.
//!
//! Holds the CLI adapter, the core and its ports, so integration tests can
//! drive them in-process.

pub mod core;
pub mod create;
pub mod imap;
pub mod mandate;
pub mod seal;
pub mod store;
pub mod terminal;
pub mod utf7;

use std::collections::HashMap;
use std::ffi::OsString;

use clap::{Parser, Subcommand};

pub use crate::core::{Context, TOKEN_VAR};
use crate::core::{CoreError, MandateListing, Outcome};
use crate::terminal::escape_controls;

/// Command-line interface of the `agent-mail` binary.
#[derive(Debug, Parser)]
#[command(name = "agent-mail", version, about, arg_required_else_help = true)]
pub struct Cli {
    /// Print human-readable text instead of JSON
    #[arg(long, global = true)]
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
    List,
    /// Create and manage Mandates (run by the human, not by agents)
    Mandate {
        #[command(subcommand)]
        command: MandateCommand,
    },
}

/// Subcommands of `mandate`.
#[derive(Debug, Subcommand)]
pub enum MandateCommand {
    /// Create a Mandate through a guided setup in your own terminal
    ///
    /// Asks for each Account's settings and Credential with hidden input,
    /// checks a live login, lets you tick mailboxes and operations, and shows
    /// the new Agent Token once. Refuses to run without a terminal or inside
    /// an agent sandbox. No option can supply a Credential.
    Create,
    /// List every stored Mandate: ID, name, Accounts, Grants, creation date
    ///
    /// Needs no agent token. Never shows a Credential or an Agent Token.
    List,
    /// Delete a Mandate by its ID, revoking its Agent Token at once
    ///
    /// Needs no agent token or Credential and asks no confirmation.
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
    let command = match cli.command {
        CliCommand::List => crate::core::Command::List,
        CliCommand::Mandate {
            command: MandateCommand::Create,
        } => crate::core::Command::MandateCreate,
        CliCommand::Mandate {
            command: MandateCommand::List,
        } => crate::core::Command::MandateList,
        CliCommand::Mandate {
            command: MandateCommand::Delete { id },
        } => crate::core::Command::MandateDelete { id },
    };
    match crate::core::execute(env, &command, context) {
        Ok(outcome) => RunOutput {
            stdout: render_outcome(&outcome, cli.text),
            stderr: String::new(),
            code: 0,
        },
        Err(error) => RunOutput {
            stdout: String::new(),
            stderr: render_error(&error, cli.text),
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
        Outcome::MandateCreated => String::new(),
        Outcome::Mandates(listing) if text => render_listing(listing),
        Outcome::Mandates(listing) => json_line(listing),
        Outcome::Deleted(deleted) if text => format!(
            "Deleted Mandate \"{}\" (ID {}).\n",
            escape_controls(&deleted.name),
            escape_controls(&deleted.id)
        ),
        Outcome::Deleted(deleted) => json_line(&serde_json::json!({ "deleted": deleted })),
    }
}

fn json_line(value: &impl serde::Serialize) -> String {
    let mut out = serde_json::to_string(value).expect("results always serialise");
    out.push('\n');
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
}
