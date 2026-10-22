//! Connecting through a proxy (`HTTPS_PROXY`, `ALL_PROXY`), as inside the
//! Claude Code sandbox, driven in-process against in-process proxies and
//! the fake IMAP server.

mod support;

use std::collections::HashMap;
use std::path::Path;

use agent_mail::audit::AuditLog;
use agent_mail::mandate::{Operation, TlsMode};
use agent_mail::seal::AgentToken;
use agent_mail::store::InMemoryStore;
use agent_mail::{Context, RunOutput};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use support::proxy::{FakeProxy, Protocol, Seen};
use support::{
    FakeImap, LOGIN, Mode, assert_no_secrets, audit_lines, error_of, grant, mandate, store_mandate,
    temp_audit_log,
};

const PROXY_USER: &str = "proxyuser";
const PROXY_PASSWORD: &str = "pr@xy-secret";
const PROXY_PASSWORD_ENCODED: &str = "pr%40xy-secret";

/// A fake server in `mode` with one Mandate granting `list` on `INBOX`.
fn setup(mode: Mode, tls: TlsMode) -> (FakeImap, InMemoryStore, AgentToken) {
    let server = FakeImap::start(mode);
    let store = InMemoryStore::new();
    let token = store_mandate(
        &store,
        &mandate(server.port, tls, vec![grant(Operation::List, &["INBOX"])]),
    );
    (server, store, token)
}

fn env_with(token: &AgentToken, vars: &[(&str, &str)]) -> HashMap<String, String> {
    let mut env: HashMap<String, String> = vars
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    env.insert(
        agent_mail::TOKEN_VAR.to_string(),
        token.expose().to_string(),
    );
    env
}

/// Runs `agent-mail <args>` with `env`, trusting only the fake CA and
/// appending audit entries to `log`.
fn run_env(
    args: &[&str],
    env: &HashMap<String, String>,
    store: &InMemoryStore,
    server: &FakeImap,
    log: &Path,
) -> RunOutput {
    let argv = std::iter::once("agent-mail").chain(args.iter().copied());
    let context =
        Context::trusting_only(store, vec![server.ca.clone()]).with_audit_log(AuditLog::at(log));
    agent_mail::run(argv, env, &context)
}

fn stdout_json(output: &RunOutput) -> Value {
    serde_json::from_str(&output.stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {:?}", output.stdout))
}

fn inbox_only() -> Value {
    json!({"account": LOGIN, "mailboxes": [{"name": "INBOX"}]})
}

fn targets(seen: &[Seen]) -> Vec<String> {
    seen.iter().map(|s| s.target.clone()).collect()
}

/// The proxy credentials in every form they could leak in.
fn secret_forms() -> Vec<String> {
    vec![
        PROXY_USER.to_string(),
        PROXY_PASSWORD.to_string(),
        PROXY_PASSWORD_ENCODED.to_string(),
        STANDARD.encode(format!("{PROXY_USER}:{PROXY_PASSWORD}")),
    ]
}

fn assert_no_proxy_secrets(output: &RunOutput, token: &AgentToken, log: &Path) {
    assert_no_secrets(output, Some(token.expose()));
    let lines = audit_lines(log);
    assert!(!lines.is_empty(), "no audit line was written");
    for secret in secret_forms() {
        assert!(
            !output.stdout.contains(&secret),
            "stdout leaks a credential"
        );
        assert!(
            !output.stderr.contains(&secret),
            "stderr leaks a credential"
        );
        for line in &lines {
            assert!(!line.contains(&secret), "audit log leaks a credential");
        }
    }
}

fn last_audit(log: &Path) -> Value {
    let lines = audit_lines(log);
    serde_json::from_str(lines.last().expect("an audit line")).unwrap()
}

/// `list` through the CONNECT proxy, for a server in `mode`.
fn list_through_connect(mode: Mode, tls: TlsMode) {
    let (server, store, token) = setup(mode, tls);
    let proxy = FakeProxy::start(Protocol::Connect);
    let log = temp_audit_log();
    let env = env_with(&token, &[("HTTPS_PROXY", &proxy.url("http"))]);
    let out = run_env(&["list"], &env, &store, &server, &log);

    assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
    assert_eq!(out.stderr, "");
    assert_eq!(stdout_json(&out), inbox_only());
    assert_eq!(
        targets(&proxy.seen()),
        vec![format!("localhost:{}", server.port)]
    );
    assert_eq!(server.connections(), 1);
    assert_eq!(proxy.seen()[0].authorization, None);

    // The same list as a direct connection gives.
    let direct = run_env(
        &["list"],
        &env_with(&token, &[]),
        &store,
        &server,
        &temp_audit_log(),
    );
    assert_eq!(direct.stdout, out.stdout);
}

#[test]
fn ac1_list_through_connect_proxy_implicit_tls() {
    list_through_connect(Mode::Implicit, TlsMode::Implicit);
}

#[test]
fn ac1_list_through_connect_proxy_starttls() {
    list_through_connect(Mode::Starttls, TlsMode::Starttls);
}

#[test]
fn ac1_lowercase_https_proxy_is_used() {
    let (server, store, token) = setup(Mode::Implicit, TlsMode::Implicit);
    let proxy = FakeProxy::start(Protocol::Connect);
    let env = env_with(&token, &[("https_proxy", &proxy.url("http"))]);
    let out = run_env(&["list"], &env, &store, &server, &temp_audit_log());
    assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
    assert_eq!(proxy.seen().len(), 1);
}

#[test]
fn ac2_list_through_socks5_proxy() {
    for (mode, tls, scheme) in [
        (Mode::Implicit, TlsMode::Implicit, "socks5"),
        (Mode::Starttls, TlsMode::Starttls, "socks5h"),
    ] {
        let (server, store, token) = setup(mode, tls);
        let proxy = FakeProxy::start(Protocol::Socks5);
        let env = env_with(&token, &[("ALL_PROXY", &proxy.url(scheme))]);
        let out = run_env(&["list"], &env, &store, &server, &temp_audit_log());

        assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
        assert_eq!(stdout_json(&out), inbox_only());
        let seen = proxy.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].methods, vec![0x00], "no credentials: method 0 only");
        assert_eq!(seen[0].address_type, Some(0x03), "DOMAINNAME");
        assert_eq!(seen[0].target, format!("localhost:{}", server.port));
        assert_eq!(server.connections(), 1);
    }
}

