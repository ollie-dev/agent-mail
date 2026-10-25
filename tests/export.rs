//! The Mandate file: the export offer at the end of `mandate create` and
//! `mandate create --from <file>`, driven in-process through scripted
//! terminal sessions against the fake IMAP server and the in-memory store.

mod support;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use agent_mail::RunOutput;
use agent_mail::create::{ABORT, EXPORT_OFFER, EXPORT_PATH, GENERIC, RETRY, TLS_IMPLICIT};
use agent_mail::export::MandateFile;
use agent_mail::mandate::{Grant, Mandate, Operation, TlsMode};
use agent_mail::seal::{self, AgentToken};
use agent_mail::store::{InMemoryStore, MandateStore, StoreError};
use agent_mail::terminal::{Answer, Entry, Kind, ScriptedTerminal};
use support::{
    CREDENTIAL, CountingStore, FakeImap, LOGIN, Mode, OTHER_LOGIN, contains_bytes, error_of,
    grant_on, run_with_terminal, token_hash, tokens_in,
};

const WRONG: &str = "wrong-password";
const FILE_SECRET: &str = "hunter2-file-secret";

/// A fresh, empty directory under the system temp directory.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "agent-mail-export-{}-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::SeqCst),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    fn is_empty(&self) -> bool {
        std::fs::read_dir(&self.0).unwrap().next().is_none()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Session {
    output: RunOutput,
    terminal: ScriptedTerminal,
}

impl Session {
    fn entries(&self) -> Vec<Entry> {
        self.terminal.transcript()
    }

    fn transcript(&self) -> String {
        self.terminal.transcript_text()
    }

    fn token(&self) -> String {
        let tokens = tokens_in(&self.transcript());
        assert_eq!(tokens.len(), 1, "expected one token in the transcript");
        tokens[0].clone()
    }

    /// The first prompt whose text is `text`.
    fn prompt(&self, text: &str) -> Entry {
        self.entries()
            .into_iter()
            .find(|e| e.kind != Kind::Message && e.text == text)
            .unwrap_or_else(|| panic!("no prompt {text:?}"))
    }

    /// The first prompt whose text starts with `start`.
    fn prompt_starting(&self, start: &str) -> Entry {
        self.entries()
            .into_iter()
            .find(|e| e.kind != Kind::Message && e.text.starts_with(start))
            .unwrap_or_else(|| panic!("no prompt starting {start:?}"))
    }

    fn assert_completed(&self) {
        assert_eq!(self.output.code, 0, "stderr: {:?}", self.output.stderr);
        assert_eq!(self.output.stdout, "");
        assert_eq!(self.terminal.remaining(), 0, "script not used up");
    }
}

fn session_on(
    terminal: ScriptedTerminal,
    args: &[&str],
    env: &HashMap<String, String>,
    store: &dyn MandateStore,
    servers: &[&FakeImap],
) -> Session {
    let roots = servers.iter().map(|s| s.ca.clone()).collect();
    let output = run_with_terminal(args, env, store, &terminal, roots, &[]);
    Session { output, terminal }
}

fn create(
    from: Option<&Path>,
    answers: Vec<Answer>,
    store: &dyn MandateStore,
    servers: &[&FakeImap],
) -> Session {
    let path = from.map(|p| p.to_str().unwrap().to_string());
    let mut args = vec!["mandate", "create"];
    if let Some(path) = &path {
        args.extend(["--from", path.as_str()]);
    }
    session_on(
        ScriptedTerminal::new(answers),
        &args,
        &HashMap::new(),
        store,
        servers,
    )
}

fn yes() -> Answer {
    Answer::Confirm(true)
}

fn no() -> Answer {
    Answer::Confirm(false)
}

fn accept() -> Answer {
    Answer::Accept
}

