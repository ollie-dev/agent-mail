APPROVE
All eight criteria have passing tests that I ran, all four round-1 reviews pass, the diff stays in scope, and the nit commit only adds "but not both" to the draft help.

## Acceptance criteria
- AC1: pass (`ac1_draft_is_appended_with_the_draft_flag_to_the_drafts_mailbox`)
- AC2: pass (`ac2_drafts_mailbox_is_found_by_special_use_not_by_name`). The `ref` it asserts is the percent-encoded `agent@example.test/%5BGmail%5D%2FEntw%C3%BCrfe/1700000003/1` from `MessageRef::format`, not the spec's literal string. The spec's string cannot be parsed back, which breaks AC2's own "works as a reference" bullet. The code and UX reviewers accepted this, and the test checks that `read` with the new ref returns `Status`.
- AC3: pass (`ac3_no_drafts_mailbox_fails_with_a_structured_error_and_creates_nothing`)
- AC4: pass (`ac4_reply_draft_threads_with_the_original_and_prefixes_re_once`)
- AC5: pass (`ac5_reply_needs_read_on_the_original_mailbox`)
- AC6: pass (`ac6_without_a_draft_grant_the_call_is_denied_and_audited`)
- AC7: pass (`ac7_no_command_edits_or_deletes_a_draft`, plus the `Drop for Setup` check for forbidden verbs in every test)
- AC8: pass (`ac8_audit_entry_records_recipients_and_references_only`)
- Other behaviour: pass (`header_injection_values_are_refused_by_the_cli_and_the_core`, `non_ascii_subjects_and_long_bodies_are_encoded`, `stale_or_missing_reply_references_create_nothing`)

Verified in the worktree: `cargo test` passes every suite with 0 failures (tests/draft.rs 11/11, lib 59/59). `cargo clippy -- -D warnings` and `cargo fmt --check` are clean.

## For the human
Nothing needs your decision. No new dependency: the CSPRNG is `ring`, already in use, and `Cargo.toml`/`Cargo.lock` are unchanged. Credential handling and storage are unchanged. Security review passed with no findings or caveats. The one change outside the listed files, `src/create.rs`, adds the new `ImapError::DraftsNotFound` variant to an exhaustive match. For reference, these are the builder's deviations that reviewers accepted:
- the AC2 ref format, explained above;
- `recipients: []` when the core rejects a `--to` value;
- the stricter login check for `From:`;
- extra RFC 2047 encoding for subjects that contain `=?` or are longer than 900 characters.

## Follow-ups
- code · src/core.rs:973: when the core gets an invalid `--to`, the audit entry logs `"recipients":[]` (a deliberate deviation) → when the MCP `draft` tool lands (ticket 15), confirm this is the behaviour wanted for rejected input from that adapter.
- code · src/imap.rs:856: an APPEND the server refuses (`NO [TRYCREATE]`, `NO`, `BAD`) becomes a generic `Protocol`/`server_error` → map these to a clearer error that says no draft was created and names a next step.
- ui · src/lib.rs:112: the `draft --help` long description is one unwrapped paragraph of about 980 characters, the same style as other subcommands → wrap or split long help text across all commands in one pass.
- ux · src/lib.rs:226: usage errors from clap value parsers (bad `--to`, bad `--reply-to`) are plain text on stderr, not the JSON error shape the core uses; the other commands behave the same → make usage errors machine-readable for agents across all commands.
