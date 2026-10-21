//! Shared test support: an in-process fake IMAP server and helpers that drive
//! the CLI adapter in-process. All values here are fake.
#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use agent_mail::mandate::{Account, Credential, Grant, Mandate, Operation, Policy, TlsMode};
use agent_mail::seal::{self, AgentToken};
use agent_mail::store::{InMemoryStore, MandateStore};
use agent_mail::{Context, RunOutput};
use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

/// Fake Account login.
pub const LOGIN: &str = "agent@example.test";
/// Second fake Account login.
pub const OTHER_LOGIN: &str = "other@example.test";
/// Logins the fake server knows.
pub const KNOWN_LOGINS: [&str; 2] = [LOGIN, OTHER_LOGIN];
/// Fake Credential.
pub const CREDENTIAL: &str = "fake-app-password-123";
/// Mailboxes the fake server lists, raw, in this order.
pub const SERVER_MAILBOXES: [&str; 5] =
    ["INBOX", "Archive", "Sent", "Spam", "[Gmail]/Entw&APw-rfe"];

/// How the fake server secures connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// TLS from the first byte.
    Implicit,
    /// Plaintext greeting, advertises and accepts STARTTLS.
    Starttls,
    /// Plaintext only, never advertises STARTTLS, accepts LOGIN in clear.
    PlaintextOnly,
}

/// An in-process fake IMAP server on a real TCP port.
pub struct FakeImap {
    pub port: u16,
    pub ca: CertificateDer<'static>,
    commands: Arc<Mutex<Vec<String>>>,
    connections: Arc<AtomicUsize>,
}

impl FakeImap {
    /// Starts a server in `mode` on an ephemeral localhost port.
    pub fn start(mode: Mode) -> Self {
        let (ca, tls) = tls_config();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake server");
        let port = listener.local_addr().unwrap().port();
        let commands = Arc::new(Mutex::new(Vec::new()));
        let connections = Arc::new(AtomicUsize::new(0));
        let (c, n) = (commands.clone(), connections.clone());
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                n.fetch_add(1, Ordering::SeqCst);
                let (c, tls) = (c.clone(), tls.clone());
                thread::spawn(move || serve(stream, mode, tls, c));
            }
        });
        Self {
            port,
            ca,
            commands,
            connections,
        }
    }

    /// Command verbs received, upper-cased, in order. Arguments are dropped
    /// so the Credential never lands in test output.
    pub fn commands(&self) -> Vec<String> {
        self.commands.lock().unwrap().clone()
    }

    /// Number of TCP connections accepted.
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }
}

fn tls_config() -> (CertificateDer<'static>, Arc<ServerConfig>) {
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca_key = KeyPair::generate().unwrap();
    let ca_cert = ca_params.self_signed(&ca_key).unwrap();
    let issuer = Issuer::new(ca_params, ca_key);

    let leaf_key = KeyPair::generate().unwrap();
    let leaf = CertificateParams::new(vec!["localhost".to_string(), "imap.gmail.com".to_string()])
        .unwrap()
        .signed_by(&leaf_key, &issuer)
        .unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![leaf.der().clone()],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der())),
        )
        .unwrap();
    (ca_cert.der().clone(), Arc::new(config))
}

