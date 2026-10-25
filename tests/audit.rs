//! The audit log: one metadata-only entry per `list` call (ticket 06).

mod support;

use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use agent_mail::core::rfc3339_utc;
use agent_mail::mandate::{Credential, Mandate, Operation, Policy, TlsMode};
use agent_mail::seal::{self, AgentToken};
use agent_mail::store::InMemoryStore;
use agent_mail::{Context, RunOutput};
use serde_json::{Map, Value, json};
use support::{
    CREDENTIAL, FakeImap, LOGIN, Mode, account, audit_lines, contains_bytes, error_of, grant,
    run_logged, store_mandate, temp_audit_log, token_hash,
};

/// The wrong Credential of AC4 (iii). Fake.
const WRONG_CREDENTIAL: &str = "wrong-app-password-456";

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Mandate `aaaa1111` over the fake Account on `port` with `grants`.
fn mandate_with(port: u16, grants: Vec<agent_mail::mandate::Grant>) -> Mandate {
    Mandate {
        name: "work-inbox".to_string(),
        id: "aaaa1111".to_string(),
        created: 1_792_483_200,
        accounts: vec![account(LOGIN, port, TlsMode::Implicit)],
        policy: Policy { grants },
    }
}

/// The AC1 Mandate: `list` on `INBOX` and `Archive`.
fn allowed_mandate(port: u16) -> Mandate {
    mandate_with(port, vec![grant(Operation::List, &["INBOX", "Archive"])])
}

/// The AC2 Mandate: only `read` on `INBOX`.
fn denied_mandate(port: u16) -> Mandate {
    mandate_with(port, vec![grant(Operation::Read, &["INBOX"])])
}

/// The line as a JSON object, with its keys in file order.
fn parse(line: &str) -> Map<String, Value> {
    match serde_json::from_str::<Value>(line) {
        Ok(Value::Object(map)) => map,
        other => panic!("audit line is not a JSON object: {other:?}"),
    }
}

/// Key names of `line` in the order they appear. serde_json sorts keys
/// when parsing, so the order is read from the text.
fn keys_in_order(line: &str) -> Vec<String> {
    let entry = parse(line);
    let mut found: Vec<(usize, String)> = entry
        .keys()
        .map(|k| {
            let at = line
                .find(&format!("\"{k}\":"))
                .unwrap_or_else(|| panic!("key {k} not found in {line}"));
            (at, k.clone())
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, k)| k).collect()
}

/// Asserts `time` is RFC 3339 UTC to the second within `[before, after]`.
fn assert_time(time: &Value, before: u64, after: u64) -> String {
    let time = time.as_str().expect("time is a string").to_string();
    let b = time.as_bytes();
    let shape_ok = b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        });
    assert!(shape_ok, "time {time:?} is not RFC 3339 UTC to the second");
    // Fixed-width, so string order is time order.
    assert!(
        rfc3339_utc(before) <= time && time <= rfc3339_utc(after),
        "time {time:?} is not within the call"
    );
    time
}

/// Asserts the log holds exactly one line equal to `expected` plus a
/// `time` within the call; returns the line.
fn assert_single_entry(log: &Path, expected: Value, before: u64, after: u64) -> String {
    let lines = audit_lines(log);
    assert_eq!(lines.len(), 1, "expected one audit line, got {lines:?}");
    let mut entry = parse(&lines[0]);
    let time = entry.remove("time").expect("entry has a time");
    assert_time(&time, before, after);
    assert_eq!(Value::Object(entry), expected);
    lines[0].clone()
}

fn assert_key_order(line: &str, order: &[&str]) {
    assert_eq!(keys_in_order(line), order, "key order of {line}");
}

/// Hex of `bytes`, lowercase.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A localhost port nothing listens on.
fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

#[test]
fn ac1_successful_list_appends_one_allowed_entry() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::default();
    let token = store_mandate(&store, &allowed_mandate(server.port));
    let log = temp_audit_log();

    let before = now_secs();
    let out = run_logged(&["list"], Some(token.expose()), &store, &server.ca, &log);
    let after = now_secs();
    assert_eq!(out.code, 0, "{out:?}");

    let line = assert_single_entry(
        &log,
        json!({
            "mandate": "aaaa1111",
            "operation": "list",
            "account": LOGIN,
            "outcome": "allowed",
        }),
        before,
        after,
    );
    assert_key_order(
        &line,
        &["time", "mandate", "operation", "account", "outcome"],
    );
}