#[test]
fn ac2_https_proxy_wins_over_all_proxy() {
    let (server, store, token) = setup(Mode::Implicit, TlsMode::Implicit);
    let connect = FakeProxy::start(Protocol::Connect);
    let socks = FakeProxy::start(Protocol::Socks5);
    let env = env_with(
        &token,
        &[
            ("HTTPS_PROXY", &connect.url("http")),
            ("ALL_PROXY", &socks.url("socks5")),
        ],
    );
    let out = run_env(&["list"], &env, &store, &server, &temp_audit_log());
    assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
    assert_eq!(connect.seen().len(), 1);
    assert!(socks.seen().is_empty());
}

fn with_credentials(url: &str) -> String {
    url.replacen(
        "://",
        &format!("://{PROXY_USER}:{PROXY_PASSWORD_ENCODED}@"),
        1,
    )
}

#[test]
fn ac3_connect_proxy_credentials_are_sent_and_never_shown() {
    let (server, store, token) = setup(Mode::Implicit, TlsMode::Implicit);
    let proxy = FakeProxy::start(Protocol::Connect);
    let log = temp_audit_log();
    let env = env_with(
        &token,
        &[("HTTPS_PROXY", &with_credentials(&proxy.url("http")))],
    );
    for args in [&["list"][..], &["--text", "list"][..]] {
        let out = run_env(args, &env, &store, &server, &log);
        assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
        assert_no_proxy_secrets(&out, &token, &log);
    }
    let expected = format!(
        "Basic {}",
        STANDARD.encode(format!("{PROXY_USER}:{PROXY_PASSWORD}"))
    );
    for seen in proxy.seen() {
        assert_eq!(seen.authorization.as_deref(), Some(expected.as_str()));
    }
}

#[test]
fn ac3_socks5_proxy_credentials_are_sent_and_never_shown() {
    let (server, store, token) = setup(Mode::Implicit, TlsMode::Implicit);
    let proxy = FakeProxy::start(Protocol::Socks5);
    let log = temp_audit_log();
    let env = env_with(
        &token,
        &[("ALL_PROXY", &with_credentials(&proxy.url("socks5")))],
    );
    let out = run_env(&["list"], &env, &store, &server, &log);
    assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
    assert_no_proxy_secrets(&out, &token, &log);
    let seen = proxy.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].methods, vec![0x02], "credentials: method 2 only");
    assert_eq!(
        seen[0].credentials,
        Some((PROXY_USER.to_string(), PROXY_PASSWORD.to_string()))
    );
}

