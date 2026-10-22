//! `agent-mail move` driven in-process against the fake IMAP server
//! (ticket 10). Named `move_message` because `move` is a Rust keyword.

mod support;

use std::path::PathBuf;

use agent_mail::RunOutput;
use agent_mail::mandate::{Grant, Mandate, Operation, Policy, TlsMode};
use agent_mail::seal::AgentToken;
use agent_mail::store::InMemoryStore;
use serde_json::{Map, Value, json};
use support::{
    CREDENTIAL, FILING_UIDVALIDITY, FakeImap, INBOX_UIDVALIDITY, LOGIN, Mode, OTHER_LOGIN, account,
    assert_no_secrets, audit_lines, contains_bytes, error_of, grant, run_logged, store_mandate,
    temp_audit_log, token_hash,
};

/// Every fixture mailbox, raw names.
const ALL_MAILBOXES: [&str; 8] = [
    "INBOX",
    "Archive",
    "Sent",
    "Spam",
    "[Gmail]/Entw&APw-rfe",
    "Read",
    "Triage",
    "Filing",
];

type Snapshot = Vec<(String, Vec<(u32, String, Vec<String>)>)>;

/// A fake server, a store holding Mandate `aaaa1111` with `grants`, its
/// token and a fresh audit log.
struct Setup {
    server: FakeImap,
    store: InMemoryStore,
    token: AgentToken,
    log: PathBuf,
}

impl Setup {
    /// The fake advertising `capabilities`, the Mandate holding `grants`.
    fn with(capabilities: &[&str], grants: Vec<Grant>) -> Self {
        let server = FakeImap::start_with_capabilities(Mode::Implicit, capabilities);
        Self::on(server, grants)
    }

    fn on(server: FakeImap, grants: Vec<Grant>) -> Self {
        let store = InMemoryStore::new();
        let mandate = Mandate {
            name: "work-inbox".to_string(),
            id: "aaaa1111".to_string(),
            created: 1_792_483_200,
            accounts: vec![account(LOGIN, server.port, TlsMode::Implicit)],
            policy: Policy { grants },
        };
        let token = store_mandate(&store, &mandate);
        Self {
            server,
            store,
            token,
            log: temp_audit_log(),
        }
    }

    fn default_grants() -> Vec<Grant> {
        vec![
            grant(Operation::Move, &["INBOX", "Archive", "Filing"]),
            grant(Operation::Search, &["*"]),
        ]
    }

    /// The default fake (`MOVE` and `UIDPLUS`) with `grants`.
    fn new(grants: Vec<Grant>) -> Self {
        Self::with(&["MOVE", "UIDPLUS"], grants)
    }

    /// The default fake and the default Grants.
    fn granted() -> Self {
        Self::new(Self::default_grants())
    }

    /// Runs `agent-mail <args>` and checks no secret is printed.
    fn run(&self, args: &[&str]) -> RunOutput {
        let out = run_logged(
            args,
            Some(self.token.expose()),
            &self.store,
            &self.server.ca,
            &self.log,
        );
        assert_no_secrets(&out, Some(self.token.expose()));
        out
    }

    /// Runs `agent-mail <args>`, expects success and one line, returns it.
    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert_eq!(out.code, 0, "{args:?}: stderr {:?}", out.stderr);
        assert_eq!(out.stderr, "");
        assert!(out.stdout.ends_with('\n') && out.stdout.lines().count() == 1);
        out.stdout
    }

    /// Runs `agent-mail move <args>`, expects success, returns the JSON.
    fn move_json(&self, args: &[&str]) -> Value {
        let argv: Vec<&str> = std::iter::once("move")
            .chain(args.iter().copied())
            .collect();
        let stdout = self.ok(&argv);
        serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout:?}"))
    }

    /// Runs `agent-mail <args>`, expects exit 1 and empty stdout, returns
    /// the parsed error.
    fn fails(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert_eq!(out.code, 1, "{args:?}: stdout {:?}", out.stdout);
        assert_eq!(out.stdout, "", "{args:?}");
        error_of(&out)
    }

    fn snapshot(&self) -> Snapshot {
        ALL_MAILBOXES
            .iter()
            .map(|m| (m.to_string(), self.server.messages_in(m)))
            .collect()
    }

    fn audit(&self) -> Vec<String> {
        audit_lines(&self.log)
    }

    fn audit_bytes(&self) -> Vec<u8> {
        std::fs::read(&self.log).unwrap_or_default()
    }
}