#[test]
fn ac2_denied_list_appends_one_denied_entry_with_reason() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::default();
    let token = store_mandate(&store, &denied_mandate(server.port));
    let log = temp_audit_log();

    let before = now_secs();
    let out = run_logged(&["list"], Some(token.expose()), &store, &server.ca, &log);
    let after = now_secs();
    assert_eq!(out.code, 1);
    assert_eq!(error_of(&out)["code"], "denied");
    assert_eq!(server.connections(), 0, "denied before any connection");

    let line = assert_single_entry(
        &log,
        json!({
            "mandate": "aaaa1111",
            "operation": "list",
            "account": LOGIN,
            "outcome": "denied",
            "reason": "denied",
        }),
        before,
        after,
    );
    assert_key_order(
        &line,
        &[
            "time",
            "mandate",
            "operation",
            "account",
            "outcome",
            "reason",
        ],
    );
}

#[test]
fn ac3_unknown_tokens_are_logged_without_mandate_or_token() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::default();
    // A real Mandate in the store, so the unknown token is the only problem.
    store_mandate(&store, &allowed_mandate(server.port));
    let unstored = AgentToken::mint();
    let cases: [(&str, Option<&str>, &str); 3] = [
        ("unstored", Some(unstored.expose()), "invalid_token"),
        ("malformed", Some("not-a-token"), "invalid_token"),
        ("missing", None, "missing_token"),
    ];
    for (case, token, reason) in cases {
        let log = temp_audit_log();
        let before = now_secs();
        let out = run_logged(&["list"], token, &store, &server.ca, &log);
        let after = now_secs();
        assert_eq!(out.code, 1, "{case}");
        assert_eq!(error_of(&out)["code"], reason, "{case}");

        let line = assert_single_entry(
            &log,
            json!({"operation": "list", "outcome": "denied", "reason": reason}),
            before,
            after,
        );
        assert_key_order(&line, &["time", "operation", "outcome", "reason"]);
        let bytes = fs::read(&log).unwrap();
        if let Some(token) = token {
            assert!(!contains_bytes(&bytes, token.as_bytes()), "{case}: token");
        }
        assert!(
            !contains_bytes(&bytes, token_hash(unstored.expose()).as_bytes()),
            "{case}: token hash"
        );
    }
    assert_eq!(server.connections(), 0);
}

