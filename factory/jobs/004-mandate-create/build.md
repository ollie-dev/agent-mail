## Round 1
**Changed:**
- Mandate is now `name`, `id` (8 hex), `created` (Unix s), `accounts: Vec<Account>`, `policy`; sealing unchanged in guarantees (`src/mandate.rs`, `src/seal.rs` adds `short_id`).
- Store port gains `delete`; new macOS `KeychainStore` (Security framework via `security-framework` 3, service `agent-mail`, item account = base64url store key) is `RuntimeStore` on macOS; other OSes use `EmptyStore` whose `put` fails (`store_unavailable`) (`src/store.rs`).
- New Terminal port (`src/terminal.rs`): `InquireTerminal` (stdin, stdout and stderr must be TTYs, since inquire draws on stderr; hidden input via `Password` with `PasswordDisplayMode::Hidden`, no toggle), `NoTerminal` default, and test-only `ScriptedTerminal` (kind-checked answers, transcript, hidden answers never recorded).
- New `src/create.rs` guided flow driven through `core::execute`: TTY check, then `SANDBOX_INDICATORS` check (CLAUDECODE, CLAUDE_CODE_ENTRYPOINT, SANDBOX_RUNTIME, CODEX_SANDBOX, CODEX_SANDBOX_NETWORK_DISABLED, GEMINI_CLI, GEMINI_SANDBOX, CURSOR_AGENT), both before any prompt or store access; Gmail preset or generic; live login via `imap::list_mailboxes` with retry (re-asks only the Credential) or abort; duplicate login re-prompted; checklists escape control characters; summary; seal, store, token shown once via the Terminal only. Draft Grants get an empty mailbox list.
- `core`: `execute` now takes the env map (token read from `AGENT_MAIL_TOKEN` inside); new errors `interactive_terminal_required`, `sandbox_detected`, `aborted`, `account_required` (with `accounts` field); `store_unavailable` now covers writes. `list` returns `account_required` for multi-Account Mandates.
- `imap::Network` replaces the bare trust; test-only `with_route` connect override (TLS still verifies the Account host). Release-marker check extended to the scripted terminal and connect override.
- CLI: `mandate create` subcommand without options; `main.rs` wires `InquireTerminal` and `RuntimeStore`. Fake server now accepts LOGIN only with the fake Credential for known logins and its cert names `imap.gmail.com`. `tests/cli.rs` "help does not mention mandates" replaced by "help lists the mandate subcommand" (job 002 spec allowed mentioning Mandates once they exist). CLAUDE.md layout updated.

