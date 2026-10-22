//! `agent-mail flag` driven in-process against the fake IMAP server
//! (ticket 09).

mod support;

use std::path::PathBuf;

use agent_mail::RunOutput;
use agent_mail::mandate::{Grant, Mandate, Operation, Policy, TlsMode};
use agent_mail::seal::AgentToken;
use agent_mail::store::InMemoryStore;
use serde_json::{Map, Value, json};
use support::{
    CREDENTIAL, FakeImap, LOGIN, Mode, OTHER_LOGIN, READ_UIDVALIDITY, TRIAGE_UIDVALIDITY, account,
    assert_no_secrets, audit_lines, contains_bytes, error_of, grant, run_logged, store_mandate,
    temp_audit_log, token_hash,
};

/// A fake server, a store holding Mandate `aaaa1111` with `grants`, its
/// token and a fresh audit log.
struct Setup {
    server: FakeImap,
    store: InMemoryStore,
    token: AgentToken,
    log: PathBuf,
}

impl Setup {
    fn new(grants: Vec<Grant>) -> Self {
        let server = FakeImap::start(Mode::Implicit);
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
            grant(Operation::Flag, &["Triage"]),
            grant(Operation::Search, &["*"]),
        ]
    }

    /// The default Grants: `flag` on `Triage`, `search` on `*`.
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

    /// Runs `agent-mail flag <args>`, expects success, returns stdout.
    fn flag(&self, args: &[&str]) -> String {
        let argv: Vec<&str> = std::iter::once("flag")
            .chain(args.iter().copied())
            .collect();
        let out = self.run(&argv);
        assert_eq!(out.code, 0, "{argv:?}: stderr {:?}", out.stderr);
        assert_eq!(out.stderr, "");
        assert!(out.stdout.ends_with('\n') && out.stdout.lines().count() == 1);
        out.stdout
    }

    /// Runs `agent-mail flag <args>`, expects success, returns the JSON.
    fn flag_json(&self, args: &[&str]) -> Value {
        let stdout = self.flag(args);
        serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout:?}"))
    }

    /// The flags of `Triage` UID `uid` on the fake server.
    fn triage_flags(&self, uid: u32) -> Vec<String> {
        self.server
            .all_flags()
            .into_iter()
            .find(|(m, u, _)| m == "Triage" && *u == uid)
            .map(|(_, _, flags)| flags)
            .expect("Triage message exists")
    }

    fn audit(&self) -> Vec<String> {
        audit_lines(&self.log)
    }

    fn audit_bytes(&self) -> Vec<u8> {
        std::fs::read(&self.log).unwrap_or_default()
    }
}

/// `T(n)`: the reference of UID `n` in the fixture `Triage`.
fn t(uid: u32) -> String {
    format!("{LOGIN}/Triage/{TRIAGE_UIDVALIDITY}/{uid}")
}

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

