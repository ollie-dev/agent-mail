## Round 1
**Changed:**
- `src/lib.rs`: split every long `--help` description (root `after_help`, `search`, `read`, `flag`, `move`, `draft`, `whoami`, `mcp`, `mandate create`, `search --gmail`) into paragraphs of at most 300 characters, and removed repeats within a page: `read` no longer says the 20,000-character cut twice, `flag` names `seen`/`flagged` meanings only on the `--set`/`--clear` lines, `move --to` no longer repeats the grant rule, `draft` drops the sentences that repeated the `--to`, `--reply-to` and `--account` option lines, and `mandate create` says pre-filling once. The global `--text` line now has the exact AC3 wording. First lines (shown by `-h`) are unchanged.
- `src/core.rs`: `hardened_runtime_missing` now has the exact AC7 wording; `proxy_refused`, `unsupported`, the `Uncovered` `denied` message and `ambiguous_mandate` trimmed (same next steps, no new interpolated values).
- `.claude/agents/review-ui.md`: new "Look for" bullet for the CLAUDE.md help/error text convention.
- Tests: AC1-AC8 in `tests/cli.rs` (in-process via `agent_mail::run`), AC9 in `tests/project.rs`; `tests/selfcheck.rs` `assert_refused` now checks "refused to access the Keychain" instead of the removed "not accessed".
- Deviation from the spec's paragraph rule, for reviewers to check: the AC1 test treats blocks starting with `Options:` or `Arguments:` like `Commands:` (each line measured on its own). Clap's short `-h` page renders all options as one block without blank lines (e.g. `search -h` is one 866-character "paragraph"), so the rule as written cannot pass without hacking clap's layout. Help is not wrapped (no `wrap_help`), so each option's description is still one line and is still held to 300 characters.
- `draft --help` keeps "but not both" in the description, since `tests/draft.rs::help_says_subject_and_reply_to_cannot_be_combined` pins it.
- AC4 already held before this change (the substring appeared once); the test now guards it.

**Commits:** 9632ff3
**Tests:** `cargo test --no-fail-fast` → every binary `test result: ok`, 0 failed (cli: 20 passed); `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | tests/cli.rs `ac1_no_help_paragraph_is_longer_than_300_characters` | pass |
| AC2 | tests/cli.rs `ac2_help_keeps_the_facts_an_agent_depends_on` | pass |
| AC3 | tests/cli.rs `ac3_global_text_flag_help_line`, `ac3_text_before_or_after_whoami_prints_the_same` | pass |
| AC4 | tests/cli.rs `ac4_search_help_names_human_readable_output_once` | pass |
| AC5 | tests/cli.rs `ac5_every_error_message_is_at_most_300_characters` (exhaustive `match` in `error_samples`) | pass |
| AC6 | tests/cli.rs `ac6_proxy_refused_and_unsupported_keep_their_facts` | pass |
| AC7 | tests/cli.rs `ac7_hardened_runtime_missing_says_once_the_keychain_was_not_accessed` | pass |
| AC8 | tests/cli.rs `ac8_invalid_token_warning_is_at_most_300_characters` | pass |
| AC9 | tests/project.rs `ac9_ui_reviewer_checks_the_help_and_error_text_convention` | pass |
| AC10 | `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check` | pass |

**Review findings:** none (round 1).
