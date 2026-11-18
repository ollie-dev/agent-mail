//! `agent-mail setup`: the setup wizard the human runs in their own
//! terminal, also started by a bare `agent-mail` before any Mandate exists.
//!
//! Flow: the `mandate create` guided flow (refusals, name, Provider,
//! Credential, live login, Grants, summary, store, export offer), then
//! "Connect it where?" in a loop with "Connect it somewhere else too?",
//! then the final check: the new token opened as the MCP server would,
//! with its `whoami --text` lines and one next step per destination.
//!
//! Destinations: "A Claude Code project" writes the token into the
//! project's `.claude/settings.local.json` under `env.AGENT_MAIL_TOKEN`,
//! after checking git cannot pick the file up; "I'll configure it myself"
//! shows the token and the snippets. A Claude Code project that cannot be
//! written safely falls back to "I'll configure it myself".
//!
//! The Agent Token goes only to [`Terminal::reveal`] or into a project's
//! settings file, and is revealed at most once. Once the Mandate is
//! stored, a cancel or a failure first shows the token with the snippets
//! (if not shown yet), so the stored Mandate is never left without a token
//! anyone saw; a cancel then offers to delete it. Tokens already in a
//! settings file are never shown: they are only parsed to find the
//! Mandates they open.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::value::RawValue;
use zeroize::{Zeroize, Zeroizing};

use crate::core::{Command, Context, CoreError, Outcome, TOKEN_VAR};
use crate::create::{self, Stored};
use crate::machine::{Machine, RunOutcome};
use crate::seal::{self, AgentToken};
use crate::terminal::escape_controls;

/// The destination prompt.
pub const CONNECT_WHERE: &str = "Connect it where?";
/// Destination choice: write the token into a Claude Code project.
pub const CLAUDE_CODE_PROJECT: &str = "A Claude Code project";
/// The project folder prompt; the default is the current folder.
pub const PROJECT_FOLDER: &str = "Claude Code project folder:";
/// Asked when the project's settings already hold Agent Tokens.
pub const EXISTING_TOKENS: &str = "This project already has an Agent Token.";
/// Existing-token choice: keep the old tokens and add the new one.
pub const ADD_TOKEN: &str = "Add the new token";
/// Existing-token choice: put the new token in place of the old ones.
pub const REPLACE_TOKENS: &str = "Replace the old token";
/// Ends each offer to delete a Mandate an old token opened; default No.
pub const DELETE_OLD_WARNING: &str = "Other Agent Apps holding its token stop working.";
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
    /// Write the token into a Claude Code project's settings file.
    ClaudeCode,
    /// Show the token and snippets; the human pastes them.
    ConfigureMyself,
}

impl Destination {
    /// Every destination, in the order "Connect it where?" offers them.
    const ALL: [Self; 2] = [Self::ClaudeCode, Self::ConfigureMyself];

    fn label(self) -> &'static str {
        match self {
            Self::ClaudeCode => CLAUDE_CODE_PROJECT,
            Self::ConfigureMyself => CONFIGURE_MYSELF,
        }
    }
}

/// Where the token ended up for one chosen destination.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Connected {
    /// Shown with the snippets, chosen or as a fallback.
    Myself,
    /// Written into `file`, the settings file of the project `folder`.
    ClaudeCode { folder: PathBuf, file: PathBuf },
}

impl Connected {
    fn next_step(&self) -> String {
        match self {
            Self::Myself => NEXT_CONFIGURE_MYSELF.to_string(),
            Self::ClaudeCode { folder, .. } => format!(
                "Restart Claude Code in {}, then ask it to list your mailboxes.",
                escape_controls(&folder.display().to_string())
            ),
        }
    }
}

