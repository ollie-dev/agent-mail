## Round 1
**Changed:**
- New `src/move_message.rs` (`MoveResult`, `--text` line helper) and `move <REF> --to <MAILBOX>` subcommand in `src/lib.rs` (help text, `--to` rejects empty/CR/LF/NUL as usage error, JSON and `--text` rendering).
- `src/core.rs`: `Command::Move`, `Outcome::Moved`, new errors `same_mailbox` and `move_unsupported` (both covered by `every_error_message_names_a_next_step`); reference re-validated, same-mailbox check, then exact-login Account match and `move` Grant checks on source then target, all before any connection; audit entry with `target` and, on success, the reported `new_ref` in `messages`. `stale_reference`/`message_not_found` say "nothing was moved" for `move`.
- `src/audit.rs`: optional `target` key between `mailbox` and `messages`.
- `src/imap.rs`: `move_message` — `CAPABILITY` after login, `MoveUnsupported` before any `SELECT` if neither `MOVE` nor `UIDPLUS`; `SELECT`, UIDVALIDITY check, `UID FETCH <uid> (UID FLAGS)` existence check; then `UID MOVE`, or `UID COPY` + `UID STORE <uid> +FLAGS.SILENT (\Deleted)` + `UID EXPUNGE <uid>`. `COPYUID` read from untagged or tagged responses via `run_command`/`read_response` (accepted only when it names exactly the one source UID, sets never expanded). `NO` to move/copy maps to `mailbox_not_found` naming the target. Target sent as quoted modified UTF-7. `LOGOUT` only.
- Fake server (`tests/support/mod.rs`): `start_with_capabilities`, `UID MOVE` (MOVE only), `COPY`/`UID COPY`, `EXPUNGE`, `UID EXPUNGE` (UIDPLUS only), `COPYUID` codes, `NO [TRYCREATE]`, `messages_in`, `Filing` mailbox.
- README "Move" section and Audit log updates; CLAUDE.md layout names `src/move_message.rs` and `tests/move_message.rs`.

**Commits:** 97145c3

**Tests:** `cargo test` — all suites `test result: ok` (202 passed, 0 failed, 1 ignored keychain). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | `tests/move_message.rs::ac1_moving_leaves_the_message_in_archive_and_removes_it_from_inbox` | pass |
| AC2 | `tests/move_message.rs::ac2_move_granted_only_on_the_source_is_denied_and_audited` | pass |
| AC3 | `tests/move_message.rs::ac3_move_granted_only_on_the_target_is_denied` | pass |
| AC4 | `tests/move_message.rs::ac4_uidplus_fallback_moves_only_that_message` | pass |
| AC5 | `tests/move_message.rs::ac5_a_server_without_move_or_uidplus_is_refused` | pass |
| AC6 | `tests/move_message.rs::ac6_a_stale_reference_moves_nothing` | pass |
| AC7 | `tests/move_message.rs::ac7_the_result_returns_the_new_reference_where_reported` | pass |

Also: `moving_to_the_same_mailbox_fails_before_any_connection`, `a_missing_target_fails_with_mailbox_not_found`, `usage_errors_make_no_connection_and_no_audit_entry`, `move_help_explains_the_reference_grants_and_result`, unit tests in `src/move_message.rs` and `src/lib.rs::move_requires_a_reference_and_a_valid_target`.

**Review findings:** none (round 1).
