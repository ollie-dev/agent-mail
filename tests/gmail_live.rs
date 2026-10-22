//! Opt-in test against a real, dedicated Gmail test account. Ignored by
//! default; run with `cargo test --test gmail_live -- --ignored` after
//! exporting `AGENT_MAIL_LIVE_GMAIL_ADDRESS` and
//! `AGENT_MAIL_LIVE_GMAIL_APP_PASSWORD` (ticket 12).
//!
//! It only reads: it creates no draft, sets no flag and moves no message.
//! The app password lives only in a `Credential` in an in-memory store. It
//! is checked not to appear in any output or in the temporary audit log,
//! and no message in this file formats it, the Mandate, the Account or a
//! `RunOutput`: failures name the step and an error code only.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use agent_mail::audit::AuditLog;
use agent_mail::imap::{self, Network};
use agent_mail::mandate::{Account, Credential, Grant, Mandate, Operation, Policy, TlsMode};
use agent_mail::seal::{self, AgentToken};
use agent_mail::search::MessageRef;
use agent_mail::store::{InMemoryStore, MandateStore};
use agent_mail::{Context, RunOutput};
use serde_json::Value;
use zeroize::Zeroizing;

/// The test account's address, the login.
const ADDRESS_VAR: &str = "AGENT_MAIL_LIVE_GMAIL_ADDRESS";
/// The test account's app password, the Credential.
const PASSWORD_VAR: &str = "AGENT_MAIL_LIVE_GMAIL_APP_PASSWORD";

/// What the live test needs from the environment. `Debug` never shows the
/// password: [`Credential`] redacts it.
#[derive(Debug)]
struct LiveConfig {
    login: String,
    password: Credential,
}

/// Reads the live test's settings through `lookup` (the environment, or a
/// fake in tests). An unset or empty variable is an `Err` naming it; the
/// message never holds a variable's value.
fn live_config(lookup: impl Fn(&str) -> Option<String>) -> Result<LiveConfig, String> {
    let login = lookup(ADDRESS_VAR).filter(|v| !v.is_empty());
    let password = lookup(PASSWORD_VAR)
        .filter(|v| !v.is_empty())
        .map(Credential::new);
    match (login, password) {
        (Some(login), Some(password)) => Ok(LiveConfig { login, password }),
        (None, None) => Err(format!(
            "gmail_live skipped: set {ADDRESS_VAR} and {PASSWORD_VAR} to run it"
        )),
        (None, Some(_)) => Err(format!(
            "gmail_live skipped: {ADDRESS_VAR} is unset or empty; set it to the test account's address to run it"
        )),
        (Some(_), None) => Err(format!(
            "gmail_live skipped: {PASSWORD_VAR} is unset or empty; set it to the test account's app password to run it"
        )),
    }
}

/// The live session: an in-memory store holding the Mandate, its token and
/// the temporary audit log.
struct Live {
    store: InMemoryStore,
    token: AgentToken,
    account: Account,
    /// The app password in plain text, only to check it is never shown;
    /// wiped on drop.
    secret: Zeroizing<String>,
    log: PathBuf,
}

impl Live {
    fn new(config: LiveConfig, secret: Zeroizing<String>) -> Self {
        let account = Account {
            login: config.login.clone(),
            host: "imap.gmail.com".to_string(),
            port: 993,
            tls: TlsMode::Implicit,
            credential: config.password,
        };
        let grant = |operation| Grant {
            operation,
            account: config.login.clone(),
            mailboxes: vec!["INBOX".into()],
        };
        let mandate = Mandate {
            name: "gmail-live-test".to_string(),
            id: "1a1a1a1a".to_string(),
            created: 1_792_483_200,
            accounts: vec![account.clone()],
            policy: Policy {
                grants: vec![
                    grant(Operation::List),
                    grant(Operation::Search),
                    grant(Operation::Read),
                ],
            },
        };
        let store = InMemoryStore::new();
        let token = AgentToken::mint();
        let record = seal::seal(&token, &mandate);
        store
            .put(&record.key, &record.value)
            .unwrap_or_else(|_| panic!("setup: the in-memory store refused the Mandate"));
        let log = AuditLog::temporary()
            .path()
            .unwrap_or_else(|| panic!("setup: the temporary audit log has no path"))
            .to_path_buf();
        Self {
            store,
            token,
            account,
            secret,
            log,
        }
    }

    /// The app password, for the checks that it is never shown.
    fn password(&self) -> &str {
        &self.secret
    }

