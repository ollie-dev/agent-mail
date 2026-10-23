//! `agent-mail mandate create` driven in-process through scripted terminal
//! sessions against the fake IMAP server and the in-memory store.

mod support;

use std::collections::HashMap;

use agent_mail::RunOutput;
use agent_mail::create::{
    ABORT, GENERIC, GMAIL, NOTHING_STORED, RETRY, SANDBOX_INDICATORS, TLS_IMPLICIT,
};
use agent_mail::mandate::{Credential, Mandate, Operation, TlsMode};
use agent_mail::seal::{self, AgentToken};
use agent_mail::store::{InMemoryStore, MandateStore};
use agent_mail::terminal::{Answer, Kind, ScriptedTerminal};
use serde_json::json;
use support::{
    CREDENTIAL, CountingStore, FakeImap, LOGIN, Mode, OTHER_LOGIN, contains_bytes, error_of,
    grant_on, run_with_terminal, tokens_in,
};

const WRONG: &str = "wrong-password";
const ENV_SECRET: &str = "env-supplied-secret";

/// One `mandate create` run: what it printed, what the terminal showed.
struct Session {
    output: RunOutput,
    terminal: ScriptedTerminal,
}

impl Session {
    fn transcript(&self) -> String {
        self.terminal.transcript_text()
    }

    /// The single Agent Token shown in the transcript.
    fn token(&self) -> String {
        let tokens = tokens_in(&self.transcript());
        assert_eq!(tokens.len(), 1, "expected one token in the transcript");
        tokens[0].clone()
    }
}

fn create_with(
    args: &[&str],
    answers: Vec<Answer>,
    env: &HashMap<String, String>,
    store: &dyn MandateStore,
    servers: &[&FakeImap],
    routes: &[(&str, u16, u16)],
) -> Session {
    let terminal = ScriptedTerminal::new(answers);
    let roots = servers.iter().map(|s| s.ca.clone()).collect();
    let output = run_with_terminal(args, env, store, &terminal, roots, routes);
    Session { output, terminal }
}

fn create(answers: Vec<Answer>, store: &dyn MandateStore, servers: &[&FakeImap]) -> Session {
    create_with(
        &["mandate", "create"],
        answers,
        &HashMap::new(),
        store,
        servers,
        &[],
    )
}

fn yes() -> Answer {
    Answer::Confirm(true)
}

fn no() -> Answer {
    Answer::Confirm(false)
}

/// Generic Account answers up to and including the hidden Credential.
fn generic(port: u16, login: &str, credential: &str) -> Vec<Answer> {
    vec![
        Answer::choose(GENERIC),
        Answer::text("localhost"),
        Answer::text(&port.to_string()),
        Answer::choose(TLS_IMPLICIT),
        Answer::text(login),
        Answer::hidden(credential),
    ]
}

/// AC1's script, with `credentials` answered in turn at the Credential
/// prompt (after the first, each is preceded by choosing `Retry`), and
/// `confirm` at the summary.
fn ac1_script(port: u16, credentials: &[&str], confirm: bool) -> Vec<Answer> {
    let mut script = vec![Answer::text("test-mandate")];
    script.extend(generic(port, LOGIN, credentials[0]));
    for credential in &credentials[1..] {
        script.push(Answer::choose(RETRY));
        script.push(Answer::hidden(credential));
    }
    script.extend([
        Answer::tick(&["INBOX", "Archive"]),
        Answer::tick(&["list", "search"]),
        no(),
        Answer::Confirm(confirm),
    ]);
    if confirm {
        // Decline the export offer shown after the Agent Token.
        script.push(no());
    }
    script
}

fn ac5_script(a: u16, b: u16) -> Vec<Answer> {
    let mut script = vec![Answer::text("two-accounts")];
    script.extend(generic(a, LOGIN, CREDENTIAL));
    script.extend([Answer::tick(&["INBOX"]), Answer::tick(&["list"]), yes()]);
    script.extend(generic(b, OTHER_LOGIN, CREDENTIAL));
    script.extend([
        Answer::tick(&["Archive"]),
        Answer::tick(&["list", "search"]),
        no(),
        yes(),
        no(),
    ]);
    script
}

/// Opens the store's only entry with `token`.
fn stored_mandate(store: &InMemoryStore, token: &str) -> Mandate {
    let entries = store.entries();
    assert_eq!(entries.len(), 1, "store should hold exactly one entry");
    let token = AgentToken::parse(token).expect("printed token parses");
    assert_eq!(entries[0].0, token.store_key());
    seal::open(&token, &entries[0].1).expect("printed token opens the record")
}

