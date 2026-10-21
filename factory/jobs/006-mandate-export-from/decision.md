APPROVE
Round 2 makes every change the human asked for after the round 1 escalation, all four reviews pass, and every AC has a test that I watched pass.

## Acceptance criteria
I ran `cargo test` in the worktree at commit 8a13f8a. Every suite passed: lib 13, cli 10, create 20, export 20, list 15, mandates 12, project 5, sealing 9, and the keychain test was ignored as designed. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.
- AC1: pass (`ac1_accepting_the_export_offer_writes_name_accounts_and_grants`)
- AC2: pass (`ac2_the_exported_file_holds_no_credential_or_agent_token`)
- AC3: pass (`ac3_from_file_pre_selects_the_file_choices`, `ac3_from_file_choices_can_be_changed_before_confirming`, `ac3_a_file_account_can_be_dropped`)
- AC4: pass (`ac4_every_file_account_is_credential_checked`, `ac4_a_wrong_credential_from_a_file_session_can_be_retried`, `ac4_abort_after_a_wrong_credential_stores_nothing`)
- AC5: pass (`ac5_a_mailbox_missing_on_the_server_is_shown_unavailable`)
- AC6: pass (`ac6_malformed_files_are_refused_before_any_prompt`, cases a-i; `ac6_refusal_checks_take_precedence_over_file_errors`). Case (b) changed on purpose, following human-notes.md: the message is now `line 8: unknown key ...`, and the test checks that it does not contain `password` or the secret value.
- AC7: pass (`ac7_declining_the_export_offer_writes_nothing`, `ac7_no_export_offer_when_the_store_write_or_reveal_fails`)
- Human feedback: pass (`credential_prompt_names_the_server_host_and_port`, `gmail_login_on_another_host_is_warned_before_the_credential`, `from_must_name_a_regular_file_and_is_never_opened_otherwise`, unit test `create::tests::gmail_addresses_are_recognised`)

Scope: the diff touches only the files the spec lists (src/create.rs, terminal.rs, core.rs, lib.rs, the new export.rs, Cargo.toml/lock, tests/create.rs, the new tests/export.rs, and the CLAUDE.md layout line). Round 2 added no new dependencies.

Reviews (round 2): code, security, UI and UX all say `VERDICT: PASS`, with nits only.

## For the human
Round 2 fixes what the round 1 escalation raised:
- The Credential prompt now names `host:port`.
- A Gmail login paired with a host other than imap.gmail.com gets a warning before the Credential prompt.
- File errors give the line number only and repeat no key from the file.
- `--from` is refused unless it is a regular file.

The union pre-tick (built as the spec describes) and the `toml` dependency were already flagged in round 1, and your feedback did not ask to change them. The `toml` crate only parses files and never handles a secret.

Optional hardening nits are left. They are defence in depth and could go in a follow-up:
- A FIFO swapped in between the metadata check and `open` can still hang the run. The fix would be to open with `O_NONBLOCK`.
- The Gmail check misses look-alike logins and a trailing-dot `imap.gmail.com.`.
- Three reviewers suggest adding "or abort with Ctrl-C" to the Gmail warning.

On approve, the orchestrator merges `factory/006-mandate-export-from` (commit 8a13f8a).
