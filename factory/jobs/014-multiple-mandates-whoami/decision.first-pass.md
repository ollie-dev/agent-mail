APPROVE
All four round-1 reviews pass with no fix-now findings, every criterion is covered by a passing test in tests/tokens.rs, and the diff stays inside the spec.

## Acceptance criteria
- AC1: pass (`tokens::ac1_account_selects_the_mandate_covering_it`, `tokens::ac1_unacceptable_account_values_are_usage_errors`, `tokens::ac1_core_refuses_unacceptable_account_values_from_other_adapters`)
- AC2: pass (`tokens::ac2_account_may_be_omitted_only_when_one_account_is_reachable`)
- AC3: pass (`tokens::ac3_two_mandates_covering_one_account_are_ambiguous`, `tokens::ac3_ambiguity_is_reported_even_when_only_one_mandate_grants`)
- AC4: pass (`tokens::ac4_grants_never_combine_across_mandates`)
- AC5: pass (`tokens::ac5_invalid_tokens_are_reported_without_blocking_valid_ones`, `tokens::ac5_only_invalid_or_no_tokens_are_the_existing_errors`, `tokens::ac5_every_agent_command_warns_and_still_succeeds`)
- AC6: pass (`tokens::ac6_whoami_shows_each_presented_mandate_and_never_secrets`, `tokens::ac6_whoami_lists_ambiguous_mandates_and_handles_bad_tokens`, `tokens::ac6_whoami_text_output`)
- AC7: pass (`tokens::ac7_readme_and_help_document_several_tokens_and_whoami`)

I ran it myself: `cargo test` passed every suite (tokens 14/14, 2 opt-in tests ignored), `cargo clippy --all-targets -- -D warnings` was clean and `cargo fmt --check` was clean.

## For the human
Nothing needs a human decision. Mandate selection decides which Credential a call uses. Security review passed it without caveats: `Presented::select` matches the login exactly, refuses with `ambiguous_mandate` before any Grant check or connection, and denies an uncovered Account before connecting. No new dependencies, no new way of storing credentials, and invalid tokens are reported by position only. Two small, intended behaviour changes: the `account_required` message is reworded, and audit lines for references to an Account that no presented Mandate covers no longer carry `mandate` (AC4 requires this). Error objects keep sorted key order (`account, code, mandates, message`), not the order in the spec's example. This follows the existing convention.

## Follow-ups
- ui · src/lib.rs:70: the global `--text` help still says "(for `search`, put it before the subcommand)", though `whoami` and `list` take it after the subcommand → reword the `--text` help so it is right for every subcommand.
