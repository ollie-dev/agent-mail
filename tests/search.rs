//! `agent-mail search` driven in-process against the fake IMAP server
//! (ticket 07).

mod support;

use std::path::PathBuf;

use agent_mail::RunOutput;
use agent_mail::mandate::{Grant, Mandate, Operation, Policy, TlsMode};
use agent_mail::seal::AgentToken;
use agent_mail::search::MessageRef;
use agent_mail::store::InMemoryStore;
use serde_json::{Map, Value, json};
use support::{
    ARCHIVE_UIDVALIDITY, CREDENTIAL, FakeImap, INBOX_UIDVALIDITY, LOGIN, Mode, account,
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
        Self::on(FakeImap::start(Mode::Implicit), grants)
    }

    /// Like [`Setup::new`], against a fake advertising `X-GM-EXT-1`.
    fn gmail(grants: Vec<Grant>) -> Self {
        Self::on(
            FakeImap::start_with_capabilities(Mode::Implicit, &["X-GM-EXT-1"]),
            grants,
        )
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

    /// `search` granted on INBOX and Archive.
    fn granted() -> Self {
        Self::new(vec![grant(Operation::Search, &["INBOX", "Archive"])])
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

    /// Runs `agent-mail search <args>`, expects success, returns the JSON.
    fn search(&self, args: &[&str]) -> Value {
        let argv: Vec<&str> = std::iter::once("search")
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
}

fn uids(result: &Value) -> Vec<u32> {
    result["messages"]
        .as_array()
        .expect("messages is an array")
        .iter()
        .map(|m| {
            let reference = m["ref"].as_str().expect("ref is a string");
            MessageRef::parse(reference)
                .unwrap_or_else(|_| panic!("ref does not parse: {reference}"))
                .uid
        })
        .collect()
}

/// AC6: every `ref` parses into the Account, `mailbox`, `uidvalidity` and
/// a UID in the result.
fn assert_refs(result: &Value, mailbox: &str, uidvalidity: u32) {
    for m in result["messages"].as_array().unwrap() {
        let reference = MessageRef::parse(m["ref"].as_str().unwrap()).expect("ref parses");
        assert_eq!(reference.account, LOGIN);
        assert_eq!(reference.mailbox, mailbox);
        assert_eq!(reference.uidvalidity, uidvalidity);
        assert!(reference.uid >= 1);
    }
}

fn parse_line(line: &str) -> Map<String, Value> {
    match serde_json::from_str::<Value>(line) {
        Ok(Value::Object(map)) => map,
        other => panic!("audit line is not a JSON object: {other:?}"),
    }
}

/// Key names of `line` in the order they appear.
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

fn inbox_ref(uid: u32) -> String {
    format!("{LOGIN}/INBOX/{INBOX_UIDVALIDITY}/{uid}")
}

#[test]
fn ac1_search_by_from_returns_matching_messages_newest_first() {
    let s = Setup::new(vec![grant(Operation::Search, &["INBOX"])]);
    let result = s.search(&["--mailbox", "INBOX", "--from", "alice@example.com"]);
    assert_eq!(
        result,
        json!({
            "account": LOGIN,
            "mailbox": "INBOX",
            "total": 2,
            "more": false,
            "messages": [
                {
                    "ref": inbox_ref(3),
                    "date": "2026-10-10T09:00:00Z",
                    "from": "alice@example.com",
                    "subject": "Re: Quarterly report",
                    "flags": ["\\Flagged"]
                },
                {
                    "ref": inbox_ref(1),
                    "date": "2026-10-01T09:00:00Z",
                    "from": "Alice Example <alice@example.com>",
                    "subject": "Quarterly report",
                    "flags": ["\\Seen"]
                }
            ]
        })
    );
    for m in result["messages"].as_array().unwrap() {
        let keys: Vec<&String> = m.as_object().unwrap().keys().collect();
        assert_eq!(keys.len(), 5, "{m}");
    }
    assert_refs(&result, "INBOX", INBOX_UIDVALIDITY);

    let all = s.search(&[]);
    let first = &all["messages"][0];
    assert_eq!(uids(&all)[0], 4);
    assert_eq!(first["date"], "2026-10-15T09:00:00Z");
    assert_eq!(first["subject"], "Rechnung für Oktober");
    assert_eq!(first["from"], "Carol <carol@example.com>");
    assert_eq!(first["flags"], json!(["\\Seen", "\\Flagged"]));
    assert_refs(&all, "INBOX", INBOX_UIDVALIDITY);
}

#[test]
fn ac2_each_criterion_narrows_and_criteria_combine_with_and() {
    let s = Setup::granted();
    let cases: [(&[&str], &[u32]); 13] = [
        (&[], &[4, 3, 2, 1]),
        (&["--from", "alice@example.com"], &[3, 1]),
        (&["--to", "team@example.com"], &[3]),
        (&["--subject", "quarterly"], &[3, 1]),
        (&["--text", "pizza"], &[3, 2]),
        (&["--since", "2026-10-05"], &[4, 3, 2]),
        (&["--before", "2026-10-10"], &[2, 1]),
        (&["--unseen"], &[3, 2]),
        (&["--flagged"], &[4, 3]),
        (&["--from", "alice@example.com", "--unseen"], &[3]),
        (&["--text", "pizza", "--flagged"], &[3]),
        (
            &["--since", "2026-10-05", "--before", "2026-10-15"],
            &[3, 2],
        ),
        (&["--from", "bob@example.com", "--flagged"], &[]),
    ];
    for (args, expected) in cases {
        let result = s.search(args);
        assert_eq!(uids(&result), expected, "{args:?}");
        assert_eq!(result["total"], expected.len(), "{args:?}");
        assert_eq!(result["more"], false, "{args:?}");
        assert_refs(&result, "INBOX", INBOX_UIDVALIDITY);
    }
    // One UID SEARCH per call: criteria go together, so the server ANDs them.
    let searches = s.server.searches();
    assert_eq!(searches.len(), cases.len());
    assert_eq!(searches[0], "ALL");
    assert_eq!(searches[9], "FROM \"alice@example.com\" UNSEEN");
    assert_eq!(searches[11], "SINCE 05-Oct-2026 BEFORE 15-Oct-2026");
}

#[test]
fn ac3_default_limit_is_20_and_limit_adjusts_it() {
    let s = Setup::granted();

    let result = s.search(&["--mailbox", "Archive"]);
    assert_eq!(uids(&result), (6..=25).rev().collect::<Vec<u32>>());
    assert_eq!(result["total"], 25);
    assert_eq!(result["more"], true);
    assert_eq!(s.server.fetch_uid_counts().iter().sum::<usize>(), 20);
    assert_refs(&result, "Archive", ARCHIVE_UIDVALIDITY);

    let before = s.server.fetch_uid_counts().len();
    let result = s.search(&["--mailbox", "Archive", "--limit", "5"]);
    assert_eq!(uids(&result), [25, 24, 23, 22, 21]);
    assert_eq!(result["total"], 25);
    assert_eq!(result["more"], true);
    assert_eq!(
        s.server.fetch_uid_counts()[before..].iter().sum::<usize>(),
        5
    );
    assert_refs(&result, "Archive", ARCHIVE_UIDVALIDITY);

    let result = s.search(&["--mailbox", "Archive", "--limit", "30"]);
    assert_eq!(uids(&result).len(), 25);
    assert_eq!(result["more"], false);
    assert_refs(&result, "Archive", ARCHIVE_UIDVALIDITY);

    let result = s.search(&["--limit", "5"]);
    assert_eq!(uids(&result), [4, 3, 2, 1]);
    assert_eq!(result["more"], false);
}

#[test]
fn ac3_bad_limits_are_usage_errors_without_a_connection() {
    let s = Setup::granted();
    for limit in ["0", "1001", "x"] {
        let out = s.run(&["search", "--mailbox", "Archive", "--limit", limit]);
        assert_eq!(out.code, 2, "--limit {limit}");
        assert_eq!(out.stdout, "");
        assert!(out.stderr.contains("--limit"), "{:?}", out.stderr);
    }
    assert_eq!(s.server.connections(), 0);
    assert!(s.audit().is_empty());
}

#[test]
fn usage_errors_for_dates_and_line_breaks_make_no_connection() {
    let s = Setup::granted();
    let cases: [&[&str]; 8] = [
        &["--since", "2026-13-01"],
        &["--before", "10/05/2026"],
        &["--since", "2026-02-30"],
        &["--subject", "a\r\nb"],
        &["--from", "x\ny"],
        &["--text", "a\0b"],
        &["--to", "a\rb"],
        &["--mailbox", "INBOX\r\nA1 DELETE INBOX"],
    ];
    for args in cases {
        let argv: Vec<&str> = std::iter::once("search")
            .chain(args.iter().copied())
            .collect();
        let out = s.run(&argv);
        assert_eq!(out.code, 2, "{args:?}");
        assert_eq!(out.stdout, "");
        assert!(!out.stderr.is_empty());
    }
    assert_eq!(s.server.connections(), 0);
    assert!(s.audit().is_empty());
}

fn assert_denied(grants: Vec<Grant>, args: &[&str], mailbox: &str) {
    let s = Setup::new(grants);
    let mut argv = vec!["search"];
    argv.extend_from_slice(args);
    let out = s.run(&argv);
    assert_eq!(out.code, 1, "{out:?}");
    assert_eq!(out.stdout, "");
    let error = error_of(&out);
    let keys: Vec<&String> = error.as_object().unwrap().keys().collect();
    assert_eq!(keys.len(), 5, "{error}");
    assert_eq!(error["code"], "denied");
    assert_eq!(error["operation"], "search");
    assert_eq!(error["account"], LOGIN);
    assert_eq!(error["mailbox"], mailbox);
    assert!(error["message"].as_str().unwrap().contains(mailbox));

    assert_eq!(s.server.connections(), 0, "the server saw a connection");
    let lines = s.audit();
    assert_eq!(
        single_entry(&lines),
        json!({
            "mandate": "aaaa1111",
            "operation": "search",
            "account": LOGIN,
            "mailbox": mailbox,
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
            "outcome",
            "reason"
        ]
    );

    let mut text_argv = vec!["--text"];
    text_argv.extend_from_slice(&argv);
    let text = s.run(&text_argv);
    assert_eq!(text.code, 1);
    assert!(
        text.stderr.starts_with("error: denied: ")
            && text.stderr.contains("search")
            && text.stderr.contains(mailbox),
        "{:?}",
        text.stderr
    );
    assert_eq!(s.server.connections(), 0, "the server saw a connection");
}

#[test]
fn ac4_search_outside_the_granted_mailbox_is_denied() {
    assert_denied(
        vec![
            grant(Operation::Search, &["INBOX"]),
            grant(Operation::List, &["*"]),
        ],
        &["--mailbox", "Archive"],
        "Archive",
    );
}

#[test]
fn ac4_search_without_a_search_grant_is_denied() {
    assert_denied(
        vec![
            grant(Operation::List, &["INBOX"]),
            grant(Operation::Read, &["INBOX"]),
        ],
        &["--mailbox", "INBOX"],
        "INBOX",
    );
}

#[test]
fn ac4_empty_policy_denies_the_default_mailbox() {
    assert_denied(vec![], &[], "INBOX");
}

#[test]
fn ac4_list_denial_still_has_no_mailbox_key() {
    let s = Setup::new(vec![grant(Operation::Search, &["INBOX"])]);
    let out = s.run(&["list"]);
    assert_eq!(out.code, 1);
    let error = error_of(&out);
    assert_eq!(error["code"], "denied");
    assert_eq!(error["operation"], "list");
    assert!(error.get("mailbox").is_none(), "{error}");
}

#[test]
fn ac5_search_appends_one_entry_with_references_and_count() {
    let s = Setup::new(vec![grant(Operation::Search, &["INBOX"])]);
    s.search(&["--mailbox", "INBOX", "--from", "alice@example.com"]);
    let lines = s.audit();
    assert_eq!(
        single_entry(&lines),
        json!({
            "mandate": "aaaa1111",
            "operation": "search",
            "account": LOGIN,
            "mailbox": "INBOX",
            "messages": [inbox_ref(3), inbox_ref(1)],
            "results": 2,
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
            "results",
            "outcome"
        ]
    );
}

#[test]
fn ac5_no_matches_default_limit_and_missing_mailbox_are_logged() {
    let s = Setup::new(vec![grant(Operation::Search, &["*"])]);

    s.search(&["--from", "bob@example.com", "--flagged"]);
    let entry = parse_line(&s.audit()[0]);
    assert_eq!(entry["results"], 0);
    assert_eq!(entry["messages"], json!([]));

    s.search(&["--mailbox", "Archive"]);
    let entry = parse_line(&s.audit()[1]);
    assert_eq!(entry["results"], 20);
    let refs = entry["messages"].as_array().unwrap();
    assert_eq!(refs.len(), 20);
    assert_eq!(refs[0], format!("{LOGIN}/Archive/{ARCHIVE_UIDVALIDITY}/25"));
    assert!(entry.get("total").is_none());

    let out = s.run(&["search", "--mailbox", "Missing"]);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    let error = error_of(&out);
    assert_eq!(error["code"], "mailbox_not_found");
    assert_eq!(error["account"], LOGIN);
    assert_eq!(error["mailbox"], "Missing");
    let lines = s.audit();
    assert_eq!(lines.len(), 3);
    let mut entry = parse_line(&lines[2]);
    entry.remove("time");
    assert_eq!(
        Value::Object(entry),
        json!({
            "mandate": "aaaa1111",
            "operation": "search",
            "account": LOGIN,
            "mailbox": "Missing",
            "outcome": "allowed",
            "reason": "mailbox_not_found"
        })
    );
}

#[test]
fn ac5_log_never_holds_subjects_bodies_or_secrets() {
    let s = Setup::new(vec![grant(Operation::Search, &["*"])]);
    s.search(&[]);
    s.search(&["--text", "pizza"]);
    s.search(&["--subject", "Quarterly"]);
    s.search(&["--mailbox", "Archive"]);
    let bytes = std::fs::read(&s.log).unwrap();
    for needle in [
        "Quarterly",
        "Rechnung",
        "Lunch",
        "Newsletter",
        "pizza",
        "numbers attached",
        "alice@example.com",
        CREDENTIAL,
        s.token.expose(),
        &token_hash(s.token.expose()),
    ] {
        assert!(
            !contains_bytes(&bytes, needle.as_bytes()),
            "the audit log contains {needle:?}"
        );
    }
}

#[test]
fn ac6_references_carry_uidvalidity_and_uid() {
    let s = Setup::granted();
    for (args, mailbox, uidvalidity) in [
        (
            &["--from", "alice@example.com"][..],
            "INBOX",
            INBOX_UIDVALIDITY,
        ),
        (&[][..], "INBOX", INBOX_UIDVALIDITY),
        (
            &["--mailbox", "Archive", "--limit", "30"][..],
            "Archive",
            ARCHIVE_UIDVALIDITY,
        ),
    ] {
        let result = s.search(args);
        assert!(!uids(&result).is_empty());
        assert_refs(&result, mailbox, uidvalidity);
    }
    let reference = MessageRef {
        account: "a/b%c@example.test".to_string(),
        mailbox: "[Gmail]/Entwürfe".to_string(),
        uidvalidity: 4_294_967_295,
        uid: 1,
    };
    let text = reference.format();
    assert_eq!(
        text,
        "a%2Fb%25c@example.test/%5BGmail%5D%2FEntw%C3%BCrfe/4294967295/1"
    );
    assert_eq!(text.matches('/').count(), 3);
    assert_eq!(MessageRef::parse(&text), Ok(reference));
    for bad in [
        "INBOX/1/2",
        "a@b/INBOX/x/2",
        "a@b/INBOX/1/0",
        "a@b/INBOX/1/2/3",
    ] {
        assert!(MessageRef::parse(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn quotes_in_a_criterion_cannot_inject_search_keys() {
    let s = Setup::granted();
    let result = s.search(&["--subject", "x\" ALL \"y"]);
    assert_eq!(uids(&result), Vec::<u32>::new());
    assert_eq!(result["total"], 0);
    let searches: Vec<String> = s
        .server
        .commands()
        .into_iter()
        .filter(|c| c == "UID SEARCH")
        .collect();
    assert_eq!(searches.len(), 1);
    assert_eq!(s.server.searches(), ["SUBJECT \"x\\\" ALL \\\"y\""]);

    let result = s.search(&["--subject", "back\\slash"]);
    assert_eq!(result["total"], 0);
    assert_eq!(s.server.searches()[1], "SUBJECT \"back\\\\slash\"");
}

#[test]
fn search_never_changes_mailbox_state() {
    let s = Setup::granted();
    let before = s.server.all_flags();
    s.search(&[]);
    s.search(&["--unseen"]);
    s.search(&["--mailbox", "Archive", "--limit", "1000"]);
    assert_eq!(s.server.all_flags(), before);
    let commands = s.server.commands();
    assert!(commands.iter().any(|c| c == "EXAMINE"), "{commands:?}");
    for forbidden in ["SELECT", "STORE", "UID STORE", "EXPUNGE"] {
        assert!(
            !commands.iter().any(|c| c == forbidden),
            "the server saw {forbidden}: {commands:?}"
        );
    }
    // The fake does set \Seen on a non-peek body fetch, so the check above
    // would catch a client that forgot `.PEEK`.
}

#[test]
fn non_ascii_criteria_are_sent_as_utf8_with_charset() {
    let s = Setup::granted();
    let result = s.search(&["--subject", "für"]);
    // The fake matches raw headers, where the subject is RFC 2047-encoded.
    assert_eq!(result["total"], 0);
    assert_eq!(s.server.searches(), ["CHARSET UTF-8 SUBJECT \"für\""]);
}

#[test]
fn non_ascii_mailbox_is_granted_by_its_decoded_name_and_sent_as_utf7() {
    let s = Setup::new(vec![grant(Operation::Search, &["[Gmail]/Entwürfe"])]);
    let result = s.search(&["--mailbox", "[Gmail]/Entwürfe"]);
    assert_eq!(result["mailbox"], "[Gmail]/Entwürfe");
    assert_eq!(result["total"], 0);
    assert_eq!(result["messages"], json!([]));
    let entry = parse_line(&s.audit()[0]);
    assert_eq!(entry["mailbox"], "[Gmail]/Entwürfe");
    assert_eq!(entry["outcome"], "allowed");
    // The raw UTF-7 name is not a granted name.
    let out = s.run(&["search", "--mailbox", "[Gmail]/Entw&APw-rfe"]);
    assert_eq!(error_of(&out)["code"], "denied");
}

#[test]
fn spam_is_empty_and_text_output_has_header_lines_and_more_hint() {
    let s = Setup::new(vec![grant(Operation::Search, &["*"])]);
    let result = s.search(&["--mailbox", "Spam"]);
    assert_eq!(result["total"], 0);
    assert_eq!(result["more"], false);

    let out = s.run(&["--text", "search", "--from", "alice@example.com"]);
    assert_eq!(out.code, 0, "{:?}", out.stderr);
    assert_eq!(
        out.stdout,
        format!(
            "{LOGIN} INBOX: showing 2 of 2 messages\n\
             2026-10-10T09:00:00Z  alice@example.com  Re: Quarterly report  [\\Flagged]  {}\n\
             2026-10-01T09:00:00Z  Alice Example <alice@example.com>  Quarterly report  [\\Seen]  {}\n",
            inbox_ref(3),
            inbox_ref(1)
        )
    );

    let out = s.run(&[
        "--text",
        "search",
        "--mailbox",
        "Archive",
        "--limit",
        "2",
        "--text",
        "news",
    ]);
    assert_eq!(out.code, 0, "{:?}", out.stderr);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(
        lines[0],
        format!("{LOGIN} Archive: showing 2 of 25 messages")
    );
    assert_eq!(lines.len(), 4);
    assert_eq!(
        lines[3],
        "more messages match; narrow the search or raise --limit"
    );
}

#[test]
fn search_help_lists_every_option_and_default() {
    let s = Setup::granted();
    let out = s.run(&["search", "--help"]);
    assert_eq!(out.code, 0);
    for option in [
        "--mailbox",
        "--from",
        "--to",
        "--subject",
        "--text",
        "--since",
        "--before",
        "--unseen",
        "--flagged",
        "--limit",
        "YYYY-MM-DD",
        "[default: INBOX]",
        "[default: 20]",
    ] {
        assert!(
            out.stdout.contains(option),
            "help lacks {option}: {}",
            out.stdout
        );
    }
    assert_eq!(s.server.connections(), 0);
}

#[test]
fn text_flag_still_works_after_list_and_mandate() {
    let s = Setup::new(vec![grant(Operation::List, &["INBOX"])]);
    for args in [
        &["--text", "list"][..],
        &["list", "--text"],
        &["mandate", "--text", "list"],
        &["mandate", "list", "--text"],
    ] {
        let out = s.run(args);
        assert_eq!(out.code, 0, "{args:?}: {:?}", out.stderr);
        assert!(
            serde_json::from_str::<Value>(&out.stdout).is_err(),
            "{args:?} printed JSON"
        );
    }
}

/// The body of the README section `## <name>`.
fn readme_section(name: &str) -> String {
    let readme =
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md"))
            .unwrap();
    let start = readme
        .find(&format!("\n## {name}\n"))
        .unwrap_or_else(|| panic!("README has no `## {name}` section"));
    let rest = &readme[start + 1..];
    match rest[3..].find("\n## ") {
        Some(end) => rest[..end + 3].to_string(),
        None => rest.to_string(),
    }
}

#[test]
fn readme_documents_search_and_its_audit_fields() {
    let search = readme_section("Search");
    for needle in [
        "--mailbox",
        "--from",
        "--to",
        "--subject",
        "--text",
        "--since",
        "--before",
        "--unseen",
        "--flagged",
        "--limit",
        "`ref`",
        "`date`",
        "`from`",
        "`subject`",
        "`flags`",
        "`total`",
        "`more`",
        "<account>/<mailbox>/<uidvalidity>/<uid>",
        "agent@example.test/%5BGmail%5D%2FEntw%C3%BCrfe/1700000003/7",
    ] {
        assert!(
            search.contains(needle),
            "README Search section lacks {needle}"
        );
    }
    let audit = readme_section("Audit log");
    for needle in ["`search`", "`mailbox`", "`messages`", "`results`"] {
        assert!(
            audit.contains(needle),
            "README audit section lacks {needle}"
        );
    }
}

#[test]
fn ac1_gmail_query_is_sent_unchanged_as_x_gm_raw_and_decides_the_result() {
    let s = Setup::gmail(vec![grant(Operation::Search, &["INBOX"])]);
    s.server
        .script_gmail_search("from:alice has:attachment", &[1]);
    let result = s.search(&["--gmail", "from:alice has:attachment"]);
    assert_eq!(
        s.server.searches(),
        ["X-GM-RAW \"from:alice has:attachment\""]
    );
    assert_eq!(result["mailbox"], "INBOX");
    assert_eq!(result["total"], 1);
    assert_eq!(result["more"], false);
    let messages = result["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["ref"], inbox_ref(1));
    assert_eq!(messages[0]["subject"], "Quarterly report");

    // The standard criterion would have found UIDs 3 and 1.
    let standard = s.search(&["--from", "alice"]);
    assert_eq!(uids(&standard), [3, 1]);
}

#[test]
fn ac1_gmail_query_is_quoted_and_marked_utf8() {
    let s = Setup::gmail(vec![grant(Operation::Search, &["INBOX"])]);
    s.search(&["--gmail", "von:jürgen"]);
    s.search(&["--gmail", "subject:\"a\\b\""]);
    let injection = s.search(&["--gmail", "x\" ALL \"y"]);
    assert_eq!(injection["total"], 0);
    assert_eq!(
        s.server.searches(),
        [
            "CHARSET UTF-8 X-GM-RAW \"von:jürgen\"",
            "X-GM-RAW \"subject:\\\"a\\\\b\\\"\"",
            "X-GM-RAW \"x\\\" ALL \\\"y\"",
        ]
    );
}

#[test]
fn ac1_bad_gmail_queries_and_combinations_are_usage_errors_without_a_connection() {
    let s = Setup::gmail(vec![grant(Operation::Search, &["*"])]);
    let cases: [&[&str]; 12] = [
        &["--gmail", ""],
        &["--gmail", "a\rb"],
        &["--gmail", "a\nb"],
        &["--gmail", "a\0b"],
        &["--gmail", "x", "--from", "alice"],
        &["--gmail", "x", "--to", "bob"],
        &["--to", "bob", "--gmail", "x"],
        &["--gmail", "x", "--subject", "s"],
        &["--gmail", "x", "--text", "t"],
        &["--gmail", "x", "--since", "2026-10-05"],
        &["--gmail", "x", "--before", "2026-10-05"],
        &["--gmail", "x", "--unseen"],
    ];
    let flagged: &[&str] = &["--flagged", "--gmail", "x"];
    for args in cases.iter().copied().chain(std::iter::once(flagged)) {
        let argv: Vec<&str> = std::iter::once("search")
            .chain(args.iter().copied())
            .collect();
        let out = s.run(&argv);
        assert_eq!(out.code, 2, "{args:?}");
        assert_eq!(out.stdout, "");
        assert!(out.stderr.contains("--gmail"), "{args:?}: {:?}", out.stderr);
    }
    assert_eq!(s.server.connections(), 0);
    assert!(s.audit().is_empty());
}

#[test]
fn ac2_gmail_on_a_server_without_x_gm_ext_1_is_unsupported() {
    let s = Setup::new(vec![grant(Operation::Search, &["INBOX"])]);
    let out = s.run(&["search", "--gmail", "has:attachment"]);
    assert_eq!(out.code, 1, "{:?}", out.stderr);
    assert_eq!(out.stdout, "");
    let error = error_of(&out);
    assert_eq!(error["code"], "unsupported");
    assert_eq!(error["account"], LOGIN);
    assert_eq!(error["mailbox"], "INBOX");
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("Gmail search"), "{message}");
    assert!(
        message.contains("--from") && message.contains("--subject"),
        "{message}"
    );
    assert!(!message.contains("has:attachment"), "{message}");
    assert!(!out.stderr.contains("has:attachment"));

    let commands = s.server.commands();
    for verb in ["EXAMINE", "SELECT", "UID SEARCH"] {
        assert!(
            !commands.iter().any(|c| c == verb),
            "{verb} in {commands:?}"
        );
    }
    assert_eq!(commands.last().map(String::as_str), Some("LOGOUT"));
    assert!(s.server.searches().is_empty());

    let text = s.run(&["--text", "search", "--gmail", "has:attachment"]);
    assert_eq!(text.code, 1);
    assert_eq!(text.stdout, "");
    assert!(
        text.stderr.starts_with("error: unsupported: ") && text.stderr.contains("Gmail search"),
        "{:?}",
        text.stderr
    );
    assert!(!text.stderr.contains("has:attachment"));
}

#[test]
fn ac3_gmail_outside_the_granted_mailbox_is_denied() {
    assert_denied(
        vec![grant(Operation::Search, &["INBOX"])],
        &["--mailbox", "Archive", "--gmail", "from:alice"],
        "Archive",
    );
}

#[test]
fn ac3_gmail_without_a_search_grant_is_denied() {
    assert_denied(
        vec![grant(Operation::Read, &["INBOX"])],
        &["--gmail", "x"],
        "INBOX",
    );
}

#[test]
fn ac3_gmail_respects_the_limit() {
    let s = Setup::gmail(vec![grant(Operation::Search, &["Archive"])]);
    let all: Vec<u32> = (1..=25).collect();
    s.server
        .script_gmail_search_in("Archive", "label:all", &all);

    let five = s.search(&[
        "--mailbox",
        "Archive",
        "--gmail",
        "label:all",
        "--limit",
        "5",
    ]);
    assert_eq!(uids(&five), [25, 24, 23, 22, 21]);
    assert_eq!(five["total"], 25);
    assert_eq!(five["more"], true);
    assert_refs(&five, "Archive", ARCHIVE_UIDVALIDITY);

    let default = s.search(&["--mailbox", "Archive", "--gmail", "label:all"]);
    assert_eq!(uids(&default), (6..=25).rev().collect::<Vec<u32>>());
    assert_eq!(default["total"], 25);
    assert_eq!(default["more"], true);

    let connections = s.server.connections();
    for limit in ["0", "1001"] {
        let out = s.run(&[
            "search",
            "--mailbox",
            "Archive",
            "--gmail",
            "label:all",
            "--limit",
            limit,
        ]);
        assert_eq!(out.code, 2, "--limit {limit}");
        assert_eq!(out.stdout, "");
    }
    assert_eq!(s.server.connections(), connections);
}

#[test]
fn ac3_gmail_audit_lines_match_standard_search_and_never_hold_the_query() {
    let s = Setup::gmail(vec![grant(Operation::Search, &["INBOX"])]);
    s.server
        .script_gmail_search("from:alice has:attachment", &[1]);
    s.search(&["--gmail", "from:alice has:attachment"]);
    let lines = s.audit();
    assert_eq!(
        single_entry(&lines),
        json!({
            "mandate": "aaaa1111",
            "operation": "search",
            "account": LOGIN,
            "mailbox": "INBOX",
            "messages": [inbox_ref(1)],
            "results": 1,
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
            "results",
            "outcome"
        ]
    );

    // Denied: outside the Grant.
    let out = s.run(&[
        "search",
        "--mailbox",
        "Archive",
        "--gmail",
        "has:attachment",
    ]);
    assert_eq!(out.code, 1);
    let mut entry = parse_line(&s.audit()[1]);
    entry.remove("time");
    assert_eq!(
        Value::Object(entry),
        json!({
            "mandate": "aaaa1111",
            "operation": "search",
            "account": LOGIN,
            "mailbox": "Archive",
            "outcome": "denied",
            "reason": "denied"
        })
    );

    // Unsupported: a server without X-GM-EXT-1.
    let plain = Setup::new(vec![grant(Operation::Search, &["INBOX"])]);
    let out = plain.run(&["search", "--gmail", "has:attachment"]);
    assert_eq!(out.code, 1);
    let lines = plain.audit();
    assert_eq!(
        single_entry(&lines),
        json!({
            "mandate": "aaaa1111",
            "operation": "search",
            "account": LOGIN,
            "mailbox": "INBOX",
            "outcome": "allowed",
            "reason": "unsupported"
        })
    );

    for log in [&s.log, &plain.log] {
        let bytes = std::fs::read(log).unwrap();
        for needle in [
            "has:attachment",
            "from:alice",
            CREDENTIAL,
            s.token.expose(),
            plain.token.expose(),
        ] {
            assert!(
                !contains_bytes(&bytes, needle.as_bytes()),
                "the audit log holds {needle:?}"
            );
        }
    }
}

#[test]
fn ac6_readme_and_help_document_gmail_search_and_the_live_test() {
    let search = readme_section("Search");
    for needle in [
        "--gmail QUERY",
        "agent-mail search --gmail \"has:attachment newer_than:7d\"",
        "X-GM-EXT-1",
        "unsupported",
    ] {
        assert!(
            search.contains(needle),
            "README Search section lacks {needle}"
        );
    }
    let readme =
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md"))
            .unwrap();
    for needle in [
        "AGENT_MAIL_LIVE_GMAIL_ADDRESS",
        "AGENT_MAIL_LIVE_GMAIL_APP_PASSWORD",
        "cargo test --test gmail_live -- --ignored",
    ] {
        assert!(readme.contains(needle), "README lacks {needle}");
    }

    let audit = readme_section("Audit log");
    let reason = audit
        .lines()
        .find(|l| l.starts_with("- `reason`"))
        .expect("README audit section has a `reason` bullet");
    assert!(
        reason.contains("`unsupported`"),
        "README `reason` bullet lacks `unsupported`"
    );

    let s = Setup::granted();
    let out = s.run(&["search", "--help"]);
    assert_eq!(out.code, 0);
    for needle in [
        "--gmail <QUERY>",
        "X-GM-EXT-1",
        "`unsupported`",
        "Cannot be combined with --from, --to, --subject, --text, --since, --before, --unseen or --flagged",
        "works with --mailbox and --limit",
    ] {
        assert!(
            out.stdout.contains(needle),
            "help lacks {needle}: {}",
            out.stdout
        );
    }

    // `-h` shows one short line per option; the detail is in `--help`.
    let short = s.run(&["search", "-h"]);
    assert_eq!(short.code, 0);
    let gmail = short
        .stdout
        .lines()
        .find(|l| l.contains("--gmail"))
        .expect("-h lists --gmail");
    assert!(
        gmail.chars().count() < 100,
        "-h --gmail line is {} characters: {gmail}",
        gmail.chars().count()
    );
    assert!(!short.stdout.contains("X-GM-EXT-1"), "{}", short.stdout);
    for line in short.stdout.lines() {
        assert!(
            line.chars().count() < 100,
            "-h line is {} characters: {line}",
            line.chars().count()
        );
    }
    assert_eq!(s.server.connections(), 0);
}