/// The AC1 session up to and including the summary confirmation: one
/// generic Account, `INBOX` and `Archive`, `list`, `search` and `draft`.
fn ac1_answers(port: u16) -> Vec<Answer> {
    vec![
        Answer::text("test-mandate"),
        Answer::choose(GENERIC),
        Answer::text("localhost"),
        Answer::text(&port.to_string()),
        Answer::choose(TLS_IMPLICIT),
        Answer::text(LOGIN),
        Answer::hidden(CREDENTIAL),
        Answer::tick(&["INBOX", "Archive"]),
        Answer::tick(&["list", "search", "draft"]),
        no(),
        yes(),
    ]
}

/// The AC1 session that accepts the export offer and saves to `path`.
fn ac1_exporting(store: &InMemoryStore, server: &FakeImap, path: &Path) -> Session {
    let mut answers = ac1_answers(server.port);
    answers.extend([yes(), Answer::text(path.to_str().unwrap())]);
    create(None, answers, store, &[server])
}

fn stored_mandate(store: &InMemoryStore, token: &str) -> Mandate {
    let entries = store.entries();
    assert_eq!(entries.len(), 1, "store should hold exactly one entry");
    let token = AgentToken::parse(token).unwrap();
    seal::open(&token, &entries[0].1).unwrap()
}

fn file_grants(file: &MandateFile) -> Vec<Grant> {
    file.grants
        .iter()
        .map(|g| Grant {
            operation: g.operation,
            account: g.account.clone(),
            mailboxes: g.mailboxes.iter().map(|m| m.as_str().into()).collect(),
        })
        .collect()
}

fn account_toml(login: &str, port: u16) -> String {
    format!(
        "[[accounts]]\nlogin = \"{login}\"\nhost = \"localhost\"\nport = {port}\ntls = \"implicit\"\n\n"
    )
}

fn grant_toml(operation: &str, login: &str, mailboxes: &[&str]) -> String {
    let names: Vec<String> = mailboxes
        .iter()
        .map(|m| toml::Value::String((*m).to_string()).to_string())
        .collect();
    format!(
        "[[grants]]\noperation = \"{operation}\"\naccount = \"{login}\"\nmailboxes = [{}]\n\n",
        names.join(", ")
    )
}

/// A file like the AC1 export, without `draft`: `list` and `search` on
/// `INBOX` and `Archive`.
fn ac3_file(port: u16) -> String {
    format!(
        "name = \"test-mandate\"\n\n{}{}{}",
        account_toml(LOGIN, port),
        grant_toml("list", LOGIN, &["INBOX", "Archive"]),
        grant_toml("search", LOGIN, &["INBOX", "Archive"]),
    )
}

/// Two Accounts, each with `list` on `INBOX`.
fn two_account_file(a: u16, b: u16) -> String {
    format!(
        "name = \"two-accounts\"\n\n{}{}{}{}",
        account_toml(LOGIN, a),
        account_toml(OTHER_LOGIN, b),
        grant_toml("list", LOGIN, &["INBOX"]),
        grant_toml("list", OTHER_LOGIN, &["Archive"]),
    )
}

/// Accepts every pre-fill of one kept file Account, typing `credentials`
/// (each after the first preceded by `Retry`).
fn kept_account(credentials: &[&str]) -> Vec<Answer> {
    let mut answers = vec![accept(), accept(), accept(), accept()];
    answers.push(Answer::hidden(credentials[0]));
    for credential in &credentials[1..] {
        answers.push(Answer::choose(RETRY));
        answers.push(Answer::hidden(credential));
    }
    answers
}