/// Why the token was not written into a Claude Code project.
enum NotWritten {
    /// The human cancelled a prompt: the cancel path runs.
    Cancelled,
    /// A git risk or an unusable file, already reported: fall back to
    /// "I'll configure it myself".
    FallBack,
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
        let mut chosen: Vec<Connected> = Vec::new();
        loop {
            let Ok(index) = terminal.choose(CONNECT_WHERE, &labels, 0) else {
                return self.cancelled();
            };
            let connected = match Destination::ALL[index] {
                Destination::ConfigureMyself => {
                    self.show_myself()?;
                    Connected::Myself
                }
                Destination::ClaudeCode => match self.claude_code() {
                    Ok(connected) => connected,
                    Err(NotWritten::Cancelled) => return self.cancelled(),
                    Err(NotWritten::FallBack) => {
                        self.show_myself()?;
                        Connected::Myself
                    }
                },
            };
            if !chosen.contains(&connected) {
                chosen.push(connected);
            }
            match terminal.confirm(CONNECT_ELSEWHERE, false) {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => return self.cancelled(),
            }
        }
        self.final_check(&chosen)
    }

    /// "I'll configure it myself": the token and snippets, or a pointer
    /// to them when they were already shown.
    fn show_myself(&mut self) -> Result<(), CoreError> {
        if self.revealed {
            self.context.terminal.message(SHOWN_ABOVE);
            Ok(())
        } else {
            self.reveal()
        }
    }

    /// "A Claude Code project": the folder, the git-risk check, the
    /// existing tokens, the write, then on Replace the delete offers.
    fn claude_code(&mut self) -> Result<Connected, NotWritten> {
        let terminal = self.context.terminal;
        let machine = self.context.machine();
        let folder = self.project_folder()?;
        let file = folder.join(".claude").join("settings.local.json");
        if let Some(risk) = git_risk(machine, &folder) {
            terminal.message(&risk.warning(&folder));
            return Err(NotWritten::FallBack);
        }
        let shown = escape_controls(&file.display().to_string());
        let mut settings = match Settings::load(&file) {
            Ok(settings) => settings,
            Err(problem) => {
                terminal.message(&problem.message(&shown));
                return Err(NotWritten::FallBack);
            }
        };
        let new = self.stored.token.expose();
        let old = settings.tokens();
        let replace = if !old.is_empty() {
            let options = [ADD_TOKEN.to_string(), REPLACE_TOKENS.to_string()];
            match terminal.choose(EXISTING_TOKENS, &options, 0) {
                Ok(index) => index == 1,
                Err(_) => return Err(NotWritten::Cancelled),
            }
        } else {
            false
        };
        let value = if replace || old.is_empty() {
            Zeroizing::new(new.to_string())
        } else if old.iter().any(|t| t.as_str() == new) {
            Zeroizing::new(old.iter().map(|t| t.as_str()).collect::<Vec<_>>().join(","))
        } else {
            let mut all: Vec<&str> = old.iter().map(|t| t.as_str()).collect();
            all.push(new);
            Zeroizing::new(all.join(","))
        };
        settings.set_token(&value);
        if write_settings(&file, &settings.to_bytes(), settings.existed).is_err() {
            terminal.message(&format!(
                "Could not write {shown}, so the token was not written there."
            ));
            return Err(NotWritten::FallBack);
        }
        if replace {
            self.offer_old_deletes(&old);
        }
        Ok(Connected::ClaudeCode { folder, file })
    }

    /// Asks for the project folder until it names an existing folder. A
    /// relative path is taken from the current folder.
    fn project_folder(&self) -> Result<PathBuf, NotWritten> {
        let terminal = self.context.terminal;
        let current = self.context.machine().current_dir();
        let default = current
            .as_ref()
            .map(|dir| dir.to_string_lossy().into_owned());
        loop {
            let answer = terminal
                .text(PROJECT_FOLDER, default.as_deref())
                .map_err(|_| NotWritten::Cancelled)?;
            let typed = answer.trim();
            let path = Path::new(typed);
            let resolved = if path.is_absolute() {
                Some(path.to_path_buf())
            } else {
                current.as_ref().map(|dir| dir.join(path))
            };
            match resolved {
                Some(folder) if !typed.is_empty() && folder.is_dir() => return Ok(folder),
                _ => terminal.message(&format!("No such folder: {}", escape_controls(typed))),
            }
        }
    }

    /// One offer per stored Mandate an old token opens (default No).
    /// Malformed entries and tokens that open nothing get no offer.
    fn offer_old_deletes(&self, old: &[Zeroizing<String>]) {
        let terminal = self.context.terminal;
        let mut offered: Vec<String> = Vec::new();
        for text in old {
            let Ok(token) = AgentToken::parse(text.trim()) else {
                continue;
            };
            if token == self.stored.token {
                continue;
            }
            let Ok(Some(record)) = self.context.store.get(&token.store_key()) else {
                continue;
            };
            let Ok(mandate) = seal::open(&token, &record) else {
                continue;
            };
            // Deleting is by ID: never one the new Mandate shares.
            if mandate.id == self.stored.mandate.id || offered.contains(&mandate.id) {
                continue;
            }
            offered.push(mandate.id.clone());
            let question = format!(
                "Delete the old Mandate \"{}\" (ID {})? {DELETE_OLD_WARNING}",
                escape_controls(&mandate.name),
                mandate.id
            );
            if !terminal.confirm(&question, false).unwrap_or(false) {
                continue;
            }
            let command = Command::MandateDelete {
                id: mandate.id.clone(),
            };
            match crate::core::execute(&HashMap::new(), &command, self.context) {
                Ok(_) => terminal.message(&format!(
                    "Deleted the old Mandate (ID {}); its Agent Token no longer works.",
                    mandate.id
                )),
                Err(_) => terminal.message(&format!(
                    "The old Mandate could not be deleted; run `agent-mail mandate delete {}`.",
                    mandate.id
                )),
            }
        }
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

    /// Opens the new Mandate as the MCP server would: with its token when
    /// it was shown, and with the value read back from each written
    /// settings file. Shows the `whoami --text` lines of each check, then
    /// one next step per destination.
    fn final_check(&mut self, chosen: &[Connected]) -> Result<(), CoreError> {
        let mut lines = Vec::new();
        let mut failure = None;
        if chosen.contains(&Connected::Myself) {
            let value = Zeroizing::new(self.stored.token.expose().to_string());
            match self.check(&value, true) {
                Ok(text) => lines.push(text),
                Err(error) => failure = Some(error),
            }
        }
        for connected in chosen {
            if failure.is_some() {
                break;
            }
            let Connected::ClaudeCode { file, .. } = connected else {
                continue;
            };
            #[cfg(any(test, feature = "test-support"))]
            hook::before_file_check(file);
            let result = match read_back(file) {
                Some(value) => self.check(&value, false),
                None => Err(CoreError::InvalidToken),
            };
            match result {
                Ok(text) => lines.push(text),
                Err(error) => failure = Some(error),
            }
        }
        let Some(failure) = failure else {
            let terminal = self.context.terminal;
            for text in &lines {
                terminal.message(text);
            }
            for connected in chosen {
                terminal.message(&connected.next_step());
            }
            return Ok(());
        };
        self.reveal()?;
        self.context.terminal.message(
            "Final check failed: the new Agent Token did not open its Mandate. Check it with `agent-mail mandate list`.",
        );
        Err(failure)
    }

    /// Runs `whoami` with `value` in `AGENT_MAIL_TOKEN`. Passes when the
    /// new Mandate is opened: as the only one when `only`, else among the
    /// others. Gives the `whoami --text` lines.
    fn check(&self, value: &str, only: bool) -> Result<String, CoreError> {
        let mut env = HashMap::from([(TOKEN_VAR.to_string(), value.to_string())]);
        let result = crate::core::execute(&env, &Command::Whoami, self.context);
        if let Some(mut token) = env.remove(TOKEN_VAR) {
            token.zeroize();
        }
        let id = &self.stored.mandate.id;
        match result {
            Ok(Outcome::Whoami(whoami))
                if (!only || whoami.mandates.len() == 1)
                    && whoami.mandates.iter().any(|m| m.id == *id) =>
            {
                Ok(crate::render_whoami(&whoami))
            }
            Ok(_) => Err(CoreError::InvalidToken),
            Err(error) => Err(error),
        }
    }
}

