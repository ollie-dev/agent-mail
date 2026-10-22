APPROVE
All six criteria have passing tests, all four round-1 reviews pass with nits only, and the diff stays within the spec's scope with no new dependencies or credential paths.

## Acceptance criteria
- AC1: pass (tests/flag.rs::ac1_marking_read_sets_and_unread_clears_seen)
- AC2: pass (tests/flag.rs::ac2_starring_sets_and_unstarring_clears_flagged)
- AC3: pass (tests/flag.rs::ac3_any_other_flag_is_rejected_with_a_structured_error, core::tests::every_error_message_names_a_next_step, flag::tests::parses_only_seen_and_flagged)
- AC4: pass (tests/flag.rs::ac4_flagging_without_a_flag_grant_is_denied_and_audited)
- AC5: pass (tests/flag.rs::ac5_stale_reference_changes_no_flag)
- AC6: pass (tests/flag.rs::ac6_each_change_is_audited_with_its_reference_and_change)

I ran it myself in the worktree on commit 99214f1. `cargo test` passed every suite (tests/flag.rs: 7 passed; 1 ignored keychain test). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` were both clean.

## Reviews (round 1)
- code: PASS (nits: the `nothing_done` wording assumes every non-read operation is flag; the flag name is parsed twice to build the audit `change`)
- security: PASS (nits: if a store's untagged FETCH has no FLAGS item, the connector reports empty flags when it could fetch them again; the same double parse)
- ui: PASS (nit: the long help paragraph is dense)
- ux: PASS (nits: optional wording tweaks)

## Notes
- Scope: the diff touches only the files the spec lists. `InvalidReference` also gained an `operation` field. The spec did not ask for this, but it fits the same pattern and changes no output for `read`.
- No new dependencies (Cargo.toml and Cargo.lock unchanged) and no new paths where credentials are stored or exposed. The flag name is mapped to the fixed literals `\Seen` or `\Flagged` before any connection, and the Policy check runs before connecting.
- Follow-up worth a ticket: the security reviewer's point about a store FETCH with no FLAGS item. If the server's reply after the change has no flag list, the connector reports "no flags" instead of asking again, so the returned flags could be wrong. Nothing on the mailbox is changed wrongly.