fn assert_completed(session: &Session) {
    assert_eq!(
        session.output.code, 0,
        "stderr: {:?}",
        session.output.stderr
    );
    assert_eq!(session.terminal.remaining(), 0, "script not used up");
}

fn assert_ac1_mandate(store: &InMemoryStore, session: &Session, server: &FakeImap) {
    let mandate = stored_mandate(store, &session.token());
    assert_eq!(mandate.name, "test-mandate");
    assert_eq!(mandate.accounts.len(), 1);
    let account = &mandate.accounts[0];
    assert_eq!(account.login, LOGIN);
    assert_eq!(account.host, "localhost");
    assert_eq!(account.port, server.port);
    assert_eq!(account.tls, TlsMode::Implicit);
    assert!(account.credential == Credential::new(CREDENTIAL));
    assert_eq!(
        mandate.policy.grants,
        vec![
            grant_on(Operation::List, LOGIN, &["INBOX", "Archive"]),
            grant_on(Operation::Search, LOGIN, &["INBOX", "Archive"]),
        ]
    );
    assert_eq!(mandate.id.len(), 8);
    assert!(mandate.created > 1_700_000_000);
}

fn ac1_session(store: &InMemoryStore, server: &FakeImap) -> Session {
    create(
        ac1_script(server.port, &[CREDENTIAL], true),
        store,
        &[server],
    )
}

#[test]
fn ac1_generic_account_session_stores_exactly_the_ticked_grants() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let session = ac1_session(&store, &server);

    assert_completed(&session);
    assert_ac1_mandate(&store, &session, &server);
    assert_eq!(tokens_in(&session.transcript()).len(), 1);
}

#[test]
fn ac2_printed_token_lists_exactly_the_ticked_mailboxes() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let session = ac1_session(&store, &server);
    assert_completed(&session);

    let token = session.token();
    let out = support::run(&["list"], Some(&token), &store, &server);
    assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
    let value: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(
        value,
        json!({"account": LOGIN, "mailboxes": [{"name": "INBOX"}, {"name": "Archive"}]})
    );
}

#[test]
fn ac3_gmail_preset_asks_only_address_and_app_password() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let script = vec![
        Answer::text("gmail-mandate"),
        Answer::choose(GMAIL),
        Answer::text(LOGIN),
        Answer::hidden(CREDENTIAL),
        Answer::tick(&["INBOX"]),
        Answer::tick(&["list"]),
        no(),
        yes(),
        no(),
    ];
    let session = create_with(
        &["mandate", "create"],
        script,
        &HashMap::new(),
        &store,
        &[&server],
        &[("imap.gmail.com", 993, server.port)],
    );
    assert_completed(&session);

    let transcript = session.terminal.transcript();
    let start = transcript
        .iter()
        .position(|e| e.answer.as_deref() == Some(GMAIL))
        .expect("Gmail chosen");
    let end = transcript
        .iter()
        .position(|e| e.kind == Kind::Checklist)
        .expect("checklist shown");
    let prompts: Vec<_> = transcript[start + 1..end]
        .iter()
        .filter(|e| e.kind != Kind::Message)
        .collect();
    let kinds: Vec<Kind> = prompts.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, [Kind::Text, Kind::Hidden], "{prompts:?}");
    for entry in &prompts {
        let text = entry.text.to_lowercase();
        for word in ["host", "port", "tls"] {
            assert!(!text.contains(word), "asked for {word}: {:?}", entry.text);
        }
    }

    let mandate = stored_mandate(&store, &session.token());
    let account = &mandate.accounts[0];
    assert_eq!(account.login, LOGIN);
    assert_eq!(account.host, "imap.gmail.com");
    assert_eq!(account.port, 993);
    assert_eq!(account.tls, TlsMode::Implicit);
}

