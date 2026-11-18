## Round 1
**Changed:**
- `src/create.rs`: factored `create::run` into `create_and_store` (refusals, guided flow, seal, put; no reveal), `stored_line`, `take_back` and a `pub(crate)` `offer_export`. `mandate create` builds the same reveal text and transcript as before; the Broker path is untouched.
- New `src/setup.rs`: the wizard. The Mandate step is `create_and_store` followed by the export offer, then the "Connect it where?" loop (only `I'll configure it myself`) with "Connect it somewhere else too?" (default No). The single `Terminal::reveal` holds the token plus the Claude Code `env` snippet and the Claude Desktop `mcpServers` snippet. Both snippets are serialised by `serde_json` from structs that borrow the token, into zeroized buffers, so the `command` path is JSON-escaped. The final check runs `whoami` with only `AGENT_MAIL_TOKEN=<new token>` and shows `render_whoami` plus one next-step line per destination. After the store, a Cancel reveals the token if it has not been shown yet, then offers "Delete this Mandate?" (default No; Cancel counts as No). A failed reveal takes the Mandate back out of the store, as `mandate create` does. A failed final check reveals the token if needed, shows one message and exits 1.
- `src/lib.rs`: added `CliCommand::Setup` (one-line doc, with `--text`). `run` sends a bare call to the wizard only when `setup::starts_on_bare_call` holds: no Broker route, an interactive terminal, no `SANDBOX_INDICATORS` variable, `store.check()` ok, and `mandate list` empty with no unreadable record. Otherwise it parses as before. `setup` on the Broker route refuses before any prompt or request with code `unsupported`, pointing to `agent-mail mandate create` in an elevated terminal. The `setup` errors `sandbox_detected`, `interactive_terminal_required` and `aborted` keep their codes, but their messages name `agent-mail setup` (`setup::error_message`). Split `render_body` out of `render_error`, and made `render_whoami` `pub(crate)`.
- `src/main.rs`: a bare call goes through `agent_mail::run`, so it follows the same decision.
- `tests/draft.rs`: `ac7_no_command_edits_or_deletes_a_draft` asserts the exact `--help` command list, so `setup` was added to that list. This is the only change to an existing test.
- `CLAUDE.md`: added `src/setup.rs` and `tests/setup.rs` to the layout.

**Commits:** 75acd76
**Tests:** `cargo test`: every binary `test result: ok`, 610 passed, 0 failed, 6 ignored (`tests/setup.rs`: 28 passed). `cargo clippy -- -D warnings` (and `--all-targets`): clean. `cargo fmt --check`: clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | `ac1_help_lists_setup_on_one_line` | pass |
| AC2a | `ac2a_bare_call_on_a_terminal_with_no_mandate_starts_the_wizard`, `ac2a_bare_call_runs_the_whole_wizard` | pass |
| AC2b | `ac2b_bare_call_without_a_terminal_prints_help` | pass |
| AC2c | `ac2c_bare_call_in_an_agent_session_prints_help` | pass |
| AC2d | `ac2d_bare_call_with_a_mandate_stored_prints_help` | pass |
| AC2e | `tests/cli.rs` `bare_call_prints_usage_hint_on_stderr`, `bare_call_is_a_usage_error_with_empty_stdout` (unchanged), plus `ac2e_binary_bare_call_tests_are_unchanged` | pass |
| AC2f | `ac2f_bare_call_on_the_broker_route_prints_help_and_sends_nothing` | pass |
| AC3 | `ac3_setup_in_an_agent_session_is_refused_before_any_prompt`, `ac3_setup_without_a_terminal_is_refused`, `ac3_setup_on_the_broker_route_refuses_before_any_prompt_or_request` | pass |
| AC4a | `ac4a_mandate_step_matches_mandate_create_up_to_the_export_offer` | pass |
| AC4b | `ac4b_store_holds_exactly_the_ticked_mandate` | pass |
| AC4c | `ac4c_wrong_credential_then_abort_stores_nothing` | pass |
| AC4d | all of `tests/create.rs` (unchanged), plus `ac4d_mandate_create_tests_are_unchanged` | pass |
| AC5 | `ac5_configure_myself_reveals_the_token_and_both_snippets` | pass |
| AC6 | `ac6_destinations_loop_until_no_and_reveal_only_once` | pass |
| AC7 | `ac7_final_check_shows_whoami_text_then_the_next_step`, `ac7_final_check_failure_shows_one_message_and_exits_1` | pass |
| AC8a | `ac8a_happy_path_shows_the_token_once` | pass |
| AC8b | `ac8b_cancel_at_connect_where_reveals_then_keeps_by_default` | pass |
| AC8c | `ac8c_cancel_at_connect_where_and_delete_removes_the_mandate` | pass |
| AC8d | `ac8d_cancel_at_connect_elsewhere_does_not_reveal_again` | pass |
| AC8e | `ac8e_cancel_at_the_delete_offer_counts_as_no` | pass |
| AC8f | `ac8f_token_that_cannot_be_shown_deletes_the_mandate` | pass |
| AC8 (export cancel) | `ac8_cancelling_the_export_offer_continues` | pass |
| AC9 | `ac9_lines_are_short_and_no_credential_is_shown` | pass |
| AC10 | `ac10_tests_run_in_process_only` (and every test in `tests/setup.rs`) | pass |
| AC11 | `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check` | pass |

**Review findings:** none (round 1).

**Choices for reviewers:**
- Bare call with unreadable store records: the wizard does not start and help is printed. The store does not "list no Mandate" in that case, so a record that may be a Mandate is not ignored.
- The final-check message is exactly `render_whoami`'s output, trailing newline included, so it equals `whoami --text` stdout byte for byte.
- A failed final check keeps the Mandate, because its token has already been shown. stderr carries the `whoami` error (`store_unavailable` in the test), or `invalid_token` if the token opened nothing.
- `current_exe()` returning `None` falls back to `agent-mail` as the Desktop `command`.
