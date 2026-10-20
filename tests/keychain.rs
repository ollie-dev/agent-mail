//! Opt-in test against the real macOS Keychain. Ignored by default; run with
//! `cargo test -- --ignored keychain`. It writes one dummy item with fake
//! values through agent-mail's own `KeychainStore` (so the item is created by
//! this process via the Security framework, never by the `security` CLI) and
//! deletes it again.
#![cfg(target_os = "macos")]

mod support;

use agent_mail::mandate::{Operation, TlsMode};
use agent_mail::seal::{self, AgentToken};
use agent_mail::store::{KeychainStore, MandateStore};
use support::{grant, mandate};

#[test]
#[ignore = "writes to the real Keychain; run with `cargo test -- --ignored keychain`"]
fn keychain_round_trips_a_dummy_mandate_and_deletes_it() {
    let store = KeychainStore;
    let dummy = mandate(
        993,
        TlsMode::Implicit,
        vec![grant(Operation::List, &["INBOX"])],
    );
    let token = AgentToken::mint();
    let record = seal::seal(&token, &dummy);

    assert_eq!(
        store.get(&record.key),
        Ok(None),
        "fresh key already present"
    );
    store
        .put(&record.key, &record.value)
        .expect("put into the Keychain");
    let stored = store.get(&record.key).expect("get from the Keychain");
    let deleted = store.delete(&record.key);

    assert_eq!(stored.as_deref(), Some(record.value.as_slice()));
    assert_eq!(seal::open(&token, stored.as_deref().unwrap()), Ok(dummy));
    assert_eq!(deleted, Ok(true));
    assert_eq!(store.get(&record.key), Ok(None));
    assert_eq!(store.delete(&record.key), Ok(false));
}