#[test]
fn ac4_wrong_credential_can_be_retried_and_completes() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let session = create(
        ac1_script(server.port, &[WRONG, CREDENTIAL], true),
        &store,
        &[&server],
    );
    assert_completed(&session);

    let transcript = session.terminal.transcript();
    let failed = transcript
        .iter()
        .position(|e| e.kind == Kind::Message && e.text.contains("failed"))
        .expect("login-failed error shown");
    let error = &transcript[failed].text;
    assert!(error.contains(LOGIN), "error does not name the Account");
    assert!(!error.contains(WRONG) && !error.contains(CREDENTIAL));

    let choice = &transcript[failed + 1];
    assert_eq!(choice.kind, Kind::Choice);
    assert_eq!(choice.options, [RETRY, ABORT]);
    let next_prompt = transcript[failed + 2..]
        .iter()
        .find(|e| e.kind != Kind::Message)
        .unwrap();
    assert_eq!(
        next_prompt.kind,
        Kind::Hidden,
        "retry re-asks the Credential"
    );
    let after = transcript[failed + 2..]
        .iter()
        .filter(|e| e.kind != Kind::Message)
        .nth(1)
        .unwrap();
    assert_eq!(after.kind, Kind::Checklist, "retry continues to mailboxes");

    assert_ac1_mandate(&store, &session, &server);
}

#[test]
fn ac4_abort_after_wrong_credential_stores_nothing() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let mut script = vec![Answer::text("test-mandate")];
    script.extend(generic(server.port, LOGIN, WRONG));
    script.push(Answer::choose(ABORT));
    let session = create(script, &store, &[&server]);

    assert_ne!(session.output.code, 0);
    assert_eq!(error_of(&session.output)["code"], "aborted");
    assert!(session.transcript().contains(NOTHING_STORED));
    assert!(store.entries().is_empty());
    assert!(tokens_in(&session.transcript()).is_empty());
}

#[test]
fn ac5_two_accounts_make_one_mandate_with_separate_grants() {
    let a = FakeImap::start(Mode::Implicit);
    let b = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let session = create(ac5_script(a.port, b.port), &store, &[&a, &b]);
    assert_completed(&session);

    let mandate = stored_mandate(&store, &session.token());
    let logins: Vec<_> = mandate.accounts.iter().map(|a| a.login.as_str()).collect();
    assert_eq!(logins, [LOGIN, OTHER_LOGIN]);
    assert_eq!(mandate.account(LOGIN).unwrap().port, a.port);
    assert_eq!(mandate.account(OTHER_LOGIN).unwrap().port, b.port);
    assert_eq!(
        mandate.policy.grants,
        vec![
            grant_on(Operation::List, LOGIN, &["INBOX"]),
            grant_on(Operation::List, OTHER_LOGIN, &["Archive"]),
            grant_on(Operation::Search, OTHER_LOGIN, &["Archive"]),
        ]
    );
    assert!(!mandate.policy.grants.iter().any(|g| (g.account == LOGIN
        && g.mailboxes.contains(&"Archive".into()))
        || (g.account == OTHER_LOGIN && g.mailboxes.contains(&"INBOX".into()))));
    assert_eq!(tokens_in(&session.transcript()).len(), 1);
}

#[test]
fn ac5_same_login_twice_is_refused_and_reasked() {
    let a = FakeImap::start(Mode::Implicit);
    let b = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let mut script = vec![Answer::text("dupe")];
    script.extend(generic(a.port, LOGIN, CREDENTIAL));
    script.extend([Answer::tick(&["INBOX"]), Answer::tick(&["list"]), yes()]);
    script.extend([
        Answer::choose(GENERIC),
        Answer::text("localhost"),
        Answer::text(&b.port.to_string()),
        Answer::choose(TLS_IMPLICIT),
        Answer::text(LOGIN),
        Answer::text(OTHER_LOGIN),
        Answer::hidden(CREDENTIAL),
        Answer::tick(&["Archive"]),
        Answer::tick(&["list"]),
        no(),
        yes(),
        no(),
    ]);
    let session = create(script, &store, &[&a, &b]);
    assert_completed(&session);
    assert!(session.transcript().contains("already in this Mandate"));
    let mandate = stored_mandate(&store, &session.token());
    assert_eq!(mandate.accounts.len(), 2);
}

#[test]
fn ac6_declining_the_summary_stores_nothing() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let session = create(
        ac1_script(server.port, &[CREDENTIAL], false),
        &store,
        &[&server],
    );

    assert_ne!(session.output.code, 0);
    assert_eq!(session.terminal.remaining(), 0);
    let transcript = session.transcript();
    assert!(transcript.contains(NOTHING_STORED));
    assert!(tokens_in(&transcript).is_empty());
    assert!(!transcript.contains(seal::TOKEN_PREFIX));
    assert!(store.entries().is_empty());
}

