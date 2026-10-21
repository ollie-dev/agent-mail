APPROVE
All six criteria have passing tests I ran, all four round-1 reviews say PASS, there are no new dependencies or credential changes, and the two spec deviations were forced by contradictions in the spec and are documented.

## Acceptance criteria
- AC1: pass (`tests/search.rs::ac1_search_by_from_returns_matching_messages_newest_first`)
- AC2: pass (`tests/search.rs::ac2_each_criterion_narrows_and_criteria_combine_with_and`)
- AC3: pass (`tests/search.rs::ac3_default_limit_is_20_and_limit_adjusts_it`, `ac3_bad_limits_are_usage_errors_without_a_connection`)
- AC4: pass (`tests/search.rs::ac4_search_outside_the_granted_mailbox_is_denied`, `ac4_search_without_a_search_grant_is_denied`, `ac4_empty_policy_denies_the_default_mailbox`, `ac4_list_denial_still_has_no_mailbox_key`; `src/core.rs::every_error_message_names_a_next_step`)
- AC5: pass (`tests/search.rs::ac5_search_appends_one_entry_with_references_and_count`, `ac5_no_matches_default_limit_and_missing_mailbox_are_logged`, `ac5_log_never_holds_subjects_bodies_or_secrets`)
- AC6: pass (`tests/search.rs::ac6_references_carry_uidvalidity_and_uid`; `src/search.rs::reference_round_trips_and_encodes_separators`, `malformed_references_are_refused`)

Verified in the worktree: `cargo test` passed (159 passed, 0 failed, 1 ignored: the opt-in keychain test). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean. The diff touches only the files the spec lists, plus a 4-line change in `src/create.rs` and 1 line in `tests/audit.rs` for the new audit field. `Cargo.toml` and `Cargo.lock` are unchanged.

## For the human
Nothing here needs your decision, but two spec deviations are worth knowing about:
- **`--text` is no longer global.** clap cannot have both a global `--text` flag and the `search --text S` criterion. `list` and `mandate` now each declare their own `--text`, so every position that worked before still works (`text_flag_still_works_after_list_and_mandate`). For search, human-readable output is `agent-mail --text search ...`. A bare `search ... --text` at the end is a clap usage error. Nothing breaks for existing commands. When the skill or MCP (ticket 15) is next touched, document where `--text` goes, as the UX review suggests.
- **Message reference encoding.** The spec's prose rule contradicts its own example and AC6 (`[`/`]` encoded). The builder followed the ACs and used RFC 3986 `pchar` encoding. Parsing is strict and only accepts the canonical form. Tickets 08 to 10 will depend on this format, so if you want the prose rule instead, change it before those tickets start.
- Optional hardening nits from the security review: also refuse other C0 controls in criteria, and quote mailbox names in error messages.