#[test]
fn ac4_no_entry_holds_a_credential_token_or_hash() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::default();
    let log = temp_audit_log();
    let mut presented: Vec<Option<String>> = Vec::new();

    // (i) allowed, (ii) denied.
    let allowed = store_mandate(&store, &allowed_mandate(server.port));
    let mut denied = denied_mandate(server.port);
    denied.id = "bbbb2222".to_string();
    let denied = store_mandate(&store, &denied);
    // (iii) the server refuses the login.
    let mut wrong = allowed_mandate(server.port);
    wrong.id = "cccc3333".to_string();
    wrong.accounts[0].credential = Credential::new(WRONG_CREDENTIAL);
    let wrong = store_mandate(&store, &wrong);
    // (iv) nothing listens.
    let mut unreachable = allowed_mandate(closed_port());
    unreachable.id = "dddd4444".to_string();
    let unreachable = store_mandate(&store, &unreachable);
    // (v) unknown tokens.
    let unstored = AgentToken::mint();

    let calls: Vec<(Option<&str>, i32, Option<&str>)> = vec![
        (Some(allowed.expose()), 0, None),
        (Some(denied.expose()), 1, Some("denied")),
        (Some(wrong.expose()), 1, Some("login_failed")),
        (Some(unreachable.expose()), 1, Some("connection_failed")),
        (Some(unstored.expose()), 1, Some("invalid_token")),
        (Some("not-a-token"), 1, Some("invalid_token")),
        (None, 1, Some("missing_token")),
    ];
    for (token, code, reason) in &calls {
        let out = run_logged(&["list"], *token, &store, &server.ca, &log);
        assert_eq!(out.code, *code, "{reason:?}: {out:?}");
        if let Some(reason) = reason {
            assert_eq!(error_of(&out)["code"], *reason);
        }
        presented.push(token.map(str::to_string));
    }
    let tokens = [&allowed, &denied, &wrong, &unreachable, &unstored];

    let lines = audit_lines(&log);
    assert_eq!(lines.len(), calls.len(), "one line per call: {lines:?}");
    let outcomes: Vec<(String, Option<String>)> = lines
        .iter()
        .map(|l| {
            let e = parse(l);
            (
                e["outcome"].as_str().unwrap().to_string(),
                e.get("reason").map(|r| r.as_str().unwrap().to_string()),
            )
        })
        .collect();
    let expected_outcomes = [
        "allowed", "denied", "allowed", "allowed", "denied", "denied", "denied",
    ];
    for ((outcome, reason), (expected, (_, _, expected_reason))) in outcomes
        .iter()
        .zip(expected_outcomes.iter().zip(calls.iter()))
    {
        assert_eq!(outcome, expected);
        assert_eq!(reason.as_deref(), *expected_reason);
    }

    let bytes = fs::read(&log).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    for secret in [CREDENTIAL, WRONG_CREDENTIAL] {
        assert!(!contains_bytes(&bytes, secret.as_bytes()), "a Credential");
    }
    for token in presented.iter().flatten() {
        assert!(!contains_bytes(&bytes, token.as_bytes()), "a token");
    }
    for token in tokens {
        assert!(
            !contains_bytes(&bytes, token_hash(token.expose()).as_bytes()),
            "a token hash"
        );
        assert!(
            !contains_bytes(&bytes, hex(&token.store_key()).as_bytes()),
            "a store key"
        );
    }
    assert!(!text.contains(seal::TOKEN_PREFIX), "the token prefix");
    assert!(support::tokens_in(&text).is_empty());
}

#[test]
fn ac5_entries_are_appended_never_rewritten() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::default();
    let allowed = store_mandate(&store, &allowed_mandate(server.port));
    let denied = store_mandate(&store, &denied_mandate(server.port));
    let log = temp_audit_log();
    let first = "{\"time\":\"2026-01-01T00:00:00Z\",\"operation\":\"list\",\"outcome\":\"denied\",\"reason\":\"missing_token\"}\n";
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    fs::write(&log, first).unwrap();

    let mut previous = fs::read(&log).unwrap();
    for (token, code) in [(&allowed, 0), (&denied, 1), (&allowed, 0)] {
        let out = run_logged(&["list"], Some(token.expose()), &store, &server.ca, &log);
        assert_eq!(out.code, code);
        let now = fs::read(&log).unwrap();
        assert!(now.starts_with(&previous), "earlier content was rewritten");
        assert!(now.len() > previous.len());
        previous = now;
    }
    let lines = audit_lines(&log);
    assert_eq!(lines.len(), 4);
    assert_eq!(format!("{}\n", lines[0]), first);
    let outcomes: Vec<Value> = lines[1..]
        .iter()
        .map(|l| parse(l)["outcome"].clone())
        .collect();
    assert_eq!(outcomes, ["allowed", "denied", "allowed"]);

    // A login carrying line breaks stays on one line.
    let evil_login = "evil\n{\"outcome\":\"allowed\"}\r@example.test";
    let mut evil = denied_mandate(server.port);
    evil.accounts[0].login = evil_login.to_string();
    evil.policy.grants[0].account = evil_login.to_string();
    let evil = store_mandate(&store, &evil);
    let out = run_logged(&["list"], Some(evil.expose()), &store, &server.ca, &log);
    assert_eq!(out.code, 1);
    let now = fs::read(&log).unwrap();
    assert!(now.starts_with(&previous));
    let lines = audit_lines(&log);
    assert_eq!(lines.len(), 5, "the crafted login added more than one line");
    let last = parse(&lines[4]);
    assert_eq!(last["account"], evil_login);
    assert_eq!(last["outcome"], "denied");
}

