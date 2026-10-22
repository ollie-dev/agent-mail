APPROVE
All seven criteria have passing tests I ran, all four round-1 reviews pass, and the one UX [fix-now] was fixed in nit pass 1, then verified and re-checked.

## Acceptance criteria
- AC1: pass (tokens::ac1_account_selects_the_mandate_covering_it, tokens::ac1_unacceptable_account_values_are_usage_errors, tokens::ac1_core_refuses_unacceptable_account_values_from_other_adapters)
- AC2: pass (tokens::ac2_account_may_be_omitted_only_when_one_account_is_reachable)
- AC3: pass (tokens::ac3_two_mandates_covering_one_account_are_ambiguous, tokens::ac3_ambiguity_is_reported_even_when_only_one_mandate_grants)
- AC4: pass (tokens::ac4_grants_never_combine_across_mandates, tokens::ac4_uncovered_account_denial_points_to_whoami_not_to_a_grant)
- AC5: pass (tokens::ac5_invalid_tokens_are_reported_without_blocking_valid_ones, tokens::ac5_only_invalid_or_no_tokens_are_the_existing_errors, tokens::ac5_every_agent_command_warns_and_still_succeeds)
- AC6: pass (tokens::ac6_whoami_shows_each_presented_mandate_and_never_secrets, tokens::ac6_whoami_lists_ambiguous_mandates_and_handles_bad_tokens, tokens::ac6_whoami_text_output)
- AC7: pass (tokens::ac7_readme_and_help_document_several_tokens_and_whoami)

I ran `cargo test` in the worktree. Every suite passed (tokens: 15 passed) with 0 failures; the only skipped tests are the 2 opt-in ignored ones. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are both clean.

Review state:
- All four round-1 reviews (code, ux, security, ui) say `VERDICT: PASS`. For code and ux I used the redone reviews, not the first-pass files.
- The one `[fix-now]` was from ux: the uncovered-Account denial pointed the Agent to a Grant.
  - build.md has a `## Nit pass 1` section for it, and `round-1/verify-ux-1.md` starts with `VERIFIED`.
  - I checked nit commit 9fc3dda line by line. It adds only `CoreError::Uncovered` (code `denied`, same fields, new message), the `uncovered()` helper used at each empty-selection site, two cases in `every_error_message_names_a_next_step`, one README sentence and the new test.
  - I ran the Done-when check (`cargo test --test tokens ac4_uncovered`) and it passes. It covers list, search, draft --account, read, flag, move and draft --reply-to, and confirms that Grant-missing denials keep "ask the human to grant it".

Scope and security:
- The diff touches only files the spec lists. Cargo.toml and Cargo.lock are unchanged, so there are no new dependencies.
- How credentials are stored is unchanged. Invalid tokens are reported by position only, never by their text.

## For the human
Nothing needs your decision. Two behaviour changes the spec asked for are worth knowing about:
- Audit lines for `read`, `flag` and `move` on an Account that no presented Mandate covers no longer carry a `mandate` key (AC4).
- `ambiguous_mandate` serialises its keys in sorted order (`account, code, mandates, message`), not in the order of the spec's example. This matches the existing error convention, and both the code and UX reviewers accepted it.

## Follow-ups
- code · src/core.rs:1402: each presented token costs one store lookup (one Keychain prompt per token on macOS), and the number of tokens has no cap → add a limit on presented tokens in `AGENT_MAIL_TOKEN`.
- code · tests/tokens.rs:578: `ac5_every_agent_command_warns_and_still_succeeds` checks the warning and exit code for each command, but not which Mandate the audit line names → also assert the audit `mandate` for each command.
- ui · src/lib.rs:52: the global `--text` help still says "for `search`, put it before the subcommand", although `whoami` and `list` accept it after the subcommand → make the global-versus-subcommand `--text` wording consistent across commands.