/// `I(n)`: the reference of UID `n` in the fixture `INBOX`.
fn i(uid: u32) -> String {
    format!("{LOGIN}/INBOX/{INBOX_UIDVALIDITY}/{uid}")
}

/// `F(n)`: the reference of UID `n` in the fixture `Filing`.
fn f(uid: u32) -> String {
    format!("{LOGIN}/Filing/{FILING_UIDVALIDITY}/{uid}")
}

const ARCHIVE_26: &str = "agent@example.test/Archive/1700000002/26";

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

fn parse_line(line: &str) -> Map<String, Value> {
    match serde_json::from_str::<Value>(line) {
        Ok(Value::Object(map)) => map,
        other => panic!("not a JSON object: {other:?}"),
    }
}

/// Key names of the top-level object `line` in the order they appear.
fn keys_in_order(line: &str) -> Vec<String> {
    let mut found: Vec<(usize, String)> = parse_line(line)
        .keys()
        .map(|k| (line.find(&format!("\"{k}\":")).unwrap(), k.clone()))
        .collect();
    found.sort();
    found.into_iter().map(|(_, k)| k).collect()
}

/// The audit line without `time`, after checking `time` is RFC 3339 UTC
/// to the second.
fn entry(line: &str) -> Value {
    let mut entry = parse_line(line);
    let time = entry.remove("time").expect("entry has a time");
    let time = time.as_str().unwrap();
    assert_eq!(time.len(), 20, "{time}");
    assert!(time.ends_with('Z') && time.as_bytes()[10] == b'T', "{time}");
    Value::Object(entry)
}