#[test]
fn ac7_non_interactive_terminal_is_refused_before_any_prompt_or_store_access() {
    let store = CountingStore::default();
    let terminal = ScriptedTerminal::non_interactive();
    let output = run_with_terminal(
        &["mandate", "create"],
        &HashMap::new(),
        &store,
        &terminal,
        Vec::new(),
        &[],
    );

    assert_ne!(output.code, 0);
    assert_eq!(output.stdout, "");
    let error = error_of(&output);
    assert_eq!(error["code"], "interactive_terminal_required");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("must be run by a human in a terminal")
    );
    assert!(terminal.transcript().is_empty());
    assert_eq!(store.accesses(), 0);
    assert!(store.inner.entries().is_empty());
}

#[test]
fn ac7_binary_with_piped_stdin_and_stdout_refuses() {
    use std::process::{Command, Stdio};
    let output = Command::new(env!("CARGO_BIN_EXE_agent-mail"))
        .args(["mandate", "create"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run agent-mail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(output.status.code(), Some(0));
    assert!(
        stderr.contains("must be run by a human in a terminal"),
        "{stderr:?}"
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
}

#[test]
fn ac8_each_sandbox_indicator_is_refused() {
    for required in [
        "CLAUDECODE",
        "CLAUDE_CODE_ENTRYPOINT",
        "CODEX_SANDBOX",
        "CODEX_SANDBOX_NETWORK_DISABLED",
        "SANDBOX_RUNTIME",
        "OPENCODE",
    ] {
        assert!(SANDBOX_INDICATORS.contains(&required), "{required} missing");
    }
    for indicator in SANDBOX_INDICATORS {
        let store = CountingStore::default();
        let terminal = ScriptedTerminal::new(vec![Answer::text("never-asked")]);
        let env = HashMap::from([(indicator.to_string(), "1".to_string())]);
        let output = run_with_terminal(
            &["mandate", "create"],
            &env,
            &store,
            &terminal,
            Vec::new(),
            &[],
        );

        assert_ne!(output.code, 0, "{indicator}");
        let error = error_of(&output);
        assert_eq!(error["code"], "sandbox_detected", "{indicator}");
        let message = error["message"].as_str().unwrap();
        assert!(message.contains("cannot run inside an agent sandbox or session"));
        assert!(message.contains("must be run by the human in their own terminal"));
        assert!(terminal.transcript().is_empty(), "{indicator}: prompted");
        assert_eq!(store.accesses(), 0, "{indicator}: store touched");
    }

    // Without indicators the same session reaches the name prompt.
    let store = CountingStore::default();
    let terminal = ScriptedTerminal::new(vec![Answer::Cancel]);
    let output = run_with_terminal(
        &["mandate", "create"],
        &HashMap::new(),
        &store,
        &terminal,
        Vec::new(),
        &[],
    );
    let transcript = terminal.transcript();
    assert_eq!(transcript[0].kind, Kind::Text);
    assert_eq!(transcript[0].text, "Mandate name:");
    assert_eq!(error_of(&output)["code"], "aborted");
    assert!(store.inner.entries().is_empty());
}

#[test]
fn ac9_credential_flags_are_usage_errors() {
    for flag in ["--password", "--credential", "--app-password"] {
        let store = InMemoryStore::new();
        let session = create_with(
            &["mandate", "create", flag, "x"],
            Vec::new(),
            &HashMap::new(),
            &store,
            &[],
            &[],
        );
        assert_eq!(session.output.code, 2, "{flag}");
        assert!(
            session
                .output
                .stderr
                .contains(&format!("unknown option {flag};")),
            "{flag}: {:?}",
            session.output.stderr
        );
        assert!(
            session
                .output
                .stderr
                .contains("run `agent-mail mandate create --help` for usage")
        );
        assert!(session.terminal.transcript().is_empty());
        assert!(store.entries().is_empty());
    }
}

#[test]
fn ac9_help_lists_no_credential_option() {
    let store = InMemoryStore::new();
    let session = create_with(
        &["mandate", "create", "--help"],
        Vec::new(),
        &HashMap::new(),
        &store,
        &[],
        &[],
    );
    assert_eq!(session.output.code, 0);
    let help = &session.output.stdout;
    let options = help
        .split_once("Options:")
        .expect("help has an Options section")
        .1;
    let flags: Vec<&str> = options
        .lines()
        .map(str::trim_start)
        .filter(|l| l.starts_with('-'))
        .collect();
    assert!(!flags.is_empty());
    for line in flags {
        let lower = line.to_lowercase();
        for word in ["password", "credential", "secret", "pass"] {
            assert!(!lower.contains(word), "credential-bearing option: {line:?}");
        }
    }
    assert!(session.terminal.transcript().is_empty());
}

#[test]
fn ac9_environment_cannot_supply_the_credential() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let env: HashMap<String, String> = [
        "AGENT_MAIL_CREDENTIAL",
        "AGENT_MAIL_PASSWORD",
        "IMAP_PASSWORD",
    ]
    .iter()
    .map(|k| (k.to_string(), ENV_SECRET.to_string()))
    .collect();
    let session = create_with(
        &["mandate", "create"],
        ac1_script(server.port, &[CREDENTIAL], true),
        &env,
        &store,
        &[&server],
        &[],
    );
    assert_completed(&session);

    assert_eq!(
        session
            .terminal
            .transcript()
            .iter()
            .filter(|e| e.kind == Kind::Hidden)
            .count(),
        1,
        "hidden prompt still asked"
    );
    let token = session.token();
    let mandate = stored_mandate(&store, &token);
    assert!(mandate.accounts[0].credential == Credential::new(CREDENTIAL));
    assert!(mandate.accounts[0].credential != Credential::new(ENV_SECRET));
    assert!(!session.transcript().contains(ENV_SECRET));
    for (key, value) in store.entries() {
        assert!(!contains_bytes(&key, ENV_SECRET.as_bytes()));
        assert!(!contains_bytes(&value, ENV_SECRET.as_bytes()));
    }
    assert!(!session.output.stdout.contains(ENV_SECRET));
    assert!(!session.output.stderr.contains(ENV_SECRET));
}

fn assert_credentials_never_shown(session: &Session, store: &InMemoryStore) {
    let transcript = session.terminal.transcript();
    for entry in &transcript {
        let text = entry.text.to_lowercase();
        if text.contains("password") || text.contains("credential (") {
            assert_eq!(entry.kind, Kind::Hidden, "not hidden: {:?}", entry.text);
        }
        if entry.kind == Kind::Hidden {
            assert_eq!(entry.answer, None, "hidden answer recorded");
        }
    }
    assert!(transcript.iter().any(|e| e.kind == Kind::Hidden));
    let text = session.transcript();
    for secret in [CREDENTIAL, WRONG] {
        assert!(!text.contains(secret), "transcript holds a Credential");
        assert!(!session.output.stdout.contains(secret));
        assert!(!session.output.stderr.contains(secret));
        for (key, value) in store.entries() {
            assert!(!contains_bytes(&key, secret.as_bytes()));
            assert!(!contains_bytes(&value, secret.as_bytes()));
        }
    }
}

#[test]
fn ac10_credentials_are_hidden_and_never_shown_or_stored_in_clear() {
    let a = FakeImap::start(Mode::Implicit);
    let b = FakeImap::start(Mode::Implicit);

    let store = InMemoryStore::new();
    let session = ac1_session(&store, &a);
    assert_completed(&session);
    assert_credentials_never_shown(&session, &store);

    let store = InMemoryStore::new();
    let session = create(
        ac1_script(a.port, &[WRONG, CREDENTIAL], true),
        &store,
        &[&a],
    );
    assert_completed(&session);
    assert_eq!(
        session
            .terminal
            .transcript()
            .iter()
            .filter(|e| e.kind == Kind::Hidden)
            .count(),
        2
    );
    assert_credentials_never_shown(&session, &store);

    let store = InMemoryStore::new();
    let session = create(ac5_script(a.port, b.port), &store, &[&a, &b]);
    assert_completed(&session);
    assert_credentials_never_shown(&session, &store);
}

#[test]
fn ac10_real_terminal_reads_credentials_hidden_without_mask_or_echo() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/terminal.rs"),
    )
    .unwrap();
    let hidden = source
        .split("fn hidden(&self, prompt: &str) -> Result<Credential, Cancelled> {")
        .nth(1)
        .expect("InquireTerminal::hidden")
        .split("\n    }\n")
        .next()
        .unwrap();
    assert!(hidden.contains("inquire::Password::new"));
    assert!(hidden.contains("PasswordDisplayMode::Hidden"));
    assert!(!hidden.contains("Masked") && !hidden.contains("Full"));
    assert!(!hidden.contains("with_display_toggle_enabled"));
}

