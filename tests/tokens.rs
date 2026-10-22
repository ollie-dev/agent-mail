//! Several Agent Tokens in `AGENT_MAIL_TOKEN`, `--account`, Mandate
//! selection, `ambiguous_mandate`, the invalid-token warning and `whoami`.
//! All values are fake.

mod support;

use std::path::PathBuf;

use agent_mail::RunOutput;
use agent_mail::mandate::{Grant, Mandate, Operation};
use agent_mail::seal::AgentToken;
use agent_mail::store::{InMemoryStore, MandateStore};
use serde_json::{Value, json};
use support::*;

/// A fresh fake server (with `MOVE`, `UIDPLUS` and a `\Drafts` mailbox), an
/// empty store and a fresh audit log.
struct Env {
    server: FakeImap,
    store: InMemoryStore,
    log: PathBuf,
}

impl Env {
    fn new() -> Self {
        Self {
            server: FakeImap::start_with_list(
                Mode::Implicit,
                &["MOVE", "UIDPLUS"],
                &["Drafts"],
                Some("Drafts"),
            ),
            store: InMemoryStore::new(),
            log: temp_audit_log(),
        }
    }

    /// Stores a Mandate `id` covering `logins` with `grants`; returns its
    /// token text.
    fn add(&self, id: &str, logins: &[&str], grants: Vec<Grant>) -> String {
        self.add_named(id, "test-mandate", logins, grants)
    }

    fn add_named(&self, id: &str, name: &str, logins: &[&str], grants: Vec<Grant>) -> String {
        let mandate: Mandate = mandate_with_id(id, name, logins, self.server.port, grants);
        store_mandate(&self.store, &mandate).expose().to_string()
    }

    /// Runs with the raw `AGENT_MAIL_TOKEN` string `tokens`, and checks no
    /// Credential or token-shaped text is printed.
    fn run(&self, args: &[&str], tokens: Option<&str>) -> RunOutput {
        let out = run_raw_tokens(args, tokens, &self.store, &self.server, &self.log);
        for stream in [&out.stdout, &out.stderr] {
            assert!(!stream.contains(CREDENTIAL), "output holds the Credential");
            assert!(
                tokens_in(stream).is_empty(),
                "output holds a token: {stream}"
            );
            assert!(
                !stream.contains("garbage"),
                "output echoes a token: {stream}"
            );
        }
        out
    }

