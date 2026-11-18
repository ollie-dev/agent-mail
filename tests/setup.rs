//! `agent-mail setup` and the bare call (job 035) and the Claude Code
//! destination (job 036), driven in-process with
//! `agent_mail::run`, the scripted terminal, the in-memory store, the fake
//! IMAP server (Gmail routed to it) and the scripted machine. No test here
//! spawns a process or touches the user's home, Keychain or audit log. All
//! values are placeholders.

mod support;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use agent_mail::create::{ABORT, EXPORT_OFFER, MAIL_PROVIDER, NOTHING_STORED, SANDBOX_INDICATORS};
use agent_mail::machine::{RecordedCommand, Reply, ScriptedMachine};
use agent_mail::mandate::{Credential, Operation, TlsMode};
use agent_mail::provider;
use agent_mail::seal::{self, AgentToken};
use agent_mail::setup::{
    ADD_TOKEN, CLAUDE_CODE_FILE, CLAUDE_CODE_PROJECT, CLAUDE_DESKTOP_FILE, CONFIGURE_MYSELF,
    CONNECT_ELSEWHERE, CONNECT_WHERE, DELETE_OFFER, DELETE_OLD_WARNING, EXISTING_TOKENS, KEPT,
    NEXT_CONFIGURE_MYSELF, PROJECT_FOLDER, REPLACE_TOKENS, SHOWN_ABOVE,
};
use agent_mail::store::{InMemoryStore, MandateStore, StoreError};
use agent_mail::terminal::{Answer, Entry, Kind, ScriptedTerminal};
use agent_mail::{Context, RunOutput, TOKEN_VAR};
use serde_json::{Value, json};
use support::broker::FakeBroker;
use support::{CREDENTIAL, FakeImap, GMAIL_LOGIN, Mode, error_of, grant_on, tokens_in};

const GMAIL_LABEL: &str = "Gmail (gmail.com, googlemail.com)";
const WRONG: &str = "wrong-password";
const NAME: &str = "setup-mandate";

/// One run: what it printed, what the terminal showed, and the machine.
struct Wizard {
    output: RunOutput,
    terminal: ScriptedTerminal,
    machine: ScriptedMachine,
}

impl Wizard {
    fn entries(&self) -> Vec<Entry> {
        self.terminal.transcript()
    }

    fn texts(&self) -> Vec<String> {
        self.entries().into_iter().map(|e| e.text).collect()
    }

    fn count(&self, text: &str) -> usize {
        self.entries().iter().filter(|e| e.text == text).count()
    }

    fn reveals(&self) -> Vec<Entry> {
        self.entries()
            .into_iter()
            .filter(|e| e.kind == Kind::Reveal)
            .collect()
    }
}

/// Runs `agent-mail <args>` in-process on `terminal` against `store`, with
/// Gmail routed to `server` and a fresh `ScriptedMachine`.
fn run_on(
    args: &[&str],
    terminal: ScriptedTerminal,
    env: &HashMap<String, String>,
    store: &dyn MandateStore,
    server: &FakeImap,
) -> Wizard {
    run_with(
        args,
        terminal,
        env,
        store,
        server,
        ScriptedMachine::new(Vec::new()),
    )
}

/// [`run_on`] with a given machine: its scripted `git` replies, and any
/// files a test put under its project folder beforehand.
fn run_with(
    args: &[&str],
    terminal: ScriptedTerminal,
    env: &HashMap<String, String>,
    store: &dyn MandateStore,
    server: &FakeImap,
    machine: ScriptedMachine,
) -> Wizard {
    let output = {
        let context = Context::trusting_only(store, vec![server.ca.clone()])
            .with_terminal(&terminal)
            .with_machine(&machine)
            .with_route("imap.gmail.com", 993, "localhost", server.port);
        let argv = std::iter::once("agent-mail").chain(args.iter().copied());
        agent_mail::run(argv, env, &context)
    };
    Wizard {
        output,
        terminal,
        machine,
    }
}

fn setup(answers: Vec<Answer>, store: &dyn MandateStore, server: &FakeImap) -> Wizard {
    run_on(
        &["setup"],
        ScriptedTerminal::new(answers),
        &HashMap::new(),
        store,
        server,
    )
}

fn yes() -> Answer {
    Answer::Confirm(true)
}

fn no() -> Answer {
    Answer::Confirm(false)
}

/// The Mandate step with Gmail, up to the summary confirm: INBOX and
/// Archive ticked, list and search granted.
fn mandate_answers() -> Vec<Answer> {
    vec![
        Answer::text(NAME),
        Answer::search("gmail", GMAIL_LABEL),
        Answer::text(GMAIL_LOGIN),
        Answer::hidden(CREDENTIAL),
        Answer::tick(&["INBOX", "Archive"]),
        Answer::tick(&["list", "search"]),
        no(),
        yes(),
    ]
}

/// The Mandate step, export declined, then `rest`.
fn script(rest: Vec<Answer>) -> Vec<Answer> {
    let mut answers = mandate_answers();
    answers.push(no());
    answers.extend(rest);
    answers
}

/// The happy path: configure-myself once, no more destinations.
fn happy() -> Vec<Answer> {
    script(vec![Answer::choose(CONFIGURE_MYSELF), no()])
}

/// Text of an entry, its options and answer.
fn entry_text(entry: &Entry) -> String {
    let mut out = entry.text.clone();
    for option in &entry.options {
        out.push('\n');
        out.push_str(option);
    }
    if let Some(answer) = &entry.answer {
        out.push('\n');
        out.push_str(answer);
    }
    out
}

/// AC8's invariant: the token occurs in exactly one transcript entry, a
/// `Reveal`, and nowhere in stdout or stderr. Returns the token.
fn assert_token_once(wizard: &Wizard) -> String {
    let entries = wizard.entries();
    let holding: Vec<&Entry> = entries
        .iter()
        .filter(|e| !tokens_in(&entry_text(e)).is_empty())
        .collect();
    assert_eq!(holding.len(), 1, "token in several entries: {holding:?}");
    assert_eq!(holding[0].kind, Kind::Reveal);
    let tokens = tokens_in(&holding[0].text);
    let token = tokens[0].clone();
    assert!(tokens.iter().all(|t| *t == token));
    assert!(tokens_in(&wizard.output.stdout).is_empty());
    assert!(tokens_in(&wizard.output.stderr).is_empty());
    assert!(!wizard.output.stdout.contains(&token));
    assert!(!wizard.output.stderr.contains(&token));
    token
}

/// The output of a bare call when no wizard starts: clap's help.
fn todays_bare_output() -> RunOutput {
    agent_mail::parse(["agent-mail"]).expect_err("a bare call is not a command")
}

fn assert_is_help(output: &RunOutput) {
    assert_eq!(*output, todays_bare_output());
    assert_eq!(output.stdout, "");
    assert_eq!(output.code, 2);
    assert!(
        output
            .stderr
            .lines()
            .any(|l| l.starts_with("Usage: agent-mail"))
    );
    assert!(output.stderr.contains("--help"));
}

fn env_with(name: &str) -> HashMap<String, String> {
    HashMap::from([(name.to_string(), String::new())])
}

/// A store holding one Mandate.
fn store_with_one() -> InMemoryStore {
    let store = InMemoryStore::new();
    let record = seal::seal(&AgentToken::mint(), &support::mandate_a(993));
    store.put(&record.key, &record.value).unwrap();
    store
}

/// A snippet: the first JSON value after the line naming `file`.
fn snippet(reveal: &str, file: &str) -> Value {
    let at = reveal.find(file).unwrap_or_else(|| panic!("{file} named"));
    let start = at + reveal[at..].find('{').expect("a JSON snippet follows");
    serde_json::Deserializer::from_str(&reveal[start..])
        .into_iter::<Value>()
        .next()
        .expect("a value")
        .expect("valid JSON")
}

// ---------------------------------------------------------------- AC1