#[test]
fn ac11_token_is_shown_once_and_no_command_reveals_it_again() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let session = ac1_session(&store, &server);
    assert_completed(&session);

    let token = session.token();
    assert_eq!(session.transcript().matches(&token).count(), 1);
    let shown: Vec<_> = session
        .terminal
        .transcript()
        .into_iter()
        .filter(|e| e.text.contains(&token))
        .collect();
    assert_eq!(shown.len(), 1);
    assert_eq!(
        shown[0].kind,
        Kind::Reveal,
        "token not revealed via the tty"
    );
    assert!(!session.output.stdout.contains(&token));
    assert!(!session.output.stderr.contains(&token));
    for (key, value) in store.entries() {
        assert!(!contains_bytes(&key, token.as_bytes()));
        assert!(!contains_bytes(&value, token.as_bytes()));
    }

    let env = HashMap::from([(agent_mail::TOKEN_VAR.to_string(), token.clone())]);
    for args in [
        &["--help"][..],
        &["list"][..],
        &["--text", "list"][..],
        &["mandate", "create"][..],
    ] {
        let terminal = ScriptedTerminal::non_interactive();
        let out = run_with_terminal(args, &env, &store, &terminal, vec![server.ca.clone()], &[]);
        assert!(!out.stdout.contains(&token), "{args:?} printed the token");
        assert!(!out.stderr.contains(&token), "{args:?} printed the token");
        assert!(terminal.transcript().is_empty());
    }

    let second_store = InMemoryStore::new();
    let second = ac1_session(&second_store, &server);
    assert_completed(&second);
    assert_ne!(second.token(), token);
}

