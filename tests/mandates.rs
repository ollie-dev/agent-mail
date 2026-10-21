//! `mandate list` and `mandate delete`, in-process with the in-memory store
//! and the fake IMAP server.

mod support;

use agent_mail::RunOutput;
use agent_mail::seal::{AgentToken, TOKEN_PREFIX};
use agent_mail::store::{InMemoryStore, MandateStore};
use serde_json::{Value, json};
use support::{
    CREDENTIAL, FakeImap, LOGIN, Mode, OTHER_LOGIN, error_of, mandate_a, mandate_b, run,
    store_mandate, token_hash,
};

/// A fake server and a store holding B, then A, with their tokens.
struct Fixture {
    server: FakeImap,
    store: InMemoryStore,
    a: AgentToken,
    b: AgentToken,
}

fn fixture() -> Fixture {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    // B first, so the listing order cannot come from insertion order.
    let b = store_mandate(&store, &mandate_b(server.port));
    let a = store_mandate(&store, &mandate_a(server.port));
    Fixture {
        server,
        store,
        a,
        b,
    }
}

fn stdout_json(output: &RunOutput) -> Value {
    assert!(output.stdout.ends_with('\n'), "stdout lacks final newline");
    assert_eq!(output.stdout.matches('\n').count(), 1, "more than one line");
    serde_json::from_str(&output.stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {:?}", output.stdout))
}

fn expected_a(port: u16) -> Value {
    json!({
        "id": "aaaa1111",
        "name": "work-inbox",
        "created": "2026-10-20T08:00:00Z",
        "accounts": [{"login": LOGIN, "host": "localhost", "port": port, "tls": "implicit"}],
        "grants": [
            {"operation": "list", "account": LOGIN, "mailboxes": ["INBOX", "Archive"]},
            {"operation": "search", "account": LOGIN, "mailboxes": ["INBOX"]}
        ]
    })
}

fn expected_b(port: u16) -> Value {
    json!({
        "id": "bbbb2222",
        "name": "other-archive",
        "created": "2026-10-21T09:30:15Z",
        "accounts": [{"login": OTHER_LOGIN, "host": "localhost", "port": port, "tls": "implicit"}],
        "grants": [
            {"operation": "list", "account": OTHER_LOGIN, "mailboxes": ["Archive"]}
        ]
    })
}

fn text_a(port: u16) -> String {
    format!(
        "aaaa1111  work-inbox  created 2026-10-20T08:00:00Z\n  account {LOGIN} (localhost:{port}, implicit)\n  grant list on {LOGIN}: INBOX, Archive\n  grant search on {LOGIN}: INBOX\n"
    )
}

fn text_b(port: u16) -> String {
    format!(
        "bbbb2222  other-archive  created 2026-10-21T09:30:15Z\n  account {OTHER_LOGIN} (localhost:{port}, implicit)\n  grant list on {OTHER_LOGIN}: Archive\n"
    )
}

fn keys(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

#[test]
fn ac1_list_shows_every_mandate_as_json_and_text() {
    let f = fixture();
    let port = f.server.port;

    let out = run(&["mandate", "list"], None, &f.store, &f.server);
    assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
    assert_eq!(out.stderr, "");
    assert_eq!(
        stdout_json(&out),
        json!({"mandates": [expected_a(port), expected_b(port)], "unreadable": 0})
    );
    assert!(
        out.stdout
            .starts_with("{\"mandates\":[{\"id\":\"aaaa1111\",\"name\":\"work-inbox\",\"created\":\"2026-10-20T08:00:00Z\",\"accounts\":[{\"login\":\"agent@example.test\",\"host\":\"localhost\",\"port\":"),
        "field order differs: {:?}",
        out.stdout
    );

    let out = run(&["--text", "mandate", "list"], None, &f.store, &f.server);
    assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
    assert_eq!(out.stderr, "");
    assert_eq!(out.stdout, format!("{}{}", text_a(port), text_b(port)));

    // Ties on `created` are broken by ID.
    let tie = InMemoryStore::new();
    let mut later = mandate_a(port);
    later.id = "aaaa0000".to_string();
    store_mandate(&tie, &mandate_a(port));
    store_mandate(&tie, &later);
    let out = run(&["mandate", "list"], None, &tie, &f.server);
    let ids: Vec<_> = stdout_json(&out)["mandates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, ["aaaa0000", "aaaa1111"]);

    assert_eq!(f.server.connections(), 0, "listing connected to the server");
}

#[test]
fn ac1_empty_store_lists_nothing() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();

    let out = run(&["mandate", "list"], None, &store, &server);
    assert_eq!(out.code, 0);
    assert_eq!(out.stderr, "");
    assert_eq!(out.stdout, "{\"mandates\":[],\"unreadable\":0}\n");

    let out = run(&["--text", "mandate", "list"], None, &store, &server);
    assert_eq!(out.code, 0);
    assert_eq!(out.stderr, "");
    assert_eq!(out.stdout, "No Mandates stored.\n");
}

#[test]
fn ac1_text_listing_escapes_control_characters() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let mut evil = mandate_a(server.port);
    evil.name = "evil\nname".to_string();
    evil.policy.grants[0].mailboxes = vec!["Spam\r\nbbbb2222  fake".into()];
    store_mandate(&store, &evil);

    let out = run(&["--text", "mandate", "list"], None, &store, &server);
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout.lines().count(), 4, "{:?}", out.stdout);
    assert!(
        out.stdout
            .starts_with("aaaa1111  evil\\u{a}name  created 2026-10-20T08:00:00Z\n"),
        "{:?}",
        out.stdout
    );
    assert!(
        out.stdout
            .contains("  grant list on agent@example.test: Spam\\u{d}\\u{a}bbbb2222  fake\n"),
        "{:?}",
        out.stdout
    );

    // JSON carries the name verbatim, escaped by JSON itself.
    let out = run(&["mandate", "list"], None, &store, &server);
    assert_eq!(stdout_json(&out)["mandates"][0]["name"], "evil\nname");
}

