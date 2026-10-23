VERDICT: PASS

## Findings
- [follow-up] README.md (audit `reason` list): the list of reason examples was shortened (dropped `invalid_token`, `stale_reference`, `same_mailbox`, `invalid_draft` and others) to stay under the paragraph length; a later job could restore a full list as bullets.

## Checked
- Read: full `git diff main...factory/020-agent-readable-errors` (src/imap.rs `append`, src/core.rs `DraftRefused`/`connector_error`/`draft`, src/lib.rs `run`/`usage_message`/`subcommand_path`, src/mcp.rs `call_tool`/`tool_error`, tests/support `refuse_append`, README, SKILL.md, test changes in audit/create/cli/draft/mcp/plugin).
- Ran: `cargo test` all suites green, 0 failed; `cargo clippy -- -D warnings` clean; `cargo fmt --check` clean.
- Criteria: AC1-AC4 covered by job020_ac1..ac4 in tests/draft.rs and tests/mcp.rs (fail without the new variant/code); AC5-AC10 by job020_ac5..ac10 in tests/cli.rs; AC11-AC13 by job020_ac11..ac13 in tests/mcp.rs; AC14 by job020_ac14 in tests/plugin.rs; AC15 by the commands above. Values are not echoed: messages are built from clap context only; the server text is not kept in `AppendRefused`.
