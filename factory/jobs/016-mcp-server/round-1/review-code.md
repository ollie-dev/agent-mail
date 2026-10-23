VERDICT: PASS

## Findings
- [follow-up] src/main.rs:18-25: `main` parses the arguments with `Cli::try_parse_from` to detect `mcp`, and `run` parses them again for every other command. Harmless now; a later job could parse once and pass the result on.

## Checked
- Read: the full diff `git diff main...factory/016-mcp-server` (CLAUDE.md, README.md, src/core.rs `no_token_presented`, src/lib.rs validators made `pub(crate)` and render functions, the `Mcp` subcommand and its `run` fallback, src/main.rs, tests/draft.rs, tests/support/mod.rs), src/mcp.rs in full (the loop, `handle_line`, `call_tool`, `render_report`, `build_command`, `search_command`, `draft_command`, the tool schemas), and the test names in tests/mcp.rs.
- Ran: `cargo test` (every suite ok, 0 failed), `cargo clippy -- -D warnings` (clean), `cargo fmt --check` (clean).
- Criteria:
  - AC1: `ac1_tools_list_...` and `ac1_bad_arguments_...` cover the tool list, schemas and `invalid_arguments`. The code matches the spec: seven tools in order, `additionalProperties` false, `limit` min/max/default, `to` with `minItems` 1.
  - AC2: the `ac2_*` tests compare each tool with the CLI, and `ac2_invalid_token_warning_...` covers the warning item. `render_report` reuses the CLI's render functions, so they would fail without the change.
  - AC3: `ac3_*` tests cover the error codes and the warning-before-error order, which `render_report` implements.
  - AC4: `ac4_*` tests cover the token. `_meta` is never read, and an unknown `token` property is rejected. `no_token_presented` returns `missing_token` for every tool.
  - AC5: `ac5_*` tests cover the unknown-tool `-32602` error and `mcp --help`. The adapter never builds a Mandate command.
  - AC6: `ac6_*` tests cover the audit lines and `audit_unavailable`. Every call goes through `execute_reporting`, and nothing is cached across calls.
  - AC7: `ac7_*` tests cover the protocol, bad JSON, escaping of echoed names and the binary run over stdio.
  - AC8: `ac8_readme_help_and_claude_md_...` covers the README, `--help` and CLAUDE.md. Both docs are updated, and I read the README section.