#[test]
fn ac2_listing_shows_no_credential_token_hash_or_store_key() {
    let f = fixture();
    let json_out = run(&["mandate", "list"], None, &f.store, &f.server);
    let text_out = run(&["--text", "mandate", "list"], None, &f.store, &f.server);
    assert_eq!(json_out.code, 0);
    assert_eq!(text_out.code, 0);

    let mut forbidden = vec![CREDENTIAL.to_string(), TOKEN_PREFIX.to_string()];
    for token in [&f.a, &f.b] {
        forbidden.push(token.expose().to_string());
        forbidden.push(token_hash(token.expose()));
    }
    for (key, _) in f.store.entries() {
        use base64::Engine;
        forbidden.push(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&key));
        forbidden.push(base64::engine::general_purpose::STANDARD.encode(&key));
    }
    for out in [&json_out, &text_out] {
        for stream in [&out.stdout, &out.stderr] {
            for needle in &forbidden {
                assert!(!stream.contains(needle.as_str()), "listing leaks a secret");
            }
        }
    }

    let value = stdout_json(&json_out);
    assert_eq!(keys(&value), ["mandates", "unreadable"]);
    for m in value["mandates"].as_array().unwrap() {
        assert_eq!(keys(m), ["accounts", "created", "grants", "id", "name"]);
        for a in m["accounts"].as_array().unwrap() {
            assert_eq!(keys(a), ["host", "login", "port", "tls"]);
        }
    }
}

#[test]
fn ac3_delete_removes_one_mandate_and_the_other_still_works() {
    for text in [false, true] {
        let f = fixture();
        let args: &[&str] = if text {
            &["--text", "mandate", "delete", "aaaa1111"]
        } else {
            &["mandate", "delete", "aaaa1111"]
        };
        // `run` uses a context with no Terminal, so any prompt would fail.
        let out = run(args, None, &f.store, &f.server);
        assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
        assert_eq!(out.stderr, "");
        if text {
            assert_eq!(
                out.stdout,
                "Deleted Mandate \"work-inbox\" (ID aaaa1111).\n"
            );
        } else {
            assert_eq!(
                out.stdout,
                "{\"deleted\":{\"id\":\"aaaa1111\",\"name\":\"work-inbox\"}}\n"
            );
        }
        assert!(!out.stdout.contains(CREDENTIAL));

        assert_eq!(f.store.entries().len(), 1);
        let listed = run(&["mandate", "list"], None, &f.store, &f.server);
        assert_eq!(
            stdout_json(&listed),
            json!({"mandates": [expected_b(f.server.port)], "unreadable": 0})
        );

        let out = run(&["list"], Some(f.b.expose()), &f.store, &f.server);
        assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
        assert_eq!(
            stdout_json(&out),
            json!({"account": OTHER_LOGIN, "mailboxes": [{"name": "Archive"}]})
        );
    }
}

