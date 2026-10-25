//! The Broker (ADR-0004): request handling through the in-memory fake
//! transport on every OS, and on Windows the real pipe, its access list,
//! the audit folder checks and stopping. The service control manager test
//! is opt-in: `cargo test --test broker -- --ignored`, in an elevated shell.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use agent_mail::Context;
use agent_mail::audit::AuditLog;
use agent_mail::broker::{self, HIGH_INTEGRITY, SerializedStore};
use agent_mail::mandate::{Mandate, Operation};
use agent_mail::seal::{self, AgentToken};
use agent_mail::store::{InMemoryStore, MandateStore, StoreError};
use base64::Engine;
use serde_json::{Value, json};
use support::broker::{caller, hello, run, session};
use support::{
    BROKER_CREDENTIAL, CREDENTIAL, FakeImap, LOGIN, Mode, audit_lines, grant, mandate_with_id,
    run_trusting,
};

const X: &str = "S-1-5-21-1-2-3-1001";
const Y: &str = "S-1-5-21-1-2-3-1002";
const MEDIUM: u32 = 0x2000;

/// Seals `mandate` under a fresh token for `owner` and stores it.
fn store_owned(store: &InMemoryStore, mandate: &Mandate, owner: Option<&str>) -> AgentToken {
    let token = AgentToken::mint();
    let record = seal::seal_for(&token, mandate, owner);
    store.put(&record.key, &record.value).unwrap();
    token
}

/// A Mandate with ID `id` for the fake Account with `list` on INBOX.
fn listable(id: &str, port: u16) -> Mandate {
    mandate_with_id(
        id,
        &format!("mandate-{id}"),
        &[LOGIN],
        port,
        vec![grant(Operation::List, &["INBOX"])],
    )
}

/// A context trusting only `server`'s CA, logging to a fresh temporary file.
fn context_for<'a>(store: &'a dyn MandateStore, server: &FakeImap) -> Context<'a> {
    Context::trusting_only(store, vec![server.ca.clone()])
}

fn binary(args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_agent-mail"))
        .args(args)
        .output()
        .expect("failed to run agent-mail")
}

#[test]
fn ac1_service_name_is_agent_mail() {
    assert_eq!(broker::SERVICE_NAME, "agent-mail");
    assert_eq!(broker::PIPE_NAME, r"\\.\pipe\agent-mail");
}

#[test]
fn ac1_broker_is_hidden_from_help() {
    let help = binary(&["--help"]);
    assert_eq!(help.status.code(), Some(0));
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(text.contains("mandate"), "{text}");
    assert!(!text.contains("broker"), "{text}");
}

