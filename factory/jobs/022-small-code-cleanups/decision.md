APPROVE
All nine criteria are tested and pass, all four round-1 reviews pass with no fix-now findings, and the diff stays inside the five follow-ups the spec names.

## Acceptance criteria
- AC1: pass (tests/tokens.rs `ac1_at_most_eight_distinct_tokens_are_accepted`)
- AC2: pass (tests/tokens.rs `ac2_more_than_eight_distinct_tokens_is_too_many_tokens`)
- AC3: pass (tests/tokens.rs `ac3_the_cap_is_checked_before_any_store_lookup`)
- AC4: pass (tests/tokens.rs `ac4_whoami_applies_the_same_cap_and_is_not_audited`; tests/mcp.rs `job022_ac4_too_many_tokens_matches_the_cli`)
- AC5: pass (tests/mcp.rs `job022_ac5_draft_rejects_an_invalid_recipient_without_audit_or_echo`)
- AC6: pass (tests/tokens.rs `ac5_every_agent_command_warns_and_still_succeeds`)
- AC7: pass (tests/cli.rs `job022_ac7_run_cli_on_a_parsed_cli_matches_run`)
- AC8: pass (tests/cli.rs `job022_ac8_main_parses_its_arguments_once`; existing binary tests, including `ac7_the_binary_serves_mcp_on_stdio`)
- AC9: pass (tests/project.rs `job022_ac9_the_guarded_keys_method_is_gone`; `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` all clean at 3d7b76a)

## For the human
Nothing here needs a decision from you. The one judgement call is the cap of 8 distinct tokens in `AGENT_MAIL_TOKEN`. The spec chose that number, and no reviewer objected. Anyone who sets more than 8 tokens now gets `too_many_tokens` where they used to get one Keychain lookup per token. The cap is checked before any store call. The new error and its audit line hold no token data. In `main.rs`, the test-support audit redirect is still set before parsing. Removing the guarded `keys()` leaves no unguarded store path. There are no new dependencies.

## Follow-ups
- [pattern] ui · src/core.rs:912: the `too_many_tokens` message names `AGENT_MAIL_TOKEN` twice ("...more than 8 distinct tokens; set AGENT_MAIL_TOKEN to at most 8 tokens"). The same kind of finding (wordy or repetitive error and help text) is logged in 012 (F3), 015 (F9), 016 (F13), 017 (F17) and 018 (F22) → trim it to "AGENT_MAIL_TOKEN holds more than 8 distinct tokens; keep at most 8". The CLAUDE.md "say each thing once" convention already exists, so the ui reviewer's checklist should log a breach of it as `[fix-now]`, not `[follow-up]`.
- ux · README.md:114: "every command fails with `too_many_tokens`" is too broad, because `mandate` and other operator commands do not read the token → change it to "every agent command".
