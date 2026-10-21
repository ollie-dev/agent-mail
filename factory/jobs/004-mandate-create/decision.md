ESCALATE
Your round-3 asks are done and all four reviews pass, but this is still the first code that stores Credentials (sealed, in the macOS Keychain), and your rework note did not say you accept that, so you need to sign off.

## Acceptance criteria
I ran these in the worktree on 6047490. `cargo test`: lib 8, cli 10, create 20, list 15, project 5, sealing 7, all ok. The keychain test was ignored, as it should be by default. `cargo clippy --all-targets -- -D warnings` found nothing and `cargo fmt --check` passed. I also ran `cargo test --test keychain -- --ignored keychain` on this Mac, and its 1 test passed.
- AC1: pass (tests/create.rs `ac1_generic_account_session_stores_exactly_the_ticked_grants`)
- AC2: pass (`ac2_printed_token_lists_exactly_the_ticked_mailboxes`)
- AC3: pass (`ac3_gmail_preset_asks_only_address_and_app_password`)
- AC4: pass (`ac4_wrong_credential_can_be_retried_and_completes`, `ac4_abort_after_wrong_credential_stores_nothing`)
- AC5: pass (`ac5_two_accounts_make_one_mandate_with_separate_grants`, `ac5_same_login_twice_is_refused_and_reasked`)
- AC6: pass (`ac6_declining_the_summary_stores_nothing`)
- AC7: pass (`ac7_non_interactive_terminal_is_refused_before_any_prompt_or_store_access`, `ac7_binary_with_piped_stdin_and_stdout_refuses`)
- AC8: pass (`ac8_each_sandbox_indicator_is_refused`, which now includes `OPENCODE`)
- AC9: pass (`ac9_credential_flags_are_usage_errors`, `ac9_help_lists_no_credential_option`, `ac9_environment_cannot_supply_the_credential`)
- AC10: pass (`ac10_credentials_are_hidden_and_never_shown_or_stored_in_clear`, `ac10_real_terminal_reads_credentials_hidden_without_mask_or_echo`). The real-adapter part only inspects the source.
- AC11: pass (`ac11_token_is_shown_once_and_no_command_reveals_it_again`, `ac11_token_that_cannot_be_shown_on_the_tty_leaves_nothing_stored`, `ac11_real_terminal_writes_the_token_to_dev_tty_only`). The last one only inspects the source.
- AC12: pass (tests/keychain.rs `keychain_round_trips_a_dummy_mandate_and_deletes_it`). It is ignored by default. It passed when I ran it with `--ignored`, and it now also checks that a second `put` under the same key fails.

Round 3 reviews: code, security, ui and ux all say `VERDICT: PASS` and raise only nits. The diff touches 19 files (about 2.6k lines) and stays inside the spec's file list and scope. Round 3 changed only the store's `put`, the indicator list and the tests for both.

## For the human
**Your note is resolved.** "add OPENCODE, make put add-only":
- `OPENCODE` is now in `SANDBOX_INDICATORS` (src/create.rs).
- `KeychainStore::put` now calls `SecItemAdd` through `ItemAddOptions`, so it can never update an existing item or change that item's access list. A duplicate fails, and `create` then reports `store_unavailable` without showing a token.
- `InMemoryStore::put` follows the same add-only rule (tests/sealing.rs `store_put_is_add_only`).
- The fix adds `core-foundation` 0.10 as a direct macOS-only dependency. It is only used to build the `CFData` that carries the sealed bytes. It was already in Cargo.lock through security-framework, so no new code enters the build.

**Still yours to decide.** A rework note is not sign-off, so these stay open:
1. **Credential storage.** Real builds write sealed Mandates, which contain the IMAP Credentials, to your login Keychain through `security-framework` 3. That is a new macOS-only dependency that handles secrets. Each Mandate is one generic-password item: service `agent-mail`, item account = base64url of the store key. The item gets the default access list, which trusts the binary that created it. An unsigned binary changes with every rebuild, and signing is ticket 16. The spec asks for exactly this design.
2. **Rollback message** (security nit, carried over; src/create.rs around line 97). If showing the token fails and the Keychain delete then also fails, `let _ =` drops the delete error. The terminal still says "Nothing was stored." No secret leaks, because nobody holds the token, so nobody can unseal the leftover record. But in that rare case the message is wrong.
3. **Smaller items:** a duplicate item and a real Keychain failure produce the same `StoreError` (code nit). `inquire`'s input buffer is not zeroized. Nobody has yet checked in a real terminal that inquire's prompts on stderr and the `/dev/tty` messages appear in the right order, so run `agent-mail mandate create` once by hand. UX nits: a wrong host is a dead end until you abort and start over, and the abort message could suggest running the command again.

- **Approve:** the factory merges as is. Real `mandate create` then writes to your Keychain, and items 2 and 3 become follow-ups (for example with tickets 04 and 16).
- **Rework:** send it back with a note. The likely ask is to report a failed rollback delete honestly, which is a one-line change plus one scripted test with a failing store.
