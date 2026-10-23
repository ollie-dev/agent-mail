APPROVE
Only help text and error wording change, every AC has a passing test, and all four round-1 reviews pass with no fix-now findings.

## Acceptance criteria
- AC1: pass (tests/cli.rs `ac1_no_help_paragraph_is_longer_than_300_characters`, 26 invocations). The test also treats blocks that start with `Options:` or `Arguments:` as lists, which the spec only does for `Commands:`. The spec's rule cannot pass as written, because clap's short `-h` page prints all options as one block with no blank lines. Help text is not wrapped, so each option description is still one line held to 300 characters. I checked every page with the binary: the longest line is 261 characters (`mcp --help`).
- AC2: pass (`ac2_help_keeps_the_facts_an_agent_depends_on`)
- AC3: pass (`ac3_global_text_flag_help_line`, `ac3_text_before_or_after_whoami_prints_the_same`)
- AC4: pass (`ac4_search_help_names_human_readable_output_once`)
- AC5: pass (`ac5_every_error_message_is_at_most_300_characters`; `error_samples` uses an exhaustive `match` with no `_` arm)
- AC6: pass (`ac6_proxy_refused_and_unsupported_keep_their_facts`)
- AC7: pass (`ac7_hardened_runtime_missing_says_once_the_keychain_was_not_accessed`; the tests/selfcheck.rs `assert_refused` check was updated to the new wording)
- AC8: pass (`ac8_invalid_token_warning_is_at_most_300_characters`)
- AC9: pass (tests/project.rs `ac9_ui_reviewer_checks_the_help_and_error_text_convention`)
- AC10: pass. I ran `cargo test` in the worktree at 9632ff3 and every suite passed (cli 20, project 6, selfcheck 11 + 4 ignored). `cargo clippy -- -D warnings` and `cargo fmt --check` were both clean.

## For the human
Nothing needs a decision. The change only rewords clap doc comments and five `CoreError` messages, adds tests and adds one ui-reviewer bullet. It has no new dependencies and no behaviour or code changes. The trimmed messages show the same values as before (account, mailbox, host, port, Mandate IDs) and never a token. Only one thing differs from the spec, and it is already accepted above: the AC1 test treats the `Options:`/`Arguments:` blocks as lists (see AC1).

## Follow-ups
- code · tests/cli.rs:169: `binary_appends_an_audit_entry_to_the_redirected_test_log` failed once (audit entry count None, expected Some(1)) and passed on reruns. It looks like an older race unrelated to this job → find and fix the race in the audit-log test or its redirect.
- ux · src/lib.rs:46: the root `-h` prints the full three-paragraph `after_help`, the same as `--help`, so the short form is not shorter at the root → move the account/Mandate explanation into the long help only (for example `after_long_help`).
- [pattern] decider · src/mcp.rs (tool descriptions and server instructions): several strings run 300 to 740 characters, and this spec left them out of scope. The same kind of finding (overlong help, error or doc text) is logged in 012 (F3), 015 (F9), 016 (F13), 017 (F17) and 018 (F22) → shorten them, or exempt them explicitly in the CLAUDE.md convention. Either way, add a test that holds every MCP description to the same limit, so new tools cannot regress.
