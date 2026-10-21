## Round 1
**Changed:**
- New `src/export.rs`: `MandateFile` / `FileAccount` / `FileGrant` (serde, `deny_unknown_fields`, no Credential field), `to_toml`, `from_toml`, `load`. Untrusted input is parsed to a `toml::Table`, then checked key by key; problems name a key path (`accounts[1].tls`) or a line/column only, never a file value (the `toml` error `Display`/`message()` is not used). Rejects: unknown keys, wrong types, bad `tls`/`operation`, port 0 or out of range, no `[[accounts]]`, duplicate logins, Grants naming a login not in the file, `draft` Grants with mailboxes, files over 1 MiB, non-UTF-8, unreadable paths.
- `src/create.rs`: `run` loads `--from` after the TTY and sandbox checks and before any prompt. For each file Account it asks "Keep Account …?" (default yes), then host, port, TLS and login pre-filled (no Account-type prompt), then the hidden Credential and the live check with Retry/Abort. Mailbox checklist pre-ticks the union of the file's non-draft mailboxes (`*` = all) and shows an escaped "unavailable" message for each one the server lacks. Operation checklist pre-ticks the file's operations. If no file Account is kept, the normal flow starts. After a successful reveal, the export offer runs (default no), then the path prompt (default `agent-mail-<id>.toml`). The file is written with `create_new` (O_EXCL, so no overwrite and no symlink follow); an existing path re-asks, any other error is shown with path and reason and the run still exits 0.
- `src/terminal.rs`: `choose(.., default)` and `checklist(.., ticked)`; `InquireTerminal` uses `with_starting_cursor` / `MultiSelect::with_default`. `ScriptedTerminal` records `Entry.default` (text default, choice default label, confirm yes/no) and `Entry.pre_ticked`, and adds `Answer::Accept`.
- `src/core.rs`: `Command::MandateCreate { from }` and `CoreError::InvalidMandateFile { problem }` (`invalid_mandate_file`). `src/lib.rs`: `--from <FILE>` plus help text. Added the `toml` crate. `CLAUDE.md` layout lists `export.rs` and `tests/export.rs`.
- `tests/create.rs`: successful scripts now answer no to the export offer.

**Commits:** eed4054
**Tests:** `cargo test`: all suites pass, 100 passed, 0 failed, 1 ignored (keychain). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | tests/export.rs `ac1_accepting_the_export_offer_writes_name_accounts_and_grants` | pass |
| AC2 | tests/export.rs `ac2_the_exported_file_holds_no_credential_or_agent_token` | pass |
| AC3 | tests/export.rs `ac3_from_file_pre_selects_the_file_choices`, `ac3_from_file_choices_can_be_changed_before_confirming`, `ac3_a_file_account_can_be_dropped` | pass |
| AC4 | tests/export.rs `ac4_every_file_account_is_credential_checked`, `ac4_a_wrong_credential_from_a_file_session_can_be_retried`, `ac4_abort_after_a_wrong_credential_stores_nothing` | pass |
| AC5 | tests/export.rs `ac5_a_mailbox_missing_on_the_server_is_shown_unavailable` | pass |
| AC6 | tests/export.rs `ac6_malformed_files_are_refused_before_any_prompt` (cases a–i), `ac6_refusal_checks_take_precedence_over_file_errors` | pass |
| AC7 | tests/export.rs `ac7_declining_the_export_offer_writes_nothing`, `ac7_no_export_offer_when_the_store_write_or_reveal_fails` | pass |

Extra tests: `star_in_a_file_pre_ticks_every_mailbox`, `export_never_overwrites_or_follows_a_symlink`, `export_write_error_names_the_path_and_still_exits_zero`, `help_documents_from_and_still_no_credential_option`, unit tests in `src/export.rs` (round trip, no file value quoted in any problem message).

**Notes for reviewers:**
- AC7 checks the default path (`agent-mail-<id>.toml` in the test's current directory) instead of changing the process's current directory, because tests run in parallel threads and cwd is process-wide.
- Union pre-tick is implemented as the spec says (not rejected). The summary before confirmation shows the result.
- The `--from` help line avoids the words "credential" and "password" so the existing AC9 help check still holds; the command's long help says every Credential is still typed and live-checked.

**Review findings:** none (round 1).

## Round 2
**Changed:** (human feedback, human-notes.md 2026-10-21)
- The hidden Credential prompt now names the server: `Credential (password) for <login> on <host>:<port>:`, and the Gmail preset gets the same treatment. The host is escaped.
- A login ending in `@gmail.com` or `@googlemail.com` (any case) whose host is not `imap.gmail.com` gets a warning before the Credential prompt. The warning names the login, the actual `host:port` and `imap.gmail.com`. It applies to both the generic flow and `--from` Accounts. The Gmail preset never triggers it.
- Mandate file errors now give only the line and what is wrong there, and never repeat a key name from the file. Examples: `line 8: unknown key in an [[accounts]] entry`, `line 7: the TLS mode must be implicit or starttls`. Parsing now walks toml's spanned `DeTable`, so every value and key error has a line. Syntax errors still report line and column only. The strict serde types (`deny_unknown_fields`) still parse the file afterwards as a cross-check. A login is still named where it is the problem (AC6 f, h), escaped.
- `--from` is checked with `fs::metadata` (symlinks followed) and must be a regular file before it is opened, so a FIFO, device or directory is refused without opening it. The check is repeated on the opened handle in case the path was swapped in between.

**Commits:** 8a13f8a
**Tests:** `cargo test`: 104 passed, 0 failed, 1 ignored (keychain). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | tests/export.rs `ac1_accepting_the_export_offer_writes_name_accounts_and_grants` | pass |
| AC2 | tests/export.rs `ac2_the_exported_file_holds_no_credential_or_agent_token` | pass |
| AC3 | tests/export.rs `ac3_from_file_pre_selects_the_file_choices`, `ac3_from_file_choices_can_be_changed_before_confirming`, `ac3_a_file_account_can_be_dropped` | pass |
| AC4 | tests/export.rs `ac4_every_file_account_is_credential_checked`, `ac4_a_wrong_credential_from_a_file_session_can_be_retried`, `ac4_abort_after_a_wrong_credential_stores_nothing` | pass |
| AC5 | tests/export.rs `ac5_a_mailbox_missing_on_the_server_is_shown_unavailable` | pass |
| AC6 | tests/export.rs `ac6_malformed_files_are_refused_before_any_prompt` (a–i, now asserting the line number and that no file key name such as `password` is repeated), `ac6_refusal_checks_take_precedence_over_file_errors` | pass |
| AC7 | tests/export.rs `ac7_declining_the_export_offer_writes_nothing`, `ac7_no_export_offer_when_the_store_write_or_reveal_fails` | pass |
| Feedback | tests/export.rs `credential_prompt_names_the_server_host_and_port`, `gmail_login_on_another_host_is_warned_before_the_credential`, `from_must_name_a_regular_file_and_is_never_opened_otherwise` (directory and FIFO); unit test `create::tests::gmail_addresses_are_recognised` | pass |

**Review findings:** no reviewer findings this round. Human feedback is handled as above. One deviation from the spec: AC6(b) originally required the message to name `password`. Following the human note, it now gives `line 8: unknown key in an [[accounts]] entry` and the test asserts that `password` is absent.
