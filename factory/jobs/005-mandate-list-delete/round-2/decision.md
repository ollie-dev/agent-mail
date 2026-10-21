ESCALATE
All ACs pass and all four round-2 reviews pass, but security accepted a residual case where `mandate delete` reports success while the real Mandate survives, and a human should own that revocation trade-off.

## Acceptance criteria
I ran `cargo test` in the worktree at 0d9d2a0: 78 passed, 0 failed, 1 ignored (the opt-in keychain test). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

- AC1: pass (mandates::ac1_list_shows_every_mandate_as_json_and_text, ac1_empty_store_lists_nothing, ac1_text_listing_escapes_control_characters, text_listing_escapes_bidi_and_zero_width_characters)
- AC2: pass (mandates::ac2_listing_shows_no_credential_token_hash_or_store_key)
- AC3: pass (mandates::ac3_delete_removes_one_mandate_and_the_other_still_works, delete_removes_every_record_with_the_id_so_a_decoy_cannot_block_revocation)
- AC4: pass (mandates::ac4_deleted_token_is_indistinguishable_from_a_never_issued_one)
- AC5: pass (mandates::ac5_unknown_or_already_deleted_id_is_not_found)
- AC6: pass (sealing::ac6_tampered_listing_fails_to_open_and_to_list covers (a) to (c) and checks that no connection is made; sealing::ac6_listing_spliced_from_another_record_fails_to_open covers (d))
- AC7: pass (mandates::ac7_unreadable_records_are_counted_not_dropped, run with both `b"garbage"` and a version-1 record)

Scope: the diff touches only the files the spec lists (src/core.rs, lib.rs, seal.rs, store.rs, terminal.rs, tests, CLAUDE.md). Cargo.toml and Cargo.lock are unchanged, so there are no new dependencies. The wider `escape_controls` (Unicode Cf characters) affects every text output. It is a small, defensive change that the round-1 security review asked for.

## For the human
Everything else is ready to merge. Two things need your decision.

1. **Residual false-success on revocation** (round-2 security nit, src/core.rs `mandate_delete`). Delete matches records by their unauthenticated clear listing. Suppose a real Mandate's value cannot be read: for example, a rebuild changed the code signature that the Keychain ACL trusts, so `entries()` returns an empty value. That record has no listing, so delete never matches it. If an Agent has also planted a readable decoy with the same ID, `mandate delete <id>` removes the decoy, prints `Deleted Mandate ...`, exits 0, and the real token keeps working. The reviewer judged this narrower than the round-1 blocker, because an attacker cannot make the real record unreadable without a prompt, and passed it as a nit. Revocation is the one guarantee delete exists for, which is why I am asking you rather than approving.
   - Approve as is: you accept this edge case for v1, and it can become a follow-up ticket.
   - Rework: have delete warn or refuse when the store holds unreadable records ("revocation may be incomplete; N records could not be read"). This would also be a good moment to report the name from a record that opens, not from the decoy's forged listing (second security nit, cosmetic only).

2. **Deliberate weakening of a ticket-03 guarantee (spec-mandated, for awareness).** Mandate names, logins, hosts, mailboxes and Grants are now stored in clear inside the Keychain item's value. They are not in its attributes, and they are authenticated as AEAD associated data. The ticket-03 test `ac8_store_holds_no_clear_text_secrets` was narrowed to the Credential, the token and the token hash. Any process that can run the binary can also list every Mandate's Grants without a token. Your spec and ADR-0003 asked for this, and reviewers confirmed that every listed field is authenticated. It needs no action unless you have changed your mind.

Also not verified here: the opt-in Keychain test (`cargo test -- --ignored keychain`) was not re-run in this review. build.md reports it passing, using `keys()` rather than `entries()` to avoid a Keychain prompt from an existing real item.