/// The sorted keys of an error object.
fn error_keys(error: &Value) -> Vec<String> {
    let mut keys: Vec<String> = error.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

fn assert_sent_none(server: &FakeImap, forbidden: &[&str]) {
    let commands = server.commands();
    for verb in forbidden {
        assert!(
            !commands.iter().any(|c| c == verb),
            "{verb} sent: {commands:?}"
        );
    }
}

/// Asserts no fetch item other than `UID` and `FLAGS` was requested.
fn assert_only_uid_and_flags_fetched(server: &FakeImap) {
    for fetch in server.fetches() {
        let (_, items) = fetch.split_once(' ').unwrap_or((&fetch, ""));
        let items = items.trim_start_matches('(').trim_end_matches(')');
        for item in items.split_whitespace() {
            assert!(
                item.eq_ignore_ascii_case("UID") || item.eq_ignore_ascii_case("FLAGS"),
                "fetch item {item:?} requested in {fetch:?}"
            );
        }
    }
}

#[test]
fn ac1_moving_leaves_the_message_in_archive_and_removes_it_from_inbox() {
    let s = Setup::granted();
    let inbox_before = s.server.messages_in("INBOX");
    let out = s.ok(&["move", &i(2), "--to", "Archive"]);
    assert_eq!(
        out,
        "{\"ref\":\"agent@example.test/INBOX/1700000001/2\",\"account\":\"agent@example.test\",\"mailbox\":\"INBOX\",\"target\":\"Archive\",\"new_ref\":\"agent@example.test/Archive/1700000002/26\"}\n"
    );
    let inbox = s.server.messages_in("INBOX");
    let expected: Vec<_> = inbox_before.into_iter().filter(|m| m.0 != 2).collect();
    assert_eq!(inbox, expected);
    assert_eq!(inbox.iter().map(|m| m.0).collect::<Vec<_>>(), vec![1, 3, 4]);
    let archive = s.server.messages_in("Archive");
    assert_eq!(
        archive.iter().map(|m| m.0).collect::<Vec<_>>(),
        (1..=26).collect::<Vec<_>>()
    );
    assert_eq!(archive[25], (26, "Lunch?".to_string(), Vec::new()));

    let commands = s.server.commands();
    assert!(commands.iter().any(|c| c == "SELECT"), "{commands:?}");
    assert!(commands.iter().any(|c| c == "UID MOVE"), "{commands:?}");
    assert_sent_none(
        &s.server,
        &[
            "UID COPY",
            "COPY",
            "STORE",
            "UID STORE",
            "EXPUNGE",
            "UID EXPUNGE",
            "CLOSE",
        ],
    );
    assert_only_uid_and_flags_fetched(&s.server);

    // --text, after the subcommand or global, each on a fresh server.
    let t = Setup::granted();
    assert_eq!(
        t.ok(&["move", &i(2), "--to", "Archive", "--text"]),
        format!("agent@example.test/INBOX/1700000001/2 -> {ARCHIVE_26}\n")
    );
    let g = Setup::granted();
    assert_eq!(
        g.ok(&["--text", "move", &i(2), "--to", "Archive"]),
        format!("agent@example.test/INBOX/1700000001/2 -> {ARCHIVE_26}\n")
    );
}

/// One denial case against a fresh server and audit log: exits 1, names
/// `mailbox` as the one lacking the Grant, makes no connection, changes
/// nothing, and logs one denied entry.
fn assert_denied(grants: Vec<Grant>, reference: &str, account: &str, mailbox: &str) {
    let s = Setup::new(grants);
    let before = s.snapshot();
    let error = s.fails(&["move", reference, "--to", "Archive"]);
    assert_eq!(
        error_keys(&error),
        ["account", "code", "mailbox", "message", "operation"]
    );
    assert_eq!(error["code"], json!("denied"));
    assert_eq!(error["operation"], json!("move"));
    assert_eq!(error["account"], json!(account));
    assert_eq!(error["mailbox"], json!(mailbox));
    assert!(error["message"].is_string());
    assert_eq!(s.server.connections(), 0, "the server saw a connection");
    assert_eq!(s.snapshot(), before);
    let lines = s.audit();
    assert_eq!(lines.len(), 1, "{lines:?}");
    let mut without_mandate = entry(&lines[0]);
    without_mandate.as_object_mut().unwrap().remove("mandate");
    assert_eq!(
        without_mandate,
        json!({
            "operation": "move",
            "account": account,
            "mailbox": "INBOX",
            "target": "Archive",
            "messages": [reference],
            "outcome": "denied",
            "reason": "denied"
        })
    );
    assert_eq!(
        keys_in_order(&lines[0])
            .into_iter()
            .filter(|k| k != "mandate")
            .collect::<Vec<_>>(),
        [
            "time",
            "operation",
            "account",
            "mailbox",
            "target",
            "messages",
            "outcome",
            "reason"
        ]
    );
    // An Account the Mandate does not cover selects no Mandate, so the
    // line names none.
    let expected_mandate = (account == LOGIN).then_some("aaaa1111");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&lines[0]).unwrap()["mandate"].as_str(),
        expected_mandate
    );
}

#[test]
fn ac2_move_granted_only_on_the_source_is_denied_and_audited() {
    assert_denied(
        vec![grant(Operation::Move, &["INBOX"])],
        &i(2),
        LOGIN,
        "Archive",
    );
    // An empty Policy: the source is checked first.
    assert_denied(Vec::new(), &i(2), LOGIN, "INBOX");
    // An Account the Mandate does not cover.
    assert_denied(
        Setup::default_grants(),
        &format!("{OTHER_LOGIN}/INBOX/{INBOX_UIDVALIDITY}/2"),
        OTHER_LOGIN,
        "INBOX",
    );
}

