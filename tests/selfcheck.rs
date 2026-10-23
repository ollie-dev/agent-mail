//! The hardened-runtime self-check: the in-process guard around the store
//! (AC8), debug builds skipping the check (AC9), and the opt-in release-binary
//! tests (AC1, AC2, AC4, AC6), ignored by default and run on macOS with
//! `cargo test --test selfcheck -- --ignored`.
//!
//! All values are fake. The release-binary tests present only a freshly
//! minted token that no store holds, so the one Keychain lookup they make
//! finds nothing and raises no prompt.

mod support;

use std::collections::HashMap;
use std::process::Command;

use agent_mail::mandate::{Mandate, Operation};
use agent_mail::seal::AgentToken;
use agent_mail::store::{GuardedStore, InMemoryStore, MandateStore, StoreError};
use agent_mail::terminal::{Answer, ScriptedTerminal};
use agent_mail::{Context, RunOutput, selfcheck};
use serde_json::{Value, json};
use support::*;

/// `CS_RUNTIME` plus valid and ad-hoc bits: a hardened process.
const HARDENED: u32 = 0x0001_0003;
/// Ad-hoc and linker-signed, no `CS_RUNTIME`: an unhardened process.
const UNHARDENED: u32 = 0x0002_0003;

/// Mandate W: `agent@example.test` with `list` on `INBOX`.
fn mandate_w(port: u16) -> Mandate {
    mandate_with_id(
        "77777777",
        "w",
        &[LOGIN],
        port,
        vec![grant(Operation::List, &["INBOX"])],
    )
}

/// A fake server and an in-memory store holding W, guarded with `status`.
struct Fixture {
    server: FakeImap,
    store: GuardedStore<InMemoryStore>,
    token: String,
    log: std::path::PathBuf,
}

fn fixture(status: Option<u32>) -> Fixture {
    let server = FakeImap::start(Mode::Implicit);
    let inner = InMemoryStore::new();
    let token = store_mandate(&inner, &mandate_w(server.port))
        .expose()
        .to_string();
    Fixture {
        server,
        store: GuardedStore::with_status(inner, status),
        token,
        log: temp_audit_log(),
    }
}

impl Fixture {
    fn run(&self, args: &[&str]) -> RunOutput {
        run_logged(
            args,
            Some(&self.token),
            &self.store,
            &self.server.ca,
            &self.log,
        )
    }

    fn stored(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.store.inner().entries()
    }
}

fn assert_refused(output: &RunOutput) {
    assert_eq!(output.code, 1, "{output:?}");
    assert_eq!(output.stdout, "");
    let error = error_of(output);
    assert_eq!(error["code"], "hardened_runtime_missing");
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("hardened runtime"), "{message}");
    assert!(message.contains("Keychain"), "{message}");
    assert!(message.contains("not accessed"), "{message}");
    assert!(
        message.contains("https://github.com/ollie-dev/agent-mail/releases"),
        "{message}"
    );
}

// ---------------------------------------------------------------- AC8

#[test]
fn ac8_hardened_runtime_ok_decides_on_the_runtime_bit() {
    assert!(selfcheck::hardened_runtime_ok(Some(0x0001_0003)));
    assert!(!selfcheck::hardened_runtime_ok(Some(0x0002_0003)));
    assert!(!selfcheck::hardened_runtime_ok(Some(0)));
    assert!(!selfcheck::hardened_runtime_ok(None));
}

/// A store that panics on any access: the guard must never reach it.
struct Untouchable;

impl MandateStore for Untouchable {
    fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        panic!("inner store reached through a failed guard")
    }
    fn put(&self, _: &[u8], _: &[u8]) -> Result<(), StoreError> {
        panic!("inner store reached through a failed guard")
    }
    fn delete(&self, _: &[u8]) -> Result<bool, StoreError> {
        panic!("inner store reached through a failed guard")
    }
    fn entries(&self) -> Result<Vec<agent_mail::store::Entry>, StoreError> {
        panic!("inner store reached through a failed guard")
    }
    fn check(&self) -> Result<(), StoreError> {
        panic!("inner store reached through a failed guard")
    }
}

