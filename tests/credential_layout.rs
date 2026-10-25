//! How the Windows Credential Manager store lays out a record (job 024,
//! AC1c): target names, splitting values over the 2,560-byte blob limit and
//! joining them back. Pure functions, run on every OS.

use agent_mail::store::StoreError;
use agent_mail::store::credential_layout::{
    BLOB_LIMIT, TARGET_PREFIX, Target, classify, join, part_count, split, target_name, target_names,
};

const KEY: &[u8] = b"\x00\x01\xfe\xff store key";

fn value(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 7 % 251) as u8).collect()
}

#[test]
fn job024_ac1c_target_name_is_prefix_and_base64url_key() {
    assert_eq!(TARGET_PREFIX, "agent-mail:");
    assert_eq!(BLOB_LIMIT, 2560);
    assert_eq!(target_name(KEY), "agent-mail:AAH-_yBzdG9yZSBrZXk");
    // No padding, URL-safe alphabet.
    assert!(!target_name(b"x").contains('='));
}

#[test]
fn job024_ac1c_split_then_join_returns_the_value() {
    for len in [0, 1, 2560, 2561, 6000] {
        let original = value(len);
        let parts = split(&original).unwrap();
        assert!(
            parts.iter().all(|p| p.len() <= BLOB_LIMIT),
            "a part of {len} is over the limit"
        );
        assert_eq!(part_count(&parts[0]), Ok(parts.len()), "len {len}");
        assert_eq!(join(&parts), Ok(original), "len {len}");
    }
}

#[test]
fn job024_ac1c_a_6000_byte_value_gives_three_targets_in_order() {
    let parts = split(&value(6000)).unwrap();
    assert_eq!(parts.len(), 3);
    let base = target_name(KEY);
    assert_eq!(
        target_names(KEY, parts.len()),
        [base.clone(), format!("{base}#1"), format!("{base}#2")]
    );
    // A value that fits stays in one credential.
    assert_eq!(split(&value(100)).unwrap().len(), 1);
}

#[test]
fn job024_ac1c_join_with_a_missing_part_fails() {
    let mut parts = split(&value(6000)).unwrap();
    parts.remove(2);
    assert_eq!(join(&parts), Err(StoreError::Unavailable));
    parts.remove(1);
    assert_eq!(join(&parts), Err(StoreError::Unavailable));
    assert_eq!(join(&[]), Err(StoreError::Unavailable));
    // An extra part is refused too.
    let mut parts = split(&value(10)).unwrap();
    parts.push(vec![1]);
    assert_eq!(join(&parts), Err(StoreError::Unavailable));
}

#[test]
fn job024_ac1c_names_that_do_not_decode_are_foreign_and_parts_are_not_records() {
    let base = target_name(KEY);
    assert_eq!(classify(&base), Some(Target::Record(KEY.to_vec())));
    assert_eq!(classify(&format!("{base}#1")), Some(Target::Part));
    assert_eq!(classify(&format!("{base}#12")), Some(Target::Part));
    for foreign in [
        "agent-mail:not base64!",
        "agent-mail:a",
        "agent-mail:AAAA#",
        "agent-mail:AAAA#01",
        "agent-mail:AAAA#x",
        "agent-mail:!!#1",
    ] {
        assert_eq!(classify(foreign), Some(Target::Foreign), "{foreign}");
    }
    assert_eq!(classify("other:AAAA"), None);
}