    fn audit(&self) -> Vec<Value> {
        let lines = audit_lines(&self.log);
        for line in &lines {
            assert!(tokens_in(line).is_empty(), "audit holds a token: {line}");
            assert!(!line.contains("garbage"), "audit echoes a token: {line}");
            assert!(!line.contains(CREDENTIAL), "audit holds the Credential");
        }
        lines
            .iter()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn last_audit(&self) -> Value {
        self.audit().pop().expect("an audit line")
    }
}

fn inbox_ref(login: &str) -> String {
    format!("{login}/INBOX/{INBOX_UIDVALIDITY}/1")
}

fn stderr_error(out: &RunOutput) -> Value {
    error_of(out)
}

/// A token minted but never stored.
fn unknown_token() -> String {
    AgentToken::mint().expose().to_string()
}

/// Stores a Mandate and then flips one byte of its sealed part, so the
/// record no longer opens. Returns its token text.
fn tampered_token(env: &Env) -> String {
    let text = env.add("dddd4444", &[LOGIN], vec![grant(Operation::List, &["*"])]);
    let token = AgentToken::parse(&text).unwrap();
    let key = token.store_key();
    let mut value = env.store.get(&key).unwrap().unwrap();
    let last = value.len() - 1;
    value[last] ^= 0x01;
    assert!(env.store.delete(&key).unwrap());
    env.store.put(&key, &value).unwrap();
    text
}

const AC1_WORK: &str =
    "{\"account\":\"agent@example.test\",\"mailboxes\":[{\"name\":\"INBOX\"}]}\n";
const AC1_PRIVATE: &str =
    "{\"account\":\"other@example.test\",\"mailboxes\":[{\"name\":\"Archive\"}]}\n";

#[test]
fn ac1_account_selects_the_mandate_covering_it() {
    let env = Env::new();
    let tw = env.add(
        "aaaa1111",
        &[LOGIN],
        vec![
            grant(Operation::List, &["INBOX"]),
            grant(Operation::Read, &["INBOX"]),
        ],
    );
    let tp = env.add(
        "bbbb2222",
        &[OTHER_LOGIN],
        vec![
            grant_on(Operation::List, OTHER_LOGIN, &["Archive"]),
            grant_on(Operation::Search, OTHER_LOGIN, &["Archive"]),
        ],
    );
    for tokens in [format!("{tw},{tp}"), format!("{tp}, {tw}")] {
        let out = env.run(&["list", "--account", LOGIN], Some(&tokens));
        assert_eq!(out.code, 0, "{out:?}");
        assert_eq!(out.stdout, AC1_WORK);
        assert_eq!(out.stderr, "");
        let line = env.last_audit();
        assert_eq!(line["mandate"], "aaaa1111");
        assert_eq!(line["account"], LOGIN);
        assert_eq!(line["outcome"], "allowed");

        let out = env.run(&["list", "--account", OTHER_LOGIN], Some(&tokens));
        assert_eq!(out.code, 0, "{out:?}");
        assert_eq!(out.stdout, AC1_PRIVATE);
        assert_eq!(out.stderr, "");
        let line = env.last_audit();
        assert_eq!(line["mandate"], "bbbb2222");
        assert_eq!(line["account"], OTHER_LOGIN);
    }

    // Reference commands select by the reference's account.
    let tokens = format!("{tw},{tp}");
    let out = env.run(&["read", &inbox_ref(LOGIN)], Some(&tokens));
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(env.last_audit()["mandate"], "aaaa1111");

    let out = env.run(
        &["search", "--account", OTHER_LOGIN, "--mailbox", "Archive"],
        Some(&tokens),
    );
    assert_eq!(out.code, 0, "{out:?}");
    let result: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(result["account"], OTHER_LOGIN);
    assert_eq!(env.last_audit()["mandate"], "bbbb2222");
}

#[test]
fn ac1_unacceptable_account_values_are_usage_errors() {
    let env = Env::new();
    let tw = env.add("aaaa1111", &[LOGIN], vec![grant(Operation::List, &["*"])]);
    let r = inbox_ref(LOGIN);
    for args in [
        &["list", "--account", ""][..],
        &["list", "--account", "a\nb"],
        &["list", "--account", "a\rb"],
        &["list", "--account", "a\0b"],
        &["search", "--account", ""],
        &["search", "--account", "a\nb"],
        &[
            "draft",
            "--account",
            "",
            "--to",
            "carol@example.com",
            "--subject",
            "S",
            "--body",
            "B",
        ],
        &[
            "draft",
            "--account",
            LOGIN,
            "--reply-to",
            &r,
            "--to",
            "carol@example.com",
            "--body",
            "B",
        ],
    ] {
        let out = env.run(args, Some(&tw));
        assert_eq!(out.code, 2, "{args:?}: {out:?}");
        assert_eq!(out.stdout, "", "{args:?}");
    }
    assert_eq!(env.server.connections(), 0);
    assert!(env.audit().is_empty());
}

#[test]
fn ac2_account_may_be_omitted_only_when_one_account_is_reachable() {
    let env = Env::new();
    let tw = env.add(
        "aaaa1111",
        &[LOGIN],
        vec![grant(Operation::List, &["INBOX"])],
    );
    let tp = env.add(
        "bbbb2222",
        &[OTHER_LOGIN],
        vec![grant_on(Operation::List, OTHER_LOGIN, &["Archive"])],
    );

    // One reachable Account.
    for tokens in [tw.clone(), format!("{tw},garbage")] {
        let out = env.run(&["list"], Some(&tokens));
        assert_eq!(out.code, 0, "{out:?}");
        assert_eq!(out.stdout, AC1_WORK);
        assert_eq!(env.last_audit()["mandate"], "aaaa1111");
    }

    // Several reachable Accounts.
    let both = env.add(
        "cccc3333",
        &[LOGIN, OTHER_LOGIN],
        vec![
            grant_on(Operation::List, LOGIN, &["*"]),
            grant_on(Operation::Search, LOGIN, &["*"]),
            grant_on(Operation::Draft, LOGIN, &["*"]),
        ],
    );
    let connections = env.server.connections();
    for tokens in [format!("{tw},{tp}"), both] {
        for args in [
            &["list"][..],
            &["search"],
            &[
                "draft",
                "--to",
                "carol@example.com",
                "--subject",
                "S",
                "--body",
                "B",
            ],
        ] {
            let before = env.audit().len();
            let out = env.run(args, Some(&tokens));
            assert_eq!(out.code, 1, "{args:?}: {out:?}");
            assert_eq!(out.stdout, "");
            let value: Value = serde_json::from_str(&out.stderr).unwrap();
            let error = &value["error"];
            assert_eq!(error["code"], "account_required");
            assert_eq!(error["accounts"], json!([LOGIN, OTHER_LOGIN]));
            let keys: Vec<&String> = error.as_object().unwrap().keys().collect();
            assert_eq!(keys.len(), 3, "{error}");
            let message = error["message"].as_str().unwrap();
            assert!(message.contains("--account"), "{message}");
            assert!(!message.contains("not supported yet"), "{message}");

            let lines = env.audit();
            assert_eq!(lines.len(), before + 1);
            let line = lines.last().unwrap();
            assert_eq!(line["outcome"], "denied");
            assert_eq!(line["reason"], "account_required");
            assert!(line.get("mandate").is_none(), "{line}");
            assert!(line.get("account").is_none(), "{line}");
        }
    }
    assert_eq!(env.server.connections(), connections);
}

/// AC3 fixture: X and Y both cover the work Account, P the private one.
fn ac3_env(y_grants: bool) -> (Env, String, String, String) {
    let env = Env::new();
    let all = |login: &str| -> Vec<Grant> {
        Operation::ALL
            .iter()
            .map(|op| grant_on(*op, login, &["*"]))
            .collect()
    };
    let tx = env.add("aaaa1111", &[LOGIN], all(LOGIN));
    let ty = env.add(
        "bbbb2222",
        &[LOGIN],
        if y_grants { all(LOGIN) } else { Vec::new() },
    );
    let tp = env.add(
        "cccc3333",
        &[OTHER_LOGIN],
        vec![grant_on(Operation::List, OTHER_LOGIN, &["Archive"])],
    );
    (env, tx, ty, tp)
}

fn assert_ambiguous(env: &Env, args: &[&str], tokens: &str) {
    let before = env.audit().len();
    let out = env.run(args, Some(tokens));
    assert_eq!(out.code, 1, "{args:?}: {out:?}");
    assert_eq!(out.stdout, "");
    let value: Value = serde_json::from_str(&out.stderr).unwrap();
    let error = &value["error"];
    assert_eq!(error["code"], "ambiguous_mandate", "{args:?}");
    assert_eq!(error["account"], LOGIN);
    assert_eq!(error["mandates"], json!(["aaaa1111", "bbbb2222"]));
    // Sorted keys, as in every error object.
    assert!(
        out.stderr.starts_with(
            "{\"error\":{\"account\":\"agent@example.test\",\"code\":\"ambiguous_mandate\",\"mandates\":[\"aaaa1111\",\"bbbb2222\"],\"message\":"
        ),
        "{}",
        out.stderr
    );
    assert_eq!(error.as_object().unwrap().len(), 4, "{error}");
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("never picks"), "{message}");
    assert!(message.contains("only one of these tokens"), "{message}");
    assert!(message.contains("ask the human to delete"), "{message}");

