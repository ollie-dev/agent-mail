//! Sealed Mandate records: tamper detection, token binding, no clear text.

mod support;

use agent_mail::mandate::{Operation, TlsMode};
use agent_mail::seal::{self, AgentToken, TOKEN_PREFIX};
use agent_mail::store::{InMemoryStore, MandateStore};
use support::{CREDENTIAL, LOGIN, grant, mandate, store_mandate};

fn sample() -> agent_mail::mandate::Mandate {
    mandate(
        993,
        TlsMode::Implicit,
        vec![grant(Operation::List, &["INBOX", "Archive"])],
    )
}

#[test]
fn sealed_record_opens_with_its_own_token() {
    let token = AgentToken::mint();
    let record = seal::seal(&token, &sample());
    assert_eq!(record.key, token.store_key());
    assert_eq!(seal::open(&token, &record.value), Ok(sample()));
}

#[test]
fn ac7_altered_ciphertext_fails_to_open() {
    let token = AgentToken::mint();
    let record = seal::seal(&token, &sample());
    for index in 0..record.value.len() {
        let mut value = record.value.clone();
        value[index] ^= 0x01;
        assert!(
            seal::open(&token, &value).is_err(),
            "flipping byte {index} still opened"
        );
    }
}

#[test]
fn ac7_record_fails_to_open_with_another_token() {
    let token = AgentToken::mint();
    let record = seal::seal(&token, &sample());
    let other = AgentToken::mint();
    assert!(seal::open(&other, &record.value).is_err());
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn ac8_store_holds_no_clear_text_secrets_or_names() {
    let store = InMemoryStore::new();
    let token = store_mandate(&store, &sample());
    let entries = store.entries();
    assert_eq!(entries.len(), 1);

    for (key, value) in &entries {
        for bytes in [key, value] {
            let as_text = String::from_utf8_lossy(bytes);
            for needle in [token.expose(), CREDENTIAL, LOGIN, "Archive"] {
                assert!(
                    !contains(bytes, needle.as_bytes()),
                    "raw bytes contain a secret or name"
                );
                assert!(
                    !as_text.contains(needle),
                    "UTF-8 view contains a secret or name"
                );
            }
        }
    }
    assert_eq!(
        store.get(&token.store_key()).unwrap().as_ref(),
        Some(&entries[0].1)
    );
}

#[test]
fn tokens_are_prefixed_and_carry_256_random_bits() {
    let token = AgentToken::mint();
    let body = token.expose().strip_prefix(TOKEN_PREFIX).unwrap();
    assert_eq!(body.len(), 43);
    assert!(
        body.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    );
    assert_ne!(token, AgentToken::mint());
    assert_eq!(AgentToken::parse(token.expose()), Ok(token.clone()));
    assert!(AgentToken::parse("hello").is_err());
    assert!(AgentToken::parse(&format!("{TOKEN_PREFIX}short")).is_err());
}

#[test]
fn debug_output_redacts_token_and_credential() {
    let token = AgentToken::mint();
    let record = seal::seal(&token, &sample());
    let debug = format!("{token:?} {:?} {record:?}", sample());
    assert!(!debug.contains(token.expose()));
    assert!(!debug.contains(CREDENTIAL));
}