/// A reason not to write the token into a project.
enum GitRisk {
    /// `git check-ignore` says the file is not ignored.
    NotIgnored,
    /// `git ls-files` lists the file.
    Tracked,
    /// The folder is in a repository but git could not answer.
    GitFailed,
}

impl GitRisk {
    fn warning(&self, folder: &Path) -> String {
        let folder = escape_controls(&folder.display().to_string());
        match self {
            Self::NotIgnored => format!(
                "{CLAUDE_CODE_FILE} in {folder} is not ignored by git, so the token was not written there."
            ),
            Self::Tracked => format!(
                "{CLAUDE_CODE_FILE} in {folder} is tracked by git, so the token was not written there."
            ),
            Self::GitFailed => format!(
                "git could not be run to check {folder}, so the token was not written there."
            ),
        }
    }
}

/// Whether git could pick up the settings file in `folder`. Outside any
/// repository (no `.git` entry in the folder or an ancestor, as typed or
/// with symlinks resolved) git is not run. Inside one, the file must be ignored and untracked; any git
/// failure counts as a risk.
fn git_risk(machine: &dyn Machine, folder: &Path) -> Option<GitRisk> {
    // `ancestors` is lexical: a typed path through a symlink or junction
    // can name a folder inside a repository with no `.git` on its own
    // ancestors, so the resolved folder is walked too. A folder that
    // cannot be resolved counts as a risk.
    let has_git = |path: &Path| {
        path.ancestors()
            .any(|dir| dir.join(".git").symlink_metadata().is_ok())
    };
    let Ok(resolved) = std::fs::canonicalize(folder) else {
        return Some(GitRisk::GitFailed);
    };
    if !has_git(folder) && !has_git(&resolved) {
        return None;
    }
    let git = Path::new("git");
    let ignored = machine.run(git, &["check-ignore", "-q", "--", CLAUDE_CODE_FILE], folder);
    match ignored {
        RunOutcome::Exited(output) if output.code == Some(0) => {}
        RunOutcome::Exited(output) if output.code == Some(1) => return Some(GitRisk::NotIgnored),
        _ => return Some(GitRisk::GitFailed),
    }
    match machine.run(git, &["ls-files", "--", CLAUDE_CODE_FILE], folder) {
        RunOutcome::Exited(output) if output.code == Some(0) => {
            (!output.stdout.trim().is_empty()).then_some(GitRisk::Tracked)
        }
        _ => Some(GitRisk::GitFailed),
    }
}