/// Asserts the fake saw no `CLOSE`, `EXPUNGE` or `UID EXPUNGE` and no
/// fetch item other than `UID` and `FLAGS`.
fn assert_no_side_effects(server: &FakeImap) {
    let commands = server.commands();
    for forbidden in ["CLOSE", "EXPUNGE", "UID EXPUNGE"] {
        assert!(
            !commands.iter().any(|c| c == forbidden),
            "{forbidden} sent: {commands:?}"
        );
    }
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
fn ac1_marking_read_sets_and_unread_clears_seen() {
    let s = Setup::granted();
    let t2_before = s.triage_flags(2);
    let out = s.flag(&[&t(1), "--set", "seen"]);
    assert_eq!(
        out,
        "{\"ref\":\"agent@example.test/Triage/1700000007/1\",\"account\":\"agent@example.test\",\"mailbox\":\"Triage\",\"flags\":[\"\\\\Seen\"]}\n"
    );
    assert_eq!(s.triage_flags(1), strings(&["\\Seen"]));
    assert_eq!(s.triage_flags(2), t2_before);
    let commands = s.server.commands();
    assert!(commands.iter().any(|c| c == "SELECT"), "{commands:?}");
    assert!(commands.iter().any(|c| c == "UID STORE"), "{commands:?}");

    let cleared = s.flag_json(&[&t(1), "--clear", "seen"]);
    assert_eq!(cleared["flags"], json!([]));
    assert_eq!(s.triage_flags(1), Vec::<String>::new());

    let t2 = s.flag_json(&[&t(2), "--clear", "\\SEEN"]);
    assert_eq!(t2["flags"], json!(["\\Flagged", "\\Answered"]));
    assert_eq!(s.triage_flags(2), strings(&["\\Flagged", "\\Answered"]));

    // Clearing a flag that is not set is harmless.
    let again = s.flag_json(&[&t(1), "--clear", "seen"]);
    assert_eq!(again["flags"], json!([]));
    assert_eq!(s.triage_flags(1), Vec::<String>::new());

    // --text, after the subcommand or global.
    assert_eq!(
        s.flag(&[&t(1), "--set", "seen", "--text"]),
        "agent@example.test/Triage/1700000007/1: \\Seen\n"
    );
    let out = s.run(&["--text", "flag", &t(1), "--clear", "seen"]);
    assert_eq!(out.code, 0, "{:?}", out.stderr);
    assert_eq!(
        out.stdout,
        "agent@example.test/Triage/1700000007/1: no flags\n"
    );

    assert_no_side_effects(&s.server);
}

#[test]
fn ac2_starring_sets_and_unstarring_clears_flagged() {
    let s = Setup::granted();
    let starred = s.flag_json(&[&t(1), "--set", "flagged"]);
    assert_eq!(starred["flags"], json!(["\\Flagged"]));
    assert_eq!(s.triage_flags(1), strings(&["\\Flagged"]));

    let unstarred = s.flag_json(&[&t(1), "--clear", "flagged"]);
    assert_eq!(unstarred["flags"], json!([]));
    assert_eq!(s.triage_flags(1), Vec::<String>::new());

    let t2 = s.flag_json(&[&t(2), "--clear", "\\Flagged"]);
    assert_eq!(t2["flags"], json!(["\\Seen", "\\Answered"]));

    let stores = s.server.stores();
    let matches = |want: &str, silent: &str| stores.iter().any(|st| st == want || st == silent);
    assert!(
        matches("1 +FLAGS (\\Flagged)", "1 +FLAGS.SILENT (\\Flagged)"),
        "{stores:?}"
    );
    assert!(
        matches("1 -FLAGS (\\Flagged)", "1 -FLAGS.SILENT (\\Flagged)"),
        "{stores:?}"
    );
    assert_no_side_effects(&s.server);
}

#[test]
fn ac3_any_other_flag_is_rejected_with_a_structured_error() {
    let s = Setup::granted();
    let before = s.server.all_flags();
    for (action, value) in [
        ("--set", "deleted"),
        ("--set", "\\Deleted"),
        ("--clear", "\\Answered"),
        ("--set", "$Junk"),
        ("--set", "\\Draft"),
        ("--set", ""),
        ("--set", "seen\r\n"),
    ] {
        let out = s.run(&["flag", &t(1), action, value]);
        assert_eq!(out.code, 1, "{value:?}: {:?}", out.stderr);
        assert_eq!(out.stdout, "", "{value:?}");
        let error = error_of(&out);
        let mut keys: Vec<&String> = error.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(keys, ["code", "message", "operation"], "{value:?}");
        assert_eq!(error["code"], json!("unsupported_flag"));
        assert_eq!(error["operation"], json!("flag"));
        let message = error["message"].as_str().unwrap();
        assert!(
            message.contains("`seen`") && message.contains("`flagged`"),
            "{message}"
        );
        if !value.is_empty() && value != "seen\r\n" {
            assert!(!message.contains(value), "{value:?} echoed: {message}");
        }
        assert!(!message.contains('\r') && !message.contains('\n'));

        let text = s.run(&["--text", "flag", &t(1), action, value]);
        assert_eq!(text.code, 1);
        assert!(
            text.stderr.starts_with("error: unsupported_flag: "),
            "{:?}",
            text.stderr
        );
    }
    assert_eq!(s.server.connections(), 0, "the server saw a connection");
    assert_eq!(s.server.all_flags(), before);

    // Usage errors: no action, or both.
    for args in [
        &["flag", &t(1)][..],
        &["flag", &t(1), "--set", "seen", "--clear", "flagged"],
        &["flag", &t(1), "--set", "seen", "--set", "flagged"],
    ] {
        let out = s.run(args);
        assert_eq!(out.code, 2, "{args:?}");
        assert_eq!(out.stdout, "", "{args:?}");
    }
    assert_eq!(s.server.connections(), 0);
}

/// AC4: one denial case against a fresh audit log.
fn assert_denied(grants: Vec<Grant>, reference: &str, account: &str) {
    let s = Setup::new(grants);
    let before = s.triage_flags(1);
    let out = s.run(&["flag", reference, "--set", "seen"]);
    assert_eq!(out.code, 1, "{reference}");
    assert_eq!(out.stdout, "");
    let error = error_of(&out);
    let mut keys: Vec<&String> = error.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["account", "code", "mailbox", "message", "operation"]);
    assert_eq!(error["code"], json!("denied"));
    assert_eq!(error["operation"], json!("flag"));
    assert_eq!(error["account"], json!(account));
    assert_eq!(error["mailbox"], json!("Triage"));
    assert!(error["message"].is_string());
    assert_eq!(s.server.connections(), 0, "the server saw a connection");
    assert_eq!(s.triage_flags(1), before);
    let lines = s.audit();
    assert_eq!(lines.len(), 1, "{lines:?}");
    // An Account the Mandate does not cover selects no Mandate, so the
    // line names none.
    let covered = account == LOGIN;
    let mut expected = json!({
        "operation": "flag",
        "account": account,
        "mailbox": "Triage",
        "messages": [reference],
        "change": "+\\Seen",
        "outcome": "denied",
        "reason": "denied"
    });
    let mut keys = vec![
        "time",
        "operation",
        "account",
        "mailbox",
        "messages",
        "change",
        "outcome",
        "reason",
    ];
    if covered {
        expected["mandate"] = json!("aaaa1111");
        keys.insert(1, "mandate");
    }
    assert_eq!(entry(&lines[0]), expected);
    assert_eq!(keys_in_order(&lines[0]), keys);
}

