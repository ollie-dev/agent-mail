//! `agent-mail list` driven in-process against the fake IMAP server.

mod support;

use agent_mail::RunOutput;
use agent_mail::mandate::{Operation, TlsMode};
use agent_mail::seal::AgentToken;
use agent_mail::store::InMemoryStore;
use serde_json::{Value, json};
use support::{
    FakeImap, LOGIN, Mode, OTHER_LOGIN, account, assert_no_secrets, error_of, grant, grant_on,
    mandate, mandate_for, run, run_trusting, store_mandate,
};

/// Stores one Mandate with `grants` for a fresh server in `mode`.
fn setup(
    mode: Mode,
    tls: TlsMode,
    grants: Vec<agent_mail::mandate::Grant>,
) -> (FakeImap, InMemoryStore, AgentToken) {
    let server = FakeImap::start(mode);
    let store = InMemoryStore::new();
    let token = store_mandate(&store, &mandate(server.port, tls, grants));
    (server, store, token)
}

/// Runs `list` in JSON mode and in `--text` mode, checks AC10 on both,
/// and returns the JSON-mode output.
fn list_both_modes(
    token: Option<&str>,
    store: &InMemoryStore,
    server: &FakeImap,
) -> (RunOutput, RunOutput) {
    let json = run(&["list"], token, store, server);
    let text = run(&["--text", "list"], token, store, server);
    assert_no_secrets(&json, token);
    assert_no_secrets(&text, token);
    assert_eq!(json.code, text.code, "JSON and --text exit codes differ");
    (json, text)
}

fn stdout_json(output: &RunOutput) -> Value {
    assert!(output.stdout.ends_with('\n'), "stdout lacks final newline");
    serde_json::from_str(&output.stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {:?}", output.stdout))
}

fn names(output: &RunOutput) -> Vec<String> {
    stdout_json(output)["mailboxes"]
        .as_array()
        .expect("mailboxes is an array")
        .iter()
        .map(|m| m["name"].as_str().unwrap().to_string())
        .collect()
}

fn assert_granted_inbox_and_archive(mode: Mode, tls: TlsMode) {
    let (server, store, token) = setup(
        mode,
        tls,
        vec![grant(Operation::List, &["INBOX", "Archive"])],
    );
    let (out, _) = list_both_modes(Some(token.expose()), &store, &server);

    assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
    assert_eq!(out.stderr, "");
    assert_eq!(
        stdout_json(&out),
        json!({"account": LOGIN, "mailboxes": [{"name": "INBOX"}, {"name": "Archive"}]})
    );
}

#[test]
fn ac1_list_returns_only_granted_mailboxes_implicit_tls() {
    assert_granted_inbox_and_archive(Mode::Implicit, TlsMode::Implicit);
}

#[test]
fn ac1_list_returns_only_granted_mailboxes_starttls() {
    assert_granted_inbox_and_archive(Mode::Starttls, TlsMode::Starttls);
}

#[test]
fn ac1_starttls_logs_in_only_after_the_upgrade() {
    let (server, store, token) = setup(
        Mode::Starttls,
        TlsMode::Starttls,
        vec![grant(Operation::List, &["INBOX"])],
    );
    let out = run(&["list"], Some(token.expose()), &store, &server);
    assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);

    let commands = server.commands();
    let starttls = commands.iter().position(|c| c == "STARTTLS");
    let login = commands.iter().position(|c| c == "LOGIN");
    assert!(
        matches!((starttls, login), (Some(s), Some(l)) if s < l),
        "LOGIN not after STARTTLS: {commands:?}"
    );
}

#[test]
fn ac2_text_flag_prints_human_readable_list() {
    let (server, store, token) = setup(
        Mode::Implicit,
        TlsMode::Implicit,
        vec![grant(Operation::List, &["INBOX", "Archive"])],
    );
    let (_, text) = list_both_modes(Some(token.expose()), &store, &server);

    assert_eq!(text.code, 0, "stderr: {:?}", text.stderr);
    assert_eq!(text.stdout, "agent@example.test\n  INBOX\n  Archive\n");
    assert!(serde_json::from_str::<Value>(&text.stdout).is_err());
}

