VERDICT: PASS

## Findings
- [follow-up] tests/cli.rs:169: `binary_appends_an_audit_entry_to_the_redirected_test_log` failed once in my first full `cargo test` run (audit entry count None, expected Some(1)) and passed in four later runs. This job did not touch that test or the audit code, so the flake is likely older; a later job should find the race.

## Checked
- Read: the full `git diff main...factory/019-help-error-text` (src/lib.rs doc comments and after_help, src/core.rs messages, .claude/agents/review-ui.md, tests/cli.rs test names and helpers, tests/project.rs, tests/selfcheck.rs); `src/core.rs`: `Warning::invalid_tokens`.
- Ran: `cargo test` (all suites green except the one flaky failure above; `cargo test --test cli` run 3 more times: 20 passed each; `cargo test --no-fail-fast`: all passed), `cargo clippy -- -D warnings` (clean), `cargo fmt --check` (clean).
- Criteria: AC1 `ac1_no_help_paragraph_is_longer_than_300_characters` (long paragraphs split in lib.rs, so it fails without the change); AC2 `ac2_help_keeps_the_facts_an_agent_depends_on`; AC3 `ac3_global_text_flag_help_line` and `ac3_text_before_or_after_whoami_prints_the_same`; AC4 `ac4_search_help_names_human_readable_output_once`; AC5 `ac5_every_error_message_is_at_most_300_characters` (exhaustive match); AC6 `ac6_proxy_refused_and_unsupported_keep_their_facts`; AC7 `ac7_hardened_runtime_missing_says_once_the_keychain_was_not_accessed` (selfcheck.rs assertion updated to match); AC8 `ac8_invalid_token_warning_is_at_most_300_characters` (the warning is already under 300, so this test guards rather than drives); AC9 `ac9_ui_reviewer_checks_the_help_and_error_text_convention` plus the new bullet; AC10 run as above.
- Scope: wording changes only. The trimmed messages keep their next steps (retry, `whoami`, ask the human).
