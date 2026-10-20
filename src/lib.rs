//! Agent Mail library target.
//!
//! Holds the CLI adapter, the core and its ports, so integration tests can
//! drive them in-process.

pub mod core;
pub mod imap;
pub mod mandate;
pub mod seal;
pub mod store;
pub mod utf7;

use std::collections::HashMap;
use std::ffi::OsString;

use clap::{Parser, Subcommand};

pub use crate::core::Context;
use crate::core::{CoreError, Outcome};

/// Environment variable holding the presented Agent Token.
pub const TOKEN_VAR: &str = "AGENT_MAIL_TOKEN";

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
    };
    let token = env.get(TOKEN_VAR).map(String::as_str);
    match crate::core::execute(token, &command, context) {
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
        Outcome::Mailboxes(list) => {
            let mut out = serde_json::to_string(list).expect("results always serialise");
            out.push('\n');
            out
        }
    }
}

/// Escapes control characters (newlines, ANSI escapes) in server-supplied
/// text, so a hostile server cannot forge lines or styling under `--text`.
fn escape_controls(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() {
                c.escape_unicode().to_string()
            } else {
                c.to_string()
            }
        })
        .collect()
}

fn render_error(error: &CoreError, text: bool) -> String {
    let body = error.body();
    if text {
        return format!("error: {}: {}\n", body.code, body.message);
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