#[test]
fn ac3_credentials_never_shown_when_the_proxy_refuses() {
    for (protocol, var, scheme) in [
        (Protocol::Connect, "HTTPS_PROXY", "http"),
        (Protocol::Socks5, "ALL_PROXY", "socks5"),
    ] {
        let (server, store, token) = setup(Mode::Implicit, TlsMode::Implicit);
        let proxy = FakeProxy::start(protocol);
        proxy.refuse("localhost");
        let log = temp_audit_log();
        let env = env_with(&token, &[(var, &with_credentials(&proxy.url(scheme)))]);
        for args in [&["list"][..], &["--text", "list"][..]] {
            let out = run_env(args, &env, &store, &server, &log);
            assert_eq!(out.code, 1);
            assert!(out.stderr.contains("proxy_refused"), "{:?}", out.stderr);
            assert_no_proxy_secrets(&out, &token, &log);
        }
    }
}

#[test]
fn ac3_invalid_proxy_url_names_the_variable_only() {
    let (server, store, token) = setup(Mode::Implicit, TlsMode::Implicit);
    let log = temp_audit_log();
    let value = format!("ftp://{PROXY_USER}:{PROXY_PASSWORD_ENCODED}@127.0.0.1:1");
    let env = env_with(&token, &[("HTTPS_PROXY", &value)]);
    let out = run_env(&["list"], &env, &store, &server, &log);

    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    let error = error_of(&out);
    assert_eq!(error["code"], "invalid_proxy");
    assert_eq!(error["account"], LOGIN);
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("HTTPS_PROXY"), "{message}");
    assert!(!out.stderr.contains("ftp://"), "the value is echoed");
    assert!(!out.stderr.contains("127.0.0.1"), "the value is echoed");
    assert_no_proxy_secrets(&out, &token, &log);
    assert_eq!(server.connections(), 0, "nothing is connected to");
    let audit = last_audit(&log);
    assert_eq!(audit["reason"], "invalid_proxy");

    let text = run_env(&["--text", "list"], &env, &store, &server, &log);
    assert!(
        text.stderr.starts_with("error: invalid_proxy: "),
        "{:?}",
        text.stderr
    );
    assert_no_proxy_secrets(&text, &token, &log);
    assert_eq!(server.connections(), 0);
}

#[test]
fn ac4_proxy_refusal_names_the_host() {
    for (protocol, var, scheme) in [
        (Protocol::Connect, "HTTPS_PROXY", "http"),
        (Protocol::Socks5, "ALL_PROXY", "socks5"),
    ] {
        let (server, store, token) = setup(Mode::Implicit, TlsMode::Implicit);
        let proxy = FakeProxy::start(protocol);
        proxy.refuse("localhost");
        let log = temp_audit_log();
        let env = env_with(&token, &[(var, &proxy.url(scheme))]);
        let out = run_env(&["list"], &env, &store, &server, &log);

        assert_eq!(out.code, 1, "{protocol:?}");
        assert_eq!(out.stdout, "");
        let error = error_of(&out);
        assert_eq!(error["code"], "proxy_refused");
        assert_eq!(error["account"], LOGIN);
        assert_eq!(error["host"], "localhost");
        let message = error["message"].as_str().unwrap();
        assert!(
            message.contains(&format!("localhost:{}", server.port)),
            "{message}"
        );
        assert!(message.contains("ask the human"), "{message}");
        assert!(message.contains("allowed domains"), "{message}");
        assert_eq!(server.connections(), 0, "the server saw no connection");
        assert_eq!(proxy.seen().len(), 1);

        let audit = last_audit(&log);
        assert_eq!(audit["outcome"], "allowed");
        assert_eq!(audit["reason"], "proxy_refused");
    }
}