#[test]
fn ac1_help_lists_setup_on_one_line() {
    let server = FakeImap::start(Mode::Implicit);
    let w = run_on(
        &["--help"],
        ScriptedTerminal::new(Vec::new()),
        &HashMap::new(),
        &InMemoryStore::new(),
        &server,
    );
    assert_eq!(w.output.code, 0);
    let line = w
        .output
        .stdout
        .lines()
        .find(|l| l.trim().starts_with("setup"))
        .expect("--help lists setup");
    assert!(
        line.contains("Set up agent-mail: create a Mandate and connect it to an Agent App"),
        "{line}"
    );
    assert!(w.entries().is_empty());

    let help = run_on(
        &["setup", "--help"],
        ScriptedTerminal::new(Vec::new()),
        &HashMap::new(),
        &InMemoryStore::new(),
        &server,
    );
    assert_eq!(help.output.code, 0);
    assert!(help.output.stdout.lines().all(|l| l.chars().count() <= 300));
}

// ---------------------------------------------------------------- AC2

#[test]
fn ac2a_bare_call_on_a_terminal_with_no_mandate_starts_the_wizard() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let w = run_on(
        &[],
        ScriptedTerminal::new(vec![Answer::Cancel]),
        &HashMap::new(),
        &store,
        &server,
    );
    let first = &w.entries()[0];
    assert_eq!(first.kind, Kind::Text);
    assert_eq!(first.text, "Mandate name:");
    assert_eq!(w.output.code, 1);
    assert_eq!(error_of(&w.output)["code"], "aborted");
    assert!(store.entries().is_empty());
}

#[test]
fn ac2a_bare_call_runs_the_whole_wizard() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let w = run_on(
        &[],
        ScriptedTerminal::new(happy()),
        &HashMap::new(),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert_eq!(w.terminal.remaining(), 0);
    assert_eq!(store.entries().len(), 1);
}

#[test]
fn ac2b_bare_call_without_a_terminal_prints_help() {
    let server = FakeImap::start(Mode::Implicit);
    let w = run_on(
        &[],
        ScriptedTerminal::non_interactive(),
        &HashMap::new(),
        &InMemoryStore::new(),
        &server,
    );
    assert_is_help(&w.output);
    assert!(w.entries().is_empty());
}

#[test]
fn ac2c_bare_call_in_an_agent_session_prints_help() {
    let server = FakeImap::start(Mode::Implicit);
    for name in SANDBOX_INDICATORS {
        let store = InMemoryStore::new();
        let w = run_on(
            &[],
            ScriptedTerminal::new(Vec::new()),
            &env_with(name),
            &store,
            &server,
        );
        assert_is_help(&w.output);
        assert!(w.entries().is_empty(), "{name}");
        assert!(store.entries().is_empty());
    }
}

#[test]
fn ac2d_bare_call_with_a_mandate_stored_prints_help() {
    let server = FakeImap::start(Mode::Implicit);
    let store = store_with_one();
    let before = store.entries();
    let w = run_on(
        &[],
        ScriptedTerminal::new(Vec::new()),
        &HashMap::new(),
        &store,
        &server,
    );
    assert_is_help(&w.output);
    assert!(w.entries().is_empty());
    assert_eq!(store.entries(), before);
}

#[test]
fn ac2e_binary_bare_call_tests_are_unchanged() {
    // The binary runs with piped stdin and stdout, so it is not a terminal
    // and prints help; `tests/cli.rs` keeps both tests as they were.
    let cli = include_str!("cli.rs");
    for name in [
        "fn bare_call_prints_usage_hint_on_stderr()",
        "fn bare_call_is_a_usage_error_with_empty_stdout()",
    ] {
        assert!(cli.contains(name), "{name}");
    }
    let main = include_str!("../src/main.rs");
    assert!(main.contains("agent_mail::run(args, &env, &context)"));
}

#[test]
fn ac2f_bare_call_on_the_broker_route_prints_help_and_sends_nothing() {
    let store = InMemoryStore::new();
    let terminal = ScriptedTerminal::new(Vec::new());
    let machine = ScriptedMachine::new(Vec::new());
    let fake = FakeBroker::absent();
    let context = Context::trusting_only(&store, Vec::new())
        .with_terminal(&terminal)
        .with_machine(&machine)
        .with_broker(&fake);
    let output = agent_mail::run(["agent-mail"], &HashMap::new(), &context);
    assert_is_help(&output);
    assert!(terminal.transcript().is_empty());
    assert_eq!(fake.connections(), 0);
}

// ---------------------------------------------------------------- AC3

#[test]
fn ac3_setup_in_an_agent_session_is_refused_before_any_prompt() {
    let server = FakeImap::start(Mode::Implicit);
    for name in SANDBOX_INDICATORS {
        for text in [false, true] {
            let store = InMemoryStore::new();
            let args: &[&str] = if text {
                &["setup", "--text"]
            } else {
                &["setup"]
            };
            let w = run_on(
                args,
                ScriptedTerminal::new(Vec::new()),
                &env_with(name),
                &store,
                &server,
            );
            assert_eq!(w.output.code, 1);
            assert_eq!(w.output.stdout, "");
            assert_eq!(w.output.stderr.lines().count(), 1);
            if text {
                assert!(
                    w.output.stderr.starts_with("error: sandbox_detected: "),
                    "{}",
                    w.output.stderr
                );
            } else {
                assert_eq!(error_of(&w.output)["code"], "sandbox_detected");
            }
            assert!(w.output.stderr.contains("agent-mail setup"));
            assert!(w.entries().is_empty());
            assert!(store.entries().is_empty());
        }
    }
}

#[test]
fn ac3_setup_without_a_terminal_is_refused() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let w = run_on(
        &["setup"],
        ScriptedTerminal::non_interactive(),
        &HashMap::new(),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 1);
    assert_eq!(error_of(&w.output)["code"], "interactive_terminal_required");
    assert!(w.entries().is_empty());
    assert!(store.entries().is_empty());
}

#[test]
fn ac3_setup_on_the_broker_route_refuses_before_any_prompt_or_request() {
    let store = InMemoryStore::new();
    let terminal = ScriptedTerminal::new(Vec::new());
    let machine = ScriptedMachine::new(Vec::new());
    let fake = FakeBroker::absent();
    let context = Context::trusting_only(&store, Vec::new())
        .with_terminal(&terminal)
        .with_machine(&machine)
        .with_broker(&fake);
    for args in [
        &["agent-mail", "setup"][..],
        &["agent-mail", "--text", "setup"],
    ] {
        let output = agent_mail::run(args.iter().copied(), &HashMap::new(), &context);
        assert_eq!(output.code, 1);
        assert_eq!(output.stdout, "");
        assert_eq!(output.stderr.lines().count(), 1);
        assert!(output.stderr.contains("agent-mail mandate create"));
        assert!(output.stderr.contains("elevated"));
        assert!(output.stderr.len() <= 300, "{}", output.stderr);
    }
    assert!(terminal.transcript().is_empty());
    assert_eq!(fake.connections(), 0);
    assert!(store.entries().is_empty());
}

// ---------------------------------------------------------------- AC4

/// What a prompt showed, without the Mandate ID.
fn shape(entry: &Entry) -> (Kind, String, Vec<String>, Option<String>, Option<String>) {
    (
        entry.kind,
        entry.text.clone(),
        entry.options.clone(),
        entry.answer.clone(),
        entry.default.clone(),
    )
}