#[test]
fn ac1_accepting_the_export_offer_writes_name_accounts_and_grants() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let dir = TempDir::new();
    let path = dir.join("triage.toml");
    let session = ac1_exporting(&store, &server, &path);
    session.assert_completed();

    let offer = session.prompt(EXPORT_OFFER);
    assert_eq!(offer.kind, Kind::Confirm);
    assert_eq!(offer.default.as_deref(), Some("no"));
    let mandate = stored_mandate(&store, &session.token());
    let path_prompt = session.prompt(EXPORT_PATH);
    assert_eq!(
        path_prompt.default,
        Some(format!("agent-mail-{}.toml", mandate.id))
    );

    let text = std::fs::read_to_string(&path).expect("export written");
    let table: toml::Table = text.parse().expect("export is TOML");
    assert_eq!(table["name"].as_str(), Some("test-mandate"));
    let accounts = table["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 1);
    let account = accounts[0].as_table().unwrap();
    assert_eq!(account["login"].as_str(), Some(LOGIN));
    assert_eq!(account["host"].as_str(), Some("localhost"));
    assert_eq!(account["port"].as_integer(), Some(i64::from(server.port)));
    assert_eq!(account["tls"].as_str(), Some("implicit"));
    assert_eq!(account.len(), 4, "unexpected Account keys: {account:?}");

    let grants: Vec<(String, Vec<String>)> = table["grants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            let g = g.as_table().unwrap();
            assert_eq!(g["account"].as_str(), Some(LOGIN));
            let mailboxes = g["mailboxes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m.as_str().unwrap().to_string())
                .collect();
            (g["operation"].as_str().unwrap().to_string(), mailboxes)
        })
        .collect();
    let both = vec!["INBOX".to_string(), "Archive".to_string()];
    assert_eq!(
        grants,
        vec![
            ("list".to_string(), both.clone()),
            ("search".to_string(), both),
            ("draft".to_string(), Vec::new()),
        ]
    );

    let file = MandateFile::from_toml(&text).unwrap();
    assert_eq!(file_grants(&file), mandate.policy.grants);
    assert!(session.transcript().contains("Exported"));
}

fn keys_in(value: &toml::Value, out: &mut Vec<String>) {
    match value {
        toml::Value::Table(table) => {
            for (key, value) in table {
                out.push(key.to_lowercase());
                keys_in(value, out);
            }
        }
        toml::Value::Array(items) => items.iter().for_each(|v| keys_in(v, out)),
        _ => {}
    }
}

#[test]
fn ac2_the_exported_file_holds_no_credential_or_agent_token() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let dir = TempDir::new();
    let path = dir.join("export.toml");
    let session = ac1_exporting(&store, &server, &path);
    session.assert_completed();

    let bytes = std::fs::read(&path).unwrap();
    let token = session.token();
    let key = AgentToken::parse(&token).unwrap().store_key();
    let hex_key: String = key.iter().map(|b| format!("{b:02x}")).collect();
    for secret in [
        CREDENTIAL.as_bytes(),
        token.as_bytes(),
        token_hash(&token).as_bytes(),
        &key,
        hex_key.as_bytes(),
        seal::TOKEN_PREFIX.as_bytes(),
    ] {
        assert!(!contains_bytes(&bytes, secret), "export holds a secret");
    }
    let value = toml::Value::Table(std::str::from_utf8(&bytes).unwrap().parse().unwrap());
    let mut keys = Vec::new();
    keys_in(&value, &mut keys);
    assert!(!keys.is_empty());
    for key in keys {
        for word in ["credential", "password", "token", "secret"] {
            assert!(!key.contains(word), "export has a {key:?} key");
        }
    }
}

fn ac3_accept_all(store: &InMemoryStore, server: &FakeImap, file: &Path) -> Session {
    let mut answers = vec![accept(), accept()];
    answers.extend(kept_account(&[CREDENTIAL]));
    answers.extend([accept(), accept(), no(), yes(), no()]);
    create(Some(file), answers, store, &[server])
}