    let lines = env.audit();
    assert_eq!(lines.len(), before + 1, "{args:?}");
    let line = lines.last().unwrap();
    assert_eq!(line["account"], LOGIN);
    assert_eq!(line["outcome"], "denied");
    assert_eq!(line["reason"], "ambiguous_mandate");
    assert!(line.get("mandate").is_none(), "{line}");
}

#[test]
fn ac3_two_mandates_covering_one_account_are_ambiguous() {
    let (env, tx, ty, tp) = ac3_env(true);
    let tokens = format!("{tx},{ty},{tp}");
    let r = inbox_ref(LOGIN);
    for args in [
        &["list", "--account", LOGIN][..],
        &["search", "--account", LOGIN],
        &["read", &r],
        &["flag", &r, "--set", "seen"],
        &["move", &r, "--to", "Archive"],
        &[
            "draft",
            "--account",
            LOGIN,
            "--to",
            "carol@example.com",
            "--subject",
            "S",
            "--body",
            "B",
        ],
        &[
            "draft",
            "--reply-to",
            &r,
            "--to",
            "carol@example.com",
            "--body",
            "B",
        ],
    ] {
        assert_ambiguous(&env, args, &tokens);
    }
    assert_eq!(env.server.connections(), 0);

    // Other Accounts are unaffected.
    let out = env.run(&["list", "--account", OTHER_LOGIN], Some(&tokens));
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(out.stdout, AC1_PRIVATE);

    // The same token twice is not ambiguous.
    let out = env.run(&["list"], Some(&format!("{tx},{tx}")));
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(env.last_audit()["mandate"], "aaaa1111");
}