#[test]
fn ac4a_mandate_step_matches_mandate_create_up_to_the_export_offer() {
    let server = FakeImap::start(Mode::Implicit);
    let w = setup(happy(), &InMemoryStore::new(), &server);
    assert_eq!(w.output.code, 0, "{:?}", w.output);

    let mut create_answers = mandate_answers();
    create_answers.push(no());
    let create = run_on(
        &["mandate", "create"],
        ScriptedTerminal::new(create_answers),
        &HashMap::new(),
        &InMemoryStore::new(),
        &server,
    );
    assert_eq!(create.output.code, 0);

    let created: Vec<Entry> = create.entries();
    let reveal_at = created
        .iter()
        .position(|e| e.kind == Kind::Reveal)
        .expect("mandate create reveals the token");
    let expected: Vec<_> = created
        .iter()
        .filter(|e| e.kind != Kind::Reveal)
        .map(shape)
        .collect();
    let entries = w.entries();
    let export_at = entries.iter().position(|e| e.text == EXPORT_OFFER).unwrap();
    let got: Vec<_> = entries[..=export_at].iter().map(shape).collect();
    assert_eq!(got, expected);
    // The only difference: the reveal is not in the Mandate step.
    assert_eq!(created[reveal_at + 1].text, EXPORT_OFFER);
    assert!(entries[..=export_at].iter().all(|e| e.kind != Kind::Reveal));

    // The picker and the note, before the hidden prompt.
    let texts = w.texts();
    let picker = texts.iter().position(|t| t == MAIL_PROVIDER).unwrap();
    let note = provider::sorted()
        .iter()
        .find(|p| p.id == "gmail")
        .and_then(|p| p.note.clone())
        .expect("Gmail has a note");
    let note_at = texts.iter().position(|t| *t == note).unwrap();
    let hidden_at = entries.iter().position(|e| e.kind == Kind::Hidden).unwrap();
    assert!(picker < note_at && note_at < hidden_at);
    assert!(
        texts
            .iter()
            .any(|t| t.starts_with("Checking the login for"))
    );
}

#[test]
fn ac4b_store_holds_exactly_the_ticked_mandate() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let w = setup(happy(), &store, &server);
    assert_eq!(w.output.code, 0);
    let token = assert_token_once(&w);
    let entries = store.entries();
    assert_eq!(entries.len(), 1);
    let token = AgentToken::parse(&token).unwrap();
    assert_eq!(entries[0].0, token.store_key());
    let mandate = seal::open(&token, &entries[0].1).unwrap();
    assert_eq!(mandate.name, NAME);
    assert_eq!(mandate.accounts.len(), 1);
    assert_eq!(mandate.accounts[0].login, GMAIL_LOGIN);
    assert_eq!(mandate.accounts[0].host, "imap.gmail.com");
    assert_eq!(mandate.accounts[0].tls, TlsMode::Implicit);
    assert!(mandate.accounts[0].credential == Credential::new(CREDENTIAL));
    assert_eq!(
        mandate.policy.grants,
        vec![
            grant_on(Operation::List, GMAIL_LOGIN, &["INBOX", "Archive"]),
            grant_on(Operation::Search, GMAIL_LOGIN, &["INBOX", "Archive"]),
        ]
    );
}

#[test]
fn ac4c_wrong_credential_then_abort_stores_nothing() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let w = setup(
        vec![
            Answer::text(NAME),
            Answer::search("gmail", GMAIL_LABEL),
            Answer::text(GMAIL_LOGIN),
            Answer::hidden(WRONG),
            Answer::choose(ABORT),
        ],
        &store,
        &server,
    );
    assert_eq!(w.output.code, 1);
    assert_eq!(error_of(&w.output)["code"], "aborted");
    assert!(w.texts().iter().any(|t| t == NOTHING_STORED));
    assert!(store.entries().is_empty());
    assert!(tokens_in(&w.terminal.transcript_text()).is_empty());
}

#[test]
fn ac4d_mandate_create_tests_are_unchanged() {
    // `tests/create.rs` runs unchanged in this suite; its token tests
    // still see one reveal and the same text.
    let create = include_str!("create.rs");
    assert!(create.contains("fn ac11_token_is_shown_once_and_no_command_reveals_it_again()"));
    assert!(create.contains("fn ac4_abort_after_wrong_credential_stores_nothing()"));
}

// ---------------------------------------------------------------- AC5

#[test]
fn ac5_configure_myself_reveals_the_token_and_both_snippets() {
    let server = FakeImap::start(Mode::Implicit);
    let w = setup(happy(), &InMemoryStore::new(), &server);
    assert_eq!(w.output.code, 0);

    let choice = w
        .entries()
        .into_iter()
        .find(|e| e.text == CONNECT_WHERE)
        .unwrap();
    assert_eq!(choice.kind, Kind::Choice);
    assert_eq!(choice.options, [CLAUDE_CODE_PROJECT, CONFIGURE_MYSELF]);

    let token = assert_token_once(&w);
    let reveals = w.reveals();
    assert_eq!(reveals.len(), 1);
    let reveal = &reveals[0].text;
    assert!(reveal.lines().any(|l| l.trim() == token));

    assert_eq!(
        snippet(reveal, CLAUDE_CODE_FILE),
        json!({"env": {"AGENT_MAIL_TOKEN": token}})
    );
    let exe = w
        .machine
        .root()
        .join("downloads")
        .join(format!("agent-mail{}", std::env::consts::EXE_SUFFIX));
    assert!(exe.is_absolute());
    assert_eq!(
        snippet(reveal, CLAUDE_DESKTOP_FILE),
        json!({"mcpServers": {"agent-mail": {
            "command": exe.to_string_lossy(),
            "args": ["mcp"],
            "env": {"AGENT_MAIL_TOKEN": token}
        }}})
    );
}

// ---------------------------------------------------------------- AC6

#[test]
fn ac6_destinations_loop_until_no_and_reveal_only_once() {
    let server = FakeImap::start(Mode::Implicit);
    let w = setup(
        script(vec![
            Answer::choose(CONFIGURE_MYSELF),
            yes(),
            Answer::choose(CONFIGURE_MYSELF),
            no(),
        ]),
        &InMemoryStore::new(),
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert_eq!(w.terminal.remaining(), 0);
    assert_eq!(w.count(CONNECT_WHERE), 2);
    assert_eq!(w.count(CONNECT_ELSEWHERE), 2);
    assert_eq!(w.reveals().len(), 1);
    assert_eq!(w.count(SHOWN_ABOVE), 1);
    assert_eq!(w.count(NEXT_CONFIGURE_MYSELF), 1);
    assert_token_once(&w);
    for entry in w.entries().iter().filter(|e| e.text == CONNECT_ELSEWHERE) {
        assert_eq!(entry.kind, Kind::Confirm);
        assert_eq!(entry.default.as_deref(), Some("no"));
    }
    // The order: second choice, the "shown above" message, then the confirm.
    let texts = w.texts();
    let second = texts
        .iter()
        .enumerate()
        .filter(|(_, t)| *t == CONNECT_WHERE)
        .nth(1)
        .unwrap()
        .0;
    assert_eq!(texts[second + 1], SHOWN_ABOVE);
    assert_eq!(texts[second + 2], CONNECT_ELSEWHERE);
}

// ---------------------------------------------------------------- AC7

#[test]
fn ac7_final_check_shows_whoami_text_then_the_next_step() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let w = setup(happy(), &store, &server);
    assert_eq!(w.output.code, 0);
    assert_eq!(w.output.stdout, "");
    assert_eq!(w.output.stderr, "");
    let token = assert_token_once(&w);

    let env = HashMap::from([(TOKEN_VAR.to_string(), token)]);
    let machine = ScriptedMachine::new(Vec::new());
    let context = Context::trusting_only(&store, Vec::new()).with_machine(&machine);
    let whoami = agent_mail::run(["agent-mail", "whoami", "--text"], &env, &context);
    assert_eq!(whoami.code, 0);
    assert!(whoami.stdout.contains("  account "));
    assert!(whoami.stdout.contains("  grant "));

    let entries = w.entries();
    let n = entries.len();
    assert_eq!(entries[n - 2].kind, Kind::Message);
    assert_eq!(entries[n - 2].text, whoami.stdout);
    assert_eq!(entries[n - 1].kind, Kind::Message);
    assert_eq!(entries[n - 1].text, NEXT_CONFIGURE_MYSELF);
    assert_eq!(entries[n - 3].text, CONNECT_ELSEWHERE);
}

/// A store whose reads fail once a record was put.
#[derive(Default)]
struct ReadsFailAfterPut {
    inner: InMemoryStore,
    put: AtomicBool,
}

impl MandateStore for ReadsFailAfterPut {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        if self.put.load(Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        self.inner.get(key)
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
        self.put.store(true, Ordering::SeqCst);
        self.inner.put(key, value)
    }

    fn delete(&self, key: &[u8]) -> Result<bool, StoreError> {
        self.inner.delete(key)
    }

