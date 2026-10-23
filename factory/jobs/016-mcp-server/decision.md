APPROVE
All four round-1 reviews pass with no fix-now findings, every criterion has a passing test in tests/mcp.rs, and the diff stays in scope with no new dependency.

## Acceptance criteria
- AC1: pass (`ac1_tools_list_returns_the_seven_agent_tools_with_their_schemas`, `ac1_bad_arguments_fail_with_invalid_arguments_before_any_connection`)
- AC2: pass (`ac2_whoami_matches_the_cli_and_writes_no_audit_line`, `ac2_list_matches_the_cli`, `ac2_search_matches_the_cli`, `ac2_read_matches_the_cli`, `ac2_flag_matches_the_cli_and_the_server_state`, `ac2_move_matches_the_cli_and_the_server_state`, `ac2_draft_matches_the_cli_and_the_appended_message`, `ac2_invalid_token_warning_is_a_second_content_item`)
- AC3: pass (`ac3_missing_and_invalid_tokens_are_tool_errors`, `ac3_denied_names_operation_account_and_mailbox`, `ac3_account_required_and_ambiguous_mandate`, `ac3_mail_errors_match_the_cli`, `ac3_warning_comes_before_the_error`)
- AC4: pass (`ac4_tool_arguments_cannot_carry_a_token`, `ac4_meta_is_never_a_token_source`, `ac4_without_a_token_every_tool_fails_with_missing_token`, `ac4_the_token_is_captured_once_for_the_session`, `ac4_no_secret_leaves_in_a_whole_session`)
- AC5: pass (`ac5_no_tool_manages_mandates`: store unchanged, no audit line, empty terminal transcript, zero connections; `ac5_mcp_help_says_mandates_are_not_managed_over_mcp`)
- AC6: pass (`assert_same_audit` in every AC2/AC3 comparison, `ac6_protocol_messages_and_rejected_calls_write_no_audit_line`, `ac6_audit_unavailable_is_a_tool_error_as_on_the_cli`)
- AC7: pass (`ac7_initialize_negotiates_the_protocol_version`, `ac7_notifications_ping_and_unknown_methods`, `ac7_bad_json_is_answered_and_the_server_keeps_going`, `ac7_unknown_names_are_echoed_escaped`, `ac7_eof_ends_the_session_cleanly`, `ac7_the_binary_serves_mcp_on_stdio`)
- AC8: pass (`ac8_readme_help_and_claude_md_describe_the_mcp_server`; I ran `cargo test` with every suite ok, 31/31 in tests/mcp.rs, plus clean `cargo clippy -- -D warnings` and `cargo fmt --check`)

## For the human
Nothing here needs your decision before the merge. The spec chose a hand-written JSON-RPC loop with no new crate. Cargo.toml and Cargo.lock are unchanged. The token is read only from the server's environment, and every call goes through `core::execute_reporting`.

One thing to look at later: the README's Claude Code example says to put `"AGENT_MAIL_TOKEN": "<your token>"` in the project's `.mcp.json`. That file is project-scoped and is often committed, which would go against the "credentials never appear in commits" convention. `claude mcp add` uses local scope by default, so that path is fine. The spec asked for this README content and the security review did not flag it, so I did not block the merge on it. It is listed below as a follow-up to fix the wording: recommend local scope, or `${AGENT_MAIL_TOKEN}` expansion in `.mcp.json`. If you would rather not ship that advice even briefly, rework the README paragraph before the merge.

## Follow-ups
- decider · README.md (MCP server section, Claude Code paragraph): it suggests a literal token in the project's `.mcp.json`, a file that is often committed → recommend `claude mcp add` local scope or `${AGENT_MAIL_TOKEN}` env expansion, and warn not to commit the token.
- code · src/main.rs:18-25: `main` parses the arguments once to detect `mcp`, and `run` parses them again for every other command → parse once and pass the parsed `Cli` on.
- [pattern] ui · src/lib.rs (`Mcp` doc comment): the `mcp --help` long text is one dense paragraph of about 700 characters; the same kind of finding was logged in 012 (F3, `draft --help`) and 015 (F9, `proxy_refused` message) → add a CLAUDE.md convention that help text and error messages are split into short paragraphs or lines (for example at most about 300 characters each), and have the ui reviewer check it.
- ux · src/mcp.rs:444: with no token and invalid arguments, the call returns only `missing_token` (as AC4 requires), so the Agent learns about its argument mistake only after the token is fixed → consider mentioning both problems in the message.
- ux · src/lib.rs (`run`, `Mcp` arm): in-process `run` refuses `mcp` with a plain-text stderr line; only tests can reach it → make it the JSON error shape, or leave it as is if no caller ever needs it.