#[test]
fn ac3_ambiguity_is_reported_even_when_only_one_mandate_grants() {
    let (env, tx, ty, tp) = ac3_env(false);
    assert_ambiguous(
        &env,
        &["list", "--account", LOGIN],
        &format!("{tx},{ty},{tp}"),
    );
    assert_eq!(env.server.connections(), 0);
}

#[test]
fn ac4_grants_never_combine_across_mandates() {
    let env = Env::new();
    let tw = env.add(
        "aaaa1111",
        &[LOGIN],
        vec![
            grant(Operation::List, &["INBOX"]),
            grant_on(Operation::List, OTHER_LOGIN, &["*"]),
            grant_on(Operation::Search, OTHER_LOGIN, &["*"]),
            grant_on(Operation::Read, OTHER_LOGIN, &["*"]),
            grant_on(Operation::Draft, OTHER_LOGIN, &["*"]),
        ],
    );
    let tp = env.add(
        "bbbb2222",
        &[OTHER_LOGIN],
        vec![grant_on(Operation::List, OTHER_LOGIN, &["Archive"])],
    );
    let tokens = format!("{tw},{tp}");

    let out = env.run(&["list", "--account", OTHER_LOGIN], Some(&tokens));
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(out.stdout, AC1_PRIVATE);
    let connections = env.server.connections();

    let other_ref = inbox_ref(OTHER_LOGIN);
    for (args, operation, mailbox) in [
        (
            &["search", "--account", OTHER_LOGIN, "--mailbox", "INBOX"][..],
            "search",
            Some("INBOX"),
        ),
        (&["read", &other_ref], "read", Some("INBOX")),
        (
            &[
                "draft",
                "--account",
                OTHER_LOGIN,
                "--to",
                "carol@example.com",
                "--subject",
                "S",
                "--body",
                "B",
            ],
            "draft",
            None,
        ),
    ] {
        let out = env.run(args, Some(&tokens));
        assert_eq!(out.code, 1, "{args:?}: {out:?}");
        let error = stderr_error(&out);
        assert_eq!(error["code"], "denied", "{args:?}");
        assert_eq!(error["operation"], operation);
        assert_eq!(error["account"], OTHER_LOGIN);
        match mailbox {
            Some(mailbox) => assert_eq!(error["mailbox"], mailbox),
            None => assert!(error.get("mailbox").is_none()),
        }
        let line = env.last_audit();
        assert_eq!(line["mandate"], "bbbb2222", "{args:?}");
        assert_eq!(line["outcome"], "denied");
    }
    assert_eq!(env.server.connections(), connections);

    // An Account no opened Mandate covers.
    let out = env.run(&["list", "--account", OTHER_LOGIN], Some(&tw));
    assert_eq!(out.code, 1, "{out:?}");
    let error = stderr_error(&out);
    assert_eq!(error["code"], "denied");
    assert_eq!(error["operation"], "list");
    assert_eq!(error["account"], OTHER_LOGIN);
    assert_eq!(env.server.connections(), connections);
    let line = env.last_audit();
    assert_eq!(line["account"], OTHER_LOGIN);
    assert!(line.get("mandate").is_none(), "{line}");
}