fn assert_denied(grants: Vec<agent_mail::mandate::Grant>) {
    let (server, store, token) = setup(Mode::Implicit, TlsMode::Implicit, grants);
    let (out, text) = list_both_modes(Some(token.expose()), &store, &server);

    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    let error = error_of(&out);
    assert_eq!(error["code"], "denied");
    assert_eq!(error["operation"], "list");
    assert_eq!(error["account"], LOGIN);
    assert!(text.stderr.contains("denied"), "{:?}", text.stderr);
    assert!(
        !server.commands().iter().any(|c| c == "LIST"),
        "server saw LIST: {:?}",
        server.commands()
    );
}

#[test]
fn ac3_empty_policy_is_denied_before_listing() {
    assert_denied(vec![]);
}

#[test]
fn ac3_search_only_policy_is_denied_before_listing() {
    assert_denied(vec![grant(Operation::Search, &["INBOX"])]);
}

#[test]
fn ac4_wildcard_grant_returns_every_mailbox() {
    let (server, store, token) = setup(
        Mode::Implicit,
        TlsMode::Implicit,
        vec![grant(Operation::List, &["*"])],
    );
    let (out, text) = list_both_modes(Some(token.expose()), &store, &server);

    assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
    assert_eq!(
        names(&out),
        ["INBOX", "Archive", "Sent", "Spam", "[Gmail]/Entwürfe"]
    );
    assert_eq!(
        text.stdout,
        "agent@example.test\n  INBOX\n  Archive\n  Sent\n  Spam\n  [Gmail]/Entwürfe\n"
    );
}

#[test]
fn ac5_modified_utf7_names_are_decoded_and_matched() {
    let (server, store, token) = setup(
        Mode::Implicit,
        TlsMode::Implicit,
        vec![grant(Operation::List, &["[Gmail]/Entwürfe"])],
    );
    let (out, text) = list_both_modes(Some(token.expose()), &store, &server);

    assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
    assert_eq!(
        stdout_json(&out)["mailboxes"],
        json!([{"name": "[Gmail]/Entwürfe"}])
    );
    assert!(!out.stdout.contains("&APw-"));
    assert!(!text.stdout.contains("&APw-"));
}

fn assert_token_error(output: &RunOutput, code: &str) {
    assert_eq!(output.code, 1);
    assert_eq!(output.stdout, "");
    assert_eq!(
        error_of(output)["code"],
        code,
        "stderr: {:?}",
        output.stderr
    );
}

#[test]
fn ac6_missing_token_is_reported() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    for token in [None, Some("")] {
        let (out, text) = list_both_modes(token, &store, &server);
        assert_token_error(&out, "missing_token");
        assert!(text.stderr.contains("missing_token"));
    }
    assert_eq!(server.connections(), 0);
}

#[test]
fn ac6_malformed_and_unknown_tokens_are_indistinguishable() {
    let server = FakeImap::start(Mode::Implicit);
    let empty = InMemoryStore::new();
    let other = InMemoryStore::new();
    store_mandate(
        &other,
        &mandate(
            server.port,
            TlsMode::Implicit,
            vec![grant(Operation::List, &["*"])],
        ),
    );
    let unknown = AgentToken::mint();

    let (malformed, _) = list_both_modes(Some("hello"), &empty, &server);
    let (unknown_empty, _) = list_both_modes(Some(unknown.expose()), &empty, &server);
    let (unknown_other, _) = list_both_modes(Some(unknown.expose()), &other, &server);

    assert_token_error(&malformed, "invalid_token");
    assert_token_error(&unknown_empty, "invalid_token");
    assert_token_error(&unknown_other, "invalid_token");
    assert_eq!(malformed.stderr, unknown_empty.stderr);
    assert_eq!(unknown_empty.stderr, unknown_other.stderr);
    assert_eq!(server.connections(), 0);
}

