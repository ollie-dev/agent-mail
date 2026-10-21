//! Sealed Mandate records: tamper detection, token binding, no clear text.

mod support;

use agent_mail::mandate::{Operation, TlsMode};
use agent_mail::seal::{self, AgentToken, TOKEN_PREFIX};
use agent_mail::store::{InMemoryStore, MandateStore};
use support::{
    CREDENTIAL, FakeImap, LOGIN, Mode, error_of, grant, mandate, mandate_a, mandate_b, run,
    store_mandate, token_hash,
};

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

/// Ticket 03 asserted that no login or mailbox name sits in the raw bytes.
/// Since ticket 04 the listable metadata (names, logins, mailboxes) is in
/// clear by design, so this now covers only the Credential, the token and
/// the token hash.
#[test]
fn ac8_store_holds_no_clear_text_secrets() {
    let store = InMemoryStore::new();
    let token = store_mandate(&store, &sample());
    let entries = store.entries();
    assert_eq!(entries.len(), 1);

    let hash = token_hash(token.expose());
    for (key, value) in &entries {
        for bytes in [key, value] {
            let as_text = String::from_utf8_lossy(bytes);
            for needle in [token.expose(), CREDENTIAL, hash.as_str(), TOKEN_PREFIX] {
                assert!(
                    !contains(bytes, needle.as_bytes()),
                    "raw bytes contain a secret"
                );
                assert!(!as_text.contains(needle), "UTF-8 view contains a secret");
            }
        }
    }
    let listing = seal::listing(&entries[0].1).expect("listable metadata");
    assert_eq!(listing.accounts[0].login, LOGIN);
    assert_eq!(
        store.get(&token.store_key()).unwrap().as_ref(),
        Some(&entries[0].1)
    );
}

/// Replaces every occurrence of `from` with the same-length `to` in the
/// clear listing of `value`, checking that it occurs there.
fn replace_in_listing(value: &[u8], from: &str, to: &str) -> Vec<u8> {
    assert_eq!(from.len(), to.len());
    let parts = seal::parts(value).unwrap();
    assert!(
        contains(parts.listing, from.as_bytes()),
        "{from} not in clear"
    );
    let listing = String::from_utf8(parts.listing.to_vec())
        .unwrap()
        .replace(from, to);
    let tampered = seal::assemble(listing.as_bytes(), parts.sealed);
    assert_eq!(tampered.len(), value.len());
    tampered
}

#[test]
fn ac6_tampered_listing_fails_to_open_and_to_list() {
    let server = FakeImap::start(Mode::Implicit);
    let a = mandate_a(server.port);
    for (from, to) in [
        ("work-inbox", "evil-inbox"),
        ("agent@example.test", "agent@example.evil"),
        ("\"Archive\"", "\"Spamxyz\""),
    ] {
        let store = InMemoryStore::new();
        let token = store_mandate(&store, &a);
        let (key, value) = store.entries().remove(0);
        let tampered = replace_in_listing(&value, from, to);
        assert!(seal::open(&token, &tampered).is_err(), "{to} still opened");

        store.delete(&key).unwrap();
        store.put(&key, &tampered).unwrap();
        for args in [&["list"][..], &["--text", "list"][..]] {
            let out = run(args, Some(token.expose()), &store, &server);
            assert_eq!(out.code, 1, "{to}: {:?}", out.stdout);
            assert_eq!(out.stdout, "");
            if args.len() == 1 {
                assert_eq!(error_of(&out)["code"], "invalid_token");
            } else {
                assert!(out.stderr.starts_with("error: invalid_token: "));
            }
        }
        assert_eq!(server.connections(), 0, "{to}: tampered record connected");
    }

    // The unmodified record still opens and lists.
    let store = InMemoryStore::new();
    let token = store_mandate(&store, &a);
    let (_, value) = store.entries().remove(0);
    assert_eq!(seal::open(&token, &value), Ok(a.clone()));
    let out = run(&["list"], Some(token.expose()), &store, &server);
    assert_eq!(out.code, 0, "stderr: {:?}", out.stderr);
    assert_eq!(
        out.stdout,
        "{\"account\":\"agent@example.test\",\"mailboxes\":[{\"name\":\"INBOX\"},{\"name\":\"Archive\"}]}\n"
    );
}

#[test]
fn ac6_listing_spliced_from_another_record_fails_to_open() {
    let a = mandate_a(993);
    let token_a = AgentToken::mint();
    let token_b = AgentToken::mint();
    let record_a = seal::seal(&token_a, &a);
    let record_b = seal::seal(&token_b, &mandate_b(993));

    let parts_a = seal::parts(&record_a.value).unwrap();
    let parts_b = seal::parts(&record_b.value).unwrap();
    let spliced = seal::assemble(parts_b.listing, parts_a.sealed);
    assert_eq!(seal::listing(&spliced).unwrap().id, "bbbb2222");
    assert!(seal::open(&token_a, &spliced).is_err());
    assert!(seal::open(&token_b, &spliced).is_err());

    // Reassembling A's own parts is the identity and still opens.
    let same = seal::assemble(parts_a.listing, parts_a.sealed);
    assert_eq!(same, record_a.value);
    assert_eq!(seal::open(&token_a, &same), Ok(a));
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

#[test]
fn store_put_is_add_only() {
    let store = InMemoryStore::new();
    let token = store_mandate(&store, &sample());
    let key = token.store_key();
    let before = store.get(&key).unwrap();

    assert!(store.put(&key, b"replacement").is_err());
    assert_eq!(store.get(&key).unwrap(), before);
    assert_eq!(store.entries().len(), 1);

    assert_eq!(store.delete(&key), Ok(true));
    assert!(store.put(&key, b"replacement").is_ok());
}