fn warning_of(line: &str) -> Value {
    let value: Value =
        serde_json::from_str(line).unwrap_or_else(|e| panic!("not JSON ({e}): {line:?}"));
    value["warning"].clone()
}

#[test]
fn ac5_invalid_tokens_are_reported_without_blocking_valid_ones() {
    let env = Env::new();
    let tw = env.add(
        "aaaa1111",
        &[LOGIN],
        vec![grant(Operation::List, &["INBOX"])],
    );

    let out = env.run(&["list"], Some(&format!("garbage,{tw}")));
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(out.stdout, AC1_WORK);
    assert_eq!(out.stderr.lines().count(), 1, "{:?}", out.stderr);
    assert!(out.stderr.ends_with('\n'));
    let warning = warning_of(&out.stderr);
    assert_eq!(warning["code"], "invalid_token");
    assert_eq!(warning["positions"], json!([1]));
    assert_eq!(warning.as_object().unwrap().len(), 3);
    assert!(
        out.stderr
            .starts_with("{\"warning\":{\"code\":\"invalid_token\",\"positions\":[1],\"message\":")
    );
    let message = warning["message"].as_str().unwrap();
    assert!(
        message.contains("position 1 of AGENT_MAIL_TOKEN"),
        "{message}"
    );
    assert!(message.contains("not valid and was ignored"), "{message}");
    assert!(message.contains("check it"), "{message}");
    assert!(
        message.contains("ask the human for a new token"),
        "{message}"
    );
    let clean = env.last_audit();

    let unknown = unknown_token();
    let tampered = tampered_token(&env);
    let out = env.run(&["list"], Some(&format!("{tw},{unknown},{tampered}")));
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(out.stdout, AC1_WORK);
    assert_eq!(warning_of(&out.stderr)["positions"], json!([2, 3]));

    let out = env.run(&["list"], Some(&format!("{tw},,garbage")));
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(warning_of(&out.stderr)["positions"], json!([3]));
    // Same audit keys as without invalid tokens.
    let with_invalid = env.last_audit();
    let keys = |v: &Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys(&with_invalid), keys(&clean));
    assert_eq!(with_invalid["mandate"], "aaaa1111");

    // --text.
    let out = env.run(&["--text", "list"], Some(&format!("garbage,{tw}")));
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(out.stderr.lines().count(), 1);
    assert_eq!(out.stderr, format!("warning: invalid_token: {message}\n"),);

    // With an error, the warning comes first.
    let out = env.run(
        &["list", "--account", OTHER_LOGIN],
        Some(&format!("{tw},garbage")),
    );
    assert_eq!(out.code, 1, "{out:?}");
    let lines: Vec<&str> = out.stderr.lines().collect();
    assert_eq!(lines.len(), 2, "{:?}", out.stderr);
    assert_eq!(warning_of(lines[0])["positions"], json!([2]));
    let error: Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(error["error"]["code"], "denied");
}

#[test]
fn ac5_only_invalid_or_no_tokens_are_the_existing_errors() {
    let env = Env::new();
    let tampered = tampered_token(&env);
    let unknown = unknown_token();
    let out = env.run(&["list"], Some(&format!("garbage,{unknown},{tampered}")));
    assert_eq!(out.code, 1, "{out:?}");
    assert_eq!(out.stderr.lines().count(), 1, "{:?}", out.stderr);
    assert_eq!(stderr_error(&out)["code"], "invalid_token");
    let line = env.last_audit();
    assert_eq!(line["reason"], "invalid_token");

    for tokens in [" , ,", "", ","] {
        let out = env.run(&["list"], Some(tokens));
        assert_eq!(out.code, 1, "{tokens:?}: {out:?}");
        assert_eq!(out.stderr.lines().count(), 1);
        assert_eq!(stderr_error(&out)["code"], "missing_token", "{tokens:?}");
    }
    assert_eq!(env.server.connections(), 0);
}

