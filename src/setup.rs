//! `agent-mail setup`: the setup wizard the human runs in their own
//! terminal, also started by a bare `agent-mail` before any Mandate exists.
//!
//! Flow: the `mandate create` guided flow (refusals, name, Provider,
//! Credential, live login, Grants, summary, store, export offer), then
//! "Connect it where?" in a loop with "Connect it somewhere else too?",
//! then the final check: the new token opened as the MCP server would,
//! with its `whoami --text` lines and one next step per destination.
//!
//! The Agent Token goes only to [`Terminal::reveal`], at most once. Once
//! the Mandate is stored, a cancel or a failure first shows the token with
//! the snippets (if not shown yet), so the stored Mandate is never left
//! without a token anyone saw; a cancel then offers to delete it.

use std::collections::HashMap;

use serde::Serialize;
use zeroize::{Zeroize, Zeroizing};

use crate::core::{Command, Context, CoreError, Outcome, TOKEN_VAR};
use crate::create::{self, Stored};

/// The destination prompt.
pub const CONNECT_WHERE: &str = "Connect it where?";
/// Destination choice: show the token and snippets for the human to paste.
pub const CONFIGURE_MYSELF: &str = "I'll configure it myself";
/// Asked after each destination; default No.
pub const CONNECT_ELSEWHERE: &str = "Connect it somewhere else too?";
/// Offered after a cancel once the Mandate is stored; default No.
pub const DELETE_OFFER: &str = "Delete this Mandate? Its Agent Token stops working at once.";
/// Shown when the token and snippets were already revealed.
pub const SHOWN_ABOVE: &str = "The Agent Token and the snippets are shown above.";
/// The next step after "I'll configure it myself".
pub const NEXT_CONFIGURE_MYSELF: &str = "Paste the snippet into your Agent App's config, restart it, then ask it to list your mailboxes.";
/// Shown when a cancel keeps the Mandate.
pub const KEPT: &str = "The Mandate is kept. Connect it with a snippet shown above.";
/// The Claude Code settings file the `env` snippet goes in.
pub const CLAUDE_CODE_FILE: &str = ".claude/settings.local.json";
/// The Claude Desktop config file the `mcpServers` snippet goes in.
pub const CLAUDE_DESKTOP_FILE: &str = "claude_desktop_config.json";

/// Where the human connects the new Mandate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Destination {
    /// Show the token and snippets; the human pastes them.
    ConfigureMyself,
}

impl Destination {
    /// Every destination, in the order "Connect it where?" offers them.
    const ALL: [Self; 1] = [Self::ConfigureMyself];

    fn label(self) -> &'static str {
        match self {
            Self::ConfigureMyself => CONFIGURE_MYSELF,
        }
    }

    fn next_step(self) -> &'static str {
        match self {
            Self::ConfigureMyself => NEXT_CONFIGURE_MYSELF,
        }
    }
}

/// Whether a bare `agent-mail` starts the wizard: not on the Broker route,
/// on a terminal, outside any Agent session, and with a store that lists
/// no Mandate (and no unreadable record). The cheap checks run first, so
/// the store is touched only on a human's terminal.
pub fn starts_on_bare_call(env: &HashMap<String, String>, context: &Context<'_>) -> bool {
    context.broker.is_none()
        && context.terminal.is_interactive()
        && create::sandbox_indicator(env).is_none()
        && context.store.check().is_ok()
        && matches!(
            crate::core::execute(env, &Command::MandateList, context),
            Ok(Outcome::Mandates(listing))
                if listing.mandates.is_empty() && listing.unreadable == 0
        )
}