#[test]
fn ac3_move_granted_only_on_the_target_is_denied() {
    assert_denied(
        vec![
            grant(Operation::Move, &["Archive"]),
            grant(Operation::Read, &["INBOX"]),
            grant(Operation::Search, &["INBOX"]),
        ],
        &i(2),
        LOGIN,
        "INBOX",
    );
}

#[test]
fn ac4_uidplus_fallback_moves_only_that_message() {
    let s = Setup::with(&["UIDPLUS"], Setup::default_grants());
    let moved = s.move_json(&[&f(1), "--to", "Archive"]);
    assert_eq!(moved["new_ref"], json!(ARCHIVE_26));
    assert_eq!(
        s.server.messages_in("Filing"),
        vec![
            (2, "Filing two".to_string(), strings(&["\\Deleted"])),
            (3, "Filing three".to_string(), strings(&["\\Seen"])),
        ]
    );
    let archive = s.server.messages_in("Archive");
    assert_eq!(archive.len(), 26);
    assert_eq!(archive[25], (26, "Filing one".to_string(), Vec::new()));

    let commands = s.server.commands();
    let at = |verb: &str| {
        commands
            .iter()
            .position(|c| c == verb)
            .unwrap_or_else(|| panic!("{verb} not sent: {commands:?}"))
    };
    assert!(at("UID COPY") < at("UID STORE"), "{commands:?}");
    assert!(at("UID STORE") < at("UID EXPUNGE"), "{commands:?}");
    assert_sent_none(&s.server, &["UID MOVE", "MOVE", "EXPUNGE", "CLOSE"]);
    let stores = s.server.stores();
    assert_eq!(stores.len(), 1, "{stores:?}");
    assert!(
        stores[0] == "1 +FLAGS.SILENT (\\Deleted)" || stores[0] == "1 +FLAGS (\\Deleted)",
        "{stores:?}"
    );
    assert_only_uid_and_flags_fetched(&s.server);
}

#[test]
fn ac5_a_server_without_move_or_uidplus_is_refused() {
    let s = Setup::on(FakeImap::start(Mode::Implicit), Setup::default_grants());
    let before = s.snapshot();
    let error = s.fails(&["move", &f(1), "--to", "Archive"]);
    assert_eq!(
        error_keys(&error),
        ["account", "code", "mailbox", "message", "operation"]
    );
    assert_eq!(error["code"], json!("move_unsupported"));
    assert_eq!(error["operation"], json!("move"));
    assert_eq!(error["account"], json!(LOGIN));
    assert_eq!(error["mailbox"], json!("Filing"));
    let message = error["message"].as_str().unwrap();
    for needle in ["neither MOVE nor UIDPLUS", "nothing was moved"] {
        assert!(message.contains(needle), "{message}");
    }
    assert_sent_none(
        &s.server,
        &[
            "UID MOVE",
            "MOVE",
            "UID COPY",
            "COPY",
            "UID STORE",
            "STORE",
            "UID EXPUNGE",
            "EXPUNGE",
            "CLOSE",
        ],
    );
    assert_eq!(s.snapshot(), before);
    let lines = s.audit();
    assert_eq!(lines.len(), 1, "{lines:?}");
    let logged = entry(&lines[0]);
    assert_eq!(logged["outcome"], json!("allowed"));
    assert_eq!(logged["reason"], json!("move_unsupported"));

    let text = s.run(&["--text", "move", &f(1), "--to", "Archive"]);
    assert_eq!(text.code, 1);
    assert_eq!(text.stdout, "");
    assert!(
        text.stderr.starts_with("error: move_unsupported: "),
        "{:?}",
        text.stderr
    );
    assert_eq!(s.snapshot(), before);
}