#[test]
fn ac5_every_agent_command_warns_and_still_succeeds() {
    let env = Env::new();
    let tw = env.add(
        "aaaa1111",
        &[LOGIN],
        Operation::ALL.iter().map(|op| grant(*op, &["*"])).collect(),
    );
    let tokens = format!("garbage,{tw}");
    let read_ref = inbox_ref(LOGIN);
    let flag_ref = format!("{LOGIN}/INBOX/{INBOX_UIDVALIDITY}/2");
    let move_ref = format!("{LOGIN}/INBOX/{INBOX_UIDVALIDITY}/3");
    for args in [
        &["search"][..],
        &["read", &read_ref],
        &["flag", &flag_ref, "--set", "flagged"],
        &["move", &move_ref, "--to", "Archive"],
        &[
            "draft",
            "--to",
            "carol@example.com",
            "--subject",
            "S",
            "--body",
            "B",
        ],
    ] {
        let out = env.run(args, Some(&tokens));
        assert_eq!(out.code, 0, "{args:?}: {out:?}");
        assert_eq!(out.stderr.lines().count(), 1, "{args:?}");
        let warning = warning_of(&out.stderr);
        assert_eq!(warning["code"], "invalid_token");
        assert_eq!(warning["positions"], json!([1]));
    }
}

#[test]
fn ac6_whoami_shows_each_presented_mandate_and_never_secrets() {
    let env = Env::new();
    let tw = env.add_named(
        "aaaa1111",
        "work-inbox",
        &[LOGIN],
        vec![grant(Operation::List, &["INBOX", "Archive"])],
    );
    let tp = env.add_named("bbbb2222", "other-archive", &[OTHER_LOGIN], Vec::new());
    let port = env.server.port;
    let tokens = format!("{tw},{tp},garbage");

    let out = env.run(&["whoami"], Some(&tokens));
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(out.stderr, "");
    assert_eq!(
        out.stdout,
        format!(
            "{{\"mandates\":[{{\"id\":\"aaaa1111\",\"name\":\"work-inbox\",\"accounts\":[{{\"login\":\"agent@example.test\",\"host\":\"localhost\",\"port\":{port},\"tls\":\"implicit\"}}],\"grants\":[{{\"operation\":\"list\",\"account\":\"agent@example.test\",\"mailboxes\":[\"INBOX\",\"Archive\"]}}]}},{{\"id\":\"bbbb2222\",\"name\":\"other-archive\",\"accounts\":[{{\"login\":\"other@example.test\",\"host\":\"localhost\",\"port\":{port},\"tls\":\"implicit\"}}],\"grants\":[]}}],\"invalid_tokens\":[3]}}\n"
        )
    );
    for stream in [&out.stdout, &out.stderr] {
        for secret in [
            CREDENTIAL.to_string(),
            tw.clone(),
            tp.clone(),
            token_hash(&tw),
            token_hash(&tp),
            "garbage".to_string(),
        ] {
            assert!(!stream.contains(&secret), "output holds a secret");
        }
    }
    assert_eq!(env.server.connections(), 0);
    assert!(env.audit().is_empty(), "whoami is not audited");

    // Every token opened.
    let out = env.run(&["whoami"], Some(&tp));
    let value: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(value["invalid_tokens"], json!([]));
    assert_eq!(value["mandates"][0]["id"], "bbbb2222");

    // Token order.
    let out = env.run(&["whoami"], Some(&format!("{tp},{tw}")));
    let value: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(value["mandates"][0]["id"], "bbbb2222");
    assert_eq!(value["mandates"][1]["id"], "aaaa1111");
}