#[test]
fn ac5_without_proxy_variables_connections_are_direct() {
    for extra in [vec![], vec![("HTTPS_PROXY", ""), ("ALL_PROXY", "")]] {
        let (server, store, token) = setup(Mode::Implicit, TlsMode::Implicit);
        let connect = FakeProxy::start(Protocol::Connect);
        let socks = FakeProxy::start(Protocol::Socks5);
        let http_proxy = connect.url("http");
        let mut vars = extra.clone();
        if !extra.is_empty() {
            vars.push(("HTTP_PROXY", &http_proxy));
            vars.push(("NO_PROXY", "localhost"));
        }
        let env = env_with(&token, &vars);
        let out = run_env(&["list"], &env, &store, &server, &temp_audit_log());

        assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
        assert_eq!(stdout_json(&out), inbox_only());
        assert_eq!(server.connections(), 1);
        assert!(connect.seen().is_empty(), "the CONNECT proxy was used");
        assert!(socks.seen().is_empty(), "the SOCKS5 proxy was used");
    }
}

#[test]
fn ac6_proxy_mode_rejects_a_certificate_not_chained_to_a_built_in_root() {
    let (server, store, token) = setup(Mode::Implicit, TlsMode::Implicit);
    let proxy = FakeProxy::start(Protocol::Connect);
    let log = temp_audit_log();
    let env = env_with(&token, &[("HTTPS_PROXY", &proxy.url("http"))]);
    // Production trust: no test roots, so proxy mode uses the built-in roots.
    let context = Context::new(&store).with_audit_log(AuditLog::at(&log));
    let out = agent_mail::run(["agent-mail", "list"], &env, &context);

    assert_eq!(out.code, 1, "stdout: {:?}", out.stdout);
    let error = error_of(&out);
    assert_eq!(error["code"], "tls_certificate");
    assert_eq!(error["account"], LOGIN);
    assert_eq!(
        targets(&proxy.seen()),
        vec![format!("localhost:{}", server.port)],
        "the tunnel was opened; verification failed"
    );
    assert_no_secrets(&out, Some(token.expose()));
}

#[test]
fn unreachable_proxy_is_connection_failed() {
    let (server, store, token) = setup(Mode::Implicit, TlsMode::Implicit);
    // Bind and drop, so nothing listens on the port.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let env = env_with(
        &token,
        &[("ALL_PROXY", &format!("socks5://127.0.0.1:{port}"))],
    );
    let out = run_env(&["list"], &env, &store, &server, &temp_audit_log());
    assert_eq!(out.code, 1);
    assert_eq!(error_of(&out)["code"], "connection_failed");
    assert_eq!(server.connections(), 0);
}

#[test]
fn malformed_connect_reply_is_connection_failed() {
    let (server, store, token) = setup(Mode::Implicit, TlsMode::Implicit);
    // A "proxy" that answers with endless junk and never ends its headers.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        use std::io::Write;
        if let Ok((mut stream, _)) = listener.accept() {
            let junk = vec![b'x'; 64 * 1024];
            let _ = stream.write_all(&junk);
        }
    });
    let env = env_with(&token, &[("HTTPS_PROXY", &format!("127.0.0.1:{port}"))]);
    let out = run_env(&["list"], &env, &store, &server, &temp_audit_log());
    assert_eq!(out.code, 1);
    assert_eq!(error_of(&out)["code"], "connection_failed");
    assert_eq!(server.connections(), 0);
}

#[test]
fn mandate_create_login_check_goes_through_the_proxy() {
    use agent_mail::create::{GENERIC, TLS_IMPLICIT};
    use agent_mail::terminal::{Answer, ScriptedTerminal};

    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let proxy = FakeProxy::start(Protocol::Connect);
    let env = HashMap::from([("HTTPS_PROXY".to_string(), proxy.url("http"))]);
    let terminal = ScriptedTerminal::new(vec![
        Answer::text("test-mandate"),
        Answer::choose(GENERIC),
        Answer::text("localhost"),
        Answer::text(&server.port.to_string()),
        Answer::choose(TLS_IMPLICIT),
        Answer::text(LOGIN),
        Answer::hidden(support::CREDENTIAL),
        Answer::tick(&["INBOX"]),
        Answer::tick(&["list"]),
        Answer::Confirm(false),
        Answer::Confirm(true),
        Answer::Confirm(false),
    ]);
    let out = support::run_with_terminal(
        &["mandate", "create"],
        &env,
        &store,
        &terminal,
        vec![server.ca.clone()],
        &[],
    );
    assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
    assert_eq!(terminal.remaining(), 0, "script not used up");
    assert_eq!(
        targets(&proxy.seen()),
        vec![format!("localhost:{}", server.port)]
    );
}
