//! Opt-in tests against the real Windows Credential Manager (job 024, AC1a
//! and AC1b). Ignored by default; run with
//! `cargo test --test credential_manager -- --ignored`. They write dummy
//! records with fake values through agent-mail's own
//! `CredentialManagerStore` and delete them again.
#![cfg(windows)]

mod support;

use std::collections::HashMap;

use agent_mail::create::{GENERIC, TLS_IMPLICIT};
use agent_mail::seal::{self, AgentToken};
use agent_mail::store::credential_layout::{TARGET_PREFIX, target_name};
use agent_mail::store::{CredentialManagerStore, MandateStore};
use agent_mail::terminal::{Answer, ScriptedTerminal};
use support::{
    CREDENTIAL, FakeImap, LOGIN, Mode, error_of, run_logged, run_with_terminal, tokens_in,
};

/// Deletes the record under `key` when dropped, so a failed assertion does
/// not leave a dummy item behind.
struct Cleanup(Vec<u8>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = CredentialManagerStore.delete(&self.0);
    }
}

/// Every Credential Manager target name under `agent-mail:` that contains
/// the target name of `key`.
fn items_for(key: &[u8]) -> Vec<String> {
    let base = target_name(key);
    CredentialManagerStore
        .target_names()
        .expect("enumerate Credential Manager")
        .into_iter()
        .filter(|name| name.starts_with(TARGET_PREFIX) && name.contains(&base))
        .collect()
}

#[test]
#[ignore = "writes to the real Windows Credential Manager; run with `cargo test --test credential_manager -- --ignored`"]
fn job024_ac1a_mandate_create_then_list_with_the_printed_token() {
    let server = FakeImap::start(Mode::Implicit);
    let store = CredentialManagerStore;
    let terminal = ScriptedTerminal::new(vec![
        Answer::text("credential-manager-test"),
        Answer::choose(GENERIC),
        Answer::text("localhost"),
        Answer::text(&server.port.to_string()),
        Answer::choose(TLS_IMPLICIT),
        Answer::text(LOGIN),
        Answer::hidden(CREDENTIAL),
        Answer::tick(&["INBOX"]),
        Answer::tick(&["list"]),
        // No second Account, confirm the summary, decline the export offer.
        Answer::Confirm(false),
        Answer::Confirm(true),
        Answer::Confirm(false),
    ]);
    let created = run_with_terminal(
        &["mandate", "create"],
        &HashMap::new(),
        &store,
        &terminal,
        vec![server.ca.clone()],
        &[],
    );
    assert_eq!(created.code, 0, "stderr: {:?}", created.stderr);
    assert_eq!(terminal.remaining(), 0, "script not used up");
    let tokens = tokens_in(&terminal.transcript_text());
    assert_eq!(tokens.len(), 1, "expected one token in the transcript");
    let token = &tokens[0];
    let key = AgentToken::parse(token).unwrap().store_key();
    let _cleanup = Cleanup(key.clone());

    let log = support::temp_audit_log();
    let listed = run_logged(&["list"], Some(token), &store, &server.ca, &log);
    assert_eq!(listed.code, 0, "stderr: {:?}", listed.stderr);
    let value: serde_json::Value = serde_json::from_str(&listed.stdout).unwrap();
    assert_eq!(value["mailboxes"][0]["name"], "INBOX", "{value}");

    let record = store.get(&key).unwrap().expect("the Mandate is stored");
    let id = seal::listing(&record).unwrap().id;
    let deleted = run_logged(&["mandate", "delete", &id], None, &store, &server.ca, &log);
    assert_eq!(deleted.code, 0, "stderr: {:?}", deleted.stderr);

    let after = run_logged(&["list"], Some(token), &store, &server.ca, &log);
    assert_eq!(after.code, 1);
    assert_eq!(error_of(&after)["code"], "invalid_token");
    assert!(
        items_for(&key).is_empty(),
        "items left: {:?}",
        items_for(&key)
    );
}

#[test]
#[ignore = "writes to the real Windows Credential Manager; run with `cargo test --test credential_manager -- --ignored`"]
fn job024_ac1b_store_round_trip_is_add_only_and_leaves_nothing() {
    let store = CredentialManagerStore;
    let key = AgentToken::mint().store_key();
    let _cleanup = Cleanup(key.clone());
    let value: Vec<u8> = (0..6000).map(|i| (i % 253) as u8).collect();

    assert_eq!(store.get(&key), Ok(None), "fresh key already present");
    store
        .put(&key, &value)
        .expect("put into Credential Manager");
    assert_eq!(items_for(&key).len(), 3, "a 6,000-byte value is 3 items");
    assert_eq!(store.get(&key), Ok(Some(value.clone())));

    // Add-only: a second put under the same key fails and changes nothing.
    assert!(store.put(&key, b"replacement").is_err());
    assert_eq!(store.get(&key), Ok(Some(value.clone())));

    let entries = store.entries().expect("enumerate Credential Manager");
    let listed: Vec<_> = entries.iter().filter(|(k, _)| *k == key).collect();
    assert_eq!(listed.len(), 1, "record not listed exactly once");
    assert_eq!(listed[0].1, value);

    assert_eq!(store.delete(&key), Ok(true));
    assert_eq!(store.get(&key), Ok(None));
    assert_eq!(store.delete(&key), Ok(false));
    assert!(
        items_for(&key).is_empty(),
        "items left: {:?}",
        items_for(&key)
    );
}