#[test]
fn ac3_from_file_pre_selects_the_file_choices() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let dir = TempDir::new();
    let file = dir.write("m.toml", &ac3_file(server.port));
    let session = ac3_accept_all(&store, &server, &file);
    session.assert_completed();

    let name = session.prompt("Mandate name:");
    assert_eq!(name.default.as_deref(), Some("test-mandate"));
    let keep = session.prompt_starting("Keep Account");
    assert_eq!(keep.kind, Kind::Confirm);
    assert!(keep.text.contains(LOGIN), "{:?}", keep.text);
    assert_eq!(keep.default.as_deref(), Some("yes"));
    assert_eq!(
        session.prompt("IMAP server host:").default.as_deref(),
        Some("localhost")
    );
    assert_eq!(
        session.prompt("IMAP server port:").default,
        Some(server.port.to_string())
    );
    assert_eq!(
        session.prompt("Login name:").default.as_deref(),
        Some(LOGIN)
    );
    let tls = session.prompt("TLS mode:");
    assert_eq!(tls.kind, Kind::Choice);
    assert_eq!(tls.default.as_deref(), Some(TLS_IMPLICIT));
    assert!(
        !session.entries().iter().any(|e| e.text == "Account type:"),
        "file Accounts skip the Account type"
    );
    let mailboxes = session.prompt_starting("Mailboxes the Agent may use");
    assert_eq!(mailboxes.pre_ticked, ["INBOX", "Archive"]);
    let operations = session.prompt_starting("Operations the Agent may perform");
    assert_eq!(operations.pre_ticked, ["list", "search"]);

    let mandate = stored_mandate(&store, &session.token());
    assert_eq!(mandate.name, "test-mandate");
    assert_eq!(mandate.accounts[0].port, server.port);
    assert_eq!(mandate.accounts[0].tls, TlsMode::Implicit);
    assert_eq!(
        mandate.policy.grants,
        vec![
            grant_on(Operation::List, LOGIN, &["INBOX", "Archive"]),
            grant_on(Operation::Search, LOGIN, &["INBOX", "Archive"]),
        ]
    );
}

#[test]
fn ac3_from_file_choices_can_be_changed_before_confirming() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let dir = TempDir::new();
    let file = dir.write("m.toml", &ac3_file(server.port));
    let mut answers = vec![accept(), accept()];
    answers.extend(kept_account(&[CREDENTIAL]));
    answers.extend([
        Answer::tick(&["INBOX", "Sent"]),
        Answer::tick(&["list", "read"]),
        no(),
        yes(),
        no(),
    ]);
    let session = create(Some(&file), answers, &store, &[&server]);
    session.assert_completed();

    let mandate = stored_mandate(&store, &session.token());
    assert_eq!(
        mandate.policy.grants,
        vec![
            grant_on(Operation::List, LOGIN, &["INBOX", "Sent"]),
            grant_on(Operation::Read, LOGIN, &["INBOX", "Sent"]),
        ]
    );
}

#[test]
fn ac3_a_file_account_can_be_dropped() {
    let a = FakeImap::start(Mode::Implicit);
    let b = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let dir = TempDir::new();
    let file = dir.write("m.toml", &two_account_file(a.port, b.port));
    let mut answers = vec![accept(), accept()];
    answers.extend(kept_account(&[CREDENTIAL]));
    answers.extend([accept(), accept(), no(), no(), yes(), no()]);
    let session = create(Some(&file), answers, &store, &[&a, &b]);
    session.assert_completed();

    let keeps: Vec<Entry> = session
        .entries()
        .into_iter()
        .filter(|e| e.text.starts_with("Keep Account"))
        .collect();
    assert_eq!(keeps.len(), 2);
    assert!(keeps[1].text.contains(OTHER_LOGIN));
    let mandate = stored_mandate(&store, &session.token());
    let logins: Vec<&str> = mandate.accounts.iter().map(|a| a.login.as_str()).collect();
    assert_eq!(logins, [LOGIN]);
    assert!(mandate.policy.grants.iter().all(|g| g.account == LOGIN));
    assert_eq!(b.connections(), 0, "dropped Account was contacted");
}

fn hidden_prompts(session: &Session) -> usize {
    session
        .entries()
        .iter()
        .filter(|e| e.kind == Kind::Hidden)
        .count()
}

#[test]
fn ac4_every_file_account_is_credential_checked() {
    let a = FakeImap::start(Mode::Implicit);
    let b = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let dir = TempDir::new();
    let file = dir.write("m.toml", &two_account_file(a.port, b.port));
    let mut answers = vec![accept(), accept()];
    answers.extend(kept_account(&[CREDENTIAL]));
    answers.extend([accept(), accept(), accept()]);
    answers.extend(kept_account(&[CREDENTIAL]));
    answers.extend([accept(), accept(), no(), yes(), no()]);
    let session = create(Some(&file), answers, &store, &[&a, &b]);
    session.assert_completed();

    assert_eq!(hidden_prompts(&session), 2);
    assert!(a.commands().contains(&"LOGIN".to_string()));
    assert!(b.commands().contains(&"LOGIN".to_string()));
    let mandate = stored_mandate(&store, &session.token());
    assert_eq!(mandate.accounts.len(), 2);
    assert!(!session.transcript().contains(CREDENTIAL));
}

