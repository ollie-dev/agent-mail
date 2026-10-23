## Round 1
**Changed:**
- New `src/mcp.rs`: hand-written synchronous newline-delimited JSON-RPC 2.0 loop (`mcp::serve(input, output, env, context)`), no new dependency. Handles `initialize` (2025-06-18 / 2025-03-26 negotiation, `instructions` pointing to `whoami`), `notifications/initialized` (and any notification: no reply), `ping`, `tools/list`, `tools/call`; `-32700` (id null) for non-JSON/non-UTF-8 lines, `-32600` for malformed requests, `-32601` unknown method, `-32602` for missing `params.name` and unknown tools. Echoed names are control-escaped and capped at 64 characters.
- Seven tools (`whoami`, `list`, `search`, `read`, `flag`, `move`, `draft`) with `additionalProperties:false` schemas. Arguments are type-checked strictly (no string-for-integer/boolean coercion), unknown properties rejected, then passed through the CLI's validators in `src/lib.rs` (now `pub(crate)`: `account_login`, `reference`, `search_text`, `gmail_query`, `date`, `mailbox_name`, `address`, `subject`, `body`; `limit` goes through `Limit::new`, the same check `Limit::parse` ends in). CLI conflicts (`set`/`clear`, `subject`/`reply_to`, `account`/`reply_to`, `gmail` with other criteria) are mirrored. Failures are `invalid_arguments` tool errors naming the argument, with no connection and no audit line.
- Every valid call builds the CLI's `core::Command` and runs `core::execute_reporting`; results are rendered with the CLI's own `render_outcome`/`render_error`/`render_warning` (now `pub(crate)`), so `content[0].text` is byte-identical to the CLI line. Warning item second on success, first on error. `_meta` and every field besides `name`/`arguments` are never read. No Mandate command is ever built.
- Design choice to check: when no token is presented at all (`core::no_token_presented`, new, same rule as `open_presented`) and the arguments are also invalid, the call returns `missing_token` (no audit line, since no command exists), so AC4's "every tool returns missing_token whatever the arguments hold" holds; with valid arguments it goes through the core and audits exactly as the CLI.
- `src/lib.rs`: `pub mod mcp`, new `mcp` subcommand with help text saying Mandate management is not offered over MCP; `run` refuses `mcp` in-process (code 2) since it has no stdio. `src/main.rs`: parses once more and, for `mcp`, calls `mcp::serve` on locked stdin/stdout with the process env and the same `Context`, exit 0 (1 with a stderr line on an I/O error).
- `tests/mcp.rs` (31 tests) and `tests/support/mod.rs` helpers `mcp_raw`, `mcp_parse`, `mcp_lines`, `mcp_opening`, `mcp_call`. `tests/draft.rs` help-command list gains `mcp`. README "MCP server" section; CLAUDE.md layout lists `src/mcp.rs` and `tests/mcp.rs`.

**Commits:** 8d51066
**Tests:** `cargo test` -> all suites `ok` (313 passed, 0 failed; ignored: keychain, gmail live). `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --check` clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | `ac1_tools_list_returns_the_seven_agent_tools_with_their_schemas`, `ac1_bad_arguments_fail_with_invalid_arguments_before_any_connection` | pass |
| AC2 | `ac2_whoami_matches_the_cli_and_writes_no_audit_line`, `ac2_list_matches_the_cli`, `ac2_search_matches_the_cli`, `ac2_read_matches_the_cli`, `ac2_flag_matches_the_cli_and_the_server_state`, `ac2_move_matches_the_cli_and_the_server_state`, `ac2_draft_matches_the_cli_and_the_appended_message`, `ac2_invalid_token_warning_is_a_second_content_item` | pass |
| AC3 | `ac3_missing_and_invalid_tokens_are_tool_errors`, `ac3_denied_names_operation_account_and_mailbox`, `ac3_account_required_and_ambiguous_mandate`, `ac3_mail_errors_match_the_cli`, `ac3_warning_comes_before_the_error` | pass |
| AC4 | `ac4_tool_arguments_cannot_carry_a_token`, `ac4_meta_is_never_a_token_source`, `ac4_without_a_token_every_tool_fails_with_missing_token`, `ac4_the_token_is_captured_once_for_the_session`, `ac4_no_secret_leaves_in_a_whole_session` | pass |
| AC5 | `ac5_no_tool_manages_mandates`, `ac5_mcp_help_says_mandates_are_not_managed_over_mcp` | pass |
| AC6 | audit equality asserted in every AC2/AC3 comparison (`assert_same_audit`), `ac6_protocol_messages_and_rejected_calls_write_no_audit_line`, `ac6_audit_unavailable_is_a_tool_error_as_on_the_cli` | pass |
| AC7 | `ac7_initialize_negotiates_the_protocol_version`, `ac7_notifications_ping_and_unknown_methods`, `ac7_bad_json_is_answered_and_the_server_keeps_going`, `ac7_unknown_names_are_echoed_escaped`, `ac7_eof_ends_the_session_cleanly`, `ac7_the_binary_serves_mcp_on_stdio` | pass |
| AC8 | `ac8_readme_help_and_claude_md_describe_the_mcp_server`, full `cargo test` / clippy / fmt | pass |

**Review findings:** none (first round).
