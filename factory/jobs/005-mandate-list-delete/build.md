## Round 1
**Changed:**
- `src/seal.rs`: record format `RECORD_VERSION = 2`: `version | listing length (u32 BE) | listing JSON | nonce | ciphertext+tag`. The listing (`Listing`: id, name, created, accounts as login/host/port/tls, grants) is in clear and is AEAD associated data together with the version, the store key and its length. `open` also checks the listing equals the one derived from the decrypted Mandate. Token-free `listing()`, plus `parts()` / `assemble()` exposing the format (used by the AC6 (d) splice test). Version-1 records and junk give `UnreadableRecord`.
- `src/store.rs`: `MandateStore::entries()` (type alias `Entry`). `InMemoryStore` returns its map, `EmptyStore` returns nothing. `KeychainStore` finds items by attributes (`SecItemCopyMatching`, `kSecMatchLimitAll`, service `agent-mail`), then reads each value one by one. An item whose value cannot be read (e.g. access denied) comes back with an empty value, so it is counted as unreadable rather than dropped. New `KeychainStore::keys()` enumerates without reading values. Item attributes are unchanged: the account is still the base64url store key, and there is no label or comment.
- `src/core.rs`: `Command::MandateList` and `Command::MandateDelete { id }`. Neither reads a token or uses the Terminal. Result types `MandateListing { mandates, unreadable }`, `MandateSummary` and `DeletedMandate`. New `CoreError::NotFound { id }` (`not_found`, with an `id` field in the error body) has been added to the next-step unit test. Ordering is by `created`, then ID. `rfc3339_utc` has a unit test.
- `src/lib.rs`: `mandate list` and `mandate delete <ID>`, rendered as JSON or `--text`. All text fields go through `escape_controls`. Text error messages are now escaped too, because `not_found` echoes the untrusted ID.
- Tests: new `tests/mandates.rs`. `tests/sealing.rs` gets the AC6 tamper and splice tests. The `tests/keychain.rs` dummy item is now enumerated. `tests/cli.rs` checks `mandate --help` for create, list and delete. `CLAUDE.md` test layout is updated.
- **Deliberate change to a ticket-03 guarantee:** `ac8_store_holds_no_clear_text_secrets_or_names` is renamed `ac8_store_holds_no_clear_text_secrets`. It now asserts only that the Credential, the token, the token hash and `amt_` are absent from the raw key and value. Login and mailbox names are in clear by design.
- **Interpretation:** AC1 shows `{"mandates":[...]}`, but AC7 requires `unreadable` to be present and `0` when every record is readable. The JSON is therefore always `{"mandates":[...],"unreadable":N}`, so an empty store gives `{"mandates":[],"unreadable":0}`.
- **Keychain test note:** the opt-in test checks enumeration with `keys()`, not `entries()`. The dev machine's login Keychain already holds a real `agent-mail` item created by another build. Reading its value from the test binary raised a Keychain access prompt and hung the test. `keys()` and `entries()` share the same attribute search.

**Commits:** 3dc25e7
**Tests:** `cargo test`, every suite green: 76 passed, 0 failed, 1 ignored (keychain). `cargo test --test keychain -- --ignored keychain`: 1 passed. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | mandates::ac1_list_shows_every_mandate_as_json_and_text, ac1_empty_store_lists_nothing, ac1_text_listing_escapes_control_characters | pass |
| AC2 | mandates::ac2_listing_shows_no_credential_token_hash_or_store_key | pass |
| AC3 | mandates::ac3_delete_removes_one_mandate_and_the_other_still_works | pass |
| AC4 | mandates::ac4_deleted_token_is_indistinguishable_from_a_never_issued_one | pass |
| AC5 | mandates::ac5_unknown_or_already_deleted_id_is_not_found | pass |
| AC6 | sealing::ac6_tampered_listing_fails_to_open_and_to_list, sealing::ac6_listing_spliced_from_another_record_fails_to_open | pass |
| AC7 | mandates::ac7_unreadable_records_are_counted_not_dropped | pass |
| (ctx) | cli::help_lists_the_mandate_subcommand, keychain::keychain_round_trips_a_dummy_mandate_and_deletes_it, sealing::ac8_store_holds_no_clear_text_secrets | pass |

**Review findings:** none (first round).

## Round 2
**Changed:**
- `src/core.rs` `mandate_delete`: deletes every record whose listing has the ID, not just the first. It succeeds only if at least one record was removed; the reported name is the first deleted record's name. Any store error returns `store_unavailable`, so the human can retry and no success is printed.
- `src/terminal.rs` `escape_controls`: also escapes Unicode format characters (Cf), for example bidi overrides U+202A to U+202E and U+2066 to U+2069, zero-width characters U+200B to U+200F, and U+FEFF. This affects every text output that uses it.
- `src/store.rs`: a Keychain item whose account is not valid base64url is no longer given a raw-bytes key that `delete` would re-encode. `keys()` skips it. `entries()` returns it with an empty value, so it counts as unreadable and is never chosen for delete.
- `src/lib.rs`: the `mandate` about text now reads "Create and manage Mandates (create is run by the human; agents may list and delete)".

**Commits:** 0d9d2a0