fn serve(
    mut stream: TcpStream,
    mode: Mode,
    tls: Arc<ServerConfig>,
    commands: Arc<Mutex<Vec<String>>>,
) {
    match mode {
        Mode::Implicit => {
            let conn = ServerConnection::new(tls).unwrap();
            let mut tls_stream = StreamOwned::new(conn, stream);
            let _ = session(&mut tls_stream, mode, true, &commands, true);
        }
        Mode::Starttls | Mode::PlaintextOnly => {
            if session(&mut stream, mode, false, &commands, true) == Some(Upgrade) {
                let conn = ServerConnection::new(tls).unwrap();
                let mut tls_stream = StreamOwned::new(conn, stream);
                let _ = session(&mut tls_stream, mode, true, &commands, false);
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Upgrade;

fn send(stream: &mut impl Write, text: &str) -> Option<()> {
    stream.write_all(text.as_bytes()).ok()?;
    stream.flush().ok()
}

fn read_line(stream: &mut impl Read) -> Option<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(1) => {
                if byte[0] == b'\n' {
                    break;
                }
                line.push(byte[0]);
            }
            _ => return None,
        }
    }
    Some(String::from_utf8_lossy(&line).trim_end().to_string())
}

fn session(
    stream: &mut (impl Read + Write),
    mode: Mode,
    secure: bool,
    commands: &Mutex<Vec<String>>,
    greet: bool,
) -> Option<Upgrade> {
    if greet {
        send(stream, "* OK fake IMAP ready\r\n")?;
    }
    loop {
        let line = read_line(stream)?;
        let mut parts = line.splitn(3, ' ');
        let tag = parts.next().unwrap_or("").to_string();
        let verb = parts.next().unwrap_or("").to_ascii_uppercase();
        let args = parts.next().unwrap_or("");
        commands.lock().unwrap().push(verb.clone());
        match verb.as_str() {
            "CAPABILITY" => {
                let caps = if !secure && mode == Mode::Starttls {
                    "IMAP4rev1 STARTTLS LOGINDISABLED"
                } else {
                    "IMAP4rev1 AUTH=PLAIN"
                };
                send(
                    stream,
                    &format!("* CAPABILITY {caps}\r\n{tag} OK CAPABILITY done\r\n"),
                )?;
            }
            "STARTTLS" if !secure && mode == Mode::Starttls => {
                send(stream, &format!("{tag} OK begin TLS now\r\n"))?;
                return Some(Upgrade);
            }
            "LOGIN" if login_accepted(args) => {
                send(stream, &format!("{tag} OK logged in\r\n"))?;
            }
            "LOGIN" | "AUTHENTICATE" => {
                send(stream, &format!("{tag} NO login refused\r\n"))?;
            }
            "LIST" => {
                let mut reply = String::new();
                for name in SERVER_MAILBOXES {
                    reply.push_str(&format!("* LIST (\\HasNoChildren) \"/\" \"{name}\"\r\n"));
                }
                reply.push_str(&format!("{tag} OK LIST done\r\n"));
                send(stream, &reply)?;
            }
            "LOGOUT" => {
                send(stream, &format!("* BYE bye\r\n{tag} OK LOGOUT done\r\n"))?;
                return None;
            }
            _ => {
                send(stream, &format!("{tag} BAD unknown command\r\n"))?;
            }
        }
    }
}

/// Whether `LOGIN` arguments name a known login with the fake Credential.
fn login_accepted(args: &str) -> bool {
    let fields = imap_strings(args);
    matches!(fields.as_slice(), [login, credential]
        if KNOWN_LOGINS.contains(&login.as_str()) && credential == CREDENTIAL)
}

/// Splits IMAP atoms and quoted strings (with `\` escapes).
fn imap_strings(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = args.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c == ' ' {
            chars.next();
        } else if c == '"' {
            chars.next();
            let mut field = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => field.extend(chars.next()),
                    '"' => break,
                    c => field.push(c),
                }
            }
            out.push(field);
        } else {
            let mut field = String::new();
            while let Some(&c) = chars.peek() {
                if c == ' ' {
                    break;
                }
                field.push(c);
                chars.next();
            }
            out.push(field);
        }
    }
    out
}

/// The fake Account `login` on `localhost:port`.
pub fn account(login: &str, port: u16, tls: TlsMode) -> Account {
    Account {
        login: login.to_string(),
        host: "localhost".to_string(),
        port,
        tls,
        credential: Credential::new(CREDENTIAL),
    }
}

/// A Mandate covering `accounts` with the given Grants.
pub fn mandate_for(accounts: Vec<Account>, grants: Vec<Grant>) -> Mandate {
    Mandate {
        name: "test-mandate".to_string(),
        id: "0123abcd".to_string(),
        created: 1_700_000_000,
        accounts,
        policy: Policy { grants },
    }
}

/// A Mandate for the fake Account on `port` with the given Grants.
pub fn mandate(port: u16, tls: TlsMode, grants: Vec<Grant>) -> Mandate {
    mandate_for(vec![account(LOGIN, port, tls)], grants)
}

