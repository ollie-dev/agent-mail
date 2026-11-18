APPROVE
All four round-1 reviews pass, every criterion has a passing in-process test, and the diff stays inside the setup tracer's scope with a single token reveal.

## Acceptance criteria
- AC1: pass (`ac1_help_lists_setup_on_one_line`)
- AC2: pass (`ac2a_bare_call_on_a_terminal_with_no_mandate_starts_the_wizard`, `ac2a_bare_call_runs_the_whole_wizard`, `ac2b_bare_call_without_a_terminal_prints_help`, `ac2c_bare_call_in_an_agent_session_prints_help`, `ac2d_bare_call_with_a_mandate_stored_prints_help`, `ac2e_binary_bare_call_tests_are_unchanged` plus the unchanged `tests/cli.rs` `bare_call_prints_usage_hint_on_stderr` and `bare_call_is_a_usage_error_with_empty_stdout`, `ac2f_bare_call_on_the_broker_route_prints_help_and_sends_nothing`)
- AC3: pass (`ac3_setup_in_an_agent_session_is_refused_before_any_prompt`, `ac3_setup_without_a_terminal_is_refused`, `ac3_setup_on_the_broker_route_refuses_before_any_prompt_or_request`)
- AC4: pass (`ac4a_mandate_step_matches_mandate_create_up_to_the_export_offer`, `ac4b_store_holds_exactly_the_ticked_mandate`, `ac4c_wrong_credential_then_abort_stores_nothing`, `ac4d_mandate_create_tests_are_unchanged`, and `tests/create.rs` is unchanged with 39 passing)
- AC5: pass (`ac5_configure_myself_reveals_the_token_and_both_snippets`)
- AC6: pass (`ac6_destinations_loop_until_no_and_reveal_only_once`)
- AC7: pass (`ac7_final_check_shows_whoami_text_then_the_next_step`, `ac7_final_check_failure_shows_one_message_and_exits_1`)
- AC8: pass (`ac8a_happy_path_shows_the_token_once`, `ac8b_cancel_at_connect_where_reveals_then_keeps_by_default`, `ac8c_cancel_at_connect_where_and_delete_removes_the_mandate`, `ac8d_cancel_at_connect_elsewhere_does_not_reveal_again`, `ac8e_cancel_at_the_delete_offer_counts_as_no`, `ac8f_token_that_cannot_be_shown_deletes_the_mandate`, `ac8_cancelling_the_export_offer_continues`)
- AC9: pass (`ac9_lines_are_short_and_no_credential_is_shown`)
- AC10: pass (`ac10_tests_run_in_process_only`; every test in `tests/setup.rs` uses `agent_mail::run` in-process)
- AC11: pass (ran `cargo test` in the worktree: every binary `test result: ok`, `tests/setup.rs` 28 passed, 0 failed; `cargo clippy -- -D warnings` clean; `cargo fmt --check` clean)

## For the human
Nothing needs a human decision. Notes:
- A bare `agent-mail` on a human's terminal, outside an Agent session, now reads the store's record list (metadata only, no token) to decide whether to start the wizard. If the store errors or holds an unreadable record, it falls back to help.
- `create::run` was split into `create_and_store`, `stored_line`, `take_back` and `offer_export`. `mandate create` builds the same reveal text, and its tests are unchanged.
- The only change to an existing test is the expected `--help` command list in `tests/draft.rs`, which now includes `setup`.
- There are no new dependencies, and nothing new is stored. The token reaches only `Terminal::reveal`, and the snippets are serialised by `serde_json` into zeroized buffers.

## Follow-ups
- code · src/setup.rs (`starts_on_bare_call`): a bare call on a terminal whose store errors or holds an unreadable record prints plain help without saying why the wizard did not start → add a short hint (for example "run `agent-mail setup`") in that case.
- ux · src/setup.rs:104 (`error_message`, `BrokerUnsupported`): on Windows, `setup` points to `agent-mail mandate create` in an elevated terminal → reword it for the elevated-child flow when ticket 08 lands.