#[test]
fn ac1_broker_outside_the_service_manager_prints_one_json_error() {
    let output = binary(&["broker"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    let error: Value = serde_json::from_str(&stderr).expect("stderr is JSON");
    let expected = if cfg!(windows) {
        "service_required"
    } else {
        "unsupported"
    };
    assert_eq!(error["error"]["code"], expected, "{stderr}");
}

#[test]
fn ac2_pipe_sddl_is_exact() {
    assert_eq!(
        broker::pipe_sddl("S-1-5-80-1-2-3-4-5"),
        "O:S-1-5-80-1-2-3-4-5D:P(A;;0x12019b;;;AU)(A;;GA;;;S-1-5-80-1-2-3-4-5)"
    );
}

#[test]
fn ac8_audit_dir_sddl_is_exact() {
    assert_eq!(
        broker::audit_dir_sddl("S-1-5-80-1-2-3-4-5"),
        "O:S-1-5-80-1-2-3-4-5D:P(A;OICI;FA;;;S-1-5-80-1-2-3-4-5)(A;OICI;FR;;;BA)"
    );
}

#[test]
fn ac3_matching_version_is_ok_and_mismatch_closes() {
    let store = InMemoryStore::new();
    let context = Context::new(&store).with_audit_log(AuditLog::temporary());
    session(caller(X, MEDIUM), &context, |client| {
        let reply = client.call(&hello(env!("CARGO_PKG_VERSION")));
        assert_eq!(reply, json!({"ok": {"version": env!("CARGO_PKG_VERSION")}}));
    });
    session(caller(X, MEDIUM), &context, |client| {
        let reply = client.call(&hello("0.0.0-other"));
        assert_eq!(reply["error"]["code"], "broker_version_mismatch", "{reply}");
        assert!(client.closed());
    });
}

#[test]
fn ac3_requests_before_hello_and_malformed_frames_get_invalid_request() {
    let store = InMemoryStore::new();
    let context = Context::new(&store).with_audit_log(AuditLog::temporary());
    let invalid = |client: &mut support::broker::Client| {
        let reply = client.recv().expect("a reply");
        assert_eq!(reply["error"]["code"], "invalid_request", "{reply}");
        assert!(client.closed(), "the connection stays open");
    };
    // A request before hello.
    session(caller(X, MEDIUM), &context, |client| {
        client.send(&json!({"audit": {}}));
        invalid(client);
    });
    // A body that is not JSON.
    session(caller(X, MEDIUM), &context, |client| {
        client.send_body(b"{not json");
        invalid(client);
    });
    // An unknown request.
    session(caller(X, MEDIUM), &context, |client| {
        client.hello();
        client.send(&json!({"delete_everything": {}}));
        invalid(client);
    });
    // A frame over 1 MiB: only its header is sent.
    session(caller(X, MEDIUM), &context, |client| {
        client.send_bytes(&((broker::MAX_FRAME as u32) + 1).to_be_bytes());
        invalid(client);
    });
    // `mandate_create` inside `run`.
    session(caller(X, HIGH_INTEGRITY), &context, |client| {
        client.hello();
        client.send(&run(json!({"mandate_create": {"from": null}}), None));
        invalid(client);
    });
}

#[test]
fn ac3_run_list_equals_the_cli_output() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let token = store_owned(&store, &listable("aaaa1111", server.port), Some(X));
    let cli = run_trusting(&["list"], Some(token.expose()), &store, &server.ca);
    assert_eq!(cli.code, 0, "{}", cli.stderr);

    let context = context_for(&store, &server);
    let text = session(caller(X, MEDIUM), &context, |client| {
        client.hello();
        client.send(&run(
            json!({"list": {"account": null}}),
            Some(token.expose()),
        ));
        client.recv_text().unwrap()
    });
    let reply: broker::Reply = serde_json::from_str(&text).unwrap();
    assert_eq!(reply.result.as_ref().unwrap().get(), cli.stdout.trim_end());
    assert!(reply.error.is_none() && reply.warning.is_none());
}

#[test]
fn ac3_invalid_token_warning_is_the_cli_warning() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let token = store_owned(&store, &listable("aaaa1111", server.port), Some(X));
    let tokens = format!("not-a-token,{}", token.expose());
    let cli = run_trusting(&["list"], Some(&tokens), &store, &server.ca);
    let cli_warning: Value = serde_json::from_str(&cli.stderr).unwrap();

    let context = context_for(&store, &server);
    let reply = session(caller(X, MEDIUM), &context, |client| {
        client.hello();
        client.call(&run(json!({"list": {"account": null}}), Some(&tokens)))
    });
    assert_eq!(reply["warning"], cli_warning["warning"]);
    assert_eq!(
        reply["result"],
        serde_json::from_str::<Value>(&cli.stdout).unwrap()
    );
}

#[test]
fn ac4_another_owners_mandate_looks_like_an_unknown_token() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let token = store_owned(&store, &listable("aaaa1111", server.port), Some(X));
    let unknown = AgentToken::mint();
    let log = support::temp_audit_log();
    let context = context_for(&store, &server).with_audit_log(AuditLog::at(&log));
    let (theirs, missing) = session(caller(Y, MEDIUM), &context, |client| {
        client.hello();
        let list = json!({"list": {"account": null}});
        let theirs = client.call(&run(list.clone(), Some(token.expose())));
        let missing = client.call(&run(list, Some(unknown.expose())));
        (theirs, missing)
    });
    assert_eq!(theirs["error"]["code"], "invalid_token", "{theirs}");
    assert_eq!(theirs, missing);
    assert_eq!(server.connections(), 0);
    let lines = audit_lines(&log);
    assert_eq!(lines.len(), 2);
    for line in &lines {
        let entry: Value = serde_json::from_str(line).unwrap();
        assert_eq!(entry["outcome"], "denied");
        assert_eq!(entry["sid"], Y);
        assert!(entry.get("mandate").is_none(), "{line}");
    }
}

#[test]
fn ac4_a_mandate_without_owner_is_served_to_no_caller() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let token = store_owned(&store, &listable("aaaa1111", server.port), None);
    let context = context_for(&store, &server);
    for sid in [X, Y] {
        let (list, listing) = session(caller(sid, HIGH_INTEGRITY), &context, |client| {
            client.hello();
            let list = client.call(&run(
                json!({"list": {"account": null}}),
                Some(token.expose()),
            ));
            let listing = client.call(&run(json!("mandate_list"), None));
            (list, listing)
        });
        assert_eq!(list["error"]["code"], "invalid_token", "{list}");
        assert_eq!(
            listing["result"],
            json!({"mandates": [], "unreadable": 0}),
            "{listing}"
        );
    }
}