/// A Grant of `operation` on `mailboxes` of the fake Account.
pub fn grant(operation: Operation, mailboxes: &[&str]) -> Grant {
    grant_on(operation, LOGIN, mailboxes)
}

/// A Grant of `operation` on `mailboxes` of the Account `login`.
pub fn grant_on(operation: Operation, login: &str, mailboxes: &[&str]) -> Grant {
    Grant {
        operation,
        account: login.to_string(),
        mailboxes: mailboxes.iter().map(|m| (*m).into()).collect(),
    }
}

/// Seals `mandate` under a fresh token and stores it in `store`.
pub fn store_mandate(store: &InMemoryStore, mandate: &Mandate) -> AgentToken {
    let token = AgentToken::mint();
    let record = seal::seal(&token, mandate);
    store.put(&record.key, &record.value).unwrap();
    token
}

/// Runs `agent-mail <args>` in-process with `token` in `AGENT_MAIL_TOKEN`,
/// trusting only the fake server's CA.
pub fn run(
    args: &[&str],
    token: Option<&str>,
    store: &InMemoryStore,
    server: &FakeImap,
) -> RunOutput {
    run_trusting(args, token, store, &server.ca)
}

/// Like [`run`], trusting only the CA `ca`.
pub fn run_trusting(
    args: &[&str],
    token: Option<&str>,
    store: &InMemoryStore,
    ca: &CertificateDer<'static>,
) -> RunOutput {
    let mut env = HashMap::new();
    if let Some(token) = token {
        env.insert(agent_mail::TOKEN_VAR.to_string(), token.to_string());
    }
    let argv = std::iter::once("agent-mail").chain(args.iter().copied());
    let context = Context::trusting_only(store, vec![ca.clone()]);
    agent_mail::run(argv, &env, &context)
}

/// Asserts neither the token nor the Credential appears in the output (AC10).
pub fn assert_no_secrets(output: &RunOutput, token: Option<&str>) {
    for stream in [&output.stdout, &output.stderr] {
        assert!(
            !stream.contains(CREDENTIAL),
            "output contains the Credential"
        );
        if let Some(token) = token.filter(|t| !t.is_empty()) {
            assert!(!stream.contains(token), "output contains the agent token");
        }
    }
}

/// Parses stderr as the structured error object and returns `error`.
pub fn error_of(output: &RunOutput) -> serde_json::Value {
    let value: serde_json::Value = serde_json::from_str(&output.stderr)
        .unwrap_or_else(|e| panic!("stderr is not JSON ({e}): {:?}", output.stderr));
    value["error"].clone()
}

/// Runs `agent-mail <args>` in-process with `env`, talking through
/// `terminal`, trusting only `roots` and connecting `routes`
/// (`host`, `port`, fake server port on localhost).
pub fn run_with_terminal(
    args: &[&str],
    env: &HashMap<String, String>,
    store: &dyn MandateStore,
    terminal: &dyn agent_mail::terminal::Terminal,
    roots: Vec<CertificateDer<'static>>,
    routes: &[(&str, u16, u16)],
) -> RunOutput {
    let argv = std::iter::once("agent-mail").chain(args.iter().copied());
    let mut context = Context::trusting_only(store, roots).with_terminal(terminal);
    for (host, port, to_port) in routes {
        context = context.with_route(host, *port, "localhost", *to_port);
    }
    agent_mail::run(argv, env, &context)
}