#[test]
fn ac8_failed_guard_never_forwards_a_call() {
    for status in [Some(UNHARDENED), Some(0), None] {
        let store = GuardedStore::with_status(Untouchable, status);
        let refused = Err(StoreError::HardenedRuntimeMissing);
        assert_eq!(store.get(b"k").map(|_| ()), refused);
        assert_eq!(store.put(b"k", b"v"), refused);
        assert_eq!(store.delete(b"k").map(|_| ()), refused);
        assert_eq!(store.entries().map(|_| ()), refused);
        assert_eq!(store.check(), refused);
        assert_eq!(MandateStore::check(&store), refused);
    }
}

#[test]
fn ac8_whoami_and_list_are_refused_without_connecting_or_changing_the_store() {
    let f = fixture(Some(UNHARDENED));
    let before = f.stored();
    for args in [
        &["whoami"][..],
        &["list"],
        &["--text", "whoami"],
        &["--text", "list"],
    ] {
        let output = f.run(args);
        if args[0] == "--text" {
            assert_eq!(output.code, 1);
            assert_eq!(output.stdout, "");
            assert!(output.stderr.contains("hardened runtime"), "{output:?}");
            assert!(
                output.stderr.contains("hardened_runtime_missing"),
                "{output:?}"
            );
        } else {
            assert_refused(&output);
        }
        assert_no_secrets(&output, Some(&f.token));
    }
    assert_eq!(f.server.connections(), 0);
    assert_eq!(f.stored(), before);
}

#[test]
fn ac8_mandate_list_and_delete_are_refused_and_w_stays_stored() {
    let f = fixture(Some(UNHARDENED));
    let before = f.stored();
    assert_eq!(before.len(), 1);
    assert_refused(&f.run(&["mandate", "list"]));
    assert_refused(&f.run(&["mandate", "delete", "77777777"]));
    assert_eq!(f.stored(), before);
    // W is still there: the unguarded store opens it.
    let output = run_logged(
        &["whoami"],
        Some(&f.token),
        f.store.inner(),
        &f.server.ca,
        &f.log,
    );
    assert_eq!(output.code, 0, "{output:?}");
}

#[test]
fn ac8_mandate_create_is_refused_before_the_first_prompt() {
    let f = fixture(Some(UNHARDENED));
    let before = f.stored();
    let terminal = ScriptedTerminal::new(vec![Answer::text("never-asked")]);
    let output = run_with_terminal(
        &["mandate", "create"],
        &HashMap::new(),
        &f.store,
        &terminal,
        vec![f.server.ca.clone()],
        &[],
    );
    assert_refused(&output);
    assert!(
        terminal.transcript().is_empty(),
        "{:?}",
        terminal.transcript()
    );
    assert_eq!(terminal.remaining(), 1);
    assert_eq!(f.stored(), before);
    assert_eq!(f.server.connections(), 0);
}

#[test]
fn ac8_mcp_whoami_is_a_tool_error_with_the_same_code() {
    let f = fixture(Some(UNHARDENED));
    let mut lines = mcp_opening();
    lines.push(mcp_call(2, "whoami", json!({})));
    let out = mcp_lines(&lines, Some(&f.token), &f.store, &f.server.ca, &f.log);
    let result = &out[1]["result"];
    assert_eq!(result["isError"], json!(true));
    assert_eq!(
        result["structuredContent"]["error"]["code"],
        json!("hardened_runtime_missing")
    );
    assert!(!out[1].to_string().contains(&f.token));
}

/// A store that is always unavailable.
struct Unavailable;