    fn entries(&self) -> Result<Vec<(Vec<u8>, Vec<u8>)>, StoreError> {
        MandateStore::entries(&self.inner)
    }
}

#[test]
fn ac7_final_check_failure_shows_one_message_and_exits_1() {
    let server = FakeImap::start(Mode::Implicit);
    let store = ReadsFailAfterPut::default();
    let w = setup(happy(), &store, &server);
    assert_eq!(w.output.code, 1);
    assert_eq!(w.output.stdout, "");
    assert_eq!(w.output.stderr.lines().count(), 1);
    assert_token_once(&w);
    let last = w.entries().pop().unwrap();
    assert_eq!(last.kind, Kind::Message);
    assert!(last.text.starts_with("Final check failed"), "{}", last.text);
    assert_eq!(
        w.texts()
            .iter()
            .filter(|t| t.starts_with("Final check failed"))
            .count(),
        1
    );
    assert_eq!(w.count(NEXT_CONFIGURE_MYSELF), 0);
}

// ---------------------------------------------------------------- AC8

#[test]
fn ac8a_happy_path_shows_the_token_once() {
    let server = FakeImap::start(Mode::Implicit);
    let w = setup(happy(), &InMemoryStore::new(), &server);
    assert_eq!(w.output.code, 0);
    assert_token_once(&w);
    assert_eq!(w.count(DELETE_OFFER), 0);
}

fn delete_offer(w: &Wizard) -> Entry {
    let offers: Vec<Entry> = w
        .entries()
        .into_iter()
        .filter(|e| e.text == DELETE_OFFER)
        .collect();
    assert_eq!(offers.len(), 1);
    assert_eq!(offers[0].kind, Kind::Confirm);
    assert_eq!(offers[0].default.as_deref(), Some("no"));
    offers[0].clone()
}

#[test]
fn ac8b_cancel_at_connect_where_reveals_then_keeps_by_default() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let w = setup(
        script(vec![Answer::Cancel, Answer::Accept]),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert_eq!(w.terminal.remaining(), 0);
    let token = assert_token_once(&w);
    let reveal = &w.reveals()[0].text;
    assert!(reveal.contains(CLAUDE_CODE_FILE) && reveal.contains(CLAUDE_DESKTOP_FILE));
    let texts = w.texts();
    let reveal_at = w
        .entries()
        .iter()
        .position(|e| e.kind == Kind::Reveal)
        .unwrap();
    assert_eq!(texts[reveal_at + 1], DELETE_OFFER);
    delete_offer(&w);
    assert_eq!(w.count(KEPT), 1);
    assert_eq!(store.entries().len(), 1);
    let key = AgentToken::parse(&token).unwrap().store_key();
    assert_eq!(store.entries()[0].0, key);
}

#[test]
fn ac8c_cancel_at_connect_where_and_delete_removes_the_mandate() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let w = setup(script(vec![Answer::Cancel, yes()]), &store, &server);
    assert_eq!(w.output.code, 1);
    assert_eq!(error_of(&w.output)["code"], "aborted");
    assert_token_once(&w);
    delete_offer(&w);
    assert!(store.entries().is_empty());
    assert!(
        w.texts()
            .last()
            .unwrap()
            .starts_with("Deleted the Mandate (ID ")
    );
}

#[test]
fn ac8d_cancel_at_connect_elsewhere_does_not_reveal_again() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let w = setup(
        script(vec![
            Answer::choose(CONFIGURE_MYSELF),
            Answer::Cancel,
            Answer::Accept,
        ]),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 0);
    assert_eq!(w.reveals().len(), 1);
    assert_token_once(&w);
    delete_offer(&w);
    assert_eq!(store.entries().len(), 1);
}

#[test]
fn ac8e_cancel_at_the_delete_offer_counts_as_no() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let w = setup(
        script(vec![Answer::Cancel, Answer::Cancel]),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 0);
    assert_token_once(&w);
    delete_offer(&w);
    assert_eq!(w.count(KEPT), 1);
    assert_eq!(store.entries().len(), 1);
}

#[test]
fn ac8f_token_that_cannot_be_shown_deletes_the_mandate() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let w = run_on(
        &["setup"],
        ScriptedTerminal::failing_reveal(script(vec![Answer::choose(CONFIGURE_MYSELF)])),
        &HashMap::new(),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 1);
    assert_eq!(error_of(&w.output)["code"], "aborted");
    assert!(w.reveals().is_empty());
    assert!(tokens_in(&w.terminal.transcript_text()).is_empty());
    assert!(tokens_in(&w.output.stderr).is_empty());
    assert!(w.texts().last().unwrap().ends_with(NOTHING_STORED));
    assert!(store.entries().is_empty());
}

#[test]
fn ac8_cancelling_the_export_offer_continues() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let mut answers = mandate_answers();
    answers.extend([Answer::Cancel, Answer::choose(CONFIGURE_MYSELF), no()]);
    let w = setup(answers, &store, &server);
    assert_eq!(w.output.code, 0);
    assert_token_once(&w);
    assert_eq!(w.count(DELETE_OFFER), 0);
    assert_eq!(store.entries().len(), 1);
}

// ---------------------------------------------------------------- AC9

#[test]
fn ac9_lines_are_short_and_no_credential_is_shown() {
    let server = FakeImap::start(Mode::Implicit);
    let runs: Vec<(Vec<Answer>, bool)> = vec![
        (happy(), false),
        (
            script(vec![
                Answer::choose(CONFIGURE_MYSELF),
                yes(),
                Answer::choose(CONFIGURE_MYSELF),
                no(),
            ]),
            false,
        ),
        (script(vec![Answer::Cancel, Answer::Accept]), false),
        (script(vec![Answer::Cancel, yes()]), false),
        (
            script(vec![
                Answer::choose(CONFIGURE_MYSELF),
                Answer::Cancel,
                Answer::Cancel,
            ]),
            false,
        ),
        (script(vec![Answer::choose(CONFIGURE_MYSELF)]), true),
        (
            vec![
                Answer::text(NAME),
                Answer::search("gmail", GMAIL_LABEL),
                Answer::text(GMAIL_LOGIN),
                Answer::hidden(WRONG),
                Answer::choose(ABORT),
            ],
            false,
        ),
    ];
    for (answers, failing) in runs {
        let terminal = if failing {
            ScriptedTerminal::failing_reveal(answers)
        } else {
            ScriptedTerminal::new(answers)
        };
        let w = run_on(
            &["setup"],
            terminal,
            &HashMap::new(),
            &InMemoryStore::new(),
            &server,
        );
        for entry in w.entries() {
            for line in entry.text.lines() {
                assert!(line.chars().count() <= 300, "{line}");
            }
        }
        for line in w.output.stderr.lines() {
            assert!(line.chars().count() <= 300, "{line}");
        }
        assert!(!w.terminal.transcript_text().contains(CREDENTIAL));
        assert!(!w.output.stdout.contains(CREDENTIAL));
        assert!(!w.output.stderr.contains(CREDENTIAL));
    }
    for error in [&["setup"][..], &["setup", "--text"]] {
        let w = run_on(
            error,
            ScriptedTerminal::new(Vec::new()),
            &env_with("CLAUDECODE"),
            &InMemoryStore::new(),
            &server,
        );
        assert!(w.output.stderr.chars().count() <= 300);
    }
}

// ---------------------------------------------------------------- AC10

#[test]
fn ac10_tests_run_in_process_only() {
    let source = include_str!("setup.rs");
    let spawn = concat!("process", "::Command");
    assert!(!source.contains(spawn));
    let real = concat!("Inquire", "Terminal");
    assert!(!source.contains(real));
    let keychain = concat!("Runtime", "Store");
    assert!(!source.contains(keychain));
    // Every context comes from `trusting_only`, so the audit log is a
    // temporary file, and has the scripted machine.
    let new_context = concat!("Context", "::new(");
    assert!(!source.contains(new_context));
    let contexts = source
        .matches(concat!("Context::", "trusting_only("))
        .count();
    let machines = source
        .matches(concat!(".with_", "machine(&machine)"))
        .count();
    assert!(contexts >= 3);
    assert_eq!(machines, contexts);
}