/// The setup-specific message for `error`, where the `mandate create` one
/// would point the human at the wrong command.
pub fn error_message(error: &CoreError) -> Option<&'static str> {
    Some(match error {
        CoreError::InteractiveTerminalRequired => {
            "setup must be run by a human in a terminal, so nothing was stored; ask the human to run `agent-mail setup` in their own terminal"
        }
        CoreError::SandboxDetected => {
            "setup cannot run inside an agent sandbox or session, so nothing was stored; ask the human to run `agent-mail setup` in their own terminal"
        }
        CoreError::Aborted => {
            "setup was aborted and no Mandate was kept; run `agent-mail setup` again to start over"
        }
        // Only `setup` on the Broker route ends with this error.
        CoreError::BrokerUnsupported => {
            "setup is not available on Windows yet; create a Mandate with `agent-mail mandate create` in an elevated terminal (Run as administrator)"
        }
        _ => return None,
    })
}

/// Runs the wizard in-process. On the Broker route the caller refuses
/// before calling this, so the local store is never used there.
pub(crate) fn run(env: &HashMap<String, String>, context: &Context<'_>) -> Result<(), CoreError> {
    let stored = create::create_and_store(env, None, context)?;
    // Cancelling the export offer counts as No, as in `mandate create`.
    create::offer_export(context.terminal, &stored.mandate);
    Wizard {
        context,
        stored: &stored,
        revealed: false,
    }
    .connect()
}

/// The wizard after the Mandate is stored.
struct Wizard<'w, 'c> {
    context: &'w Context<'c>,
    stored: &'w Stored,
    /// Whether the token and snippets were shown.
    revealed: bool,
}

impl Wizard<'_, '_> {
    /// The destination loop, then the final check.
    fn connect(&mut self) -> Result<(), CoreError> {
        let terminal = self.context.terminal;
        let labels: Vec<String> = Destination::ALL
            .iter()
            .map(|d| d.label().to_string())
            .collect();
        let mut chosen: Vec<Destination> = Vec::new();
        loop {
            let Ok(index) = terminal.choose(CONNECT_WHERE, &labels, 0) else {
                return self.cancelled();
            };
            let destination = Destination::ALL[index];
            match destination {
                Destination::ConfigureMyself if self.revealed => terminal.message(SHOWN_ABOVE),
                Destination::ConfigureMyself => self.reveal()?,
            }
            if !chosen.contains(&destination) {
                chosen.push(destination);
            }
            match terminal.confirm(CONNECT_ELSEWHERE, false) {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => return self.cancelled(),
            }
        }
        self.final_check(&chosen)
    }

    /// Shows the token with both snippets, unless already shown. If it
    /// cannot be shown, the Mandate is taken back out of the store.
    fn reveal(&mut self) -> Result<(), CoreError> {
        if self.revealed {
            return Ok(());
        }
        let exe = self.context.machine().current_exe();
        let command = exe
            .as_deref()
            .map_or_else(|| "agent-mail".into(), |p| p.to_string_lossy());
        let text = reveal_text(self.stored, &command);
        if self.context.terminal.reveal(&text).is_err() {
            create::take_back(self.context, &self.stored.key);
            return Err(CoreError::Aborted);
        }
        self.revealed = true;
        Ok(())
    }

    /// A cancel after the Mandate is stored: the token is shown if it was
    /// not, then the delete offer (default No; a cancel there is No).
    fn cancelled(&mut self) -> Result<(), CoreError> {
        self.reveal()?;
        let terminal = self.context.terminal;
        if !terminal.confirm(DELETE_OFFER, false).unwrap_or(false) {
            terminal.message(KEPT);
            return Ok(());
        }
        let mandate = &self.stored.mandate;
        match self.context.store.delete(&self.stored.key) {
            Ok(_) => {
                terminal.message(&format!(
                    "Deleted the Mandate (ID {}); its Agent Token no longer works.",
                    mandate.id
                ));
                Err(CoreError::Aborted)
            }
            Err(error) => {
                terminal.message(&format!(
                    "The Mandate could not be deleted; run `agent-mail mandate delete {}`.",
                    mandate.id
                ));
                Err(error.into())
            }
        }
    }