impl MandateStore for Unavailable {
    fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        Err(StoreError::Unavailable)
    }
    fn put(&self, _: &[u8], _: &[u8]) -> Result<(), StoreError> {
        Err(StoreError::Unavailable)
    }
    fn delete(&self, _: &[u8]) -> Result<bool, StoreError> {
        Err(StoreError::Unavailable)
    }
    fn entries(&self) -> Result<Vec<agent_mail::store::Entry>, StoreError> {
        Err(StoreError::Unavailable)
    }
}

fn only_line(log: &std::path::Path) -> Value {
    let lines = audit_lines(log);
    assert_eq!(lines.len(), 1, "{lines:?}");
    serde_json::from_str(&lines[0]).unwrap()
}

#[test]
fn ac8_refused_list_is_audited_like_store_unavailable() {
    let f = fixture(Some(UNHARDENED));
    let output = f.run(&["list"]);
    assert_refused(&output);
    let refused = only_line(&f.log);
    assert!(!audit_lines(&f.log)[0].contains(&f.token));

    let unavailable_log = temp_audit_log();
    let output = run_logged(
        &["list"],
        Some(&f.token),
        &Unavailable,
        &f.server.ca,
        &unavailable_log,
    );
    assert_eq!(error_of(&output)["code"], "store_unavailable");
    let unavailable = only_line(&unavailable_log);

    assert_eq!(refused["reason"], "hardened_runtime_missing");
    assert_eq!(unavailable["reason"], "store_unavailable");
    assert_eq!(refused["outcome"], unavailable["outcome"]);
    assert_eq!(refused["operation"], "list");
    // Same fields apart from time and reason.
    let strip = |mut v: Value| {
        let o = v.as_object_mut().unwrap();
        o.remove("time");
        o.remove("reason");
        v
    };
    assert_eq!(strip(refused), strip(unavailable));
}

#[test]
fn ac8_passing_guard_matches_the_unwrapped_store() {
    let f = fixture(Some(HARDENED));
    for args in [&["whoami"][..], &["list"]] {
        let guarded = f.run(args);
        let plain = run_logged(
            args,
            Some(&f.token),
            f.store.inner(),
            &f.server.ca,
            &temp_audit_log(),
        );
        assert_eq!(guarded.code, 0, "{guarded:?}");
        assert_eq!(guarded.stdout, plain.stdout);
        assert_eq!(guarded.stderr, plain.stderr);
        assert_eq!(guarded.code, plain.code);
    }
    assert_eq!(f.store.check(), Ok(()));
}

#[test]
fn ac8_store_free_commands_work_on_an_unhardened_build() {
    let f = fixture(Some(UNHARDENED));
    let context = Context::trusting_only(&f.store, vec![f.server.ca.clone()]);
    let env = HashMap::new();
    let help = agent_mail::run(["agent-mail", "--help"], &env, &context);
    assert_eq!(help.code, 0);
    let version = agent_mail::run(["agent-mail", "--version"], &env, &context);
    assert_eq!(version.code, 0);
    let usage = agent_mail::run(["agent-mail", "bogus"], &env, &context);
    assert_eq!(usage.code, 2);
    let missing = agent_mail::run(["agent-mail", "whoami"], &env, &context);
    assert_eq!(error_of(&missing)["code"], "missing_token");
}

// ---------------------------------------------------------------- AC9

#[cfg(debug_assertions)]
#[test]
fn ac9_check_passes_in_debug_builds() {
    assert!(selfcheck::check());
    // The real guard therefore forwards.
    let store = GuardedStore::new(InMemoryStore::new());
    assert_eq!(store.check(), Ok(()));
    assert_eq!(store.get(b"k"), Ok(None));
}

/// `whoami` with one unknown token: the store was reached and found
/// nothing, so the token is reported invalid. (`whoami` answers exit 0 with
/// the invalid positions; Agent commands fail with `invalid_token`.)
const UNKNOWN_WHOAMI: &str = "{\"mandates\":[],\"invalid_tokens\":[1]}\n";