#[test]
fn ac4_flagging_without_a_flag_grant_is_denied_and_audited() {
    // (a) read and search on Triage, flag on INBOX.
    assert_denied(
        vec![
            grant(Operation::Read, &["Triage"]),
            grant(Operation::Search, &["Triage"]),
            grant(Operation::Flag, &["INBOX"]),
        ],
        &t(1),
        LOGIN,
    );
    // (b) an empty Policy.
    assert_denied(Vec::new(), &t(1), LOGIN);
    // (c) the default Grants, an Account the Mandate does not cover.
    assert_denied(
        Setup::default_grants(),
        &format!("{OTHER_LOGIN}/Triage/{TRIAGE_UIDVALIDITY}/1"),
        OTHER_LOGIN,
    );
}

#[test]
fn ac5_stale_reference_changes_no_flag() {
    let s = Setup::granted();
    let before = s.server.all_flags();
    let stale = format!("{LOGIN}/Triage/1700000099/1");
    let out = s.run(&["flag", &stale, "--set", "seen"]);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    let error = error_of(&out);
    let mut keys: Vec<&String> = error.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["account", "code", "mailbox", "message", "operation"]);
    assert_eq!(error["code"], json!("stale_reference"));
    assert_eq!(error["operation"], json!("flag"));
    assert_eq!(error["account"], json!(LOGIN));
    assert_eq!(error["mailbox"], json!("Triage"));
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("no flag was changed"), "{message}");
    assert!(
        message.contains("search") && message.contains("fresh reference"),
        "{message}"
    );
    let commands = s.server.commands();
    assert!(commands.iter().any(|c| c == "SELECT"), "{commands:?}");
    assert!(!commands.iter().any(|c| c == "UID FETCH"), "{commands:?}");
    assert!(!commands.iter().any(|c| c == "UID STORE"), "{commands:?}");
    assert_eq!(s.server.all_flags(), before);

    // A missing UID: message_not_found, no store.
    let missing = s.run(&["flag", &t(99), "--set", "seen"]);
    assert_eq!(missing.code, 1);
    assert_eq!(missing.stdout, "");
    let error = error_of(&missing);
    assert_eq!(error["code"], json!("message_not_found"));
    assert_eq!(error["operation"], json!("flag"));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("no flag was changed")
    );
    let commands = s.server.commands();
    assert!(!commands.iter().any(|c| c == "UID STORE"), "{commands:?}");
    assert_eq!(s.server.all_flags(), before);

    // `read` with a stale reference still reports `read`.
    let r = Setup::new(vec![grant(Operation::Read, &["Read"])]);
    let out = r.run(&["read", &format!("{LOGIN}/Read/1700000099/1")]);
    assert_eq!(out.code, 1);
    let error = error_of(&out);
    assert_eq!(error["code"], json!("stale_reference"));
    assert_eq!(error["operation"], json!("read"));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("the message was not read")
    );
    let missing = r.run(&["read", &format!("{LOGIN}/Read/{READ_UIDVALIDITY}/99")]);
    assert_eq!(error_of(&missing)["operation"], json!("read"));
}