    /// Opens the new Mandate with its token as the MCP server would, shows
    /// its `whoami --text` lines, then one next step per destination.
    fn final_check(&mut self, chosen: &[Destination]) -> Result<(), CoreError> {
        let mut env = HashMap::from([(
            TOKEN_VAR.to_string(),
            self.stored.token.expose().to_string(),
        )]);
        let result = crate::core::execute(&env, &Command::Whoami, self.context);
        if let Some(mut token) = env.remove(TOKEN_VAR) {
            token.zeroize();
        }
        let failure = match result {
            Ok(Outcome::Whoami(whoami))
                if whoami.mandates.len() == 1
                    && whoami.mandates[0].id == self.stored.mandate.id =>
            {
                let terminal = self.context.terminal;
                terminal.message(&crate::render_whoami(&whoami));
                for destination in chosen {
                    terminal.message(destination.next_step());
                }
                return Ok(());
            }
            Ok(_) => CoreError::InvalidToken,
            Err(error) => error,
        };
        self.reveal()?;
        self.context.terminal.message(
            "Final check failed: the new Agent Token did not open its Mandate. Check it with `agent-mail mandate list`.",
        );
        Err(failure)
    }
}

/// The `env` value both snippets carry.
#[derive(Serialize)]
struct TokenEnv<'a> {
    #[serde(rename = "AGENT_MAIL_TOKEN")]
    token: &'a str,
}

/// The Claude Code snippet for `.claude/settings.local.json`.
#[derive(Serialize)]
struct ClaudeCodeSettings<'a> {
    env: TokenEnv<'a>,
}

/// The Claude Desktop snippet for `claude_desktop_config.json`.
#[derive(Serialize)]
struct ClaudeDesktopConfig<'a> {
    #[serde(rename = "mcpServers")]
    servers: DesktopServers<'a>,
}

#[derive(Serialize)]
struct DesktopServers<'a> {
    #[serde(rename = "agent-mail")]
    agent_mail: DesktopServer<'a>,
}

#[derive(Serialize)]
struct DesktopServer<'a> {
    command: &'a str,
    args: [&'a str; 1],
    env: TokenEnv<'a>,
}

/// `value` as pretty JSON, in a buffer wiped on drop. The token is
/// borrowed, never copied into a value tree; the buffer is sized up front
/// so it is not reallocated with the token in it.
fn pretty_json(value: &impl Serialize, room: usize) -> Zeroizing<String> {
    let mut buffer = Zeroizing::new(Vec::with_capacity(room));
    serde_json::to_writer_pretty(&mut *buffer, value).expect("snippets always serialise");
    Zeroizing::new(String::from_utf8(std::mem::take(&mut *buffer)).expect("JSON is UTF-8"))
}

/// The one reveal: the token, then the Claude Code and Claude Desktop
/// snippets. `command` is the binary's absolute path, JSON-escaped by
/// `serde_json`.
fn reveal_text(stored: &Stored, command: &str) -> Zeroizing<String> {
    let token = stored.token.expose();
    let env = || TokenEnv { token };
    let room = 512 + 6 * command.len();
    let code = pretty_json(&ClaudeCodeSettings { env: env() }, room);
    let desktop = pretty_json(
        &ClaudeDesktopConfig {
            servers: DesktopServers {
                agent_mail: DesktopServer {
                    command,
                    args: ["mcp"],
                    env: env(),
                },
            },
        },
        room,
    );
    Zeroizing::new(format!(
        "{}\n\n\
         Agent Token, shown only this once (it cannot be shown again):\n\n    {token}\n\n\
         Claude Code: put this in {CLAUDE_CODE_FILE} in your project folder:\n\n{}\n\n\
         Claude Desktop: put this in {CLAUDE_DESKTOP_FILE}, next to any servers already there:\n\n{}",
        create::stored_line(&stored.mandate),
        code.as_str(),
        desktop.as_str()
    ))
}