#[test]
fn ac9_debug_binary_reaches_the_store() {
    let unknown = AgentToken::mint();
    let log = temp_audit_log();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_agent-mail"))
            .args(args)
            .env(agent_mail::TOKEN_VAR, unknown.expose())
            .env(agent_mail::audit::TEST_LOG_VAR, &log)
            .output()
            .expect("failed to run agent-mail")
    };
    let whoami = run(&["whoami"]);
    let stderr = String::from_utf8_lossy(&whoami.stderr);
    assert!(!stderr.contains("hardened_runtime_missing"), "{stderr}");
    assert_eq!(String::from_utf8_lossy(&whoami.stdout), UNKNOWN_WHOAMI);

    let list = run(&["list"]);
    let stderr = String::from_utf8_lossy(&list.stderr);
    assert_eq!(list.status.code(), Some(1), "{stderr}");
    let error: Value = serde_json::from_str(&stderr).unwrap();
    assert_eq!(error["error"]["code"], "invalid_token", "{stderr}");
    assert!(!stderr.contains(unknown.expose()));
}

// ------------------------------------------- opt-in release binary (macOS)

#[cfg(target_os = "macos")]
mod release {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::sync::OnceLock;

    use agent_mail::seal::AgentToken;
    use serde_json::Value;

    const ROOT: &str = env!("CARGO_MANIFEST_DIR");