#[test]
fn ac6_a_stale_reference_moves_nothing() {
    let s = Setup::granted();
    let before = s.snapshot();
    let stale = format!("{LOGIN}/INBOX/1700000099/2");
    let error = s.fails(&["move", &stale, "--to", "Archive"]);
    assert_eq!(
        error_keys(&error),
        ["account", "code", "mailbox", "message", "operation"]
    );
    assert_eq!(error["code"], json!("stale_reference"));
    assert_eq!(error["operation"], json!("move"));
    assert_eq!(error["account"], json!(LOGIN));
    assert_eq!(error["mailbox"], json!("INBOX"));
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("nothing was moved"), "{message}");
    assert!(
        message.contains("search") && message.contains("fresh reference"),
        "{message}"
    );
    let commands = s.server.commands();
    assert!(commands.iter().any(|c| c == "SELECT"), "{commands:?}");
    assert_sent_none(
        &s.server,
        &[
            "UID FETCH",
            "UID MOVE",
            "UID COPY",
            "UID STORE",
            "UID EXPUNGE",
        ],
    );
    assert_eq!(s.snapshot(), before);
    let lines = s.audit();
    assert_eq!(lines.len(), 1, "{lines:?}");
    let logged = entry(&lines[0]);
    assert_eq!(logged["outcome"], json!("allowed"));
    assert_eq!(logged["reason"], json!("stale_reference"));

    // A missing UID: message_not_found, no move or copy.
    let error = s.fails(&["move", &i(99), "--to", "Archive"]);
    assert_eq!(error["code"], json!("message_not_found"));
    assert_eq!(error["operation"], json!("move"));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("nothing was moved")
    );
    assert_sent_none(&s.server, &["UID MOVE", "UID COPY"]);
    assert_eq!(s.snapshot(), before);
}

#[test]
fn ac7_the_result_returns_the_new_reference_where_reported() {
    let mut grants = Setup::default_grants();
    grants.push(grant(Operation::Read, &["Archive"]));
    let s = Setup::new(grants);
    let moved = s.move_json(&[&i(2), "--to", "Archive"]);
    assert_eq!(moved["new_ref"], json!(ARCHIVE_26));
    let lines = s.audit();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(
        entry(&lines[0]),
        json!({
            "mandate": "aaaa1111",
            "operation": "move",
            "account": LOGIN,
            "mailbox": "INBOX",
            "target": "Archive",
            "messages": [i(2), ARCHIVE_26],
            "outcome": "allowed"
        })
    );
    assert_eq!(
        keys_in_order(&lines[0]),
        [
            "time",
            "mandate",
            "operation",
            "account",
            "mailbox",
            "target",
            "messages",
            "outcome"
        ]
    );

    // new_ref works as a reference.
    let read: Value = serde_json::from_str(&s.ok(&["read", ARCHIVE_26])).unwrap();
    assert_eq!(read["subject"], json!("Lunch?"));

    // MOVE without UIDPLUS: no COPYUID, so new_ref is null.
    let m = Setup::with(&["MOVE"], Setup::default_grants());
    let moved = m.move_json(&[&i(2), "--to", "Archive"]);
    assert_eq!(moved["new_ref"], Value::Null);
    let archive = m.server.messages_in("Archive");
    assert_eq!(archive[25], (26, "Lunch?".to_string(), Vec::new()));
    let lines = m.audit();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(entry(&lines[0])["messages"], json!([i(2)]));
    let t = Setup::with(&["MOVE"], Setup::default_grants());
    assert_eq!(
        t.ok(&["move", &i(2), "--to", "Archive", "--text"]),
        "agent@example.test/INBOX/1700000001/2 -> Archive (new reference not reported)\n"
    );

    // Other operations, for the `target` check below.
    assert_eq!(s.run(&["search", "--mailbox", "Archive"]).code, 0);
    assert_eq!(s.run(&["list"]).code, 1);
    assert_eq!(s.run(&["flag", ARCHIVE_26, "--set", "seen"]).code, 1);
    for setup in [&s, &m, &t] {
        let log = setup.audit_bytes();
        let token = setup.token.expose().to_string();
        let hash = token_hash(&token);
        for needle in [
            "Lunch?",
            "bob@example.com",
            CREDENTIAL,
            token.as_str(),
            hash.as_str(),
        ] {
            assert!(
                !contains_bytes(&log, needle.as_bytes()),
                "audit log contains {needle:?}"
            );
        }
        for line in setup.audit() {
            let parsed = parse_line(&line);
            let operation = parsed["operation"].as_str().unwrap();
            if operation != "move" {
                assert!(!parsed.contains_key("target"), "{line}");
            }
            assert!(!parsed.contains_key("results") || operation == "search");
            assert!(!parsed.contains_key("change") || operation == "flag");
        }
    }
    let operations: Vec<String> = s
        .audit()
        .iter()
        .map(|l| parse_line(l)["operation"].as_str().unwrap().to_string())
        .collect();
    for op in ["move", "read", "search", "list", "flag"] {
        assert!(operations.iter().any(|o| o == op), "{operations:?}");
    }
}

