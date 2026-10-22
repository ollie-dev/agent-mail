APPROVE
All six criteria have passing tests and all four round-1 reviews pass; both [fix-now] nits are fixed, verified, and I re-ran their checks myself.

## Acceptance criteria
- AC1: pass (tests/search.rs `ac1_gmail_query_is_sent_unchanged_as_x_gm_raw_and_decides_the_result`, `ac1_gmail_query_is_quoted_and_marked_utf8`, `ac1_bad_gmail_queries_and_combinations_are_usage_errors_without_a_connection`; src/search.rs `gmail_queries_are_sent_raw_and_quoted`)
- AC2: pass (tests/search.rs `ac2_gmail_on_a_server_without_x_gm_ext_1_is_unsupported`; src/core.rs `every_error_message_names_a_next_step`)
- AC3: pass (tests/search.rs `ac3_gmail_outside_the_granted_mailbox_is_denied`, `ac3_gmail_without_a_search_grant_is_denied`, `ac3_gmail_respects_the_limit`, `ac3_gmail_audit_lines_match_standard_search_and_never_hold_the_query`)
- AC4: pass as the spec defines it (tests/gmail_live.rs `gmail_live_round_trip` exists, is `#[ignore]`d and shows as ignored in `cargo test`; `live_round_trip_is_ignored_by_default`; tests/draft.rs `drafts_mailbox_returns_the_first_listed_drafts_mailbox_or_drafts_not_found`). Nobody has run the live round trip against a real Gmail account yet. The spec says only a developer with one can.
- AC5: pass (tests/gmail_live.rs `live_config_without_either_variable_skips_with_a_clear_message`, `live_config_without_the_password_names_it_and_hides_the_address`, `live_config_without_the_address_names_it`, `live_config_holds_the_password_as_a_redacted_credential`). I read the file: no `{:?}` of a RunOutput, Mandate, Account or config. Output is checked for leaked secrets before any assert that could print it.
- AC6: pass (tests/search.rs `ac6_readme_and_help_document_gmail_search_and_the_live_test`)

I ran `cargo test` in the worktree at a86c251: every binary passed, 0 failed (search 30 passed; gmail_live 5 passed, 1 ignored; keychain 1 ignored).

Nit pass (a86c251), read line by line: it changes only the `--gmail` and `--mailbox` help text in src/lib.rs, the README audit `reason` bullet, and regression asserts in the AC6 test. Both verify files start with `VERIFIED`. I re-ran each Done-when check:
- `search -h`: the `--gmail` line and the longest line are both 94 characters.
- `search --help` names X-GM-EXT-1, `unsupported`, the eight excluded criteria, and says --mailbox and --limit work.
- README.md:165, the `reason` bullet, ends with `unsupported`.

Scope: the diff touches only the files the spec lists, plus a one-line `failure_reason` entry in src/create.rs that the new variant needs. No new dependency; Cargo.toml and Cargo.lock are unchanged.

## For the human
Nothing here needs your call. Security passed with no caveats, and the only real Credential handling is the opt-in live test, which the spec designed this way.

One step is still open: no one has run the live test against a real Gmail account. To run it, export `AGENT_MAIL_LIVE_GMAIL_ADDRESS` and `AGENT_MAIL_LIVE_GMAIL_APP_PASSWORD` for a dedicated test account (INBOX not empty, at least one unread message). Then run `cargo test --test gmail_live -- --ignored`.

If it fails, open a rework job. Likely causes are Gmail's capability timing or the name of the Drafts mailbox. The merge does not depend on this run.

## Follow-ups
None.