/// Every Agent Token-shaped string (`amt_` plus 43 base64url characters)
/// in `text`.
pub fn tokens_in(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(seal::TOKEN_PREFIX) {
        let body: String = rest[at + seal::TOKEN_PREFIX.len()..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if body.len() == 43 {
            found.push(format!("{}{body}", seal::TOKEN_PREFIX));
        }
        rest = &rest[at + seal::TOKEN_PREFIX.len()..];
    }
    found
}

/// Whether `needle` occurs in `haystack`.
pub fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// A store that counts every access, wrapping an [`InMemoryStore`].
#[derive(Default)]
pub struct CountingStore {
    pub inner: InMemoryStore,
    accesses: AtomicUsize,
}

impl CountingStore {
    /// Number of `get`, `put` and `delete` calls so far.
    pub fn accesses(&self) -> usize {
        self.accesses.load(Ordering::SeqCst)
    }
}

impl MandateStore for CountingStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, agent_mail::store::StoreError> {
        self.accesses.fetch_add(1, Ordering::SeqCst);
        self.inner.get(key)
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), agent_mail::store::StoreError> {
        self.accesses.fetch_add(1, Ordering::SeqCst);
        self.inner.put(key, value)
    }

    fn delete(&self, key: &[u8]) -> Result<bool, agent_mail::store::StoreError> {
        self.accesses.fetch_add(1, Ordering::SeqCst);
        self.inner.delete(key)
    }

    fn entries(&self) -> Result<Vec<(Vec<u8>, Vec<u8>)>, agent_mail::store::StoreError> {
        self.accesses.fetch_add(1, Ordering::SeqCst);
        MandateStore::entries(&self.inner)
    }
}

/// Mandate A of the list and delete fixtures: `work-inbox`, `aaaa1111`,
/// created 2026-10-20T08:00:00Z, the fake Account on `port`.
pub fn mandate_a(port: u16) -> Mandate {
    Mandate {
        name: "work-inbox".to_string(),
        id: "aaaa1111".to_string(),
        created: 1_792_483_200,
        accounts: vec![account(LOGIN, port, TlsMode::Implicit)],
        policy: Policy {
            grants: vec![
                grant(Operation::List, &["INBOX", "Archive"]),
                grant(Operation::Search, &["INBOX"]),
            ],
        },
    }
}

/// Mandate B of the list and delete fixtures: `other-archive`, `bbbb2222`,
/// created 2026-10-21T09:30:15Z, the second fake Account on `port`.
pub fn mandate_b(port: u16) -> Mandate {
    Mandate {
        name: "other-archive".to_string(),
        id: "bbbb2222".to_string(),
        created: 1_792_575_015,
        accounts: vec![account(OTHER_LOGIN, port, TlsMode::Implicit)],
        policy: Policy {
            grants: vec![grant_on(Operation::List, OTHER_LOGIN, &["Archive"])],
        },
    }
}

/// The token hash as the record format defines it: base64url without
/// padding of SHA-256 over the token's 32 secret bytes.
pub fn token_hash(token: &str) -> String {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let body = token
        .strip_prefix(seal::TOKEN_PREFIX)
        .expect("token prefix");
    let secret = URL_SAFE_NO_PAD.decode(body).expect("token body");
    URL_SAFE_NO_PAD.encode(ring::digest::digest(&ring::digest::SHA256, &secret))
}

/// A fresh audit log path in a new temporary directory, never the user's
/// data directory. Nothing exists there yet.
pub fn temp_audit_log() -> std::path::PathBuf {
    agent_mail::audit::AuditLog::temporary()
        .path()
        .expect("temporary logs have a path")
        .to_path_buf()
}

/// Like [`run_trusting`], appending audit entries to `log`.
pub fn run_logged(
    args: &[&str],
    token: Option<&str>,
    store: &dyn MandateStore,
    ca: &CertificateDer<'static>,
    log: &std::path::Path,
) -> RunOutput {
    let mut env = HashMap::new();
    if let Some(token) = token {
        env.insert(agent_mail::TOKEN_VAR.to_string(), token.to_string());
    }
    let argv = std::iter::once("agent-mail").chain(args.iter().copied());
    let context = Context::trusting_only(store, vec![ca.clone()])
        .with_audit_log(agent_mail::audit::AuditLog::at(log));
    agent_mail::run(argv, &env, &context)
}

/// The audit log's lines, without their `\n`; empty if there is no log.
/// Panics if the file does not end in `\n`.
pub fn audit_lines(log: &std::path::Path) -> Vec<String> {
    let text = match std::fs::read_to_string(log) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => panic!("cannot read the audit log: {e}"),
    };
    assert!(
        text.is_empty() || text.ends_with('\n'),
        "audit log does not end in a newline"
    );
    text.lines().map(str::to_string).collect()
}