// ======================================================= job 036: Claude Code

/// The machine's current folder.
fn project(machine: &ScriptedMachine) -> PathBuf {
    machine.root().join("project")
}

/// The settings file in `folder`.
fn settings(folder: &Path) -> PathBuf {
    folder.join(".claude").join("settings.local.json")
}

/// Writes `content` as the settings file of `folder`.
fn put_settings(folder: &Path, content: &str) {
    std::fs::create_dir_all(folder.join(".claude")).unwrap();
    std::fs::write(settings(folder), content).unwrap();
}

/// Marks the project as in a git repository.
fn git_repo(machine: &ScriptedMachine) {
    std::fs::create_dir_all(project(machine).join(".git")).unwrap();
}

/// The Mandate step, then "A Claude Code project", then `rest` (starting
/// at the folder prompt).
fn claude(rest: Vec<Answer>) -> Vec<Answer> {
    let mut answers = vec![Answer::choose(CLAUDE_CODE_PROJECT)];
    answers.extend(rest);
    script(answers)
}

/// The plain Claude Code run: the default folder, no more destinations.
fn claude_default() -> Vec<Answer> {
    claude(vec![Answer::Accept, no()])
}

/// Runs `setup` with `answers` on `machine` against `store`.
fn setup_on(
    machine: ScriptedMachine,
    answers: Vec<Answer>,
    store: &dyn MandateStore,
    server: &FakeImap,
) -> Wizard {
    run_with(
        &["setup"],
        ScriptedTerminal::new(answers),
        &HashMap::new(),
        store,
        server,
        machine,
    )
}

/// The settings file as JSON.
fn read_json(file: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap()
}

/// The token the settings file holds.
fn written_token(file: &Path) -> String {
    read_json(file)["env"]["AGENT_MAIL_TOKEN"]
        .as_str()
        .expect("a token string")
        .to_string()
}

/// Every path under `dir`, relative, with file bytes (`None` for folders).
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        for entry in std::fs::read_dir(&at).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(dir).unwrap().to_path_buf();
            if path.is_dir() {
                out.insert(relative, None);
                stack.push(path);
            } else {
                out.insert(relative, Some(std::fs::read(&path).unwrap()));
            }
        }
    }
    out
}

/// The token never shown: no `Reveal`, no token in any entry or output.
fn assert_token_never_shown(w: &Wizard) {
    assert!(w.reveals().is_empty(), "revealed on the written path");
    assert!(tokens_in(&w.terminal.transcript_text()).is_empty());
    assert!(tokens_in(&w.output.stdout).is_empty());
    assert!(tokens_in(&w.output.stderr).is_empty());
}

/// `text` occurs in no transcript entry nor output.
fn assert_nowhere(w: &Wizard, text: &str) {
    for entry in w.entries() {
        assert!(!entry_text(&entry).contains(text), "shown: {entry:?}");
    }
    assert!(!w.output.stdout.contains(text));
    assert!(!w.output.stderr.contains(text));
}

/// The `Restart Claude Code` next step for `folder`.
fn next_claude(folder: &Path) -> String {
    format!(
        "Restart Claude Code in {}, then ask it to list your mailboxes.",
        folder.display()
    )
}

/// The new Mandate the token opens, from `store`.
fn opened(store: &InMemoryStore, token: &str) -> agent_mail::mandate::Mandate {
    let token = AgentToken::parse(token).unwrap();
    let record = store.get(&token.store_key()).unwrap().expect("stored");
    seal::open(&token, &record).unwrap()
}

/// A recorded `git` call in `dir`.
fn git(args: &[&str], dir: &Path) -> RecordedCommand {
    RecordedCommand {
        program: PathBuf::from("git"),
        args: args.iter().map(|a| a.to_string()).collect(),
        dir: dir.to_path_buf(),
    }
}

const CHECK_IGNORE: [&str; 4] = ["check-ignore", "-q", "--", ".claude/settings.local.json"];
const LS_FILES: [&str; 3] = ["ls-files", "--", ".claude/settings.local.json"];

/// The delete offers for old Mandates.
fn old_offers(w: &Wizard) -> Vec<Entry> {
    w.entries()
        .into_iter()
        .filter(|e| e.text.ends_with(DELETE_OLD_WARNING))
        .collect()
}

// ------------------------------------------------------------ 036 AC1

#[test]
fn cc_ac1a_folder_prompt_defaults_to_the_current_folder() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(Vec::new());
    let folder = project(&machine);
    let store = InMemoryStore::new();
    let w = setup_on(machine, claude_default(), &store, &server);
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert_eq!(w.terminal.remaining(), 0);
    let prompt = w
        .entries()
        .into_iter()
        .find(|e| e.text == PROJECT_FOLDER)
        .unwrap();
    assert_eq!(prompt.kind, Kind::Text);
    assert_eq!(
        prompt.default.as_deref(),
        Some(folder.to_string_lossy().as_ref())
    );
    assert!(settings(&folder).is_file());
}

#[test]
fn cc_ac1b_another_folder_absolute_or_relative() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(Vec::new());
    let other = machine.root().join("other");
    std::fs::create_dir_all(&other).unwrap();
    let folder = project(&machine);
    let w = setup_on(
        machine,
        claude(vec![Answer::text(&other.to_string_lossy()), no()]),
        &InMemoryStore::new(),
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert!(settings(&other).is_file());
    assert!(std::fs::read_dir(&folder).unwrap().next().is_none());

    let machine = ScriptedMachine::new(Vec::new());
    let relative = project(&machine).join("other-rel");
    std::fs::create_dir_all(&relative).unwrap();
    let w = setup_on(
        machine,
        claude(vec![Answer::text("other-rel"), no()]),
        &InMemoryStore::new(),
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert!(settings(&relative).is_file());
    assert!(!project(&w.machine).join(".claude").exists());
    assert_eq!(w.texts().last().unwrap(), &next_claude(&relative));
}

#[test]
fn cc_ac1c_missing_folder_or_a_file_is_refused_and_asked_again() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(Vec::new());
    let missing = machine.root().join("missing");
    let file = machine.root().join("a-file");
    std::fs::write(&file, "placeholder").unwrap();
    let other = machine.root().join("other");
    std::fs::create_dir_all(&other).unwrap();
    let w = setup_on(
        machine,
        claude(vec![
            Answer::text(&missing.to_string_lossy()),
            Answer::text(&file.to_string_lossy()),
            Answer::text(&other.to_string_lossy()),
            no(),
        ]),
        &InMemoryStore::new(),
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert_eq!(w.count(PROJECT_FOLDER), 3);
    let texts = w.texts();
    let first = texts.iter().position(|t| t == PROJECT_FOLDER).unwrap();
    assert_eq!(
        texts[first + 1],
        format!("No such folder: {}", missing.display())
    );
    assert_eq!(texts[first + 2], PROJECT_FOLDER);
    assert_eq!(
        texts[first + 3],
        format!("No such folder: {}", file.display())
    );
    assert!(!missing.exists());
    assert!(file.is_file());
    assert!(settings(&other).is_file());
}

#[test]
fn cc_ac1d_cancel_at_the_folder_prompt_follows_the_cancel_path() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(Vec::new());
    let store = InMemoryStore::new();
    let w = setup_on(
        machine,
        claude(vec![Answer::Cancel, Answer::Accept]),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert_token_once(&w);
    delete_offer(&w);
    assert_eq!(w.count(KEPT), 1);
    assert!(!project(&w.machine).join(".claude").exists());
    assert_eq!(store.entries().len(), 1);
}

// ------------------------------------------------------------ 036 AC2

/// A git-risk run: the warning containing `reason`, the fallback reveal,
/// no `.claude`, the commands, and the configure-myself next step.
fn assert_git_fallback(w: &Wizard, reason: &str, commands: &[RecordedCommand]) {
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    let warnings: Vec<String> = w
        .texts()
        .into_iter()
        .filter(|t| t.contains(reason))
        .collect();
    assert_eq!(warnings.len(), 1, "{:?}", w.texts());
    let token = assert_token_once(w);
    let reveal = &w.reveals()[0].text;
    assert_eq!(
        snippet(reveal, CLAUDE_CODE_FILE),
        json!({"env": {"AGENT_MAIL_TOKEN": token}})
    );
    assert!(reveal.contains(CLAUDE_DESKTOP_FILE));
    let folder = project(&w.machine);
    assert!(!folder.join(".claude").exists());
    assert_eq!(w.machine.commands(), commands);
    for command in w.machine.commands() {
        assert_eq!(command.program.file_stem().unwrap(), "git");
    }
    assert_eq!(w.texts().last().unwrap(), NEXT_CONFIGURE_MYSELF);
    assert!(
        !w.texts()
            .iter()
            .any(|t| t.starts_with("Restart Claude Code"))
    );
}

#[test]
fn cc_ac2a_not_ignored_falls_back_to_configure_myself() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(vec![Reply::exit(1, "", "")]);
    git_repo(&machine);
    let folder = project(&machine);
    let w = setup_on(machine, claude_default(), &InMemoryStore::new(), &server);
    assert_git_fallback(&w, "not ignored by git", &[git(&CHECK_IGNORE, &folder)]);
}

