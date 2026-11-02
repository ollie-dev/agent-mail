APPROVE
All five criteria have passing tests, all four round-1 reviews pass with no fix-now findings, and the diff stays inside the picker and Provider list scope.

## Acceptance criteria
- AC1: pass (`tests/provider.rs` `job032_ac1_all_returns_the_35_providers_in_file_order`, `job032_ac1_unsupported_providers_carry_the_starting_reason_only`; `data/providers.json` is byte-identical to `.scratch/provider-list/providers.json`)
- AC2: pass (`tests/provider.rs` `ac3_the_built_in_list_follows_the_schema`, `job032_ac2_the_parser_enforces_the_unsupported_rules`)
- AC3: pass (`tests/create.rs` `job032_ac3_mail_provider_lists_unsupported_providers_by_name`; `tests/provider.rs` `job032_ac3_unsupported_labels_carry_the_suffix`, `job032_ac3_search_finds_unsupported_providers_by_name_and_domain`, plus the updated `ac4_sorted_is_case_insensitive_by_name` and `ac4_search_finds_by_name_and_every_domain`)
- AC4: pass (`tests/create.rs` `job032_ac4_unsupported_provider_shows_the_reason_and_asks_again`, `job032_ac4_another_provider_after_an_unsupported_one_completes`)
- AC5: pass (I ran `cargo test` in the worktree at dce838a: every suite green, 0 failed; `cargo clippy -- -D warnings` clean; `cargo fmt --check` clean)

## For the human
Nothing needs a human decision. There is no new credential path: `ask_provider` returns only supported Providers, through `Provider::connection()`, so an unsupported pick cannot reach the Credential prompt, the IMAP connector or the store. The cancel test checks this. No dependencies changed. The Mandate, the CLI, MCP and the Broker are untouched.

## Follow-ups
- [pattern] ui · data/providers.json / src/terminal.rs:285: with the suffix, the Outlook label is 78 characters, so with the picker prompt prefix it may wrap in an 80-column terminal. No one has looked at the 36-option picker on a real terminal yet (open F56). The same kind of finding (terminal output wider than about 80 columns) is logged in 030 (F55) and 031 (F56) → check the picker at 80 columns and shorten long labels or show fewer domains for them. Add a ui reviewer check, or a test, that terminal-facing lines and picker labels fit in 80 columns.