    /// Runs `agent-mail <args>` in-process with the platform certificate
    /// verifier, checks that no secret was printed, and returns the run.
    fn run(&self, step: &str, args: &[&str]) -> RunOutput {
        let env = HashMap::from([(
            agent_mail::TOKEN_VAR.to_string(),
            self.token.expose().to_string(),
        )]);
        let context = Context::new(&self.store).with_audit_log(AuditLog::at(&self.log));
        let argv = std::iter::once("agent-mail").chain(args.iter().copied());
        let out = agent_mail::run(argv, &env, &context);
        // Before any assertion that could show the output.
        for stream in [&out.stdout, &out.stderr] {
            assert!(
                !stream.contains(self.password()),
                "step {step}: the output contains the app password"
            );
            assert!(
                !stream.contains(self.token.expose()),
                "step {step}: the output contains the agent token"
            );
        }
        out
    }

    /// Runs `args`, expects exit code 0, and returns stdout as JSON. A
    /// failure names the step, the exit code and the error code only.
    fn json(&self, step: &str, args: &[&str]) -> Value {
        let out = self.run(step, args);
        assert_eq!(
            out.code,
            0,
            "step {step}: exit code {}, error code {}",
            out.code,
            error_code(&out)
        );
        serde_json::from_str(&out.stdout)
            .unwrap_or_else(|_| panic!("step {step}: stdout is not JSON"))
    }
}

/// The `error.code` of a failed run's stderr, or `none`.
fn error_code(out: &RunOutput) -> String {
    serde_json::from_str::<Value>(&out.stderr)
        .ok()
        .and_then(|v| v["error"]["code"].as_str().map(str::to_string))
        .unwrap_or_else(|| "none".to_string())
}

/// The `ref` of every message in a search result, each checked to parse and
/// to name `INBOX`.
fn inbox_refs(step: &str, result: &Value) -> Vec<String> {
    let messages = result["messages"]
        .as_array()
        .unwrap_or_else(|| panic!("step {step}: `messages` is not an array"));
    messages
        .iter()
        .map(|m| {
            let text = m["ref"]
                .as_str()
                .unwrap_or_else(|| panic!("step {step}: a `ref` is not a string"));
            let reference = MessageRef::parse(text)
                .unwrap_or_else(|_| panic!("step {step}: a `ref` does not parse"));
            assert_eq!(
                reference.mailbox, "INBOX",
                "step {step}: a `ref` does not name INBOX"
            );
            text.to_string()
        })
        .collect()
}

fn total(step: &str, result: &Value) -> u64 {
    result["total"]
        .as_u64()
        .unwrap_or_else(|| panic!("step {step}: `total` is not a number"))
}