#[test]
fn ac4_a_wrong_credential_from_a_file_session_can_be_retried() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let dir = TempDir::new();
    let file = dir.write("m.toml", &ac3_file(server.port));
    let mut answers = vec![accept(), accept()];
    answers.extend(kept_account(&[WRONG, CREDENTIAL]));
    answers.extend([accept(), accept(), no(), yes(), no()]);
    let session = create(Some(&file), answers, &store, &[&server]);
    session.assert_completed();

    assert_eq!(hidden_prompts(&session), 2);
    let entries = session.entries();
    let failed = entries
        .iter()
        .position(|e| e.kind == Kind::Message && e.text.contains("failed"))
        .unwrap();
    let reveal = entries.iter().position(|e| e.kind == Kind::Reveal).unwrap();
    assert!(failed < reveal);
    let mandate = stored_mandate(&store, &session.token());
    assert_eq!(mandate.accounts[0].login, LOGIN);
}

#[test]
fn ac4_abort_after_a_wrong_credential_stores_nothing() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let dir = TempDir::new();
    let file = dir.write("m.toml", &ac3_file(server.port));
    let mut answers = vec![accept(), accept()];
    answers.extend([accept(), accept(), accept(), accept()]);
    answers.extend([Answer::hidden(WRONG), Answer::choose(ABORT)]);
    let session = create(Some(&file), answers, &store, &[&server]);

    assert_eq!(session.output.code, 1);
    assert_eq!(error_of(&session.output)["code"], "aborted");
    assert_eq!(session.terminal.remaining(), 0);
    assert!(store.entries().is_empty());
    assert!(tokens_in(&session.transcript()).is_empty());
}

#[test]
fn ac5_a_mailbox_missing_on_the_server_is_shown_unavailable() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let dir = TempDir::new();
    let text = format!(
        "name = \"gone\"\n\n{}{}",
        account_toml(LOGIN, server.port),
        grant_toml("list", LOGIN, &["INBOX", "Gone", "Bad\n\u{1b}[31mName"]),
    );
    let file = dir.write("m.toml", &text);
    let mut answers = vec![accept(), accept()];
    answers.extend(kept_account(&[CREDENTIAL]));
    answers.extend([accept(), accept(), no(), yes(), no()]);
    let session = create(Some(&file), answers, &store, &[&server]);
    session.assert_completed();

    let messages: Vec<String> = session
        .entries()
        .into_iter()
        .filter(|e| e.kind == Kind::Message && e.text.contains("unavailable"))
        .map(|e| e.text)
        .collect();
    assert_eq!(messages.len(), 2, "{messages:?}");
    assert!(messages[0].contains("Gone") && messages[0].contains(LOGIN));
    assert!(messages[1].contains("Bad\\u{a}\\u{1b}[31mName"));
    assert!(!session.transcript().contains('\u{1b}'));

    let checklist = session.prompt_starting("Mailboxes the Agent may use");
    assert!(!checklist.options.iter().any(|o| o.contains("Gone")));
    assert_eq!(checklist.pre_ticked, ["INBOX"]);
    let mandate = stored_mandate(&store, &session.token());
    assert_eq!(
        mandate.policy.grants,
        vec![grant_on(Operation::List, LOGIN, &["INBOX"])]
    );
}

#[test]
fn star_in_a_file_pre_ticks_every_mailbox() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let dir = TempDir::new();
    let text = format!(
        "name = \"all\"\n\n{}{}{}",
        account_toml(LOGIN, server.port),
        grant_toml("read", LOGIN, &["*"]),
        grant_toml("draft", LOGIN, &[]),
    );
    let file = dir.write("m.toml", &text);
    let mut answers = vec![accept(), accept()];
    answers.extend(kept_account(&[CREDENTIAL]));
    answers.extend([accept(), accept(), no(), yes(), no()]);
    let session = create(Some(&file), answers, &store, &[&server]);
    session.assert_completed();

    let checklist = session.prompt_starting("Mailboxes the Agent may use");
    assert_eq!(checklist.pre_ticked, checklist.options);
    let operations = session.prompt_starting("Operations the Agent may perform");
    assert_eq!(operations.pre_ticked, ["read", "draft"]);
}