#[test]
fn cc_ac2a_fallback_after_a_reveal_points_above() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(vec![Reply::exit(1, "", "")]);
    git_repo(&machine);
    let w = setup_on(
        machine,
        script(vec![
            Answer::choose(CONFIGURE_MYSELF),
            yes(),
            Answer::choose(CLAUDE_CODE_PROJECT),
            Answer::Accept,
            no(),
        ]),
        &InMemoryStore::new(),
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert_eq!(w.reveals().len(), 1);
    assert_eq!(w.count(SHOWN_ABOVE), 1);
    assert_eq!(w.count(NEXT_CONFIGURE_MYSELF), 1);
    assert_token_once(&w);
    assert!(!project(&w.machine).join(".claude").exists());
}

#[test]
fn cc_ac2b_tracked_falls_back() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(vec![
        Reply::exit(0, "", ""),
        Reply::exit(0, ".claude/settings.local.json\n", ""),
    ]);
    git_repo(&machine);
    let folder = project(&machine);
    let w = setup_on(machine, claude_default(), &InMemoryStore::new(), &server);
    assert_git_fallback(
        &w,
        "tracked by git",
        &[git(&CHECK_IGNORE, &folder), git(&LS_FILES, &folder)],
    );
}

#[test]
fn cc_ac2c_git_that_cannot_run_falls_back() {
    let server = FakeImap::start(Mode::Implicit);
    for reply in [Reply::not_started(), Reply::exit(128, "", "fatal")] {
        let machine = ScriptedMachine::new(vec![reply]);
        git_repo(&machine);
        let folder = project(&machine);
        let w = setup_on(machine, claude_default(), &InMemoryStore::new(), &server);
        assert_git_fallback(
            &w,
            "git could not be run to check",
            &[git(&CHECK_IGNORE, &folder)],
        );
    }
    // ls-files failing counts the same.
    for second in [Reply::not_started(), Reply::exit(128, "", "")] {
        let machine = ScriptedMachine::new(vec![Reply::exit(0, "", ""), second]);
        git_repo(&machine);
        let folder = project(&machine);
        let w = setup_on(machine, claude_default(), &InMemoryStore::new(), &server);
        assert_git_fallback(
            &w,
            "git could not be run to check",
            &[git(&CHECK_IGNORE, &folder), git(&LS_FILES, &folder)],
        );
    }
}

#[test]
fn cc_ac2d_git_file_counts_as_a_repository() {
    // A `.git` file (worktrees, submodules) in an ancestor folder.
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(vec![Reply::exit(1, "", "")]);
    std::fs::write(machine.root().join(".git"), "gitdir: placeholder\n").unwrap();
    let folder = project(&machine);
    let w = setup_on(machine, claude_default(), &InMemoryStore::new(), &server);
    assert_git_fallback(&w, "not ignored by git", &[git(&CHECK_IGNORE, &folder)]);
}

#[test]
fn cc_ac2e_ignored_and_untracked_is_written() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(vec![Reply::exit(0, "", ""), Reply::exit(0, "", "")]);
    git_repo(&machine);
    let folder = project(&machine);
    let store = InMemoryStore::new();
    let w = setup_on(machine, claude_default(), &store, &server);
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert_eq!(
        w.machine.commands(),
        [git(&CHECK_IGNORE, &folder), git(&LS_FILES, &folder)]
    );
    assert_eq!(
        opened(&store, &written_token(&settings(&folder))).name,
        NAME
    );
    assert_token_never_shown(&w);
}

// ------------------------------------------------------------ 036 AC3

fn repo_with_gitignore(machine: &ScriptedMachine) {
    git_repo(machine);
    let folder = project(machine);
    std::fs::write(folder.join(".git").join("config"), "placeholder\n").unwrap();
    std::fs::write(folder.join(".gitignore"), "target/\n").unwrap();
}

/// Every `.gitignore` under `dir`.
fn gitignores(dir: &Path) -> Vec<PathBuf> {
    snapshot(dir)
        .into_keys()
        .filter(|p| p.file_name().is_some_and(|n| n == ".gitignore"))
        .collect()
}

#[test]
fn cc_ac3a_risk_leaves_the_repository_untouched() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(vec![Reply::exit(1, "", "")]);
    repo_with_gitignore(&machine);
    let before = snapshot(&project(&machine));
    let w = setup_on(machine, claude_default(), &InMemoryStore::new(), &server);
    assert_eq!(w.output.code, 0);
    assert_eq!(snapshot(&project(&w.machine)), before);
    assert_eq!(
        gitignores(w.machine.root()),
        [PathBuf::from("project/.gitignore")]
    );
    assert_eq!(
        std::fs::read(project(&w.machine).join(".gitignore")).unwrap(),
        b"target/\n"
    );
}

#[test]
fn cc_ac3b_write_adds_only_the_settings_file() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(vec![Reply::exit(0, "", ""), Reply::exit(0, "", "")]);
    repo_with_gitignore(&machine);
    let before = snapshot(&project(&machine));
    let w = setup_on(machine, claude_default(), &InMemoryStore::new(), &server);
    assert_eq!(w.output.code, 0);
    let mut after = snapshot(&project(&w.machine));
    assert_eq!(after.remove(Path::new(".claude")), Some(None));
    assert!(
        after
            .remove(&PathBuf::from(".claude").join("settings.local.json"))
            .is_some()
    );
    assert_eq!(after, before);
    assert_eq!(
        gitignores(w.machine.root()),
        [PathBuf::from("project/.gitignore")]
    );
    assert_eq!(
        std::fs::read(project(&w.machine).join(".gitignore")).unwrap(),
        b"target/\n"
    );
}

// ------------------------------------------------------------ 036 AC4

#[test]
fn cc_ac4_outside_git_is_written_without_running_git() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(Vec::new());
    let folder = project(&machine);
    assert!(
        !folder.ancestors().any(|d| d.join(".git").exists()),
        "the temporary folder is inside a git repository"
    );
    let w = setup_on(machine, claude_default(), &InMemoryStore::new(), &server);
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert!(settings(&folder).is_file());
    assert!(w.machine.commands().is_empty());
}

// ------------------------------------------------------------ 036 AC5

#[test]
fn cc_ac5a_creates_the_folder_and_file_with_the_token() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(Vec::new());
    let folder = project(&machine);
    let store = InMemoryStore::new();
    let w = setup_on(machine, claude_default(), &store, &server);
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    let token = written_token(&settings(&folder));
    assert_eq!(
        read_json(&settings(&folder)),
        json!({"env": {"AGENT_MAIL_TOKEN": token}})
    );
    let mandate = opened(&store, &token);
    assert_eq!(mandate.name, NAME);
    assert_eq!(store.entries().len(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(settings(&folder))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "readable by others: {mode:o}");
    }
    // No temporary file is left next to it.
    let names: Vec<_> = std::fs::read_dir(folder.join(".claude"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["settings.local.json"]);
}

