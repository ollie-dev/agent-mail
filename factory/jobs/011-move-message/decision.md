APPROVE
All seven criteria have passing tests, all four round-1 reviews pass with only nits, and the diff stays inside the spec's scope with no new dependencies or credential handling.

## Acceptance criteria
- AC1: pass (tests/move_message.rs::ac1_moving_leaves_the_message_in_archive_and_removes_it_from_inbox)
- AC2: pass (tests/move_message.rs::ac2_move_granted_only_on_the_source_is_denied_and_audited; also covers the empty Policy and the uncovered Account)
- AC3: pass (tests/move_message.rs::ac3_move_granted_only_on_the_target_is_denied)
- AC4: pass (tests/move_message.rs::ac4_uidplus_fallback_moves_only_that_message)
- AC5: pass (tests/move_message.rs::ac5_a_server_without_move_or_uidplus_is_refused)
- AC6: pass (tests/move_message.rs::ac6_a_stale_reference_moves_nothing; also covers the missing UID case)
- AC7: pass (tests/move_message.rs::ac7_the_result_returns_the_new_reference_where_reported)

I ran `cargo test` in the worktree: every suite passed (tests/move_message.rs 11 passed; the keychain test is ignored, as it is meant to be). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

## For the human
Nothing here is blocking. This is the first command that removes a message from a mailbox. The spec settled the risky choices, and the tests check them:
- Both Grants are checked before any connection.
- Without MOVE or UIDPLUS the call is refused before `SELECT`.
- The fallback runs `UID EXPUNGE` on that one UID only.
- `CLOSE` and plain `EXPUNGE` are never sent.

The spec puts rollback of a half-finished fallback out of scope: if the copy succeeds and the store or expunge then fails, a duplicate can remain. Cargo.toml and Cargo.lock are unchanged. The only file outside the spec's list is a two-line match-arm addition in src/create.rs, needed because two new `ImapError` variants were added.

Possible follow-ups (security-review nits):
- The core accepts an empty `--to` target; only the CLI rejects it. A future MCP adapter would pass `""` to the server, which refuses it as `mailbox_not_found`, so nothing is harmed.
- Every `NO` to `UID MOVE` or `UID COPY` maps to `mailbox_not_found`, not only `[TRYCREATE]`. This fails closed, but the error says less than it could.
