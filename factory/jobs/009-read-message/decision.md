APPROVE
All eight acceptance criteria have passing tests I ran, all four round-1 reviews pass with nits only, and the diff stays within the spec's files and scope.

## Acceptance criteria
- AC1: pass (tests/read.rs `ac1_plain_text_message_returns_headers_and_body`)
- AC2: pass (tests/read.rs `ac2_html_only_message_returns_readable_text`)
- AC3: pass (tests/read.rs `ac3_attachments_are_listed_without_content`)
- AC4: pass (tests/read.rs `ac4_reading_leaves_messages_unread`)
- AC5: pass (tests/read.rs `ac5_long_bodies_are_truncated_unless_full`)
- AC6: pass (tests/read.rs `ac6_stale_reference_fails_before_any_fetch`, src/core.rs `every_error_message_names_a_next_step`)
- AC7: pass (tests/read.rs `ac7_read_without_a_read_grant_is_denied_and_audited`, which covers cases a, b and c and asserts zero connections)
- AC8: pass (tests/read.rs `ac8_each_read_is_audited_with_its_reference_only`, which covers the stale, message_not_found and `not-a-ref` exit-2 cases)

I ran `cargo test` in the worktree at cc38ff6. Every suite passed: lib 43, read 10, and all other suites green, with the keychain test ignored as designed.

## Notes
- Scope: the diff touches only the files the spec lists. It also makes a one-line exhaustiveness change in `src/create.rs` (new `ImapError` variants map to the existing generic reason).
- The new dependency `mail-parser` 0.11.9 (plus its build-time proc-macro `hashify`) is named in the spec. It parses MIME only and does no I/O. It does not touch credentials or secrets.
- The builder added a third error code, `invalid_reference`. Only non-CLI adapters can reach it, because the core re-validates references as the spec requires. The code, UX and security reviews all accepted it, and it is additive, not breaking.
- Nits for follow-up: in the audit entry for `invalid_reference`, leave out `mailbox` and `messages`. Decide on handling bidi and C1 characters in JSON output when the MCP adapter lands (ticket 15). Use a placeholder name for unnamed attachments in `--text`. Mention in the README that a reference to an uncovered Account returns `denied`.
