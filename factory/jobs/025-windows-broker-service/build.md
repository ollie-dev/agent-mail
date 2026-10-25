# Build: 025-windows-broker-service

## Round 1
**Changed:**
- New `src/broker.rs` (every OS): `Request`/`Reply` protocol (replies embed the CLI's JSON byte for byte via `serde_json` `raw_value`), 4-byte big-endian framing capped at 1 MiB, `hello` version check, `Caller`, `serve_session` over any `Read + Write`, `create` handling (high integrity only, live login, `seal_for` with the caller SID, token returned once), the `audit` request filtered by `sid`, `SerializedStore` (one mutex around `put`/`delete`, F31), `pipe_sddl`/`audit_dir_sddl`, and `run_service`, which gives `unsupported` off Windows.
- New `src/broker/windows.rs`: `StartServiceCtrlDispatcherW` entry point with stop handler, an overlapped named-pipe server (`FILE_FLAG_FIRST_PIPE_INSTANCE`, `PIPE_REJECT_REMOTE_CLIENTS`, one thread per client), a stop signal that also cancels waiting reads, impersonation guarded by `RevertToSelf` (the process aborts if the revert fails), DACL/owner readers, and the `%ProgramData%` audit folder: created with the SDDL, then checked before each append or read for reparse points, owner, file type and link count (F33). Test-only extras: `serve_on`, `create_test_pipe`, `AuditLog::broker_at`.
- `core.rs`: `Command` (and `Query`, `SearchText`, `Date`, `Limit`, `MessageRef`, `FlagAction`) are serde types, and deserialising re-runs the value checks. `Context.owner` makes another owner's Mandate open like an unknown token, and drops it from `mandate list`/`delete` (not counted as unreadable). Audit entries get `sid`. New codes: `store_no_logon_session`, `invalid_request`, `broker_version_mismatch`, `elevation_required`, `service_required`, `unsupported` (broker).
- `seal.rs`: `Listing.owner` (inside the AAD, left out when `None`), `seal_for`, `open_with_owner`. `store.rs`: `StoreError::NoLogonSession`, `from_win32`, the Credential Manager doc no longer cites ADR-0002. `audit.rs`: `Entry.sid`, `AuditLog::broker`, `lines()`. Hidden `broker` subcommand in `lib.rs`/`main.rs`. CLAUDE.md layout updated; F31 and F33 ticked.
- Test support: `tests/support/broker.rs` (in-memory duplex with a caller the test sets). The fake IMAP server also accepts a second fake credential `Cred-7f3a-secret` for AC10.

**Commits:** 12b3eba
**Tests:** `cargo test` → every suite `ok`; `tests/broker.rs`: `test result: ok. 28 passed; 0 failed; 1 ignored`. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean (Windows host).

| AC | Test | Status |
|----|------|--------|
| AC1 | `ac1_service_name_is_agent_mail`, `ac1_broker_is_hidden_from_help`, `ac1_broker_outside_the_service_manager_prints_one_json_error` (binary: `service_required` on Windows, `unsupported` elsewhere), `windows::ac1_stop_signal_ends_the_pipe_server_within_five_seconds` | pass |
| AC1 | `windows::ac1_service_starts_and_stops_with_exit_code_zero` (opt-in, `--ignored`, elevated) | not run: this shell is not elevated |
| AC2 | `ac2_pipe_sddl_is_exact`, `windows::ac2_pipe_dacl_allows_authenticated_users_and_the_broker_only` (protected DACL, two allow ACEs, second create fails) | pass |
| AC3 | `ac3_matching_version_is_ok_and_mismatch_closes`, `ac3_requests_before_hello_and_malformed_frames_get_invalid_request`, `ac3_run_list_equals_the_cli_output`, `ac3_invalid_token_warning_is_the_cli_warning` | pass |
| AC4 | `ac4_another_owners_mandate_looks_like_an_unknown_token`, `ac4_a_mandate_without_owner_is_served_to_no_caller`, revert check in `windows::ac11_real_pipe_round_trip_lists_only_the_callers_mandate` | pass |
| AC5 | `ac5_create_below_high_integrity_is_refused_before_any_login`, `ac5_create_with_a_bad_credential_fails_and_stores_nothing`, `ac5_create_stores_an_owned_mandate_and_returns_its_token_once` | pass |
| AC6 | `ac6_list_and_delete_see_only_the_callers_mandates` | pass |
| AC7 | `ac7_serialized_store_runs_one_write_at_a_time`, `ac7_no_logon_session_has_its_own_error` | pass |
| AC8 | `ac8_audit_dir_sddl_is_exact`, `windows::ac8_audit_folder_gets_the_protected_dacl_and_owner`, `windows::ac8_a_junction_for_the_folder_is_refused`, `windows::ac8_a_hard_link_or_symlink_for_the_log_is_refused`, `windows::ac8_a_folder_owned_by_another_user_is_refused`, `windows::ac8_broker_log_is_under_program_data`; `sid` on entries in `ac10_…` | pass |
| AC9 | `ac9_audit_returns_only_the_callers_entries_in_file_order` | pass |
| AC10 | `ac10_no_reply_carries_a_credential_or_record_and_only_create_the_token`, plus the fake-transport cases in AC3–AC5 | pass |
| AC11 | `windows::ac11_real_pipe_round_trip_lists_only_the_callers_mandate`, `windows::ac11_version_mismatch_reply_reaches_the_client_before_the_close` | pass |

**Review findings:** none yet (round 1).

**Notes for reviewers:**
- Security concern, spec followed as written: AC2's exact `GRGW` for `AU` maps on a pipe to `FILE_GENERIC_WRITE`. That includes `FILE_CREATE_PIPE_INSTANCE` (0x4). So, while the Broker runs, any authenticated user can create extra server instances of `\\.\pipe\agent-mail`. `FILE_FLAG_FIRST_PIPE_INSTANCE` only stops a squatter that arrives before the Broker. A rogue instance could receive an elevated client's `create` request, Credentials included. Suggested fix: `D:P(A;;0x12019b;;;AU)(A;;GA;;;<sid>)` (generic read and write without create-instance), or Ticket 20 checking the server process with `GetNamedPipeServerProcessId`. Changing it means changing AC2's exact string, so it needs a decision.
- Each client gets its own thread with no idle timeout, so a local user can hold many idle connections open. Stop still ends them, since a waiting read is cancelled.
- Timing: a token for another owner's Mandate costs one AEAD open before it is refused, and an unknown token costs none. The reply is identical either way.
- Not zeroized: `serde_json`'s scratch buffer for escaped strings inside a `create` frame. The frame buffer, `Credential`, `Secret`, the reply bytes and the `create` result are wiped.
- macOS was not compiled here (no Apple target installed). The cfg'd code was reviewed by hand, and the platform-neutral tests run on macOS CI.

## Round 2
**Changed:**
- Pipe access list: `pipe_sddl` is now `D:P(A;;0x12019b;;;AU)(A;;GA;;;<broker_sid>)`, which gives authenticated users read and write without `FILE_CREATE_PIPE_INSTANCE`. The mask is the new constant `broker::PIPE_CLIENT_RIGHTS`. **spec.md AC2 was updated to match, at the coordinator's direction:** the exact string, the DACL read-back wording, and the new test that a second, non-first create is refused.
- Audit log: `open_audit_log` opens `audit.log` with `FILE_SHARE_READ | FILE_SHARE_WRITE`, without `FILE_SHARE_DELETE`, for both append and read. Appends still use `FILE_APPEND_DATA` only.
- Audit folder: before each append or read, the folder's DACL is also checked (`only_broker_writes`). It must be protected and hold only allow and deny entries, and no allow entry for a SID other than the Broker may grant write, append, add-file, delete-child, attribute, delete, `WRITE_DAC`, `WRITE_OWNER` or generic write/all rights. This runs after the reparse-point and owner checks, so the earlier AC8 tests still fail for their own reasons.
- `src/store.rs:313`: the BEL character is replaced with `NT SERVICE\agent-mail`.
- New test-only helper `create_test_pipe_for(name, broker_sid, first)` (listed in CLAUDE.md).

**Commits:** 5802f95
**Tests:** `cargo test` → every suite `ok`; `tests/broker.rs`: `test result: ok. 31 passed; 0 failed; 1 ignored`. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean.

| AC | Test | Status |
|----|------|--------|
| AC2 | `ac2_pipe_sddl_is_exact` (new string), `windows::ac2_pipe_dacl_allows_authenticated_users_and_the_broker_only` (AU mask is `0x12019b`, no 0x4 bit), `windows::ac2_authenticated_users_cannot_add_an_instance_of_the_pipe` (foreign Broker SID: the non-first create fails with error 5; the Broker's own SID can still add instances) | pass |
| AC8 | `windows::ac8_concurrent_appends_and_reads_of_the_broker_log_all_succeed` (8 threads × 10 appends with reads, then a read and an append while another append handle is held; 82 lines), `windows::ac8_a_folder_with_an_inherited_dacl_is_refused` | pass |
| AC1, AC3–AC7, AC9–AC11 | unchanged from round 1 | pass |

**Review findings:**
- [blocking] security, `GRGW` lets authenticated users create pipe instances → fixed. The SDDL now uses `0x12019b`, the read-back test checks the mask, and a new test confirms a non-first create by a plain authenticated user is refused with access denied. spec.md AC2 updated.
- [blocking] code, `FILE_SHARE_READ` only, so concurrent appends and reads get `ERROR_SHARING_VIOLATION` → fixed, both modes now share read and write. Checked against the old code: with `FILE_SHARE_READ` only, the new concurrency test fails; with the fix it passes.
- [fix-now] security, the folder DACL was not checked → fixed with `only_broker_writes`. Done-when: the new test pre-creates `agent-mail` with `std::fs::create_dir` (inherited, unprotected DACL), and both `append` and `lines` fail. The junction, link and owner tests still pass, because those checks run before the DACL check.
- [fix-now] ux, BEL in `src/store.rs:313` → fixed. `grep -nP '\x07' -r src tests` prints nothing, and `grep -n 'NT SERVICE' src/store.rs` shows `NT SERVICE\agent-mail`. A scan for any other control character in `src`, `tests` and CLAUDE.md is also empty.
- [follow-up] items (connection and thread limits with an idle timeout, an audit entry for `create`, merging the identical `run_service` match arms) left for the factory as instructed.
