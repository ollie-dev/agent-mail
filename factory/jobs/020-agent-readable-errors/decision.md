APPROVE
All 15 criteria have passing tests, all four round-1 reviews pass, and the one shared ui/ux fix-now was fixed in a minimal nit commit and verified.

## Acceptance criteria
- AC1: pass (tests/draft.rs `job020_ac1_trycreate_on_the_drafts_mailbox_fails_with_draft_refused`)
- AC2: pass (tests/draft.rs `job020_ac2_other_append_refusals_fail_with_draft_refused`)
- AC3: pass (tests/draft.rs `job020_ac3_a_refused_draft_is_audited_once_without_secrets`)
- AC4: pass (tests/mcp.rs `job020_ac4_draft_refused_matches_the_cli`)
- AC5: pass (tests/cli.rs `job020_ac5_an_invalid_draft_recipient_is_a_json_usage_error`)
- AC6: pass (tests/cli.rs `job020_ac6_every_command_reports_usage_errors_as_json`)
- AC7: pass (tests/cli.rs `job020_ac7_usage_errors_never_echo_the_callers_values`)
- AC8: pass (tests/cli.rs `job020_ac8_help_and_version_keep_clap_plain_text`)
- AC9: pass (tests/cli.rs `job020_ac9_a_usage_error_reads_no_store_connects_nowhere_and_audits_nothing`)
- AC10: pass (tests/cli.rs `job020_ac10_in_process_mcp_is_refused_as_json_or_text`)
- AC11: pass (tests/mcp.rs `job020_ac11_missing_token_also_reports_argument_mistakes`)
- AC12: pass (tests/mcp.rs `job020_ac12_missing_token_alone_keeps_its_message`)
- AC13: pass (tests/mcp.rs `job020_ac13_token_like_arguments_are_named_never_echoed`)
- AC14: pass (tests/plugin.rs `job020_ac14_readme_and_skill_document_draft_refused_and_json_usage_errors`)
- AC15: pass (I ran `cargo test` in the worktree: every suite `ok`, 0 failed. `cargo clippy -- -D warnings` and `cargo fmt --check` are clean.)

Nit pass: commit d6c7762 changes only the `InvalidSubcommand` arm of `usage_message` in src/lib.rs and adds the matching assertions in tests/cli.rs. verify-ui-1.md and verify-ux-1.md both start with `VERIFIED`. I ran the Done-when checks myself. `agent-mail lsit` now prints "unknown command (did you mean `list`?); run `agent-mail --help` for usage", and `agent-mail mandate delet` prints the same with `delete` and `agent-mail mandate --help`. Each has exactly one `;` and no `?;`, and both exit with code 2.

## For the human
Nothing here needs a human call. The diff stays inside the spec. The `src/create.rs` arm and the test updates in tests/audit.rs and tests/create.rs follow from the new variant and the new usage-error text. No dependency was added, and nothing changes how credentials are stored. One change is visible to users: CLI usage errors are now JSON instead of clap's plain text, also with `--text`. The spec asks for this explicitly (F4), and the exit code stays 2. The security review found that it echoes fewer caller values than before.

This job addresses follow-ups F2, F4, F14 and F15, so those can be ticked off in the ledger.

## Follow-ups
- code · README.md (audit `reason` bullet): the list of example reasons was cut to stay under the 300-character paragraph limit, so codes such as `invalid_token`, `stale_reference`, `same_mailbox` and `invalid_draft` are no longer listed → restore the full list of reason codes as a bulleted sub-list or a short table.