#[test]
fn ac6_no_command_but_audit_reads_or_returns_the_audit_log() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::default();
    let token = store_mandate(&store, &allowed_mandate(server.port));
    let log = temp_audit_log();
    let out = run_logged(&["list"], Some(token.expose()), &store, &server.ca, &log);
    assert_eq!(out.code, 0);
    let time = parse(&audit_lines(&log)[0])["time"]
        .as_str()
        .unwrap()
        .to_string();
    let path = log.to_string_lossy().into_owned();
    let dir = log.parent().unwrap().to_string_lossy().into_owned();

    let assert_clean = |out: &RunOutput, what: &str| {
        for stream in [&out.stdout, &out.stderr] {
            assert!(!stream.contains(&path), "{what}: prints the log path");
            assert!(!stream.contains(&dir), "{what}: prints the log directory");
            assert!(!stream.contains("audit.log"), "{what}: names audit.log");
            assert!(!stream.contains(&time), "{what}: prints the entry time");
        }
    };
    let commands: [&[&str]; 5] = [
        &["--help"],
        &["mandate", "--help"],
        &["list"],
        &["mandate", "list"],
        &["mandate", "delete", "ffffffff"],
    ];
    for args in commands {
        for text in [false, true] {
            let mut argv: Vec<&str> = Vec::new();
            if text {
                argv.push("--text");
            }
            argv.extend_from_slice(args);
            let out = run_logged(&argv, Some(token.expose()), &store, &server.ca, &log);
            assert_clean(&out, &argv.join(" "));
        }
    }
    // `agent-mail audit` (job 026) is the one command that reads it.
    for sub in ["log"] {
        for text in [false, true] {
            let mut argv: Vec<&str> = Vec::new();
            if text {
                argv.push("--text");
            }
            argv.push(sub);
            let out = run_logged(&argv, Some(token.expose()), &store, &server.ca, &log);
            assert_eq!(out.code, 2, "{sub} is not a subcommand");
            assert!(out.stderr.contains("unknown command"), "{out:?}");
            assert_clean(&out, sub);
        }
    }
}

#[test]
fn ac7_readme_documents_location_and_fields() {
    let readme =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md")).unwrap();
    let start = readme
        .find("\n## Audit log\n")
        .expect("README has an `## Audit log` section");
    let rest = &readme[start + 1..];
    let section = match rest[3..].find("\n## ") {
        Some(end) => &rest[..end + 3],
        None => rest,
    };
    assert!(section.contains("~/Library/Application Support/agent-mail/audit.log"));
    for field in [
        "time",
        "mandate",
        "operation",
        "account",
        "mailbox",
        "messages",
        "results",
        "outcome",
        "reason",
    ] {
        assert!(
            section.contains(&format!("`{field}`")),
            "README audit section does not name `{field}`"
        );
    }
}

#[test]
fn unwritable_audit_log_fails_closed_without_mailboxes() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::default();
    let token = store_mandate(&store, &allowed_mandate(server.port));
    // The log's directory would have to be a regular file.
    let base = temp_audit_log();
    fs::create_dir_all(base.parent().unwrap()).unwrap();
    let blocker = base.parent().unwrap().join("blocker");
    fs::write(&blocker, "").unwrap();
    let log = blocker.join("audit.log");

    let out = run_logged(&["list"], Some(token.expose()), &store, &server.ca, &log);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    let error = error_of(&out);
    assert_eq!(error["code"], "audit_unavailable");
    assert!(!out.stderr.contains("INBOX"));
    assert!(!out.stderr.contains("audit.log"));

    let out = run_logged(
        &["--text", "list"],
        Some(token.expose()),
        &store,
        &server.ca,
        &log,
    );
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.starts_with("error: audit_unavailable: "),
        "{:?}",
        out.stderr
    );

    // A denied call fails closed too.
    let denied = store_mandate(&store, &denied_mandate(server.port));
    let out = run_logged(&["list"], Some(denied.expose()), &store, &server.ca, &log);
    assert_eq!(error_of(&out)["code"], "audit_unavailable");
}