#[test]
fn checklist_shows_decoded_names_and_draft_grants_have_no_mailboxes() {
    // The fake server lists `[Gmail]/Entw&APw-rfe`: shown decoded.
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let mut script = vec![Answer::text("utf7")];
    script.extend(generic(server.port, LOGIN, CREDENTIAL));
    script.extend([
        Answer::tick(&["[Gmail]/Entwürfe"]),
        Answer::tick(&["list", "draft"]),
        no(),
        yes(),
        no(),
    ]);
    let session = create(script, &store, &[&server]);
    assert_completed(&session);
    let checklist = session
        .terminal
        .transcript()
        .into_iter()
        .find(|e| e.kind == Kind::Checklist)
        .unwrap();
    assert_eq!(
        checklist.options,
        ["INBOX", "Archive", "Sent", "Spam", "[Gmail]/Entwürfe"]
    );
    let mandate = stored_mandate(&store, &session.token());
    assert_eq!(
        mandate.policy.grants,
        vec![
            grant_on(Operation::List, LOGIN, &["[Gmail]/Entwürfe"]),
            grant_on(Operation::Draft, LOGIN, &[]),
        ]
    );
    assert!(session.transcript().contains("the Drafts mailbox"));
}

#[test]
fn ac11_token_that_cannot_be_shown_on_the_tty_leaves_nothing_stored() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let terminal = ScriptedTerminal::failing_reveal(ac1_script(server.port, &[CREDENTIAL], true));
    let output = run_with_terminal(
        &["mandate", "create"],
        &HashMap::new(),
        &store,
        &terminal,
        vec![server.ca.clone()],
        &[],
    );
    assert_ne!(output.code, 0);
    assert_eq!(error_of(&output)["code"], "aborted");
    assert!(store.entries().is_empty(), "unseen token's Mandate kept");
    let transcript = terminal.transcript_text();
    assert!(transcript.contains(NOTHING_STORED));
    assert!(tokens_in(&transcript).is_empty());
    assert!(tokens_in(&output.stdout).is_empty() && tokens_in(&output.stderr).is_empty());
}

#[test]
fn ac11_real_terminal_writes_the_token_to_dev_tty_only() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/terminal.rs"),
    )
    .unwrap();
    let real = source
        .split("impl Terminal for InquireTerminal {")
        .nth(1)
        .expect("InquireTerminal impl")
        .split("\n}\n")
        .next()
        .unwrap();
    let writer = source
        .split("fn write_tty(")
        .nth(1)
        .expect("write_tty")
        .split("\n}\n")
        .next()
        .unwrap();
    assert!(source.contains("const TTY_PATH: &str = \"/dev/tty\";"));
    assert!(writer.contains(".open(TTY_PATH)"));
    let reveal = real
        .split("fn reveal(")
        .nth(1)
        .expect("InquireTerminal::reveal");
    assert!(reveal.contains("write_tty("));
    for forbidden in ["stdout", "stderr", "print!", "println!", "eprint"] {
        assert!(!reveal.contains(forbidden), "reveal uses {forbidden}");
        assert!(!writer.contains(forbidden), "write_tty uses {forbidden}");
    }
}