/// Runs `mandate create --from <path>` and asserts the file was refused with
/// a message containing every `needle`, before any prompt, connection or
/// store access.
fn assert_refused(path: &Path, needles: &[&str], server: &FakeImap) {
    let store = CountingStore::default();
    let terminal = ScriptedTerminal::new(Vec::new());
    let args = ["mandate", "create", "--from", path.to_str().unwrap()];
    let output = run_with_terminal(
        &args,
        &HashMap::new(),
        &store,
        &terminal,
        vec![server.ca.clone()],
        &[],
    );
    assert_eq!(output.code, 1, "{path:?}: {:?}", output.stderr);
    assert_eq!(output.stdout, "");
    let error = error_of(&output);
    assert_eq!(error["code"], "invalid_mandate_file", "{error}");
    let message = error["message"].as_str().unwrap();
    for needle in needles {
        assert!(message.contains(needle), "{needle:?} not in {message:?}");
    }
    assert!(!output.stderr.contains(FILE_SECRET), "{message:?}");
    // No key name from the file is repeated: the line says where.
    for key in ["password", "`tls`", "`port`", "`operation`", "`name`"] {
        assert!(!message.contains(key), "{key:?} repeated in {message:?}");
    }
    assert!(terminal.transcript().is_empty(), "prompted: {path:?}");
    assert_eq!(store.accesses(), 0);
    assert_eq!(server.connections(), 0);
}

#[test]
fn ac6_malformed_files_are_refused_before_any_prompt() {
    let server = FakeImap::start(Mode::Implicit);
    let dir = TempDir::new();
    let good = ac3_file(server.port);
    let port_line = format!("port = {}", server.port);
    let cases: Vec<(&str, String, Vec<String>)> = vec![
        (
            "a",
            good.replace("[[accounts]]", "[[accounts]\nbroken"),
            vec!["line 3".into(), "not valid TOML".into()],
        ),
        (
            "b",
            good.replace(
                "tls = \"implicit\"",
                &format!("tls = \"implicit\"\npassword = \"{FILE_SECRET}\""),
            ),
            vec!["line 8".into(), "unknown key".into()],
        ),
        (
            "c",
            good.replace("tls = \"implicit\"", "tls = \"none\""),
            vec!["line 7".into(), "TLS mode".into()],
        ),
        (
            "d",
            good.replace(&port_line, "port = 0"),
            vec!["line 6".into(), "port must be".into()],
        ),
        (
            "e",
            good.replace("operation = \"search\"", "operation = \"send\""),
            vec!["line 15".into(), "operation must be".into()],
        ),
        (
            "f",
            format!(
                "{good}{}",
                grant_toml("read", "stranger@example.test", &["INBOX"])
            ),
            vec!["line 21".into(), "stranger@example.test".into()],
        ),
        (
            "g",
            format!("name = \"x\"\n\n{}", grant_toml("list", LOGIN, &["INBOX"])),
            vec!["[[accounts]]".into()],
        ),
        (
            "h",
            format!(
                "name = \"x\"\n\n{}{}",
                account_toml(LOGIN, server.port),
                account_toml(LOGIN, server.port)
            ),
            vec!["line 10".into(), "two Accounts".into(), LOGIN.into()],
        ),
    ];
    for (case, text, needles) in cases {
        let path = dir.write(&format!("{case}.toml"), &text);
        let needles: Vec<&str> = needles.iter().map(String::as_str).collect();
        assert_refused(&path, &needles, &server);
    }
    // (i) a path that does not exist.
    assert_refused(&dir.join("missing.toml"), &["cannot read"], &server);
    // The secret-free file is accepted by the same parser.
    assert!(MandateFile::from_toml(&good).is_ok());
}