**Tests:** `cargo test`: every suite green, 78 passed, 0 failed, 1 ignored. `cargo test --test keychain -- --ignored keychain`: 1 passed. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`: clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | mandates::ac1_list_shows_every_mandate_as_json_and_text, ac1_empty_store_lists_nothing, ac1_text_listing_escapes_control_characters, text_listing_escapes_bidi_and_zero_width_characters | pass |
| AC2 | mandates::ac2_listing_shows_no_credential_token_hash_or_store_key | pass |
| AC3 | mandates::ac3_delete_removes_one_mandate_and_the_other_still_works, delete_removes_every_record_with_the_id_so_a_decoy_cannot_block_revocation | pass |
| AC4 | mandates::ac4_deleted_token_is_indistinguishable_from_a_never_issued_one | pass |
| AC5 | mandates::ac5_unknown_or_already_deleted_id_is_not_found | pass |
| AC6 | sealing::ac6_tampered_listing_fails_to_open_and_to_list, sealing::ac6_listing_spliced_from_another_record_fails_to_open | pass |
| AC7 | mandates::ac7_unreadable_records_are_counted_not_dropped | pass |

**Review findings:**
- [blocking] security, a decoy record defeats `mandate delete`: **fixed.** Every record with the ID is now deleted. The new test `delete_removes_every_record_with_the_id_so_a_decoy_cannot_block_revocation` stores A, B and a decoy that copies A's clear listing over a junk sealed part. It runs once with the decoy filed before A in enumeration order and once with it filed after. Each time it checks that `mandate delete aaaa1111` leaves only B, that A's token gets `invalid_token`, and that B still lists.
- Non-blocking items fixed: the security nits on base64 account handling and on escaping Cf/bidi characters, and the UX nit on the `mandate` about text.
- Non-blocking items left as they are:
  - UI nit, blank line before the unreadable notice: AC7 specifies adding exactly that one line.
  - UI nit on column alignment, and UX nit on documenting JSON in help: optional.
  - The always-present `unreadable` key: reviewers agreed with the Round 1 reading.

## Round 3
This round responds to the human note of 2026-10-21, "fix the mandate revocation incl. the name". It covers both round-2 security nits and the escalation in decision.md.

**Changed:**
- `src/core.rs` `mandate_delete`:
  - It still deletes every readable record whose listing has the ID.
  - If the store holds any record whose metadata cannot be read, it no longer reports success. It returns the new `CoreError::RevocationIncomplete { id, deleted, unreadable }`. The code is `revocation_incomplete` and the exit code is 1.
  - The JSON error carries `deleted` and `unreadable` counts.
  - The message is "deleted the readable records with ID <id>, but N stored record(s) could not be read and may still hold this Mandate, so its agent token may still work; check Keychain Access for `agent-mail` items that `agent-mail mandate list` does not show and delete them". When nothing matched, it begins "no readable Mandate has ID <id>, but … and may hold it".
  - With an all-readable store, behaviour is unchanged: success, or `not_found` (AC3 and AC5 are untouched).
- The success result's `name` is now `Option`.
  - It is reported only when every deleted record's listing agreed on it. That keeps AC3's exact output.
  - If the names disagree, which means one may be forged, JSON omits `name` (`{"deleted":{"id":"aaaa1111"}}`).
  - Text then reads `Deleted Mandate (ID aaaa1111). The stored records with this ID had different names, so none is shown.`
  - "Name from a record that opens" is not possible here: delete has no token by design (ADR-0003), so agreement is the strongest check available.
- `src/lib.rs`: the `mandate delete --help` text describes the delete-all behaviour and `revocation_incomplete`.
- The new error is added to the unit test that requires every error message to name a next step, in both its deleted and nothing-deleted forms.
- **Note for the human:** your login Keychain already holds an `agent-mail` item from another build. If this binary cannot read its value (the access prompt is denied), every `mandate delete` will now report `revocation_incomplete` until that item is removed in Keychain Access. That is the intended, conservative behaviour.
- **Note for reviewers:** JSON error objects serialise with sorted keys, through `serde_json::json!` in `render_error`, as before. The new test asserts that existing order (`code, deleted, id, message, unreadable`), so I did not change it.

**Commits:** 472cd8c
**Tests:** `cargo test`: every suite green, 80 passed, 0 failed, 1 ignored. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`: clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | mandates::ac1_list_shows_every_mandate_as_json_and_text, ac1_empty_store_lists_nothing, ac1_text_listing_escapes_control_characters, text_listing_escapes_bidi_and_zero_width_characters | pass |
| AC2 | mandates::ac2_listing_shows_no_credential_token_hash_or_store_key | pass |
| AC3 | mandates::ac3_delete_removes_one_mandate_and_the_other_still_works, delete_removes_every_record_with_the_id_so_a_decoy_cannot_block_revocation, delete_names_the_mandate_only_if_every_matching_record_agrees | pass |
| AC4 | mandates::ac4_deleted_token_is_indistinguishable_from_a_never_issued_one | pass |
| AC5 | mandates::ac5_unknown_or_already_deleted_id_is_not_found | pass |
| AC6 | sealing::ac6_tampered_listing_fails_to_open_and_to_list, sealing::ac6_listing_spliced_from_another_record_fails_to_open | pass |
| AC7 | mandates::ac7_unreadable_records_are_counted_not_dropped | pass |
| (human note) | mandates::delete_does_not_claim_success_while_an_unreadable_record_may_survive | pass |

**Review findings:**
- Human note and the security round-2 nit "false success while an unreadable real record survives": **fixed.**
  - The test uses a store wrapper that hides A's value from enumeration, as an item whose access list no longer trusts the binary would be hidden. A readable decoy with A's ID and name is also stored.
  - `mandate delete aaaa1111` now exits 1 with `revocation_incomplete` (deleted 1, unreadable 1) and empty stdout. A repeat run gives the "no readable Mandate" form. Deleting B also removes B, but still does not claim success.
- Security round-2 nit "reported name may come from a forged record": **fixed.** The name is shown only when all deleted records agree. The test covers JSON and `--text` with a decoy that has a different name.
