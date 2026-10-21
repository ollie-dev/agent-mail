//! `agent-mail read` driven in-process against the fake IMAP server
//! (ticket 08).

mod support;

use std::path::PathBuf;

use agent_mail::RunOutput;
use agent_mail::mandate::{Grant, Mandate, Operation, Policy, TlsMode};
use agent_mail::seal::AgentToken;
use agent_mail::store::InMemoryStore;
use serde_json::{Map, Value, json};
use support::{
    CREDENTIAL, FakeImap, LOGIN, Mode, NOTES_CONTENT, OTHER_LOGIN, READ_UIDVALIDITY, account,
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

    /// The default Grants: `read` on `Read`, `search` on `*`.
    fn granted() -> Self {
        Self::new(vec![
            grant(Operation::Read, &["Read"]),
            grant(Operation::Search, &["*"]),
        ])
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

    /// Runs `agent-mail read <args>`, expects success, returns the JSON.
    fn read(&self, args: &[&str]) -> Value {
        let argv: Vec<&str> = std::iter::once("read")
            .chain(args.iter().copied())
            .collect();
        let out = self.run(&argv);
        assert_eq!(out.code, 0, "{argv:?}: stderr {:?}", out.stderr);
        assert_eq!(out.stderr, "");
        assert!(out.stdout.ends_with('\n') && out.stdout.lines().count() == 1);
        serde_json::from_str(&out.stdout)
            .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {:?}", out.stdout))
    }

    fn audit(&self) -> Vec<String> {
        audit_lines(&self.log)
    }

    fn audit_bytes(&self) -> Vec<u8> {
        std::fs::read(&self.log).unwrap_or_default()
    }
}

/// `R(n)`: the reference of UID `n` in the fixture `Read`.
fn r(uid: u32) -> String {
    format!("{LOGIN}/Read/{READ_UIDVALIDITY}/{uid}")
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

/// The single audit line, without `time`, after checking `time` is
/// RFC 3339 UTC to the second.
fn single_entry(lines: &[String]) -> Value {
    assert_eq!(lines.len(), 1, "expected one audit line, got {lines:?}");
    let mut entry = parse_line(&lines[0]);
    let time = entry.remove("time").expect("entry has a time");
    let time = time.as_str().unwrap();
    assert_eq!(time.len(), 20, "{time}");
    assert!(time.ends_with('Z') && time.as_bytes()[10] == b'T', "{time}");
    Value::Object(entry)
}

fn r1_expected() -> Value {
    json!({
        "ref": r(1),
        "account": LOGIN,
        "mailbox": "Read",
        "date": "2026-10-06T08:15:00Z",
        "from": "Alice Example <alice@example.com>",
        "to": [LOGIN],
        "cc": ["Carol <carol@example.com>"],
        "subject": "Project update",
        "message_id": "<update-1@example.com>",
        "body": "Hi,\n\nthe project is on track.\n\nAlice",
        "truncated": false,
        "attachments": []
    })
}

#[test]
fn ac1_plain_text_message_returns_headers_and_body() {
    let s = Setup::granted();
    let out = s.run(&["read", &r(1)]);
    assert_eq!(out.code, 0, "stderr {:?}", out.stderr);
    let result: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(result, r1_expected());
    assert_eq!(
        keys_in_order(out.stdout.trim_end()),
        [
            "ref",
            "account",
            "mailbox",
            "date",
            "from",
            "to",
            "cc",
            "subject",
            "message_id",
            "body",
            "truncated",
            "attachments"
        ]
    );

    // A reference taken from `search` output gives the same result.
    let out = s.run(&["search", "--mailbox", "Read"]);
    assert_eq!(out.code, 0, "stderr {:?}", out.stderr);
    let found: Value = serde_json::from_str(&out.stdout).unwrap();
    let reference = found["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["ref"].as_str().unwrap().to_string())
        .find(|reference| reference.ends_with("/1"))
        .expect("search returns UID 1");
    assert_eq!(s.read(&[&reference]), r1_expected());
}

#[test]
fn ac2_html_only_message_returns_readable_text() {
    let s = Setup::granted();
    let result = s.read(&[&r(2)]);
    let body = result["body"].as_str().unwrap();
    assert!(body.contains("Hello Bob,"), "{body:?}");
    assert!(body.contains("report"), "{body:?}");
    assert!(body.contains("& reply."), "{body:?}");
    assert!(
        body.contains("See the") || body.contains("See\u{a0}the"),
        "{body:?}"
    );
    for markup in ["<", ">", "&amp;", "&nbsp;", "color:red", "alert("] {
        assert!(!body.contains(markup), "body contains {markup:?}: {body:?}");
    }
    assert_eq!(result["attachments"], json!([]));

    let both = s.read(&[&r(5)]);
    assert_eq!(both["body"], json!("Plain version"));
    assert_eq!(both["attachments"], json!([]));
}

#[test]
fn ac3_attachments_are_listed_without_content() {
    let s = Setup::granted();
    let out = s.run(&["read", &r(3)]);
    assert_eq!(out.code, 0, "stderr {:?}", out.stderr);
    let result: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(result["body"], json!("See the attached files."));
    assert_eq!(
        result["attachments"],
        json!([
            {"name": "report.pdf", "type": "application/pdf", "size": 1024},
            {"name": "notes.txt", "type": "text/plain", "size": 23}
        ])
    );
    let text = s.run(&["--text", "read", &r(3)]);
    assert_eq!(text.code, 0, "stderr {:?}", text.stderr);
    assert!(
        text.stdout
            .contains("Attachments:\n  report.pdf (application/pdf, 1024 bytes)\n  notes.txt (text/plain, 23 bytes)\n"),
        "{:?}",
        text.stdout
    );
    for stream in [&out.stdout, &text.stdout] {
        assert!(!stream.contains(NOTES_CONTENT), "{stream:?}");
        assert!(!stream.contains("AAECAwQFBgcICQoL"), "{stream:?}");
    }
}

#[test]
fn ac4_reading_leaves_messages_unread() {
    let s = Setup::granted();
    s.read(&[&r(1)]);
    s.read(&[&r(2)]);
    s.read(&[&r(3)]);
    s.read(&[&r(4), "--full"]);
    for (mailbox, uid, flags) in s.server.all_flags() {
        if mailbox == "Read" {
            assert!(
                !flags.iter().any(|f| f == "\\Seen"),
                "Read/{uid} became seen: {flags:?}"
            );
        }
    }
    let commands = s.server.commands();
    for forbidden in ["SELECT", "STORE", "UID STORE"] {
        assert!(
            !commands.iter().any(|c| c == forbidden),
            "{forbidden} sent: {commands:?}"
        );
    }
    assert_eq!(commands.iter().filter(|c| *c == "EXAMINE").count(), 4);
    assert_eq!(commands.iter().filter(|c| *c == "UID FETCH").count(), 4);
}

#[test]
fn ac5_long_bodies_are_truncated_unless_full() {
    let s = Setup::granted();
    let cut = s.read(&[&r(4)]);
    assert_eq!(cut["truncated"], json!(true));
    let expected = format!(
        "{}ü\n\n[truncated: showing 20000 of 25000 characters; read again with --full for the complete body]",
        "x".repeat(19_999)
    );
    assert_eq!(cut["body"].as_str().unwrap(), expected);

    let full = s.read(&[&r(4), "--full"]);
    assert_eq!(full["truncated"], json!(false));
    let body = full["body"].as_str().unwrap();
    assert_eq!(body.chars().count(), 25_000);
    assert_eq!(body, support::long_body());
    assert!(body.ends_with(&"y".repeat(5_000)));

    assert_eq!(s.read(&[&r(1), "--full"]), r1_expected());
    assert_eq!(s.read(&[&r(1)]), r1_expected());
}

#[test]
fn ac6_stale_reference_fails_before_any_fetch() {
    let s = Setup::granted();
    let stale = format!("{LOGIN}/Read/1700000099/1");
    let out = s.run(&["read", &stale]);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    let error = error_of(&out);
    let mut keys: Vec<&String> = error.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["account", "code", "mailbox", "message", "operation"]);
    assert_eq!(error["code"], json!("stale_reference"));
    assert_eq!(error["operation"], json!("read"));
    assert_eq!(error["account"], json!(LOGIN));
    assert_eq!(error["mailbox"], json!("Read"));
    let message = error["message"].as_str().unwrap();
    assert!(
        message.contains("search") && message.contains("fresh reference"),
        "{message}"
    );
    let commands = s.server.commands();
    assert!(commands.iter().any(|c| c == "EXAMINE"), "{commands:?}");
    assert!(!commands.iter().any(|c| c == "UID FETCH"), "{commands:?}");

    let text = s.run(&["--text", "read", &stale]);
    assert_eq!(text.code, 1);
    assert!(
        text.stderr.starts_with("error: stale_reference: "),
        "{:?}",
        text.stderr
    );
}

/// AC7: one denial case against a fresh audit log.
fn assert_denied(grants: Vec<Grant>, reference: &str, account: &str) {
    let s = Setup::new(grants);
    let out = s.run(&["read", reference]);
    assert_eq!(out.code, 1, "{reference}");
    assert_eq!(out.stdout, "");
    let error = error_of(&out);
    assert_eq!(error["code"], json!("denied"));
    assert_eq!(error["operation"], json!("read"));
    assert_eq!(error["account"], json!(account));
    assert_eq!(error["mailbox"], json!("Read"));
    assert!(error["message"].is_string());
    assert_eq!(s.server.connections(), 0, "the server saw a connection");
    let lines = s.audit();
    assert_eq!(
        single_entry(&lines),
        json!({
            "mandate": "aaaa1111",
            "operation": "read",
            "account": account,
            "mailbox": "Read",
            "messages": [reference],
            "outcome": "denied",
            "reason": "denied"
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
            "outcome",
            "reason"
        ]
    );
}

#[test]
fn ac7_read_without_a_read_grant_is_denied_and_audited() {
    // (a) read on another mailbox, search on Read.
    assert_denied(
        vec![
            grant(Operation::Read, &["INBOX"]),
            grant(Operation::Search, &["Read"]),
        ],
        &r(1),
        LOGIN,
    );
    // (b) an empty Policy.
    assert_denied(Vec::new(), &r(1), LOGIN);
    // (c) the default Grants, an Account the Mandate does not cover.
    assert_denied(
        vec![
            grant(Operation::Read, &["Read"]),
            grant(Operation::Search, &["*"]),
        ],
        &format!("{OTHER_LOGIN}/Read/{READ_UIDVALIDITY}/1"),
        OTHER_LOGIN,
    );
}

#[test]
fn ac8_each_read_is_audited_with_its_reference_only() {
    let s = Setup::granted();
    s.read(&[&r(3)]);
    let lines = s.audit();
    assert_eq!(
        single_entry(&lines),
        json!({
            "mandate": "aaaa1111",
            "operation": "read",
            "account": LOGIN,
            "mailbox": "Read",
            "messages": [r(3)],
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
            "outcome"
        ]
    );

    // Stale reference, missing UID and malformed reference.
    let stale = format!("{LOGIN}/Read/1700000099/1");
    assert_eq!(s.run(&["read", &stale]).code, 1);
    let missing = r(99);
    let out = s.run(&["read", &missing]);
    assert_eq!(out.code, 1);
    assert_eq!(error_of(&out)["code"], json!("message_not_found"));
    assert_eq!(error_of(&out)["operation"], json!("read"));
    let bad = s.run(&["read", "not-a-ref"]);
    assert_eq!(bad.code, 2);
    assert_eq!(bad.stdout, "");
    assert!(!bad.stderr.is_empty());
    let lines = s.audit();
    assert_eq!(lines.len(), 3, "{lines:?}");
    for (line, reference, reason) in [
        (&lines[1], &stale, "stale_reference"),
        (&lines[2], &missing, "message_not_found"),
    ] {
        assert_eq!(
            single_entry(std::slice::from_ref(line)),
            json!({
                "mandate": "aaaa1111",
                "operation": "read",
                "account": LOGIN,
                "mailbox": "Read",
                "messages": [reference],
                "outcome": "allowed",
                "reason": reason
            })
        );
    }

    // After AC1 to AC5's reads, the log holds no message content or secret.
    s.read(&[&r(1)]);
    s.read(&[&r(2)]);
    s.read(&[&r(4)]);
    s.read(&[&r(4), "--full"]);
    s.read(&[&r(5)]);
    assert_eq!(s.run(&["--text", "read", &r(3)]).code, 0);
    let log = s.audit_bytes();
    let token = s.token.expose().to_string();
    let hash = token_hash(&token);
    for needle in [
        "Project update",
        "Files",
        "on track",
        "Hello",
        "xxxxxxxx",
        "report.pdf",
        NOTES_CONTENT,
        "alice@example.com",
        CREDENTIAL,
        token.as_str(),
        hash.as_str(),
    ] {
        assert!(
            !contains_bytes(&log, needle.as_bytes()),
            "audit log contains {needle:?}"
        );
    }
}

#[test]
fn read_text_output_shows_headers_body_and_reference() {
    let s = Setup::granted();
    let out = s.run(&["read", &r(1), "--text"]);
    assert_eq!(out.code, 0, "stderr {:?}", out.stderr);
    assert_eq!(
        out.stdout,
        format!(
            "From: Alice Example <alice@example.com>\n\
             To: agent@example.test\n\
             Cc: Carol <carol@example.com>\n\
             Date: 2026-10-06T08:15:00Z\n\
             Subject: Project update\n\
             Ref: {}\n\
             \n\
             Hi,\n\nthe project is on track.\n\nAlice\n",
            r(1)
        )
    );
}

#[test]
fn read_help_explains_the_reference_full_and_the_limit() {
    let s = Setup::granted();
    let out = s.run(&["read", "--help"]);
    assert_eq!(out.code, 0);
    for needle in ["REF", "search", "--full", "20,000"] {
        assert!(out.stdout.contains(needle), "help lacks {needle:?}");
    }
}