**Commits:** 92f0537
**Tests:** `cargo test` -> all suites `ok` (lib 8, cli 10, create 18, keychain 0 passed / 1 ignored, list 15, project 5, sealing 6); `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean. Opt-in `cargo test --test keychain -- --ignored keychain` -> `test result: ok. 1 passed` on this Mac.

| AC | Test | Status |
|----|------|--------|
| AC1 | tests/create.rs `ac1_generic_account_session_stores_exactly_the_ticked_grants` | pass |
| AC2 | tests/create.rs `ac2_printed_token_lists_exactly_the_ticked_mailboxes` | pass |
| AC3 | tests/create.rs `ac3_gmail_preset_asks_only_address_and_app_password` | pass |
| AC4 | tests/create.rs `ac4_wrong_credential_can_be_retried_and_completes`, `ac4_abort_after_wrong_credential_stores_nothing` | pass |
| AC5 | tests/create.rs `ac5_two_accounts_make_one_mandate_with_separate_grants` (+ `ac5_same_login_twice_is_refused_and_reasked`) | pass |
| AC6 | tests/create.rs `ac6_declining_the_summary_stores_nothing` | pass |
| AC7 | tests/create.rs `ac7_non_interactive_terminal_is_refused_before_any_prompt_or_store_access`, `ac7_binary_with_piped_stdin_and_stdout_refuses` | pass |
| AC8 | tests/create.rs `ac8_each_sandbox_indicator_is_refused` | pass |
| AC9 | tests/create.rs `ac9_credential_flags_are_usage_errors`, `ac9_help_lists_no_credential_option`, `ac9_environment_cannot_supply_the_credential` | pass |
| AC10 | tests/create.rs `ac10_credentials_are_hidden_and_never_shown_or_stored_in_clear`, `ac10_real_terminal_reads_credentials_hidden_without_mask_or_echo` | pass |
| AC11 | tests/create.rs `ac11_token_is_shown_once_and_no_command_reveals_it_again` | pass |
| AC12 | tests/keychain.rs `keychain_round_trips_a_dummy_mandate_and_deletes_it` (ignored by default; passes with `--ignored`) | pass |

Extra: tests/list.rs `multi_account_mandate_requires_an_account_and_never_merges`; src/create.rs unit tests for summary escaping and indicator detection; tests/project.rs `ac11_release_binary_has_no_test_only_code` (three markers).

**Review findings:** none (round 1).

**Notes for reviewers:** The AC10 "real adapter uses hidden input" check is a source-level guard on `InquireTerminal::hidden`, since inquire cannot be driven without a TTY. Retry re-asks only the Credential; a wrong host means abort and restart, as the spec allows.

## Round 2
**Changed:**
- Human note (2026-10-21) "Write the Agent Token to /dev/tty, not stdout": the Terminal port gains `reveal(text) -> Result`. `InquireTerminal::reveal` opens `/dev/tty` directly and writes there. It never touches fd 1 or fd 2, so a wrapper that captures stdout or stderr (even through a pty) does not see the token. `InquireTerminal::message` also writes to `/dev/tty` now, so no `mandate create` text reaches stdout.
- `InquireTerminal::is_interactive` now also requires `/dev/tty` to open for writing, so a missing controlling terminal is refused up front with `interactive_terminal_required`, before any prompt.
- If the token cannot be shown, the just-stored record is deleted through the store's `delete`, the terminal says nothing was stored, and the run exits with `aborted`. A Mandate whose token nobody saw would be unusable.
- The token display string is built in a `Zeroizing<String>` (a security review nit).
- `ScriptedTerminal` records reveals as `Kind::Reveal`. New `ScriptedTerminal::failing_reveal` simulates an unwritable tty.

**Commits:** eb3d004
**Tests:** `cargo test` -> all suites `ok` (lib 8, cli 10, create 20, keychain 0 passed / 1 ignored, list 15, project 5, sealing 6); `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | tests/create.rs `ac1_generic_account_session_stores_exactly_the_ticked_grants` | pass |
| AC2 | tests/create.rs `ac2_printed_token_lists_exactly_the_ticked_mailboxes` | pass |
| AC3 | tests/create.rs `ac3_gmail_preset_asks_only_address_and_app_password` | pass |
| AC4 | tests/create.rs `ac4_wrong_credential_can_be_retried_and_completes`, `ac4_abort_after_wrong_credential_stores_nothing` | pass |
| AC5 | tests/create.rs `ac5_two_accounts_make_one_mandate_with_separate_grants` | pass |
| AC6 | tests/create.rs `ac6_declining_the_summary_stores_nothing` | pass |
| AC7 | tests/create.rs `ac7_non_interactive_terminal_is_refused_before_any_prompt_or_store_access`, `ac7_binary_with_piped_stdin_and_stdout_refuses` | pass |
| AC8 | tests/create.rs `ac8_each_sandbox_indicator_is_refused` | pass |
| AC9 | tests/create.rs `ac9_credential_flags_are_usage_errors`, `ac9_help_lists_no_credential_option`, `ac9_environment_cannot_supply_the_credential` | pass |
| AC10 | tests/create.rs `ac10_credentials_are_hidden_and_never_shown_or_stored_in_clear`, `ac10_real_terminal_reads_credentials_hidden_without_mask_or_echo` | pass |
| AC11 | tests/create.rs `ac11_token_is_shown_once_and_no_command_reveals_it_again` (now also asserts the token is shown only via `Kind::Reveal`), `ac11_token_that_cannot_be_shown_on_the_tty_leaves_nothing_stored`, `ac11_real_terminal_writes_the_token_to_dev_tty_only` | pass |
| AC12 | tests/keychain.rs `keychain_round_trips_a_dummy_mandate_and_deletes_it` (ignored by default) | pass |