/// A `create` request for the fake Account on `port` with `credential`.
fn create_request(port: u16, credential: &str) -> Value {
    json!({"create": {
        "name": "broker-made",
        "accounts": [{"login": LOGIN, "host": "localhost", "port": port, "tls": "implicit", "credential": credential}],
        "grants": [{"operation": "list", "account": LOGIN, "mailboxes": ["INBOX"]}],
    }})
}

#[test]
fn ac5_create_below_high_integrity_is_refused_before_any_login() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let context = context_for(&store, &server);
    let reply = session(caller(X, MEDIUM), &context, |client| {
        client.hello();
        client.call(&create_request(server.port, CREDENTIAL))
    });
    assert_eq!(reply["error"]["code"], "elevation_required", "{reply}");
    assert!(store.entries().is_empty());
    assert_eq!(server.connections(), 0);
}

#[test]
fn ac5_create_with_a_bad_credential_fails_and_stores_nothing() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let context = context_for(&store, &server);
    let reply = session(caller(X, HIGH_INTEGRITY), &context, |client| {
        client.hello();
        client.call(&create_request(server.port, "not-the-credential"))
    });
    assert_eq!(reply["error"]["code"], "login_failed", "{reply}");
    assert_eq!(reply["error"]["account"], LOGIN, "{reply}");
    assert!(store.entries().is_empty());
}

#[test]
fn ac5_create_stores_an_owned_mandate_and_returns_its_token_once() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let context = context_for(&store, &server);
    let (created, whoami) = session(caller(X, HIGH_INTEGRITY), &context, |client| {
        client.hello();
        let created = client.call(&create_request(server.port, CREDENTIAL));
        let token = created["result"]["token"].as_str().unwrap().to_string();
        let whoami = client.call(&run(json!("whoami"), Some(&token)));
        (created, whoami)
    });
    let result = &created["result"];
    assert_eq!(result["name"], "broker-made");
    let token = AgentToken::parse(result["token"].as_str().unwrap()).unwrap();
    let entries = store.entries();
    assert_eq!(entries.len(), 1);
    let (key, value) = &entries[0];
    assert_eq!(key, &token.store_key());
    assert_eq!(seal::listing(value).unwrap().owner.as_deref(), Some(X));
    let (mandate, owner) = seal::open_with_owner(&token, value).unwrap();
    assert_eq!(owner.as_deref(), Some(X));
    assert_eq!(mandate.id, result["id"].as_str().unwrap());
    assert_eq!(whoami["result"]["mandates"][0]["id"], result["id"]);
    assert!(!whoami.to_string().contains(token.expose()));
}

#[test]
fn ac6_list_and_delete_see_only_the_callers_mandates() {
    let store = InMemoryStore::new();
    store_owned(&store, &listable("aaaa1111", 993), Some(X));
    store_owned(&store, &listable("bbbb2222", 993), Some(X));
    store_owned(&store, &listable("cccc3333", 993), Some(Y));
    let context = Context::new(&store).with_audit_log(AuditLog::temporary());
    let ids = |listing: &Value| -> Vec<String> {
        listing["result"]["mandates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].as_str().unwrap().to_string())
            .collect()
    };
    let (listing, delete_c, delete_a, after) = session(caller(X, MEDIUM), &context, |client| {
        client.hello();
        let listing = client.call(&run(json!("mandate_list"), None));
        let delete_c = client.call(&run(json!({"mandate_delete": {"id": "cccc3333"}}), None));
        let delete_a = client.call(&run(json!({"mandate_delete": {"id": "aaaa1111"}}), None));
        let after = client.call(&run(json!("mandate_list"), None));
        (listing, delete_c, delete_a, after)
    });
    assert_eq!(ids(&listing), ["aaaa1111", "bbbb2222"]);
    assert_eq!(listing["result"]["unreadable"], 0);
    assert_eq!(delete_c["error"]["code"], "not_found", "{delete_c}");
    assert_eq!(
        delete_a["result"],
        json!({"deleted": {"id": "aaaa1111", "name": "mandate-aaaa1111"}})
    );
    assert_eq!(ids(&after), ["bbbb2222"]);
    let stored: Vec<String> = store
        .entries()
        .iter()
        .map(|(_, v)| seal::listing(v).unwrap().id)
        .collect();
    assert_eq!(stored.len(), 2);
    assert!(stored.contains(&"cccc3333".to_string()));
    assert!(!stored.contains(&"aaaa1111".to_string()));
}

/// A store whose writes sleep 10 ms and record how many run at once.
#[derive(Default)]
struct SlowStore {
    inner: InMemoryStore,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
}

impl SlowStore {
    fn write<T>(&self, f: impl FnOnce() -> T) -> T {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_in_flight.fetch_max(now, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(10));
        let result = f();
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        result
    }
}

impl MandateStore for SlowStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        self.inner.get(key)
    }
    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
        self.write(|| self.inner.put(key, value))
    }
    fn delete(&self, key: &[u8]) -> Result<bool, StoreError> {
        self.write(|| self.inner.delete(key))
    }
    fn entries(&self) -> Result<Vec<(Vec<u8>, Vec<u8>)>, StoreError> {
        MandateStore::entries(&self.inner)
    }
}

