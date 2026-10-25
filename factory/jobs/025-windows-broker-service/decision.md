APPROVE
All eleven criteria have passing tests, all four round-2 reviews pass, both round-1 blocking issues and both [fix-now] findings are fixed, and the diff stays in scope.

## Acceptance criteria
- AC1: pass (`ac1_service_name_is_agent_mail`, `ac1_broker_outside_the_service_manager_prints_one_json_error` (checks `service_required` on Windows and `unsupported` elsewhere), `ac1_broker_is_hidden_from_help`, `windows::ac1_stop_signal_ends_the_pipe_server_within_five_seconds`; the opt-in elevated `windows::ac1_service_starts_and_stops_with_exit_code_zero` is `#[ignore]` as specified and was not run)
- AC2: pass (`ac2_pipe_sddl_is_exact` with `D:P(A;;0x12019b;;;AU)(A;;GA;;;<sid>)`, `windows::ac2_pipe_dacl_allows_authenticated_users_and_the_broker_only`, `windows::ac2_authenticated_users_cannot_add_an_instance_of_the_pipe`)
- AC3: pass (`ac3_matching_version_is_ok_and_mismatch_closes`, `ac3_requests_before_hello_and_malformed_frames_get_invalid_request`, `ac3_run_list_equals_the_cli_output`, `ac3_invalid_token_warning_is_the_cli_warning`)
- AC4: pass (`ac4_another_owners_mandate_looks_like_an_unknown_token`, `ac4_a_mandate_without_owner_is_served_to_no_caller`; revert check through `RevertedStore` in `windows::ac11_real_pipe_round_trip_lists_only_the_callers_mandate`)
- AC5: pass (`ac5_create_below_high_integrity_is_refused_before_any_login`, `ac5_create_with_a_bad_credential_fails_and_stores_nothing`, `ac5_create_stores_an_owned_mandate_and_returns_its_token_once`)
- AC6: pass (`ac6_list_and_delete_see_only_the_callers_mandates`)
- AC7: pass (`ac7_serialized_store_runs_one_write_at_a_time`, `ac7_no_logon_session_has_its_own_error`)
- AC8: pass (`ac8_audit_dir_sddl_is_exact`, `windows::ac8_broker_log_is_under_program_data`, `windows::ac8_audit_folder_gets_the_protected_dacl_and_owner`, `windows::ac8_a_junction_for_the_folder_is_refused`, `windows::ac8_a_hard_link_or_symlink_for_the_log_is_refused`, `windows::ac8_a_folder_owned_by_another_user_is_refused`, `windows::ac8_a_folder_with_an_inherited_dacl_is_refused`, `windows::ac8_concurrent_appends_and_reads_of_the_broker_log_all_succeed`)
- AC9: pass (`ac9_audit_returns_only_the_callers_entries_in_file_order`)
- AC10: pass (`ac10_no_reply_carries_a_credential_or_record_and_only_create_the_token`, plus the fake-transport cases in AC3, AC4 and AC5)
- AC11: pass (`windows::ac11_real_pipe_round_trip_lists_only_the_callers_mandate`, `windows::ac11_version_mismatch_reply_reaches_the_client_before_the_close`)

Ran in the worktree on Windows: `cargo test --locked` (every suite ok; `tests/broker.rs` 31 passed, 0 failed, 1 ignored), `cargo clippy --all-targets -- -D warnings` (clean), `cargo fmt --check` (clean). Round-1 [fix-now] Done-when checks re-run: `grep -nP '\x07' -r src tests` prints nothing, `src/store.rs:313` reads `NT SERVICE\agent-mail`, and the inherited-DACL folder test passes alongside the junction, link and owner tests.

## For the human
Nothing here needs a human call. Two notes:
- AC2's exact pipe SDDL was changed in round 2, at the coordinator's direction, from `GRGW` to `0x12019b` for Authenticated Users. This closes the security reviewer's finding that any local user could add a rogue server instance and receive an elevated client's Credentials. It is strictly tighter than the original spec. I checked that it does not lock out normal clients: on this machine, a plain authenticated-user client opening a pipe with this DACL using `GENERIC_READ | GENERIC_WRITE` connected, so Ticket 20 clients can use the usual open mode.
- The builder could not compile the macOS `cfg` paths locally. The platform-neutral tests (including the `unsupported` branch of AC1) run on macOS CI, so check that the first CI run after the merge is green on macOS.

## Follow-ups
- code · src/broker/windows.rs `serve_pipe` / src/broker.rs `serve_session`: connections and threads are unbounded and frames have no read deadline, so any local user can hold many idle or slow connections and starve the Broker (raised by code and security) → cap concurrent connections in total and per caller SID, and add a read deadline per frame.
- code · src/broker/windows.rs `run_service`: the `match` has two arms with identical results → merge them.
- security · src/broker/windows.rs:305-309 (`ObjectSecurity::dacl`) with :527-533 (`only_broker_writes`): a NULL DACL is reported as an empty ACE list and passes the check if the protected bit is set; only the folder's owner (the Broker or Administrators) could set one → make `Dacl` tell NULL from empty (for example `aces: Option<Vec<Ace>>`) and refuse a NULL DACL.
- security · src/broker.rs `handle_create`: `create` writes no audit entry, on success or failure → add a metadata-only `create` entry with the caller's `sid` once Ticket 20 defines the operator view.
