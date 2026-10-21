## Round 1
**Changed:**
- New `src/audit.rs`: the `Entry` type (only `time`, `mandate`, `operation`, `account`, `mailbox`, `messages`, `outcome`, `reason`, so it has no field that could hold a Credential or token) serialised as one JSON line. `AuditLog::append` creates the directory with mode 0700 and the file with mode 0600, opens the file in append mode and writes the line in one call. The path rule `log_path(Platform, home, xdg_data_home)` is pure. `platform_path()` reads home from `getpwuid_r` (new `libc` dependency for `cfg(unix)`), never from `$HOME`.
- `src/core.rs`: `Context` now carries an `AuditLog`. `Context::new` uses the platform path. `Command::List` records which Mandate was opened, which Account was chosen and whether the Policy granted the call, then appends one entry for every outcome. The call fails closed with the new error `audit_unavailable` if the append fails, and no mailboxes are returned (its message names a next step and is covered by `every_error_message_names_a_next_step`). Allowed calls that fail afterwards (login_failed, connection_failed and so on) are logged with `outcome: allowed` plus the reason.
- Test-only (behind `cfg(any(test, feature = "test-support"))`): `AuditLog::at`, `AuditLog::temporary`, `AuditLog::for_test_binary`, `Context::with_audit_log` and `Context::audit_log`. `Context::trusting_only` points at a fresh temporary file. In a test-support build the binary (`src/main.rs`) writes to `AGENT_MAIL_TEST_AUDIT_LOG` if it is set, otherwise to a temporary file. Without this, the existing `tests/cli.rs` binary tests that run `list` wrote to the developer's real `~/Library/Application Support/agent-mail/audit.log`. They did so during my first test runs, and I deleted that directory, which held only those test entries. `tests/mandates.rs` `run_on` now uses a temporary log as well.
- A `#[used]` marker `agent-mail-audit-redirect` was added to `TEST_ONLY_MARKERS` in `tests/project.rs`, so the release-binary check also proves that the audit redirect is not in a release build. `claude_md_stack_section_records_layout` now requires the layout line to name every `src/*.rs` and `tests/*.rs` file.
- `README.md`: new `## Audit log` section with the location, permissions, fail-closed behaviour and every field. `CLAUDE.md`: the layout line lists `audit.rs` and `tests/audit.rs`, and the test-only list mentions the audit log redirect.
- `tests/support/mod.rs`: new helpers `temp_audit_log`, `run_logged` and `audit_lines`.

**Commits:** 6e287f0, 0df4fa3
**Tests:** `cargo test` gives `test result: ok` for every target (lib 18, audit 9, cli 11, create 20, export 20, list 15, mandates 12, project 5, sealing 9; keychain 1 ignored). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | tests/audit.rs `ac1_successful_list_appends_one_allowed_entry` | pass |
| AC2 | tests/audit.rs `ac2_denied_list_appends_one_denied_entry_with_reason` | pass |
| AC3 | tests/audit.rs `ac3_unknown_tokens_are_logged_without_mandate_or_token` | pass |
| AC4 | tests/audit.rs `ac4_no_entry_holds_a_credential_token_or_hash` | pass |
| AC5 | tests/audit.rs `ac5_entries_are_appended_never_rewritten` (plus unit test `audit::tests::entry_keys_are_in_order_and_control_characters_are_escaped` for mailbox/message escaping) | pass |
| AC6 | tests/audit.rs `ac6_no_command_reads_or_returns_the_audit_log` | pass |
| AC7 | src/audit.rs `macos_path_is_under_application_support`, `other_unix_path_follows_xdg_data_home`; tests/audit.rs `ac7_readme_documents_location_and_fields` | pass |
| Notes: fail closed | tests/audit.rs `unwritable_audit_log_fails_closed_without_mailboxes` | pass |
| Notes: test isolation | tests/audit.rs `test_contexts_never_write_to_the_platform_log`; tests/cli.rs `binary_appends_an_audit_entry_to_the_redirected_test_log`; tests/project.rs `ac11_release_binary_has_no_test_only_code` | pass |
| Unix modes | src/audit.rs `first_write_creates_private_directory_and_file` | pass |

**Review findings:** none (round 1).

Notes for reviewers:
- If the directory already exists with a looser mode, it is not chmod-ed.
- A relative `XDG_DATA_HOME` is treated as unset, following the XDG spec.
- On non-Unix targets there is no home lookup yet, so `list` fails with `audit_unavailable` until the Windows milestone (ticket 18).
- If the audit append fails, the call returns `audit_unavailable` even when the call had already failed for another reason, such as `denied`.

## Round 2
**Changed:**
- `AuditLog::append` now opens the log directory with `O_NOFOLLOW | O_DIRECTORY` and sets it to `0700` with `fchmod` on the open descriptor, if its mode is anything else.
- `audit.log` is opened with `O_NOFOLLOW` (plus `O_NONBLOCK`, so a planted FIFO cannot hang the call). It is refused unless it is a regular file, and is set to `0600` with `fchmod` on the open descriptor if its mode is anything else.
- Any refusal makes the append fail, so `list` fails closed with `audit_unavailable`.
- `README.md` describes the tightening and the symlink refusal.

**Commits:** 31dcc43
**Tests:** `cargo test` gives `test result: ok` for every target (lib 22, audit 11, cli 11, create 20, export 20, list 15, mandates 12, project 5, sealing 9; keychain 1 ignored). Clippy (`-D warnings`) and `cargo fmt --check` are clean. A full run still leaves `~/Library/Application Support/agent-mail` absent.

| AC | Test | Status |
|----|------|--------|
| AC1 | tests/audit.rs `ac1_successful_list_appends_one_allowed_entry` | pass |
| AC2 | tests/audit.rs `ac2_denied_list_appends_one_denied_entry_with_reason` | pass |
| AC3 | tests/audit.rs `ac3_unknown_tokens_are_logged_without_mandate_or_token` | pass |
| AC4 | tests/audit.rs `ac4_no_entry_holds_a_credential_token_or_hash` | pass |
| AC5 | tests/audit.rs `ac5_entries_are_appended_never_rewritten` | pass |
| AC6 | tests/audit.rs `ac6_no_command_reads_or_returns_the_audit_log` | pass |
| AC7 | src/audit.rs `macos_path_is_under_application_support`, `other_unix_path_follows_xdg_data_home`; tests/audit.rs `ac7_readme_documents_location_and_fields` | pass |
| Human note: tighten existing modes | src/audit.rs `existing_loose_directory_and_file_are_tightened`; tests/audit.rs `list_tightens_an_existing_loose_log_directory_and_file` | pass |
| Human note: O_NOFOLLOW | src/audit.rs `symlinked_log_file_is_refused_and_its_target_untouched`, `symlinked_log_directory_is_refused_and_its_target_untouched`, `planted_fifo_is_refused_without_hanging`; tests/audit.rs `list_refuses_a_symlinked_log_and_fails_closed` | pass |

**Review findings:** Human note (2026-10-21) "Tighten modes on an existing audit dir and file to 0700/0600, and open audit.log with O_NOFOLLOW; add a test for each" was fixed as described under Changed, with the tests listed in the table. The symlink check covers the `agent-mail` directory itself, not its parents such as `~/Library/Application Support`.