#[test]
fn test_contexts_never_write_to_the_platform_log() {
    let store = InMemoryStore::default();
    let a = Context::trusting_only(&store, Vec::new());
    let b = Context::trusting_only(&store, Vec::new());
    let path = a.audit_log().path().expect("a path").to_path_buf();
    assert!(path.starts_with(std::env::temp_dir()), "{path:?}");
    assert_ne!(Some(path.clone()), agent_mail::audit::platform_path());
    assert_ne!(
        Some(path.as_path()),
        b.audit_log().path(),
        "one log per context"
    );
}

#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777
}

#[cfg(unix)]
#[test]
fn list_tightens_an_existing_loose_log_directory_and_file() {
    use std::os::unix::fs::PermissionsExt;
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::default();
    let token = store_mandate(&store, &allowed_mandate(server.port));
    let log = temp_audit_log();
    let dir = log.parent().unwrap();
    fs::create_dir_all(dir).unwrap();
    fs::write(&log, "").unwrap();
    fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&log, fs::Permissions::from_mode(0o644)).unwrap();

    let out = run_logged(&["list"], Some(token.expose()), &store, &server.ca, &log);
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(mode_of(dir), 0o700);
    assert_eq!(mode_of(&log), 0o600);
    assert_eq!(audit_lines(&log).len(), 1);
}

#[cfg(unix)]
#[test]
fn list_refuses_a_symlinked_log_and_fails_closed() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::default();
    let token = store_mandate(&store, &allowed_mandate(server.port));
    let log = temp_audit_log();
    let dir = log.parent().unwrap();
    fs::create_dir_all(dir).unwrap();
    let target = dir.join("elsewhere");
    fs::write(&target, "victim\n").unwrap();
    std::os::unix::fs::symlink(&target, &log).unwrap();

    let out = run_logged(&["list"], Some(token.expose()), &store, &server.ca, &log);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    assert_eq!(error_of(&out)["code"], "audit_unavailable");
    assert_eq!(fs::read_to_string(&target).unwrap(), "victim\n");
}

// ---------------------------------------------------------------- job 024

#[test]
fn job024_ac1d_windows_log_is_under_local_app_data() {
    use agent_mail::audit::{Platform, log_path};
    let base = Path::new(r"C:\Users\alice\AppData\Local");
    let path = log_path(Platform::Windows, base, None);
    assert_eq!(path, base.join("agent-mail").join("audit.log"));
    // XDG_DATA_HOME plays no part on Windows.
    assert_eq!(
        log_path(
            Platform::Windows,
            base,
            Some(std::ffi::OsStr::new(r"D:\data"))
        ),
        path
    );
    #[cfg(windows)]
    assert_eq!(
        path,
        Path::new(r"C:\Users\alice\AppData\Local\agent-mail\audit.log")
    );
}

#[test]
fn job024_ac1d_windows_base_is_the_known_folder_not_an_environment_variable() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/audit.rs")).unwrap();
    assert!(source.contains("SHGetKnownFolderPath(&FOLDERID_LocalAppData"));
    assert!(!source.contains("\"LOCALAPPDATA\""));
    assert!(!source.contains("\"APPDATA\""));
}

#[cfg(windows)]
#[test]
fn job024_ac1d_platform_path_on_windows_ends_in_agent_mail_audit_log() {
    let path = agent_mail::audit::platform_path().expect("Local AppData is known");
    assert!(path.is_absolute(), "{path:?}");
    assert!(path.ends_with(r"agent-mail\audit.log"), "{path:?}");
}

// ---------------------------------------- `agent-mail audit` (job 026, AC5)

mod audit_command {
    use std::collections::HashMap;
    use std::sync::Arc;

    use agent_mail::audit::AuditLog;

    use super::support::CountingStore;
    use super::support::broker::{BrokerSetup, FakeBroker, caller};
    use super::*;