#[test]
fn ac6_refusal_checks_take_precedence_over_file_errors() {
    let dir = TempDir::new();
    let missing = dir.join("missing.toml");
    let args = ["mandate", "create", "--from", missing.to_str().unwrap()];

    let store = CountingStore::default();
    let terminal = ScriptedTerminal::non_interactive();
    let output = run_with_terminal(&args, &HashMap::new(), &store, &terminal, vec![], &[]);
    assert_eq!(error_of(&output)["code"], "interactive_terminal_required");

    let terminal = ScriptedTerminal::new(Vec::new());
    let env = HashMap::from([("CLAUDECODE".to_string(), "1".to_string())]);
    let output = run_with_terminal(&args, &env, &store, &terminal, vec![], &[]);
    assert_eq!(error_of(&output)["code"], "sandbox_detected");
    assert!(terminal.transcript().is_empty());
    assert_eq!(store.accesses(), 0);
}

#[test]
fn ac7_declining_the_export_offer_writes_nothing() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let mut answers = ac1_answers(server.port);
    answers.push(no());
    let session = create(None, answers, &store, &[&server]);
    session.assert_completed();

    assert_eq!(session.prompt(EXPORT_OFFER).answer.as_deref(), Some("no"));
    assert!(!session.entries().iter().any(|e| e.text == EXPORT_PATH));
    let mandate = stored_mandate(&store, &session.token());
    let default =
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("agent-mail-{}.toml", mandate.id));
    assert!(!default.exists());
    assert!(!Path::new(&format!("agent-mail-{}.toml", mandate.id)).exists());
}

/// A store whose writes always fail.
struct FailingStore;

impl MandateStore for FailingStore {
    fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(None)
    }
    fn put(&self, _: &[u8], _: &[u8]) -> Result<(), StoreError> {
        Err(StoreError::Unavailable)
    }
    fn delete(&self, _: &[u8]) -> Result<bool, StoreError> {
        Ok(false)
    }
    fn entries(&self) -> Result<Vec<agent_mail::store::Entry>, StoreError> {
        Ok(Vec::new())
    }
}

#[test]
fn ac7_no_export_offer_when_the_store_write_or_reveal_fails() {
    let server = FakeImap::start(Mode::Implicit);

    let session = create(None, ac1_answers(server.port), &FailingStore, &[&server]);
    assert_eq!(error_of(&session.output)["code"], "store_unavailable");
    assert_eq!(session.terminal.remaining(), 0);
    assert!(!session.entries().iter().any(|e| e.text == EXPORT_OFFER));

    let store = InMemoryStore::new();
    let session = session_on(
        ScriptedTerminal::failing_reveal(ac1_answers(server.port)),
        &["mandate", "create"],
        &HashMap::new(),
        &store,
        &[&server],
    );
    assert_eq!(error_of(&session.output)["code"], "aborted");
    assert_eq!(session.terminal.remaining(), 0);
    assert!(!session.entries().iter().any(|e| e.text == EXPORT_OFFER));
    assert!(store.entries().is_empty());
}

#[test]
fn export_never_overwrites_or_follows_a_symlink() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let dir = TempDir::new();
    let existing = dir.write("existing.toml", "keep me");
    let target = dir.write("target.txt", "untouched");
    let fresh = dir.join("fresh.toml");

    let mut answers = ac1_answers(server.port);
    answers.extend([yes(), Answer::text(existing.to_str().unwrap())]);
    // Creating a symbolic link needs extra rights on Windows.
    #[cfg(unix)]
    {
        let link = dir.join("link.toml");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        answers.push(Answer::text(link.to_str().unwrap()));
    }
    answers.push(Answer::text(fresh.to_str().unwrap()));
    let session = create(None, answers, &store, &[&server]);
    session.assert_completed();

    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "keep me");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "untouched");
    let refusals = session
        .entries()
        .iter()
        .filter(|e| e.kind == Kind::Message && e.text.contains("already exists"))
        .count();
    assert_eq!(refusals, if cfg!(unix) { 2 } else { 1 });
    assert!(MandateFile::from_toml(&std::fs::read_to_string(&fresh).unwrap()).is_ok());
}