/// Why a settings file cannot take the token. Messages name the file,
/// never its content.
enum SettingsProblem {
    /// It exists but cannot be read, or is not a file.
    Unusable,
    /// It is not JSON.
    NotJson,
    /// It is JSON without room for `env.AGENT_MAIL_TOKEN` as a string.
    WrongShape,
}

impl SettingsProblem {
    fn message(&self, shown: &str) -> String {
        match self {
            Self::Unusable => {
                format!("Could not write {shown}, so the token was not written there.")
            }
            Self::NotJson => {
                format!("{shown} is not valid JSON, so the token was not written there.")
            }
            Self::WrongShape => format!(
                "{shown} has no room for env.AGENT_MAIL_TOKEN as a string, so the token was not written there."
            ),
        }
    }
}

/// A JSON object as its members in file order, each value kept as the
/// exact JSON text it was. `serde_json`'s own map sorts its keys, and the
/// crate-wide `preserve_order` feature would reorder error objects agents
/// see, so the settings file is handled with this instead.
#[derive(Default)]
struct Members(Vec<(String, Box<RawValue>)>);

impl Members {
    /// The value of `key`; the last one wins, as in `serde_json`.
    fn get(&self, key: &str) -> Option<&RawValue> {
        self.0
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .map(|(_, value)| &**value)
    }

    /// Sets `key` in place of every value it had, at its first position,
    /// or last when it is new.
    fn set(&mut self, key: &str, value: Box<RawValue>) {
        match self.0.iter().position(|(name, _)| name == key) {
            Some(at) => {
                self.0[at].1 = value;
                let mut index = 0;
                self.0.retain(|(name, _)| {
                    index += 1;
                    index - 1 == at || name != key
                });
            }
            None => self.0.push((key.to_string(), value)),
        }
    }
}

impl<'de> serde::Deserialize<'de> for Members {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Members;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Members, A::Error> {
                let mut members = Vec::new();
                while let Some(entry) = map.next_entry::<String, Box<RawValue>>()? {
                    members.push(entry);
                }
                Ok(Members(members))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

impl Serialize for Members {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (name, value) in &self.0 {
            map.serialize_entry(name, value)?;
        }
        map.end()
    }
}

/// A project's `.claude/settings.local.json`, with its key order.
struct Settings {
    root: Members,
    /// Whether the file was there before.
    existed: bool,
}

impl Settings {
    /// Reads the file, or starts empty when it is missing.
    fn load(file: &Path) -> Result<Self, SettingsProblem> {
        let bytes = match std::fs::symlink_metadata(file) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    root: Members::default(),
                    existed: false,
                });
            }
            Err(_) => return Err(SettingsProblem::Unusable),
            Ok(_) if !file.is_file() => return Err(SettingsProblem::Unusable),
            Ok(_) => Zeroizing::new(std::fs::read(file).map_err(|_| SettingsProblem::Unusable)?),
        };
        serde_json::from_slice::<serde::de::IgnoredAny>(&bytes)
            .map_err(|_| SettingsProblem::NotJson)?;
        let root: Members =
            serde_json::from_slice(&bytes).map_err(|_| SettingsProblem::WrongShape)?;
        if let Some(env) = root.get("env") {
            let env: Members =
                serde_json::from_str(env.get()).map_err(|_| SettingsProblem::WrongShape)?;
            if let Some(token) = env.get(TOKEN_VAR) {
                serde_json::from_str::<String>(token.get())
                    .map(Zeroizing::new)
                    .map_err(|_| SettingsProblem::WrongShape)?;
            }
        }
        Ok(Self {
            root,
            existed: true,
        })
    }

    /// The fields of `env.AGENT_MAIL_TOKEN`, trimmed, empty ones dropped.
    fn tokens(&self) -> Vec<Zeroizing<String>> {
        let Some(raw) = self.root.get("env").and_then(|env| token_in(env.get())) else {
            return Vec::new();
        };
        raw.split(',')
            .map(|field| field.trim_matches(|c: char| c.is_ascii_whitespace()))
            .filter(|field| !field.is_empty())
            .map(|field| Zeroizing::new(field.to_string()))
            .collect()
    }

    /// Sets `env.AGENT_MAIL_TOKEN`, keeping every other key where it was;
    /// a missing `env` goes last.
    fn set_token(&mut self, value: &str) {
        let mut env: Members = self
            .root
            .get("env")
            .and_then(|env| serde_json::from_str(env.get()).ok())
            .unwrap_or_default();
        env.set(TOKEN_VAR, raw(&value));
        self.root.set("env", raw(&env));
    }

    fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut bytes =
            serde_json::to_vec_pretty(&self.root).expect("a JSON object always serialises");
        bytes.push(b'\n');
        Zeroizing::new(bytes)
    }
}