#[test]
fn ac7_serialized_store_runs_one_write_at_a_time() {
    let store = SerializedStore::new(SlowStore::default());
    std::thread::scope(|scope| {
        for thread in 0..8u8 {
            let store = &store;
            scope.spawn(move || {
                for round in 0..3u8 {
                    let key = [thread, round];
                    store.put(&key, b"value").unwrap();
                    assert!(store.delete(&key).unwrap());
                }
            });
        }
    });
    assert_eq!(store.inner().max_in_flight.load(Ordering::SeqCst), 1);
    assert!(store.inner().inner.entries().is_empty());
}

#[test]
fn ac7_no_logon_session_has_its_own_error() {
    use agent_mail::core::CoreError;
    use agent_mail::store::from_win32;
    assert_eq!(from_win32(1312), StoreError::NoLogonSession);
    assert_eq!(from_win32(5), StoreError::Unavailable);
    let error = CoreError::from(StoreError::NoLogonSession);
    assert_eq!(error.code(), "store_no_logon_session");
    assert!(error.body().message.chars().count() <= 300);
}

#[test]
fn ac9_audit_returns_only_the_callers_entries_in_file_order() {
    let log = support::temp_audit_log();
    let store = InMemoryStore::new();
    let context = Context::new(&store).with_audit_log(AuditLog::at(&log));
    // No log yet.
    let empty = session(caller(X, MEDIUM), &context, |client| {
        client.hello();
        client.call(&json!({"audit": {}}))
    });
    assert_eq!(empty, json!({"result": {"entries": []}}));

    let x1 = r#"{"time":"2026-10-21T09:30:15Z","operation":"list","outcome":"denied","reason":"invalid_token","sid":"S-1-5-21-1-2-3-1001"}"#;
    let y = r#"{"time":"2026-10-21T09:30:16Z","operation":"list","outcome":"denied","reason":"invalid_token","sid":"S-1-5-21-1-2-3-1002"}"#;
    let none = r#"{"time":"2026-10-21T09:30:17Z","operation":"list","outcome":"denied","reason":"missing_token"}"#;
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(&log, format!("{x1}\n{y}\n{none}\n")).unwrap();
    let text = session(caller(X, MEDIUM), &context, |client| {
        client.hello();
        client.send(&json!({"audit": {}}));
        client.recv_text().unwrap()
    });
    assert_eq!(text, format!("{{\"result\":{{\"entries\":[{x1}]}}}}"));
}

/// Every encoding of `bytes` a reply could carry: raw, base64 (standard and
/// URL-safe, padded or not) and hex in both cases.
fn encodings(bytes: &[u8]) -> Vec<Vec<u8>> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    vec![
        bytes.to_vec(),
        STANDARD.encode(bytes).into_bytes(),
        STANDARD_NO_PAD.encode(bytes).into_bytes(),
        URL_SAFE.encode(bytes).into_bytes(),
        URL_SAFE_NO_PAD.encode(bytes).into_bytes(),
        hex.clone().into_bytes(),
        hex.to_uppercase().into_bytes(),
    ]
}

