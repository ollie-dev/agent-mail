APPROVE
All seven ACs have passing tests, all four round-3 reviews pass, and the human's note to fix revocation including the name is done without new trade-offs or dependencies.

## Acceptance criteria
I ran `cargo test` in the worktree at 472cd8c: 80 passed, 0 failed, 1 ignored (the opt-in keychain test). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

- AC1: pass (mandates::ac1_list_shows_every_mandate_as_json_and_text, ac1_empty_store_lists_nothing, ac1_text_listing_escapes_control_characters, text_listing_escapes_bidi_and_zero_width_characters)
- AC2: pass (mandates::ac2_listing_shows_no_credential_token_hash_or_store_key)
- AC3: pass (mandates::ac3_delete_removes_one_mandate_and_the_other_still_works, delete_removes_every_record_with_the_id_so_a_decoy_cannot_block_revocation, delete_names_the_mandate_only_if_every_matching_record_agrees)
- AC4: pass (mandates::ac4_deleted_token_is_indistinguishable_from_a_never_issued_one)
- AC5: pass (mandates::ac5_unknown_or_already_deleted_id_is_not_found)
- AC6: pass (sealing::ac6_tampered_listing_fails_to_open_and_to_list covers (a) to (c); sealing::ac6_listing_spliced_from_another_record_fails_to_open covers (d))
- AC7: pass (mandates::ac7_unreadable_records_are_counted_not_dropped)
- Human note of 2026-10-21: pass (mandates::delete_does_not_claim_success_while_an_unreadable_record_may_survive, delete_names_the_mandate_only_if_every_matching_record_agrees)

Scope: the round-3 commit touches only src/core.rs, src/lib.rs and tests/mandates.rs. The whole branch stays within the files the spec lists. Cargo.toml and Cargo.lock are unchanged.

## For the human
- The round-2 escalation is resolved the way you asked. If the store holds any unreadable record, `mandate delete` now deletes every readable match and then exits 1 with `revocation_incomplete` instead of reporting success. It shows the name only when every deleted record agrees on it.
- Side effect to expect: your login Keychain already holds an `agent-mail` item from another build. If this binary cannot read it, every `mandate delete` will report `revocation_incomplete` until you remove that item in Keychain Access. Security also notes that any local process able to plant an unreadable `agent-mail` item can cause the same result. Readable matches are still deleted, so revocation is never blocked. Possible follow-up: let the human find or delete unreadable items from the CLI.
- Clear-text listing metadata in the Keychain value (the ticket-03 guarantee narrowed by the spec) is unchanged since round 2. Your note asked for no change there.
- Not re-run here: the opt-in Keychain test (`cargo test -- --ignored keychain`).