    /// Builds the release binary once per test run.
    fn release_binary() -> &'static Path {
        static BUILT: OnceLock<PathBuf> = OnceLock::new();
        BUILT.get_or_init(|| {
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
            let status = Command::new(cargo)
                .args(["build", "--release", "--locked"])
                .current_dir(ROOT)
                .status()
                .expect("failed to run cargo build --release");
            assert!(status.success(), "cargo build --release failed");
            Path::new(ROOT).join("target/release/agent-mail")
        })
    }

    /// A fresh temporary directory.
    fn temp_dir(label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "agent-mail-selfcheck-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A copy of the release binary in its own temporary directory.
    fn copied(label: &str) -> PathBuf {
        let path = temp_dir(label).join("agent-mail");
        std::fs::copy(release_binary(), &path).unwrap();
        path
    }

    fn sign_script() -> PathBuf {
        Path::new(ROOT).join("scripts/sign-macos.sh")
    }

    /// The copied release binary, signed with `scripts/sign-macos.sh`.
    fn hardened() -> PathBuf {
        let binary = copied("hardened");
        let status = Command::new(sign_script()).arg(&binary).status().unwrap();
        assert!(status.success(), "sign-macos.sh failed");
        binary
    }

    /// The copied release binary, signed ad hoc without the runtime option.
    fn unhardened() -> PathBuf {
        let binary = copied("unhardened");
        let status = Command::new("codesign")
            .args(["--sign", "-", "--force"])
            .arg(&binary)
            .status()
            .unwrap();
        assert!(status.success(), "codesign failed");
        binary
    }

    /// Runs `binary` with `args`, the token `token` (or none) and no other
    /// agent-mail variable.
    fn run(binary: &Path, args: &[&str], token: Option<&str>) -> Output {
        let mut command = Command::new(binary);
        command.args(args).env_remove(agent_mail::TOKEN_VAR);
        if let Some(token) = token {
            command.env(agent_mail::TOKEN_VAR, token);
        }
        command.output().expect("failed to run the release binary")
    }

    fn code_of(output: &Output) -> String {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let value: Value = serde_json::from_str(&stderr)
            .unwrap_or_else(|e| panic!("stderr is not JSON ({e}): {stderr:?}"));
        value["error"]["code"].as_str().unwrap().to_string()
    }

    fn codesign_display(binary: &Path) -> String {
        let output = Command::new("codesign")
            .args(["--display", "--verbose=2"])
            .arg(binary)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    #[test]
    #[ignore = "builds a release binary and looks up the real Keychain; run with --ignored"]
    fn ac1_hardened_release_binary_reaches_the_keychain() {
        let binary = hardened();
        let unknown = AgentToken::mint();
        let output = run(&binary, &["whoami"], Some(unknown.expose()));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.contains("hardened_runtime_missing"), "{stderr}");
        // The lookup ran and found nothing: the token is reported invalid.
        assert_eq!(output.status.code(), Some(0), "{stderr}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            super::UNKNOWN_WHOAMI
        );
        let version = run(&binary, &["--version"], None);
        assert_eq!(version.status.code(), Some(0));
    }

    #[test]
    #[ignore = "builds a release binary and runs it; run with --ignored"]
    fn ac2_unhardened_release_binary_refuses_every_keychain_access() {
        let binary = unhardened();
        let unknown = AgentToken::mint();
        // Note: the refused `list` appends one denied line to the user's
        // audit log, as any release-binary run of an Agent command does.
        for args in [
            &["whoami"][..],
            &["list"],
            &["mandate", "list"],
            &["mandate", "delete", "deadbeef"],
        ] {
            let output = run(&binary, args, Some(unknown.expose()));
            assert_eq!(output.status.code(), Some(1), "{args:?}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            let value: Value = serde_json::from_str(&stderr).unwrap();
            assert_eq!(
                value["error"]["code"], "hardened_runtime_missing",
                "{args:?}"
            );
            let message = value["error"]["message"].as_str().unwrap();
            assert!(message.contains("hardened runtime"), "{message}");
            assert!(message.contains("Keychain"), "{message}");
            assert!(!stderr.contains(unknown.expose()));
        }
        assert_eq!(run(&binary, &["--version"], None).status.code(), Some(0));
        let missing = run(&binary, &["whoami"], None);
        assert_eq!(missing.status.code(), Some(1));
        assert_eq!(code_of(&missing), "missing_token");
    }

    #[test]
    #[ignore = "builds and signs a release binary; run with --ignored"]
    fn ac4_sign_script_applies_adhoc_runtime_and_check_only_verifies() {
        let signed = hardened();
        let display = codesign_display(&signed);
        assert!(
            display.contains("flags=0x10002(adhoc,runtime)"),
            "{display}"
        );
        let check = |binary: &Path| {
            Command::new(sign_script())
                .arg("--check-only")
                .arg(binary)
                .status()
                .unwrap()
                .success()
        };
        assert!(check(&signed));
        assert!(!check(&unhardened()));
    }

    #[test]
    #[ignore = "builds and signs a release binary; run with --ignored"]
    fn ac6_release_binary_needs_no_rust_toolchain() {
        let binary = hardened();
        let otool = Command::new("otool")
            .arg("-L")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(otool.status.success());
        let listing = String::from_utf8_lossy(&otool.stdout);
        let libraries: Vec<&str> = listing
            .lines()
            .skip(1)
            .map(|line| line.trim().split(" (").next().unwrap())
            .collect();
        assert!(!libraries.is_empty());
        for library in &libraries {
            assert!(
                library.starts_with("/usr/lib/") || library.starts_with("/System/Library/"),
                "{library}"
            );
        }

        let home = temp_dir("home");
        let clean = |args: &[&str]| {
            Command::new(&binary)
                .args(args)
                .env_clear()
                .env("HOME", &home)
                .env("PATH", "/usr/bin:/bin")
                .output()
                .unwrap()
        };
        let version = clean(&["--version"]);
        assert_eq!(version.status.code(), Some(0));
        assert_eq!(
            String::from_utf8_lossy(&version.stdout),
            format!("agent-mail {}\n", env!("CARGO_PKG_VERSION"))
        );
        let missing = clean(&["whoami"]);
        assert_eq!(missing.status.code(), Some(1));
        assert_eq!(code_of(&missing), "missing_token");
    }
}