#[test]
fn ac7_tampered_or_wrongly_keyed_record_fails_through_list() {
    let server = FakeImap::start(Mode::Implicit);
    let m = mandate(
        server.port,
        TlsMode::Implicit,
        vec![grant(Operation::List, &["*"])],
    );

    // Flip one ciphertext byte, then present the original token.
    let tampered = InMemoryStore::new();
    let token = store_mandate(&tampered, &m);
    let (key, mut value) = tampered.entries().remove(0);
    let last = value.len() - 1;
    value[last] ^= 0x01;
    agent_mail::store::MandateStore::put(&tampered, &key, &value).unwrap();
    let (out, _) = list_both_modes(Some(token.expose()), &tampered, &server);
    assert_token_error(&out, "invalid_token");

    // An intact record, presented with a different token.
    let intact = InMemoryStore::new();
    store_mandate(&intact, &m);
    let stranger = AgentToken::mint();
    let (out, _) = list_both_modes(Some(stranger.expose()), &intact, &server);
    assert_token_error(&out, "invalid_token");

    assert_eq!(server.connections(), 0);
}

fn assert_tls_required(tls: TlsMode) {
    let (server, store, token) = setup(
        Mode::PlaintextOnly,
        tls,
        vec![grant(Operation::List, &["*"])],
    );
    let (out, text) = list_both_modes(Some(token.expose()), &store, &server);

    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    assert_eq!(error_of(&out)["code"], "tls_required");
    assert!(text.stderr.contains("tls_required"), "{:?}", text.stderr);
    assert!(server.connections() > 0, "client never reached the server");
    let commands = server.commands();
    assert!(
        !commands.iter().any(|c| c == "LOGIN" || c == "AUTHENTICATE"),
        "credential was sent in clear: {commands:?}"
    );
}

#[test]
fn ac9_starttls_account_refuses_server_without_starttls() {
    assert_tls_required(TlsMode::Starttls);
}

#[test]
fn ac9_implicit_account_refuses_plaintext_server() {
    assert_tls_required(TlsMode::Implicit);
}

#[test]
fn ac10_untrusted_certificate_is_refused_without_leaking_secrets() {
    // Extra coverage: a TLS server whose CA is not trusted gets no login.
    let (server, store, token) = setup(
        Mode::Implicit,
        TlsMode::Implicit,
        vec![grant(Operation::List, &["*"])],
    );
    // Trust another fake server's CA while talking to `server`.
    let other = FakeImap::start(Mode::Implicit);
    let out = run_trusting(&["list"], Some(token.expose()), &store, &other.ca);
    assert_no_secrets(&out, Some(token.expose()));
    assert_eq!(out.code, 1);
    assert_eq!(error_of(&out)["code"], "tls_certificate");
    assert!(!server.commands().iter().any(|c| c == "LOGIN"));
}

#[test]
fn multi_account_mandate_requires_an_account_and_never_merges() {
    let a = FakeImap::start(Mode::Implicit);
    let b = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let token = store_mandate(
        &store,
        &mandate_for(
            vec![
                account(LOGIN, a.port, TlsMode::Implicit),
                account(OTHER_LOGIN, b.port, TlsMode::Implicit),
            ],
            vec![
                grant_on(Operation::List, LOGIN, &["INBOX"]),
                grant_on(Operation::List, OTHER_LOGIN, &["Archive"]),
            ],
        ),
    );
    let (out, text) = list_both_modes(Some(token.expose()), &store, &a);

    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    let error = error_of(&out);
    assert_eq!(error["code"], "account_required");
    assert_eq!(error["accounts"], json!([LOGIN, OTHER_LOGIN]));
    assert!(
        text.stderr.contains("account_required"),
        "{:?}",
        text.stderr
    );
    assert_eq!(a.connections() + b.connections(), 0);
}