#[test]
fn moving_to_the_same_mailbox_fails_before_any_connection() {
    let s = Setup::granted();
    let before = s.snapshot();
    let error = s.fails(&["move", &i(2), "--to", "INBOX"]);
    assert_eq!(
        error_keys(&error),
        ["account", "code", "mailbox", "message", "operation"]
    );
    assert_eq!(error["code"], json!("same_mailbox"));
    assert_eq!(error["operation"], json!("move"));
    assert_eq!(error["account"], json!(LOGIN));
    assert_eq!(error["mailbox"], json!("INBOX"));
    assert_eq!(s.server.connections(), 0);
    assert_eq!(s.snapshot(), before);
    let lines = s.audit();
    assert_eq!(lines.len(), 1, "{lines:?}");
    let logged = entry(&lines[0]);
    assert_eq!(logged["outcome"], json!("denied"));
    assert_eq!(logged["reason"], json!("same_mailbox"));
}

#[test]
fn a_missing_target_fails_with_mailbox_not_found() {
    let s = Setup::new(vec![grant(Operation::Move, &["*"])]);
    let before = s.snapshot();
    let error = s.fails(&["move", &i(2), "--to", "Missing"]);
    assert_eq!(error["code"], json!("mailbox_not_found"));
    assert_eq!(error["mailbox"], json!("Missing"));
    assert_eq!(s.snapshot(), before);

    // The same through the UIDPLUS fallback.
    let u = Setup::with(&["UIDPLUS"], vec![grant(Operation::Move, &["*"])]);
    let error = u.fails(&["move", &f(1), "--to", "Missing"]);
    assert_eq!(error["code"], json!("mailbox_not_found"));
    assert_eq!(error["mailbox"], json!("Missing"));
    assert_eq!(u.snapshot(), before);
    assert_sent_none(&u.server, &["UID STORE", "UID EXPUNGE"]);
}

#[test]
fn usage_errors_make_no_connection_and_no_audit_entry() {
    let s = Setup::granted();
    for args in [
        &["move", "not-a-ref", "--to", "Archive"][..],
        &["move", &i(2)],
        &["move", &i(2), "--to", ""],
        &["move", &i(2), "--to", "Arc\nhive"],
        &["move", &i(2), "--to", "Arc\rhive"],
        &["move", &i(2), "--to", "Arc\0hive"],
    ] {
        let out = s.run(args);
        assert_eq!(out.code, 2, "{args:?}");
        assert_eq!(out.stdout, "", "{args:?}");
    }
    assert_eq!(s.server.connections(), 0);
    assert!(s.audit().is_empty());
}

#[test]
fn move_help_explains_the_reference_grants_and_result() {
    let s = Setup::granted();
    let out = s.run(&["move", "--help"]);
    assert_eq!(out.code, 0);
    for needle in [
        "REF",
        "search",
        "--to",
        "list",
        "both mailboxes",
        "new_ref",
        "null",
        "target",
        "stale_reference",
        "move_unsupported",
    ] {
        assert!(out.stdout.contains(needle), "help lacks {needle:?}");
    }
}