/// The byte positions of `keys` (as `"key"`) in `text`, which must rise.
fn assert_in_order(text: &str, keys: &[&str]) {
    let positions: Vec<usize> = keys
        .iter()
        .map(|k| {
            let quoted = format!("\"{k}\"");
            assert_eq!(text.matches(&quoted).count(), 1, "{k} once in {text}");
            text.find(&quoted).unwrap()
        })
        .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "{text}");
}

#[test]
fn cc_ac5b_keeps_every_other_key_in_order() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(Vec::new());
    let folder = project(&machine);
    put_settings(
        &folder,
        r#"{"permissions":{"allow":["Bash(ls)"]},"env":{"FOO":"bar"},"zeta":1,"alpha":true}"#,
    );
    let w = setup_on(machine, claude_default(), &InMemoryStore::new(), &server);
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    let text = std::fs::read_to_string(settings(&folder)).unwrap();
    assert_in_order(&text, &["permissions", "env", "zeta", "alpha"]);
    assert_in_order(&text, &["FOO", "AGENT_MAIL_TOKEN"]);
    let token = written_token(&settings(&folder));
    assert_eq!(
        read_json(&settings(&folder)),
        json!({
            "permissions": {"allow": ["Bash(ls)"]},
            "env": {"FOO": "bar", "AGENT_MAIL_TOKEN": token},
            "zeta": 1,
            "alpha": true
        })
    );
}

#[test]
fn cc_ac5c_env_is_appended_last_when_missing() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(Vec::new());
    let folder = project(&machine);
    put_settings(&folder, r#"{"zeta":1,"alpha":{"b":2,"a":1}}"#);
    let w = setup_on(machine, claude_default(), &InMemoryStore::new(), &server);
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    let text = std::fs::read_to_string(settings(&folder)).unwrap();
    assert_in_order(&text, &["zeta", "alpha", "b", "a", "env"]);
    let token = written_token(&settings(&folder));
    assert_eq!(
        read_json(&settings(&folder)),
        json!({"zeta": 1, "alpha": {"b": 2, "a": 1}, "env": {"AGENT_MAIL_TOKEN": token}})
    );
}

#[test]
fn cc_ac5d_written_path_never_shows_the_token() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(Vec::new());
    let folder = project(&machine);
    let w = setup_on(machine, claude_default(), &InMemoryStore::new(), &server);
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert_token_never_shown(&w);
    assert_nowhere(&w, &written_token(&settings(&folder)));
    assert_eq!(w.output.stdout, "");
    assert_eq!(w.output.stderr, "");
}

// ------------------------------------------------------------ 036 AC6

/// A project whose settings hold `old` (a stored Mandate's token).
fn with_old_token(store: &InMemoryStore, server: &FakeImap) -> (ScriptedMachine, String) {
    let machine = ScriptedMachine::new(Vec::new());
    let old = support::store_mandate(store, &support::mandate_a(server.port))
        .expose()
        .to_string();
    put_settings(
        &project(&machine),
        &format!(r#"{{"env":{{"AGENT_MAIL_TOKEN":"{old}"}}}}"#),
    );
    (machine, old)
}

fn existing_prompt(w: &Wizard) -> Entry {
    let prompts: Vec<Entry> = w
        .entries()
        .into_iter()
        .filter(|e| e.text == EXISTING_TOKENS)
        .collect();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].kind, Kind::Choice);
    assert_eq!(prompts[0].options, [ADD_TOKEN, REPLACE_TOKENS]);
    assert_eq!(prompts[0].default.as_deref(), Some(ADD_TOKEN));
    prompts[0].clone()
}

#[test]
fn cc_ac6a_add_appends_with_a_comma() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let (machine, old) = with_old_token(&store, &server);
    let folder = project(&machine);
    let w = setup_on(
        machine,
        claude(vec![Answer::Accept, Answer::Accept, no()]),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    existing_prompt(&w);
    let value = written_token(&settings(&folder));
    let (first, new) = value.split_once(',').expect("two tokens");
    assert_eq!(first, old);
    assert_eq!(opened(&store, new).name, NAME);
    assert_nowhere(&w, &old);
    assert_token_never_shown(&w);
}

#[test]
fn cc_ac6b_replace_swaps_the_tokens() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let (machine, old) = with_old_token(&store, &server);
    let folder = project(&machine);
    let w = setup_on(
        machine,
        claude(vec![
            Answer::Accept,
            Answer::choose(REPLACE_TOKENS),
            Answer::Accept,
            no(),
        ]),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    existing_prompt(&w);
    let value = written_token(&settings(&folder));
    assert!(!value.contains(','));
    assert_eq!(opened(&store, &value).name, NAME);
    assert_nowhere(&w, &old);
    assert_token_never_shown(&w);
}