#[test]
fn ac4_deleted_token_is_indistinguishable_from_a_never_issued_one() {
    let f = fixture();
    let out = run(
        &["mandate", "delete", "aaaa1111"],
        None,
        &f.store,
        &f.server,
    );
    assert_eq!(out.code, 0);
    let never = AgentToken::mint();
    let before = f.server.connections();

    for mode in [&["list"][..], &["--text", "list"][..]] {
        let deleted = run(mode, Some(f.a.expose()), &f.store, &f.server);
        let unknown = run(mode, Some(never.expose()), &f.store, &f.server);
        for out in [&deleted, &unknown] {
            assert_eq!(out.code, 1);
            assert_eq!(out.stdout, "");
        }
        assert_eq!(deleted.stderr, unknown.stderr);
        if mode.len() == 1 {
            assert_eq!(error_of(&deleted)["code"], "invalid_token");
        } else {
            assert!(deleted.stderr.starts_with("error: invalid_token: "));
        }
    }
    assert_eq!(f.server.connections(), before, "a revoked token connected");
}

#[test]
fn ac5_unknown_or_already_deleted_id_is_not_found() {
    let f = fixture();
    let before = f.store.entries();
    let message = "no Mandate has ID ffffffff; run `agent-mail mandate list` to see the IDs";

    let out = run(
        &["mandate", "delete", "ffffffff"],
        None,
        &f.store,
        &f.server,
    );
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    assert_eq!(
        out.stderr,
        format!(
            "{{\"error\":{{\"code\":\"not_found\",\"id\":\"ffffffff\",\"message\":\"{message}\"}}}}\n"
        )
    );
    let out = run(
        &["--text", "mandate", "delete", "ffffffff"],
        None,
        &f.store,
        &f.server,
    );
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr, format!("error: not_found: {message}\n"));
    assert_eq!(f.store.entries(), before, "store changed");

    // A second delete of the same ID.
    let out = run(
        &["mandate", "delete", "aaaa1111"],
        None,
        &f.store,
        &f.server,
    );
    assert_eq!(out.code, 0);
    let out = run(
        &["mandate", "delete", "aaaa1111"],
        None,
        &f.store,
        &f.server,
    );
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    assert_eq!(
        out.stderr,
        "{\"error\":{\"code\":\"not_found\",\"id\":\"aaaa1111\",\"message\":\"no Mandate has ID aaaa1111; run `agent-mail mandate list` to see the IDs\"}}\n"
    );

    // The untrusted ID is matched exactly and escaped in text output.
    let out = run(
        &["--text", "mandate", "delete", "bbbb2222\nx"],
        None,
        &f.store,
        &f.server,
    );
    assert_eq!(out.code, 1);
    assert_eq!(out.stderr.lines().count(), 1);
    assert!(out.stderr.contains("bbbb2222\\u{a}x"));
    let out = run(
        &["mandate", "delete", "BBBB2222"],
        None,
        &f.store,
        &f.server,
    );
    assert_eq!(out.code, 1);
    assert_eq!(f.store.entries().len(), 1);
}

#[test]
fn ac7_unreadable_records_are_counted_not_dropped() {
    let server = FakeImap::start(Mode::Implicit);
    for junk in [&b"garbage"[..], &version_one_record()[..]] {
        let store = InMemoryStore::new();
        store_mandate(&store, &mandate_a(server.port));
        MandateStore::put(&store, b"another-key", junk).unwrap();

        let out = run(&["mandate", "list"], None, &store, &server);
        assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
        assert_eq!(
            stdout_json(&out),
            json!({"mandates": [expected_a(server.port)], "unreadable": 1})
        );
        let out = run(&["--text", "mandate", "list"], None, &store, &server);
        assert_eq!(out.code, 0);
        assert_eq!(
            out.stdout,
            format!(
                "{}1 stored record could not be read and is not listed.\n",
                text_a(server.port)
            )
        );
    }

    let store = InMemoryStore::new();
    store_mandate(&store, &mandate_a(server.port));
    let out = run(&["--text", "mandate", "list"], None, &store, &server);
    assert!(!out.stdout.contains("could not be read"));
    let out = run(&["mandate", "list"], None, &store, &server);
    assert_eq!(stdout_json(&out)["unreadable"], 0);
}

/// A record in ticket 03's format: version byte 1, nonce, ciphertext and tag.
fn version_one_record() -> Vec<u8> {
    let mut value = vec![1u8];
    value.extend_from_slice(&[7u8; 12]);
    value.extend_from_slice(&[9u8; 120]);
    value
}