    const LIST_ENTRY: &str = r#"{"time":"2026-10-21T09:30:15Z","mandate":"ab12cd34","operation":"list","account":"a@example.com","outcome":"allowed"}"#;
    const READ_ENTRY: &str = r#"{"time":"2026-10-21T09:31:00Z","mandate":"ab12cd34","operation":"read","account":"a@example.com","mailbox":"INBOX","messages":["a@example.com/INBOX/7/1"],"outcome":"denied","reason":"denied"}"#;
    const X: &str = "S-1-5-21-1-2-3-1001";
    const Y: &str = "S-1-5-21-1-2-3-1002";

    /// A fresh log holding `lines`, each followed by `\n`.
    fn log_with(lines: &[&str]) -> std::path::PathBuf {
        let log = temp_audit_log();
        fs::create_dir_all(log.parent().unwrap()).unwrap();
        let mut text = String::new();
        for line in lines {
            text.push_str(line);
            text.push('\n');
        }
        fs::write(&log, text).unwrap();
        log
    }

    /// `agent-mail <args>` in-process with no token, logging to `log`.
    fn audit_at(args: &[&str], log: &Path) -> RunOutput {
        let store = CountingStore::default();
        let context = Context::trusting_only(&store, Vec::new()).with_audit_log(AuditLog::at(log));
        let argv = std::iter::once("agent-mail").chain(args.iter().copied());
        let out = agent_mail::run(argv, &HashMap::new(), &context);
        assert_eq!(store.accesses(), 0, "audit touched the store");
        out
    }

    #[test]
    fn ac5_audit_prints_every_json_entry_in_file_order() {
        let log = log_with(&[LIST_ENTRY, "not json", READ_ENTRY]);
        let before = fs::read(&log).unwrap();
        let out = audit_at(&["audit"], &log);
        assert_eq!(out.code, 0, "{out:?}");
        assert_eq!(out.stderr, "");
        assert_eq!(
            out.stdout,
            format!("{{\"entries\":[{LIST_ENTRY},{READ_ENTRY}]}}\n")
        );
        let parsed: Value = serde_json::from_str(&out.stdout).unwrap();
        let list: Value = serde_json::from_str(LIST_ENTRY).unwrap();
        let read: Value = serde_json::from_str(READ_ENTRY).unwrap();
        assert_eq!(parsed["entries"], json!([list, read]));
        // Reading the log writes no entry.
        assert_eq!(fs::read(&log).unwrap(), before);
    }

    #[test]
    fn ac5_without_a_log_audit_prints_no_entries() {
        let log = temp_audit_log();
        let out = audit_at(&["audit"], &log);
        assert_eq!(out.code, 0, "{out:?}");
        assert_eq!(out.stdout, "{\"entries\":[]}\n");
        let out = audit_at(&["audit", "--text"], &log);
        assert_eq!(out.stdout, "No audit entries.\n");
        assert!(!log.exists(), "audit created the log");
    }

    #[test]
    fn ac5_text_prints_one_escaped_line_per_entry_without_the_sid() {
        let full = json!({
            "time": "2026-10-21T09:32:00Z",
            "mandate": "ab12cd34",
            "operation": "move",
            "account": "a@example.com",
            "mailbox": "IN\nBOX",
            "target": "Archive",
            "messages": ["r/1", "r/2"],
            "recipients": ["b@example.com", "c@example.com"],
            "change": "+\\Seen",
            "results": 2,
            "outcome": "allowed",
            "reason": "x\u{1b}[31m",
            "sid": X,
        })
        .to_string();
        let log = log_with(&[LIST_ENTRY, &full]);
        let expected = concat!(
            "2026-10-21T09:30:15Z allowed list mandate=ab12cd34 account=a@example.com\n",
            "2026-10-21T09:32:00Z allowed move mandate=ab12cd34 account=a@example.com ",
            "mailbox=IN\\u{a}BOX target=Archive messages=r/1,r/2 ",
            "recipients=b@example.com,c@example.com change=+\\Seen results=2 ",
            "reason=x\\u{1b}[31m\n",
        );
        for args in [&["--text", "audit"][..], &["audit", "--text"]] {
            let out = audit_at(args, &log);
            assert_eq!(out.code, 0, "{out:?}");
            assert_eq!(out.stdout, expected, "{args:?}");
            assert!(!out.stdout.contains(X));
        }
    }