#[test]
fn cc_ac6c_same_project_twice_lists_the_token_once() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(Vec::new());
    let folder = project(&machine);
    let store = InMemoryStore::new();
    let w = setup_on(
        machine,
        claude(vec![
            Answer::Accept,
            yes(),
            Answer::choose(CLAUDE_CODE_PROJECT),
            Answer::Accept,
            Answer::choose(ADD_TOKEN),
            no(),
        ]),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    let value = written_token(&settings(&folder));
    assert!(!value.contains(','), "listed twice");
    assert_eq!(opened(&store, &value).name, NAME);
    assert_eq!(w.count(&next_claude(&folder)), 1);
    assert_token_never_shown(&w);

    // An empty value counts as no tokens: no prompt.
    let machine = ScriptedMachine::new(Vec::new());
    let folder = project(&machine);
    put_settings(&folder, r#"{"env":{"AGENT_MAIL_TOKEN":" , "}}"#);
    let w = setup_on(machine, claude_default(), &store, &server);
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert_eq!(w.count(EXISTING_TOKENS), 0);
    assert!(!written_token(&settings(&folder)).contains(','));
}

#[test]
fn cc_ac6e_cancel_at_existing_tokens_leaves_the_file() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let (machine, old) = with_old_token(&store, &server);
    let file = settings(&project(&machine));
    let before = std::fs::read(&file).unwrap();
    let w = setup_on(
        machine,
        claude(vec![Answer::Accept, Answer::Cancel, Answer::Accept]),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert_eq!(std::fs::read(&file).unwrap(), before);
    assert_token_once(&w);
    delete_offer(&w);
    assert_nowhere(&w, &old);
}

// ------------------------------------------------------------ 036 AC7

#[test]
fn cc_ac7a_replace_offers_to_delete_the_old_mandate_default_no() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let (machine, old) = with_old_token(&store, &server);
    let w = setup_on(
        machine,
        claude(vec![
            Answer::Accept,
            Answer::choose(REPLACE_TOKENS),
            Answer::Accept,
            no(),
        ]),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    let offers = old_offers(&w);
    assert_eq!(offers.len(), 1);
    assert_eq!(offers[0].kind, Kind::Confirm);
    assert_eq!(offers[0].default.as_deref(), Some("no"));
    assert!(offers[0].text.contains("work-inbox"), "{}", offers[0].text);
    assert!(offers[0].text.contains("aaaa1111"));
    assert_eq!(store.entries().len(), 2);
    assert!(opened(&store, &old).id == "aaaa1111");
}

#[test]
fn cc_ac7b_yes_deletes_the_old_mandate() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let (machine, old) = with_old_token(&store, &server);
    let folder = project(&machine);
    let w = setup_on(
        machine,
        claude(vec![
            Answer::Accept,
            Answer::choose(REPLACE_TOKENS),
            yes(),
            no(),
        ]),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    let key = AgentToken::parse(&old).unwrap().store_key();
    assert!(store.get(&key).unwrap().is_none());
    assert!(
        w.texts()
            .iter()
            .any(|t| t.starts_with("Deleted the old Mandate (ID aaaa1111)"))
    );
    assert_eq!(store.entries().len(), 1);
    assert_eq!(
        opened(&store, &written_token(&settings(&folder))).name,
        NAME
    );
    assert_nowhere(&w, &old);
}

#[test]
fn cc_ac7c_one_offer_per_old_mandate_none_for_malformed() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let a = support::store_mandate(&store, &support::mandate_a(server.port));
    let b = support::store_mandate(&store, &support::mandate_b(server.port));
    let machine = ScriptedMachine::new(Vec::new());
    put_settings(
        &project(&machine),
        &format!(
            r#"{{"env":{{"AGENT_MAIL_TOKEN":"{},not-a-token,{}"}}}}"#,
            a.expose(),
            b.expose()
        ),
    );
    let w = setup_on(
        machine,
        claude(vec![
            Answer::Accept,
            Answer::choose(REPLACE_TOKENS),
            no(),
            no(),
            no(),
        ]),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    let offers = old_offers(&w);
    assert_eq!(offers.len(), 2);
    assert!(offers[0].text.contains("aaaa1111"));
    assert!(offers[1].text.contains("bbbb2222"));
    assert_nowhere(&w, a.expose());
    assert_nowhere(&w, b.expose());
    assert_nowhere(&w, "not-a-token");
}

#[test]
fn cc_ac7d_unknown_old_token_gives_no_offer() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let machine = ScriptedMachine::new(Vec::new());
    let unknown = AgentToken::mint();
    put_settings(
        &project(&machine),
        &format!(r#"{{"env":{{"AGENT_MAIL_TOKEN":"{}"}}}}"#, unknown.expose()),
    );
    let w = setup_on(
        machine,
        claude(vec![Answer::Accept, Answer::choose(REPLACE_TOKENS), no()]),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert!(old_offers(&w).is_empty());
}

#[test]
fn cc_ac7e_add_and_failed_writes_never_offer_a_delete() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let (machine, _) = with_old_token(&store, &server);
    let w = setup_on(
        machine,
        claude(vec![Answer::Accept, Answer::choose(ADD_TOKEN), no()]),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert!(old_offers(&w).is_empty());
    assert_eq!(store.entries().len(), 2);

    let machine = ScriptedMachine::new(Vec::new());
    std::fs::create_dir_all(settings(&project(&machine))).unwrap();
    let w = setup_on(machine, claude_default(), &store, &server);
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert!(old_offers(&w).is_empty());
}

// ------------------------------------------------------------ 036 AC8

/// An unusable settings file: the message containing `says`, the fallback
/// reveal, and the file unchanged.
fn assert_unusable(content: &str, says: &str) {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(Vec::new());
    let folder = project(&machine);
    put_settings(&folder, content);
    let w = setup_on(machine, claude_default(), &InMemoryStore::new(), &server);
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert_eq!(std::fs::read_to_string(settings(&folder)).unwrap(), content);
    let messages: Vec<String> = w
        .entries()
        .into_iter()
        .filter(|e| e.kind == Kind::Message && e.text.contains(says))
        .map(|e| e.text)
        .collect();
    assert_eq!(messages.len(), 1, "{content}: {:?}", w.texts());
    assert_token_once(&w);
    assert_eq!(w.texts().last().unwrap(), NEXT_CONFIGURE_MYSELF);
    for entry in w.entries().iter().filter(|e| e.kind == Kind::Message) {
        assert!(
            content.len() < 3 || !entry.text.contains(content),
            "{entry:?}"
        );
    }
    assert_eq!(
        snapshot(&folder.join(".claude")).len(),
        1,
        "no other file was left"
    );
}

#[test]
fn cc_ac8a_invalid_json_falls_back() {
    assert_unusable(r#"{"env": "#, "is not valid JSON");
}

#[test]
fn cc_ac8b_json_that_cannot_hold_the_token_falls_back() {
    for content in [
        "[]",
        r#"{"env":"x"}"#,
        r#"{"env":{"AGENT_MAIL_TOKEN":7}}"#,
        r#"{"env":{"AGENT_MAIL_TOKEN":["placeholder"]}}"#,
    ] {
        assert_unusable(content, "has no room for env.AGENT_MAIL_TOKEN");
    }
}

#[test]
fn cc_ac8c_unwritable_file_falls_back() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(Vec::new());
    let folder = project(&machine);
    std::fs::create_dir_all(settings(&folder)).unwrap();
    let before = snapshot(&folder);
    let w = setup_on(machine, claude_default(), &InMemoryStore::new(), &server);
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    assert!(
        w.texts()
            .iter()
            .any(|t| t.starts_with("Could not write ") && t.contains("settings.local.json"))
    );
    assert_eq!(snapshot(&folder), before);
    assert!(settings(&folder).is_dir());
    assert_token_once(&w);
    assert_eq!(w.texts().last().unwrap(), NEXT_CONFIGURE_MYSELF);
}

// ------------------------------------------------------------ 036 AC9

#[test]
fn cc_ac9a_final_check_reads_the_file_back_and_lists_both_mandates() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let (machine, _) = with_old_token(&store, &server);
    let folder = project(&machine);
    let w = setup_on(
        machine,
        claude(vec![Answer::Accept, Answer::choose(ADD_TOKEN), no()]),
        &store,
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    let new = opened(
        &store,
        written_token(&settings(&folder)).split(',').nth(1).unwrap(),
    );
    let entries = w.entries();
    let n = entries.len();
    let check = &entries[n - 2];
    assert_eq!(check.kind, Kind::Message);
    assert!(
        check.text.contains("aaaa1111  work-inbox"),
        "{}",
        check.text
    );
    assert!(check.text.contains(&format!("{}  {NAME}", new.id)));
    assert!(check.text.contains("  grant "));
    assert_eq!(entries[n - 1].text, next_claude(&folder));
}

#[test]
fn cc_ac9b_a_file_that_does_not_open_the_new_mandate_fails_the_check() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(Vec::new());
    let folder = project(&machine);
    let store = InMemoryStore::new();
    let placeholder = AgentToken::mint().expose().to_string();
    let written = placeholder.clone();
    agent_mail::setup::hook::set_before_file_check(move |file| {
        std::fs::write(
            file,
            format!(r#"{{"env":{{"AGENT_MAIL_TOKEN":"{written}"}}}}"#),
        )
        .unwrap();
    });
    let w = setup_on(machine, claude_default(), &store, &server);
    agent_mail::setup::hook::clear();
    assert_eq!(w.output.code, 1, "{:?}", w.output);
    assert_eq!(w.output.stderr.lines().count(), 1);
    assert_token_once(&w);
    assert_eq!(
        w.texts()
            .iter()
            .filter(|t| t.starts_with("Final check failed"))
            .count(),
        1
    );
    assert!(w.texts().last().unwrap().starts_with("Final check failed"));
    assert_eq!(w.count(&next_claude(&folder)), 0);
    assert_nowhere(&w, &placeholder);
}

// ----------------------------------------------------------- 036 AC10

#[test]
fn cc_ac10_one_next_step_per_written_destination_in_order() {
    let server = FakeImap::start(Mode::Implicit);
    let machine = ScriptedMachine::new(Vec::new());
    let folder = project(&machine);
    let other = machine.root().join("other");
    std::fs::create_dir_all(&other).unwrap();
    let w = setup_on(
        machine,
        claude(vec![
            Answer::text(&other.to_string_lossy()),
            yes(),
            Answer::choose(CLAUDE_CODE_PROJECT),
            Answer::Accept,
            no(),
        ]),
        &InMemoryStore::new(),
        &server,
    );
    assert_eq!(w.output.code, 0, "{:?}", w.output);
    let texts = w.texts();
    let n = texts.len();
    assert_eq!(texts[n - 2], next_claude(&other));
    assert_eq!(texts[n - 1], next_claude(&folder));
    assert_eq!(
        texts[n - 1],
        format!(
            "Restart Claude Code in {}, then ask it to list your mailboxes.",
            folder.display()
        )
    );
    // Both files hold the same new token; it was never shown.
    assert_eq!(
        written_token(&settings(&other)),
        written_token(&settings(&folder))
    );
    assert_token_never_shown(&w);
    for line in texts.iter().flat_map(|t| t.lines()) {
        assert!(line.chars().count() <= 300, "{line}");
    }
}
