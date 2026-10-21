APPROVE
All four round 2 reviews pass, every AC has a test that I saw pass, and the human's round 1 note (tighten existing modes, O_NOFOLLOW, a test for each) is done.

## Acceptance criteria
- AC1: pass (tests/audit.rs `ac1_successful_list_appends_one_allowed_entry`)
- AC2: pass (tests/audit.rs `ac2_denied_list_appends_one_denied_entry_with_reason`)
- AC3: pass (tests/audit.rs `ac3_unknown_tokens_are_logged_without_mandate_or_token`)
- AC4: pass (tests/audit.rs `ac4_no_entry_holds_a_credential_token_or_hash`)
- AC5: pass (tests/audit.rs `ac5_entries_are_appended_never_rewritten`, including the `evil\n...\r@example.test` login; src/audit.rs `entry_keys_are_in_order_and_control_characters_are_escaped`)
- AC6: pass (tests/audit.rs `ac6_no_command_reads_or_returns_the_audit_log`; `audit` and `log` exit 2 in JSON and `--text` mode)
- AC7: pass (src/audit.rs `macos_path_is_under_application_support`, `other_unix_path_follows_xdg_data_home`; tests/audit.rs `ac7_readme_documents_location_and_fields`)
- Fail-closed `audit_unavailable` (spec Notes): pass (tests/audit.rs `unwritable_audit_log_fails_closed_without_mailboxes`)
- Test isolation (spec Notes): pass (tests/audit.rs `test_contexts_never_write_to_the_platform_log`; tests/cli.rs `binary_appends_an_audit_entry_to_the_redirected_test_log`; tests/project.rs `ac11_release_binary_has_no_test_only_code`)
- Human note, tighten existing modes: pass (src/audit.rs `existing_loose_directory_and_file_are_tightened`; tests/audit.rs `list_tightens_an_existing_loose_log_directory_and_file`)
- Human note, O_NOFOLLOW: pass (src/audit.rs `symlinked_log_file_is_refused_and_its_target_untouched`, `symlinked_log_directory_is_refused_and_its_target_untouched`, `planted_fifo_is_refused_without_hanging`; tests/audit.rs `list_refuses_a_symlinked_log_and_fails_closed`)

I ran `cargo test` in the worktree: every target passed (lib 22, audit 11, cli 11, create 20, export 20, list 15, mandates 12, project 5, sealing 9; keychain 1 ignored). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` were clean. `~/Library/Application Support/agent-mail` did not exist before or after the run. The diff touches only the files the spec lists. The one new dependency is `libc` (unix only, for `getpwuid_r`/`getuid` and the open flags). It does not handle secrets, and you saw it in round 1.

## For the human
Nothing here needs your decision. These are left for follow-up tickets:
- Security nit (round 2): the directory check and the file open are two separate path lookups, so a process running as the same user could swap `agent-mail/` for a symlink between them. That process could already write the target itself, and the spec treats same-user tampering as a known limit and out of scope. The fix would be to open `audit.log` with `openat` on the directory descriptor that is already held.
- If the append fails, `audit_unavailable` replaces the original result (for example `denied`). This is the fail-closed behaviour the spec asked for.
- UX nits for the README: say that `mandate create/list/delete` are not logged yet, say that the original result is withheld on `audit_unavailable`, and tell the human what to do about a refused symlink.
- `mandate delete` (token revocation) is not logged. The spec leaves it to a follow-up ticket.
- Test hygiene: `agent-mail-test-*` temporary directories accumulate in the system temp dir. They hold metadata only.