#[test]
#[ignore = "talks to a real Gmail account; run with `cargo test --test gmail_live -- --ignored`"]
fn gmail_live_round_trip() {
    let config = match live_config(|name| std::env::var(name).ok()) {
        Ok(config) => config,
        Err(message) => {
            // Written to stderr directly, not through `eprintln!`, which the
            // test harness captures and hides for a passing test.
            let _ = writeln!(std::io::stderr(), "{message}");
            return;
        }
    };
    // `Credential` keeps its secret to the crate, so the checks below hold
    // their own wiped-on-drop copy.
    let secret = Zeroizing::new(std::env::var(PASSWORD_VAR).unwrap_or_default());
    assert!(
        !secret.is_empty(),
        "setup: {PASSWORD_VAR} changed while the test ran"
    );
    let live = Live::new(config, secret);

    // (a) LIST names INBOX.
    let listed = live.json("(a) list", &["list"]);
    let names: Vec<&str> = listed["mailboxes"]
        .as_array()
        .unwrap_or_else(|| panic!("step (a) list: `mailboxes` is not an array"))
        .iter()
        .filter_map(|m| m["name"].as_str())
        .collect();
    assert!(
        names.contains(&"INBOX"),
        "step (a) list: INBOX is not listed"
    );

    // (b) A standard search.
    let step = "(b) standard search";
    let standard = live.json(step, &["search", "--mailbox", "INBOX", "--limit", "5"]);
    assert!(
        total(step, &standard) >= 1,
        "step {step}: precondition: INBOX of the live Gmail account must not be empty"
    );
    inbox_refs(step, &standard);

    // (c) A Gmail search; also shows Gmail advertises X-GM-EXT-1 after login.
    let step = "(c) gmail search";
    let gmail = live.json(
        step,
        &[
            "search",
            "--mailbox",
            "INBOX",
            "--gmail",
            "in:inbox",
            "--limit",
            "5",
        ],
    );
    assert!(
        total(step, &gmail) >= 1,
        "step {step}: `in:inbox` matched nothing"
    );
    inbox_refs(step, &gmail);

    // (d) The Drafts mailbox is found by its \Drafts attribute.
    let step = "(d) drafts detection";
    let drafts = imap::drafts_mailbox(&live.account, &Network::platform())
        .unwrap_or_else(|e| panic!("step {step}: failed with {e:?}"));
    assert!(
        !drafts.is_empty()
            && (drafts.starts_with("[Gmail]/") || drafts.starts_with("[Google Mail]/")),
        "step {step}: the Drafts mailbox is not under [Gmail]/ or [Google Mail]/"
    );

    // (e) Reading peeks: the message stays unread.
    let step = "(e) unseen search";
    let unseen = live.json(
        step,
        &["search", "--mailbox", "INBOX", "--unseen", "--limit", "1"],
    );
    let refs = inbox_refs(step, &unseen);
    let Some(reference) = refs.first().cloned() else {
        panic!(
            "step {step}: precondition: INBOX of the live Gmail account needs at least one unread message"
        );
    };
    let step = "(e) read";
    let read = live.json(step, &["read", &reference]);
    assert_eq!(
        read["ref"].as_str(),
        Some(reference.as_str()),
        "step {step}: the result's `ref` is not the one asked for"
    );
    let step = "(e) unseen after read";
    let after = live.json(step, &["search", "--mailbox", "INBOX", "--unseen"]);
    let message = after["messages"]
        .as_array()
        .and_then(|messages| {
            messages
                .iter()
                .find(|m| m["ref"].as_str() == Some(reference.as_str()))
        })
        .unwrap_or_else(|| panic!("step {step}: the read message is no longer unread"));
    let seen = message["flags"]
        .as_array()
        .is_some_and(|flags| flags.iter().any(|f| f.as_str() == Some("\\Seen")));
    assert!(!seen, "step {step}: the read message carries \\Seen");

    // The audit log never holds the app password or the token.
    let step = "audit log";
    let logged = std::fs::read_to_string(&live.log)
        .unwrap_or_else(|_| panic!("step {step}: the temporary audit log cannot be read"));
    assert!(
        !logged.contains(live.password()),
        "step {step}: the audit log contains the app password"
    );
    assert!(
        !logged.contains(live.token.expose()),
        "step {step}: the audit log contains the agent token"
    );
}

/// The fake address and app password of the `live_config` tests. Never a
/// real account.
const FAKE_ADDRESS: &str = "live@example.test";
const FAKE_PASSWORD: &str = "fake-app-password-123";

fn lookup_from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |name| map.get(name).cloned()
}

#[test]
fn live_config_without_either_variable_skips_with_a_clear_message() {
    let error = live_config(lookup_from(&[])).unwrap_err();
    assert_eq!(
        error,
        "gmail_live skipped: set AGENT_MAIL_LIVE_GMAIL_ADDRESS and AGENT_MAIL_LIVE_GMAIL_APP_PASSWORD to run it"
    );
}

#[test]
fn live_config_without_the_password_names_it_and_hides_the_address() {
    for pairs in [
        &[(ADDRESS_VAR, FAKE_ADDRESS)][..],
        &[(ADDRESS_VAR, FAKE_ADDRESS), (PASSWORD_VAR, "")],
    ] {
        let error = live_config(lookup_from(pairs)).unwrap_err();
        assert!(error.contains(PASSWORD_VAR), "{error}");
        assert!(!error.contains(FAKE_ADDRESS), "{error}");
    }
}

#[test]
fn live_config_without_the_address_names_it() {
    let error = live_config(lookup_from(&[(PASSWORD_VAR, FAKE_PASSWORD)])).unwrap_err();
    assert!(error.contains(ADDRESS_VAR), "{error}");
    assert!(!error.contains(FAKE_PASSWORD), "{error}");
}

#[test]
fn live_config_holds_the_password_as_a_redacted_credential() {
    let config = live_config(lookup_from(&[
        (ADDRESS_VAR, FAKE_ADDRESS),
        (PASSWORD_VAR, FAKE_PASSWORD),
    ]))
    .unwrap_or_else(|_| panic!("both variables set, but live_config failed"));
    assert_eq!(config.login, FAKE_ADDRESS);
    assert!(config.password == Credential::new(FAKE_PASSWORD));
    let debug = format!("{config:?}");
    assert!(!debug.contains(FAKE_PASSWORD), "Debug shows the password");
}

#[test]
fn live_round_trip_is_ignored_by_default() {
    let source = include_str!("gmail_live.rs");
    assert!(source.contains(
        "#[ignore = \"talks to a real Gmail account; run with `cargo test --test gmail_live -- --ignored`\"]\nfn gmail_live_round_trip()"
    ));
}