/// `value` as a raw JSON value.
fn raw(value: &impl Serialize) -> Box<RawValue> {
    let text = serde_json::to_string(value).expect("settings values always serialise");
    RawValue::from_string(text).expect("serde_json writes valid JSON")
}

/// The `AGENT_MAIL_TOKEN` string in the JSON object `env`, if any.
fn token_in(env: &str) -> Option<Zeroizing<String>> {
    let env: Members = serde_json::from_str(env).ok()?;
    serde_json::from_str::<String>(env.get(TOKEN_VAR)?.get())
        .ok()
        .map(Zeroizing::new)
}

/// `env.AGENT_MAIL_TOKEN` as the file now holds it.
fn read_back(file: &Path) -> Option<Zeroizing<String>> {
    let bytes = Zeroizing::new(std::fs::read(file).ok()?);
    let root: Members = serde_json::from_slice(&bytes).ok()?;
    token_in(root.get("env")?.get())
}

/// Writes `bytes` to `file` through a temporary file in the same folder
/// and a rename, so a crash leaves the old file or the new one. A new
/// file is readable by its owner only (Unix); a replaced one keeps its
/// permissions. Creates `.claude` when missing and removes it again if
/// the write fails.
fn write_settings(file: &Path, bytes: &[u8], existed: bool) -> std::io::Result<()> {
    let dir = file.parent().expect("the settings file is in .claude");
    let created = !dir.exists();
    if created {
        std::fs::create_dir(dir)?;
    }
    let temp = dir.join(format!(
        ".settings.local.json.agent-mail-{}.tmp",
        std::process::id()
    ));
    let result = (|| {
        let _ = std::fs::remove_file(&temp);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut out = options.open(&temp)?;
        out.write_all(bytes)?;
        out.sync_all()?;
        drop(out);
        if existed {
            let permissions = std::fs::metadata(file)?.permissions();
            std::fs::set_permissions(&temp, permissions)?;
        }
        std::fs::rename(&temp, file)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
        if created {
            let _ = std::fs::remove_dir(dir);
        }
    }
    result
}

/// The test-only hook that runs just before a settings file is read back
/// for the final check, so a test can change what the check sees.
#[cfg(any(test, feature = "test-support"))]
pub mod hook {
    use std::cell::RefCell;
    use std::path::Path;

    type Hook = Box<dyn Fn(&Path)>;

    thread_local! {
        static BEFORE_FILE_CHECK: RefCell<Option<Hook>> = const { RefCell::new(None) };
    }

    /// Runs `hook` with the settings file's path before each read-back on
    /// this thread, until [`clear`].
    pub fn set_before_file_check(hook: impl Fn(&Path) + 'static) {
        BEFORE_FILE_CHECK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
    }

    /// Removes the hook on this thread.
    pub fn clear() {
        BEFORE_FILE_CHECK.with(|slot| *slot.borrow_mut() = None);
    }

    pub(super) fn before_file_check(file: &Path) {
        BEFORE_FILE_CHECK.with(|slot| {
            if let Some(hook) = slot.borrow().as_ref() {
                hook(file);
            }
        });
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