    #[cfg(unix)]
    #[test]
    fn ac5_a_symlinked_log_or_directory_is_refused() {
        let real = log_with(&[LIST_ENTRY]);
        // The log itself is a link.
        let linked_log = temp_audit_log();
        fs::create_dir_all(linked_log.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&real, &linked_log).unwrap();
        // The log's directory is a link.
        let dir_link = temp_audit_log().parent().unwrap().to_path_buf();
        std::os::unix::fs::symlink(real.parent().unwrap(), &dir_link).unwrap();
        let through_dir = dir_link.join(real.file_name().unwrap());
        for log in [linked_log, through_dir] {
            for text in [false, true] {
                let args: &[&str] = if text {
                    &["--text", "audit"]
                } else {
                    &["audit"]
                };
                let out = audit_at(args, &log);
                assert_eq!(out.code, 1, "{out:?}");
                assert_eq!(out.stdout, "");
                assert!(out.stderr.contains("audit_unavailable"), "{out:?}");
            }
        }
    }

    /// `agent-mail <args>` on the Broker route for caller X, whose Broker
    /// log holds entries for X, Y and one without `sid`; the client's own
    /// log is `local`. Returns the output, the fake Broker and X's entry.
    fn audit_via_broker(args: &[&str], local: &Path) -> (RunOutput, FakeBroker, String) {
        let x_entry = format!(
            r#"{{"time":"2026-10-21T09:30:15Z","mandate":"ab12cd34","operation":"list","account":"a@example.com","outcome":"allowed","sid":"{X}"}}"#
        );
        let y_entry = format!(
            r#"{{"time":"2026-10-21T09:30:16Z","operation":"list","outcome":"denied","reason":"missing_token","sid":"{Y}"}}"#
        );
        let broker_log = log_with(&[&x_entry, &y_entry, READ_ENTRY]);
        let mut setup = BrokerSetup::new(Arc::new(InMemoryStore::new()), caller(X, 0x2000));
        setup.audit = AuditLog::at(&broker_log);
        let fake = FakeBroker::serving(setup);
        let store = CountingStore::default();
        let context = Context::trusting_only(&store, Vec::new())
            .with_audit_log(AuditLog::at(local))
            .with_broker(&fake);
        let argv = std::iter::once("agent-mail").chain(args.iter().copied());
        let out = agent_mail::run(argv, &HashMap::new(), &context);
        assert_eq!(store.accesses(), 0);
        (out, fake, x_entry)
    }

    #[test]
    fn ac5_on_the_broker_route_audit_shows_only_the_callers_entries() {
        let local = log_with(&[LIST_ENTRY]);
        let (out, fake, x_entry) = audit_via_broker(&["audit"], &local);
        assert_eq!(out.code, 0, "{out:?}");
        assert_eq!(out.stdout, format!("{{\"entries\":[{x_entry}]}}\n"));
        let requests = fake.requests();
        assert_eq!(requests[0].len(), 2, "{requests:?}");
        assert_eq!(requests[0][1], json!({"audit": {}}));

        let (out, _, _) = audit_via_broker(&["audit", "--text"], &local);
        assert_eq!(
            out.stdout,
            "2026-10-21T09:30:15Z allowed list mandate=ab12cd34 account=a@example.com\n"
        );
        // The local log was neither read into the output nor written.
        assert_eq!(
            fs::read_to_string(&local).unwrap(),
            format!("{LIST_ENTRY}\n")
        );
    }

    #[test]
    fn ac5_on_the_broker_route_the_local_log_is_not_read() {
        let local_only =
            r#"{"time":"2026-01-01T00:00:00Z","operation":"search","outcome":"allowed"}"#;
        let local = log_with(&[local_only]);
        for args in [&["audit"][..], &["--text", "audit"]] {
            let (out, _, _) = audit_via_broker(args, &local);
            assert_eq!(out.code, 0, "{out:?}");
            assert!(!out.stdout.contains("2026-01-01T00:00:00Z"), "{out:?}");
            assert!(!out.stdout.contains("search"), "{out:?}");
        }
    }
}