#[test]
fn ac6_each_change_is_audited_with_its_reference_and_change() {
    let s = Setup::granted();
    s.flag(&[&t(1), "--set", "seen"]);
    let lines = s.audit();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(
        entry(&lines[0]),
        json!({
            "mandate": "aaaa1111",
            "operation": "flag",
            "account": LOGIN,
            "mailbox": "Triage",
            "messages": [t(1)],
            "change": "+\\Seen",
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
            "messages",
            "change",
            "outcome"
        ]
    );
    assert!(
        lines[0].contains("\"change\":\"+\\\\Seen\""),
        "{}",
        lines[0]
    );

    s.flag(&[&t(1), "--clear", "flagged"]);
    let stale = format!("{LOGIN}/Triage/1700000099/1");
    assert_eq!(s.run(&["flag", &stale, "--set", "seen"]).code, 1);
    assert_eq!(s.run(&["flag", &t(99), "--set", "seen"]).code, 1);
    assert_eq!(s.run(&["flag", &t(1), "--set", "$Junk"]).code, 1);
    let bad = s.run(&["flag", "not-a-ref", "--set", "seen"]);
    assert_eq!(bad.code, 2);
    assert_eq!(bad.stdout, "");
    let lines = s.audit();
    assert_eq!(lines.len(), 5, "{lines:?}");
    assert_eq!(
        entry(&lines[1]),
        json!({
            "mandate": "aaaa1111",
            "operation": "flag",
            "account": LOGIN,
            "mailbox": "Triage",
            "messages": [t(1)],
            "change": "-\\Flagged",
            "outcome": "allowed"
        })
    );
    for (line, reference, reason) in [
        (&lines[2], &stale, "stale_reference"),
        (&lines[3], &t(99), "message_not_found"),
    ] {
        assert_eq!(
            entry(line),
            json!({
                "mandate": "aaaa1111",
                "operation": "flag",
                "account": LOGIN,
                "mailbox": "Triage",
                "messages": [reference],
                "change": "+\\Seen",
                "outcome": "allowed",
                "reason": reason
            })
        );
    }
    assert_eq!(
        entry(&lines[4]),
        json!({
            "mandate": "aaaa1111",
            "operation": "flag",
            "account": LOGIN,
            "mailbox": "Triage",
            "messages": [t(1)],
            "outcome": "denied",
            "reason": "unsupported_flag"
        })
    );

    // AC1 and AC2's calls; then the log holds no message content or secret.
    s.flag(&[&t(1), "--clear", "seen"]);
    s.flag(&[&t(2), "--clear", "\\SEEN"]);
    s.flag(&[&t(1), "--set", "flagged"]);
    s.flag(&[&t(2), "--clear", "\\Flagged"]);
    s.flag(&[&t(1), "--set", "seen", "--text"]);
    // `list`, `search` and `read` entries carry no `change` key.
    assert_eq!(s.run(&["search", "--mailbox", "Triage"]).code, 0);
    assert_eq!(s.run(&["list"]).code, 1);
    assert_eq!(s.run(&["read", &t(1)]).code, 1);
    let log = s.audit_bytes();
    let token = s.token.expose().to_string();
    let hash = token_hash(&token);
    for needle in [
        "$Junk",
        "Triage one",
        "Triage two",
        "alice@example.com",
        "bob@example.com",
        "triage body",
        CREDENTIAL,
        token.as_str(),
        hash.as_str(),
    ] {
        assert!(
            !contains_bytes(&log, needle.as_bytes()),
            "audit log contains {needle:?}"
        );
    }
    let lines = s.audit();
    for line in &lines {
        let parsed = parse_line(line);
        let operation = parsed["operation"].as_str().unwrap();
        if operation != "flag" {
            assert!(!parsed.contains_key("change"), "{line}");
        }
    }
    let operations: Vec<String> = lines
        .iter()
        .map(|l| parse_line(l)["operation"].as_str().unwrap().to_string())
        .collect();
    for op in ["search", "list", "read"] {
        assert!(operations.iter().any(|o| o == op), "{operations:?}");
    }
}

#[test]
fn flag_help_explains_the_reference_the_two_flags_and_the_action() {
    let s = Setup::granted();
    let out = s.run(&["flag", "--help"]);
    assert_eq!(out.code, 0);
    for needle in [
        "REF",
        "search",
        "--set",
        "--clear",
        "seen",
        "flagged",
        "read",
        "starred",
        "exactly one",
    ] {
        assert!(out.stdout.contains(needle), "help lacks {needle:?}");
    }
}