#[test]
fn export_write_error_names_the_path_and_still_exits_zero() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let dir = TempDir::new();
    let path = dir.join("no-such-dir").join("m.toml");
    let mut answers = ac1_answers(server.port);
    answers.extend([yes(), Answer::text(path.to_str().unwrap())]);
    let session = create(None, answers, &store, &[&server]);
    session.assert_completed();

    let message = session
        .entries()
        .into_iter()
        .find(|e| e.kind == Kind::Message && e.text.starts_with("Could not write"))
        .expect("write error shown");
    assert!(message.text.contains(path.to_str().unwrap()));
    assert!(message.text.contains("not found"), "{:?}", message.text);
    assert_eq!(store.entries().len(), 1, "Mandate stays stored");
    assert!(!path.exists());
    assert!(dir.is_empty(), "a partial export was left behind");
}

#[test]
fn help_documents_from_and_still_no_credential_option() {
    let store = InMemoryStore::new();
    let terminal = ScriptedTerminal::new(Vec::new());
    let output = run_with_terminal(
        &["mandate", "create", "--help"],
        &HashMap::new(),
        &store,
        &terminal,
        vec![],
        &[],
    );
    assert_eq!(output.code, 0);
    let options = output.stdout.split_once("Options:").unwrap().1;
    assert!(options.contains("--from <FILE>"), "{options}");
    for line in options.lines().map(str::trim_start) {
        if line.starts_with('-') {
            let lower = line.to_lowercase();
            for word in ["password", "credential", "secret", "pass"] {
                assert!(!lower.contains(word), "{line:?}");
            }
        }
    }
}

#[test]
fn from_must_name_a_regular_file_and_is_never_opened_otherwise() {
    let server = FakeImap::start(Mode::Implicit);
    let dir = TempDir::new();
    assert_refused(&dir.0, &["is not a regular file"], &server);
    // Opening a FIFO for reading would block until a writer appears; the
    // check before opening refuses it at once. FIFOs are Unix-only.
    #[cfg(unix)]
    {
        let fifo = dir.join("pipe.toml");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(made.success());
        assert_refused(&fifo, &["is not a regular file"], &server);
    }
}

#[test]
fn credential_prompt_names_the_server_host_and_port() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let mut answers = ac1_answers(server.port);
    answers.push(no());
    let session = create(None, answers, &store, &[&server]);
    session.assert_completed();
    let hidden = session
        .entries()
        .into_iter()
        .find(|e| e.kind == Kind::Hidden)
        .unwrap();
    assert_eq!(
        hidden.text,
        format!(
            "Credential (password) for {LOGIN} on localhost:{}:",
            server.port
        )
    );
    assert!(!session.transcript().contains("looks like a Gmail address"));
}

#[test]
fn gmail_login_on_another_host_is_warned_before_the_credential() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let dir = TempDir::new();
    let gmail = "someone@Gmail.com";
    let text = format!("name = \"g\"\n\n{}", account_toml(gmail, server.port));
    let file = dir.write("g.toml", &text);
    let answers = vec![
        accept(),
        accept(),
        accept(),
        accept(),
        accept(),
        accept(),
        Answer::hidden(WRONG),
        Answer::choose(ABORT),
    ];
    let session = create(Some(&file), answers, &store, &[&server]);
    assert_eq!(error_of(&session.output)["code"], "aborted");
    let entries = session.entries();
    let warning = entries
        .iter()
        .position(|e| e.kind == Kind::Message && e.text.contains("looks like a Gmail address"))
        .expect("warning shown");
    let hidden = entries.iter().position(|e| e.kind == Kind::Hidden).unwrap();
    assert!(
        warning < hidden,
        "warning comes before the credential prompt"
    );
    let text = &entries[warning].text;
    assert!(text.contains(gmail));
    assert!(text.contains(&format!("localhost:{}", server.port)));
    assert!(text.contains("imap.gmail.com"));
    assert!(store.entries().is_empty());
}