**Review findings:** round 1 had no blocking findings. Human feedback was addressed as above. This also covers the security nit at src/terminal.rs:138 (token on stdout) and the reviewer's `Zeroizing` suggestion. Other round-1 nits (add-only Keychain `put`, more sandbox indicators such as `OPENCODE`) are not part of the human note and were left as they were.

**Note:** `ac11_real_terminal_writes_the_token_to_dev_tty_only` checks the source, like the AC10 hidden-input guard, because the real `/dev/tty` path cannot run without a controlling terminal.

## Round 3
**Changed:**
- Human note (2026-10-21) "add OPENCODE, make put add-only":
  - `OPENCODE` is added to `SANDBOX_INDICATORS` (src/create.rs). AC8's test now requires it and checks every indicator separately.
  - `MandateStore::put` is now add-only. It fails if a record is already stored under the key and never replaces one.
    - `KeychainStore::put` now calls `SecItemAdd` through `security_framework::item::ItemAddOptions` (generic-password class, service `agent-mail`, account = base64url store key). It no longer uses `set_generic_password`, which updated an existing item and kept that item's old access list. `errSecDuplicateItem` is now an error.
    - This adds `core-foundation` 0.10 as a macOS-only dependency for `CFData`. It was already in Cargo.lock through security-framework.
  - `InMemoryStore::put` follows the same add-only rule.
- The tamper test in tests/list.rs now deletes the record and puts the tampered bytes back, since `put` no longer replaces.

**Commits:** 6047490
**Tests:** `cargo test` -> all suites `ok` (lib 8, cli 10, create 20, keychain 0 passed / 1 ignored, list 15, project 5, sealing 7); `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean. Opt-in `cargo test --test keychain -- --ignored keychain` -> `test result: ok. 1 passed` (now also asserts a second `put` under the same key fails and leaves the item unchanged).

| AC | Test | Status |
|----|------|--------|
| AC1 | tests/create.rs `ac1_generic_account_session_stores_exactly_the_ticked_grants` | pass |
| AC2 | tests/create.rs `ac2_printed_token_lists_exactly_the_ticked_mailboxes` | pass |
| AC3 | tests/create.rs `ac3_gmail_preset_asks_only_address_and_app_password` | pass |
| AC4 | tests/create.rs `ac4_wrong_credential_can_be_retried_and_completes`, `ac4_abort_after_wrong_credential_stores_nothing` | pass |
| AC5 | tests/create.rs `ac5_two_accounts_make_one_mandate_with_separate_grants` | pass |
| AC6 | tests/create.rs `ac6_declining_the_summary_stores_nothing` | pass |
| AC7 | tests/create.rs `ac7_non_interactive_terminal_is_refused_before_any_prompt_or_store_access`, `ac7_binary_with_piped_stdin_and_stdout_refuses` | pass |
| AC8 | tests/create.rs `ac8_each_sandbox_indicator_is_refused` (now includes `OPENCODE`) | pass |
| AC9 | tests/create.rs `ac9_credential_flags_are_usage_errors`, `ac9_help_lists_no_credential_option`, `ac9_environment_cannot_supply_the_credential` | pass |
| AC10 | tests/create.rs `ac10_credentials_are_hidden_and_never_shown_or_stored_in_clear`, `ac10_real_terminal_reads_credentials_hidden_without_mask_or_echo` | pass |
| AC11 | tests/create.rs `ac11_token_is_shown_once_and_no_command_reveals_it_again`, `ac11_token_that_cannot_be_shown_on_the_tty_leaves_nothing_stored`, `ac11_real_terminal_writes_the_token_to_dev_tty_only` | pass |
| AC12 | tests/keychain.rs `keychain_round_trips_a_dummy_mandate_and_deletes_it` (ignored by default; asserts add-only) | pass |

Extra: tests/sealing.rs `store_put_is_add_only` (in-memory store refuses a second put, keeps the original, accepts a put again after delete).

**Review findings:** round 2 had no blocking findings. The human note is addressed as above.