#[test]
fn ac6_whoami_lists_ambiguous_mandates_and_handles_bad_tokens() {
    let env = Env::new();
    let tx = env.add("aaaa1111", &[LOGIN], vec![grant(Operation::List, &["*"])]);
    let ty = env.add("bbbb2222", &[LOGIN], Vec::new());
    let out = env.run(&["whoami"], Some(&format!("{tx},{ty}")));
    assert_eq!(out.code, 0, "{out:?}");
    let value: Value = serde_json::from_str(&out.stdout).unwrap();
    let ids: Vec<&str> = value["mandates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["aaaa1111", "bbbb2222"]);

    let out = env.run(&["whoami"], None);
    assert_eq!(out.code, 1);
    assert_eq!(stderr_error(&out)["code"], "missing_token");

    let unknown = unknown_token();
    let out = env.run(&["whoami"], Some(&format!("garbage, ,{unknown}")));
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(out.stderr, "");
    assert_eq!(out.stdout, "{\"mandates\":[],\"invalid_tokens\":[1,3]}\n");
    assert_eq!(env.server.connections(), 0);
}

#[test]
fn ac6_whoami_text_output() {
    let env = Env::new();
    let tw = env.add_named(
        "aaaa1111",
        "work-inbox",
        &[LOGIN],
        vec![grant(Operation::List, &["INBOX", "Archive"])],
    );
    let tp = env.add_named(
        "bbbb2222",
        "evil\n\u{1b}[2Jname",
        &[OTHER_LOGIN],
        Vec::new(),
    );
    let port = env.server.port;
    let tokens = format!("{tw},{tp},garbage");
    let expected = format!(
        "aaaa1111  work-inbox\n  account agent@example.test (localhost:{port}, implicit)\n  grant list on agent@example.test: INBOX, Archive\nbbbb2222  evil\\u{{a}}\\u{{1b}}[2Jname\n  account other@example.test (localhost:{port}, implicit)\ntoken 3 of AGENT_MAIL_TOKEN is not valid and was ignored\n"
    );
    for args in [&["--text", "whoami"][..], &["whoami", "--text"]] {
        let out = env.run(args, Some(&tokens));
        assert_eq!(out.code, 0, "{out:?}");
        assert_eq!(out.stderr, "");
        assert_eq!(out.stdout, expected);
    }
}

#[test]
fn ac7_readme_and_help_document_several_tokens_and_whoami() {
    let readme = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))
        .expect("README.md");
    for needle in [
        "whoami",
        "--account",
        "ambiguous_mandate",
        "account_required",
        "comma-separated",
    ] {
        assert!(readme.contains(needle), "README lacks {needle}");
    }
    assert!(
        !readme.contains("a Mandate covering several accounts fails with `account_required`"),
        "the Draft section still says several accounts simply fail"
    );

    let env = Env::new();
    let help = env.run(&["--help"], None);
    assert_eq!(help.code, 0);
    let commands = help
        .stdout
        .split("Commands:")
        .nth(1)
        .unwrap()
        .split("\n\n")
        .next()
        .unwrap();
    assert!(
        commands
            .lines()
            .any(|l| l.split_whitespace().next() == Some("whoami")),
        "{commands}"
    );
    assert!(help.stdout.contains("comma-separated"), "{}", help.stdout);
    for command in ["list", "search", "draft"] {
        let out = env.run(&[command, "--help"], None);
        assert_eq!(out.code, 0);
        assert!(
            out.stdout
                .lines()
                .any(|l| l.trim_start().starts_with("--account <LOGIN>")),
            "{command} --help lacks --account: {}",
            out.stdout
        );
    }
}

#[test]
fn ac1_core_refuses_unacceptable_account_values_from_other_adapters() {
    let env = Env::new();
    let tw = env.add("aaaa1111", &[LOGIN], vec![grant(Operation::List, &["*"])]);
    let mut vars = std::collections::HashMap::new();
    vars.insert(agent_mail::TOKEN_VAR.to_string(), format!("garbage,{tw}"));
    let context = agent_mail::Context::trusting_only(&env.store, vec![env.server.ca.clone()])
        .with_audit_log(agent_mail::audit::AuditLog::at(&env.log));
    for bad in ["", "a\nb", "a\rb", "a\0b"] {
        let report = agent_mail::core::execute_reporting(
            &vars,
            &agent_mail::core::Command::List {
                account: Some(bad.to_string()),
            },
            &context,
        );
        assert_eq!(
            report.result,
            Err(agent_mail::core::CoreError::AccountRequired {
                accounts: vec![LOGIN.to_string()],
            }),
            "{bad:?}"
        );
        assert_eq!(report.invalid_tokens, vec![1]);
        let line = env.last_audit();
        assert!(line.get("account").is_none(), "{line}");
    }
    assert_eq!(env.server.connections(), 0);
}