#[test]
fn ac10_no_reply_carries_a_credential_or_record_and_only_create_the_token() {
    let server = FakeImap::start(Mode::Implicit);
    let store = InMemoryStore::new();
    let log = support::temp_audit_log();
    let context = context_for(&store, &server).with_audit_log(AuditLog::at(&log));
    let (frames, record) = session(caller(X, HIGH_INTEGRITY), &context, |client| {
        client.hello();
        let created = client.call(&create_request(server.port, BROKER_CREDENTIAL));
        let token = created["result"]["token"].as_str().unwrap().to_string();
        let id = created["result"]["id"].as_str().unwrap().to_string();
        let record = store.entries()[0].1.clone();
        for command in [
            json!("whoami"),
            json!({"list": {"account": null}}),
            json!("mandate_list"),
            json!({"mandate_delete": {"id": id}}),
        ] {
            let reply = client.call(&run(command, Some(&token)));
            assert!(reply.get("result").is_some(), "{reply}");
        }
        let audit = client.call(&json!({"audit": {}}));
        assert_eq!(audit["result"]["entries"].as_array().unwrap().len(), 1);
        (client.received.clone(), record)
    });
    assert_eq!(frames.len(), 7);
    let created: Value = serde_json::from_slice(&frames[1]).unwrap();
    let token = created["result"]["token"].as_str().unwrap();
    let sealed = seal::parts(&record).unwrap().sealed.to_vec();
    let mut secrets = vec![BROKER_CREDENTIAL.as_bytes().to_vec()];
    secrets.extend(encodings(&record));
    secrets.extend(encodings(&sealed));
    for (index, frame) in frames.iter().enumerate() {
        for secret in &secrets {
            assert!(
                !support::contains_bytes(frame, secret),
                "reply {index} carries a secret"
            );
        }
        if index != 1 {
            assert!(
                !support::contains_bytes(frame, token.as_bytes()),
                "reply {index} carries the Agent Token"
            );
        }
    }
    // Every Broker audit entry carries the caller's SID.
    let lines = audit_lines(&log);
    assert_eq!(lines.len(), 1);
    for line in lines {
        let entry: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(entry["sid"], X, "{line}");
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use agent_mail::audit::{Decision, Entry};
    use agent_mail::broker::windows::{
        self as win, StopSignal, connect, create_test_pipe, create_test_pipe_for, current_user_sid,
        path_dacl, path_owner, pipe_dacl,
    };
    use agent_mail::imap::Network;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    /// A pipe name no other test or process uses.
    fn test_pipe() -> String {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        format!(
            r"\\.\pipe\agent-mail-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        )
    }

    /// Connects to `name`, retrying while the server starts.
    fn connect_when_ready(name: &str) -> std::fs::File {
        for _ in 0..200 {
            match connect(name) {
                Ok(file) => return file,
                Err(_) => std::thread::sleep(Duration::from_millis(25)),
            }
        }
        panic!("the pipe server did not start");
    }

    fn call(pipe: &mut std::fs::File, request: &Value) -> Value {
        broker::write_frame(pipe, &serde_json::to_vec(request).unwrap()).unwrap();
        let body = broker::read_frame(pipe).unwrap().expect("a reply");
        serde_json::from_slice(&body).unwrap()
    }

    /// A fresh empty directory standing in for `%ProgramData%`.
    fn program_data() -> PathBuf {
        let log = support::temp_audit_log();
        let dir = log.parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry() -> Entry {
        Entry {
            time: "2026-10-21T09:30:15Z".to_string(),
            mandate: None,
            operation: "list",
            account: None,
            mailbox: None,
            target: None,
            messages: None,
            recipients: None,
            change: None,
            results: None,
            outcome: Decision::Denied,
            reason: Some("missing_token"),
            sid: Some(X.to_string()),
        }
    }

    /// `FILE_GENERIC_READ`, `FILE_GENERIC_WRITE`, `FILE_ALL_ACCESS`.
    const FILE_GENERIC_READ: u32 = 0x0012_0089;
    const FILE_GENERIC_WRITE: u32 = 0x0012_0116;
    const FILE_ALL_ACCESS: u32 = 0x001F_01FF;
    const GENERIC_ALL: u32 = 0x1000_0000;

    #[test]
    fn ac1_stop_signal_ends_the_pipe_server_within_five_seconds() {
        let name = test_pipe();
        let store = InMemoryStore::new();
        let stop = StopSignal::new().unwrap();
        let audit = AuditLog::temporary();
        let network = Network::platform();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let (name_ref, store_ref, network_ref, audit_ref, stop_ref) =
            (&name, &store, &network, &audit, &stop);
        std::thread::scope(|scope| {
            let server = scope.spawn(move || {
                let result = win::serve_on(name_ref, store_ref, network_ref, audit_ref, stop_ref);
                let _ = done_tx.send(());
                result
            });
            // A client that stays connected and idle must not hold it up.
            let idle = connect_when_ready(&name);
            stop.signal();
            done_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("the server did not stop within 5 seconds");
            assert!(server.join().unwrap().is_ok());
            drop(idle);
        });
        let error = connect(&name).expect_err("the pipe still exists");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound, "{error:?}");
    }

    #[test]
    fn ac2_pipe_dacl_allows_authenticated_users_and_the_broker_only() {
        let name = test_pipe();
        let sid = current_user_sid().unwrap();
        let pipe = create_test_pipe(&name).unwrap();
        let dacl = pipe_dacl(&pipe).unwrap();
        assert!(dacl.protected, "{dacl:?}");
        assert_eq!(dacl.aces.len(), 2, "{dacl:?}");
        let users = &dacl.aces[0];
        assert_eq!(
            (users.kind, users.sid.as_str()),
            (win::ACCESS_ALLOWED, "S-1-5-11")
        );
        // Read and write, but never FILE_CREATE_PIPE_INSTANCE (0x4).
        assert_eq!(users.mask, broker::PIPE_CLIENT_RIGHTS, "{users:?}");
        assert_eq!(users.mask & 0x4, 0, "{users:?}");
        assert_eq!(
            users.mask,
            (FILE_GENERIC_READ | FILE_GENERIC_WRITE) & !0x4,
            "{users:?}"
        );
        let own = &dacl.aces[1];
        assert_eq!(
            (own.kind, own.sid.as_str()),
            (win::ACCESS_ALLOWED, sid.as_str())
        );
        assert!(
            [GENERIC_ALL, FILE_ALL_ACCESS].contains(&own.mask),
            "{own:?}"
        );
        // FILE_FLAG_FIRST_PIPE_INSTANCE: a second server cannot create it.
        assert!(create_test_pipe(&name).is_err());
    }

    #[test]
    fn ac2_authenticated_users_cannot_add_an_instance_of_the_pipe() {
        // With a foreign Broker SID this process is only an authenticated
        // user on the pipe, as any other local user would be.
        let name = test_pipe();
        let foreign = "S-1-5-80-1-2-3-4-5";
        let _broker = create_test_pipe_for(&name, foreign, true).unwrap();
        let error = create_test_pipe_for(&name, foreign, false)
            .expect_err("an authenticated user added a pipe instance");
        assert_eq!(error.raw_os_error(), Some(5), "{error:?}");

        // The Broker itself, with full access, can add its instances.
        let own = test_pipe();
        let sid = current_user_sid().unwrap();
        let _first = create_test_pipe_for(&own, &sid, true).unwrap();
        assert!(create_test_pipe_for(&own, &sid, false).is_ok());
    }

    #[test]
    fn ac8_concurrent_appends_and_reads_of_the_broker_log_all_succeed() {
        let base = program_data();
        let sid = current_user_sid().unwrap();
        let log = AuditLog::broker_at(&base, &sid);
        log.append(&entry()).unwrap();
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    for _ in 0..10 {
                        log.append(&entry()).unwrap();
                        assert!(log.lines().is_ok());
                    }
                });
            }
        });
        // A read and another append while an appender holds its handle.
        let path = base.join("agent-mail").join("audit.log");
        let held = win::open_audit_log(&path, &sid, true).unwrap().unwrap();
        log.append(&entry()).unwrap();
        let lines = log.lines().unwrap();
        drop(held);
        assert_eq!(lines.len(), 82);
        assert!(lines.iter().all(|l| *l == entry().to_line().trim_end()));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn ac8_a_folder_with_an_inherited_dacl_is_refused() {
        // A folder anyone else made under %ProgramData% inherits entries
        // that let users add files, whoever owns it.
        let base = program_data();
        let dir = base.join("agent-mail");
        std::fs::create_dir(&dir).unwrap();
        assert!(!path_dacl(&dir).unwrap().protected);
        let log = AuditLog::broker_at(&base, &current_user_sid().unwrap());
        assert!(log.append(&entry()).is_err());
        assert!(log.lines().is_err());
        assert!(!dir.join("audit.log").exists());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn ac8_audit_folder_gets_the_protected_dacl_and_owner() {
        let base = program_data();
        let sid = current_user_sid().unwrap();
        let log = AuditLog::broker_at(&base, &sid);
        log.append(&entry()).unwrap();
        log.append(&entry()).unwrap();
        let dir = base.join("agent-mail");
        assert_eq!(path_owner(&dir).unwrap(), sid);
        let dacl = path_dacl(&dir).unwrap();
        assert!(dacl.protected, "{dacl:?}");
        let aces: Vec<(u8, u8, u32, &str)> = dacl
            .aces
            .iter()
            .map(|a| (a.kind, a.flags, a.mask, a.sid.as_str()))
            .collect();
        assert_eq!(
            aces,
            [
                (win::ACCESS_ALLOWED, 3, FILE_ALL_ACCESS, sid.as_str()),
                (win::ACCESS_ALLOWED, 3, FILE_GENERIC_READ, "S-1-5-32-544"),
            ]
        );
        assert_eq!(
            log.lines().unwrap(),
            vec![entry().to_line().trim_end().to_string(); 2]
        );
        std::fs::remove_dir_all(&base).unwrap();
    }

    fn mklink(args: &[&str]) -> bool {
        std::process::Command::new("cmd")
            .arg("/c")
            .arg("mklink")
            .args(args)
            .output()
            .is_ok_and(|o| o.status.success())
    }

    #[test]
    fn ac8_a_junction_for_the_folder_is_refused() {
        let base = program_data();
        let elsewhere = base.join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let dir = base.join("agent-mail");
        assert!(mklink(&[
            "/J",
            dir.to_str().unwrap(),
            elsewhere.to_str().unwrap()
        ]));
        let log = AuditLog::broker_at(&base, &current_user_sid().unwrap());
        assert!(log.append(&entry()).is_err());
        assert!(log.lines().is_err());
        assert!(!elsewhere.join("audit.log").exists());
        std::fs::remove_dir(&dir).unwrap();
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn ac8_a_hard_link_or_symlink_for_the_log_is_refused() {
        let base = program_data();
        let log = AuditLog::broker_at(&base, &current_user_sid().unwrap());
        log.append(&entry()).unwrap();
        let file = base.join("agent-mail").join("audit.log");
        let victim = base.join("victim.txt");
        std::fs::write(&victim, "victim\n").unwrap();

        std::fs::remove_file(&file).unwrap();
        std::fs::hard_link(&victim, &file).unwrap();
        assert!(log.append(&entry()).is_err());
        assert!(log.lines().is_err());
        std::fs::remove_file(&file).unwrap();

        // Creating a symlink needs a privilege or Developer Mode.
        if std::os::windows::fs::symlink_file(&victim, &file).is_ok() {
            assert!(log.append(&entry()).is_err());
            assert!(log.lines().is_err());
            std::fs::remove_file(&file).unwrap();
        }
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "victim\n");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn ac8_a_folder_owned_by_another_user_is_refused() {
        let base = program_data();
        let dir = base.join("agent-mail");
        std::fs::create_dir(&dir).unwrap();
        let owner = path_owner(&dir).unwrap();
        // The test's folder is owned by this user, or by Administrators in
        // an elevated shell; for any other Broker SID it is someone else's.
        let log = AuditLog::broker_at(&base, X);
        if owner != broker::ADMINISTRATORS_SID {
            assert!(log.append(&entry()).is_err());
            assert!(!dir.join("audit.log").exists());
        }
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn ac8_broker_log_is_under_program_data() {
        // The known folder, never an environment variable.
        let source =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/audit.rs"))
                .unwrap();
        assert!(source.contains("SHGetKnownFolderPath(&FOLDERID_ProgramData"));
        assert!(!source.contains("\"ProgramData\""));
        assert!(!source.contains("\"PROGRAMDATA\""));
    }

    /// Wraps a store, checking in every call that no impersonation is
    /// active on the calling thread.
    struct RevertedStore {
        inner: InMemoryStore,
        calls: AtomicUsize,
        impersonating: Mutex<Vec<Option<u32>>>,
    }

    impl RevertedStore {
        fn check(&self) {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let error = win::thread_token_error();
            // ERROR_NO_TOKEN: the thread has no impersonation token.
            if error != Some(1008) {
                self.impersonating.lock().unwrap().push(error);
            }
        }
    }

    impl MandateStore for RevertedStore {
        fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
            self.check();
            self.inner.get(key)
        }
        fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
            self.check();
            self.inner.put(key, value)
        }
        fn delete(&self, key: &[u8]) -> Result<bool, StoreError> {
            self.check();
            self.inner.delete(key)
        }
        fn entries(&self) -> Result<Vec<(Vec<u8>, Vec<u8>)>, StoreError> {
            self.check();
            MandateStore::entries(&self.inner)
        }
    }

    #[test]
    fn ac11_real_pipe_round_trip_lists_only_the_callers_mandate() {
        let name = test_pipe();
        let sid = current_user_sid().unwrap();
        let store = RevertedStore {
            inner: InMemoryStore::new(),
            calls: AtomicUsize::new(0),
            impersonating: Mutex::new(Vec::new()),
        };
        let token = store_owned(&store.inner, &listable("aaaa1111", 993), Some(&sid));
        store_owned(&store.inner, &listable("bbbb2222", 993), Some(X));
        let stop = StopSignal::new().unwrap();
        let audit = AuditLog::temporary();
        let network = Network::platform();
        std::thread::scope(|scope| {
            let server = scope.spawn(|| win::serve_on(&name, &store, &network, &audit, &stop));
            let mut pipe = connect_when_ready(&name);
            let ok = call(&mut pipe, &hello(broker::VERSION));
            assert_eq!(ok, json!({"ok": {"version": broker::VERSION}}));
            let listing = call(&mut pipe, &run(json!("mandate_list"), None));
            let mandates = listing["result"]["mandates"].as_array().unwrap();
            assert_eq!(mandates.len(), 1, "{listing}");
            assert_eq!(mandates[0]["id"], "aaaa1111");
            let whoami = call(&mut pipe, &run(json!("whoami"), Some(token.expose())));
            assert_eq!(
                whoami["result"]["mandates"][0]["id"], "aaaa1111",
                "{whoami}"
            );
            drop(pipe);
            stop.signal();
            assert!(server.join().unwrap().is_ok());
        });
        assert!(store.calls.load(Ordering::SeqCst) >= 2);
        assert_eq!(
            *store.impersonating.lock().unwrap(),
            Vec::<Option<u32>>::new()
        );
    }

    #[test]
    fn ac11_version_mismatch_reply_reaches_the_client_before_the_close() {
        let name = test_pipe();
        let store = InMemoryStore::new();
        let stop = StopSignal::new().unwrap();
        let audit = AuditLog::temporary();
        let network = Network::platform();
        std::thread::scope(|scope| {
            let server = scope.spawn(|| win::serve_on(&name, &store, &network, &audit, &stop));
            let mut pipe = connect_when_ready(&name);
            let reply = call(&mut pipe, &hello("0.0.0-other"));
            assert_eq!(reply["error"]["code"], "broker_version_mismatch", "{reply}");
            assert!(broker::read_frame(&mut pipe).unwrap().is_none());
            stop.signal();
            assert!(server.join().unwrap().is_ok());
        });
    }

    /// Runs `sc.exe` with `args`, returning its stdout.
    fn sc(args: &[&str]) -> String {
        sc_status(args).1
    }

    /// Runs `sc.exe` with `args`, returning whether it exited with 0 and
    /// its stdout. The `[SC]` result lines are localized, so success is
    /// read from the exit code, never from the text.
    fn sc_status(args: &[&str]) -> (bool, String) {
        let output = std::process::Command::new("sc.exe")
            .args(args)
            .output()
            .expect("sc.exe runs");
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        (output.status.success(), stdout)
    }

    /// Stops and deletes the test service when dropped, so a failed
    /// assertion never leaves it registered.
    struct ServiceGuard(&'static str);

    impl Drop for ServiceGuard {
        fn drop(&mut self) {
            let _ = sc(&["stop", self.0]);
            let _ = sc(&["delete", self.0]);
        }
    }

    /// Polls `sc query` until the service is in `state`.
    fn wait_for_state(service: &str, state: &str) -> String {
        for _ in 0..100 {
            let query = sc(&["query", service]);
            if query.contains(state) {
                return query;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("{service} never reached {state}");
    }

    #[test]
    #[ignore = "needs an elevated shell; registers a real Windows service"]
    fn ac1_service_starts_and_stops_with_exit_code_zero() {
        let service = "agent-mail-broker-test";
        let bin = format!("\"{}\" broker", env!("CARGO_BIN_EXE_agent-mail"));
        let account = format!(r"NT SERVICE\{service}");
        let (created, output) = sc_status(&[
            "create", service, "binPath=", &bin, "obj=", &account, "type=", "own", "start=",
            "demand",
        ]);
        assert!(created, "{output}");
        let _guard = ServiceGuard(service);
        let started = sc(&["start", service]);
        let running = wait_for_state(service, "RUNNING");
        // The running service's pipe is owned by its service SID, which
        // is what every client checks before sending anything.
        let service_sid = broker::service_sid(service);
        let owner = (0..200).find_map(|_| match connect(broker::PIPE_NAME) {
            Ok(pipe) => Some(
                win::pipe_owner(&pipe).map(|owner| (owner, win::served_by(&pipe, &service_sid))),
            ),
            Err(_) => {
                std::thread::sleep(Duration::from_millis(25));
                None
            }
        });
        let _ = sc(&["stop", service]);
        let stopped = wait_for_state(service, "STOPPED");
        let _ = sc(&["delete", service]);
        assert!(running.contains("RUNNING"), "{started}\n{running}");
        let exit_code = stopped
            .lines()
            .find(|l| l.contains("WIN32_EXIT_CODE"))
            .unwrap_or_default();
        assert!(exit_code.contains(": 0 "), "{stopped}");
        let (owner, served) = owner
            .expect("the service's pipe accepted no client")
            .expect("the pipe owner is readable");
        assert_eq!(owner, service_sid);
        assert!(served, "served_by refused the service's pipe");
    }
}
