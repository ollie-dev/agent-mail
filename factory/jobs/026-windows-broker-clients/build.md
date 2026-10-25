# Build: 026-windows-broker-clients

## Round 1
**Changed:**
- New `src/client.rs`, compiled on every OS. It defines the `Connector`/`Transport` port and a `Session` that sends `hello` first and then one request. Replies are rendered exactly as `run_cli` and `mcp::call_tool` render them in-process. `Context::with_broker` sets the route; `run_cli`, the MCP `tools/call`, `mandate create` and `audit` use it. `RunRequest` gains `text` (serde default `false`): with `--text` the Broker returns the CLI's text as a JSON string. Every reply is checked: a reply that cannot be read, an oversized frame, an EOF, a reply with neither `result` nor `error`, or a malformed `error`/`warning` all give `broker_unavailable`.
- `src/broker/windows.rs` gains these pieces:
  - `PipeConnector`, which waits on `ERROR_PIPE_BUSY` (`WaitNamedPipeW`) and runs the server identity check `served_by` before anything is sent.
  - `process_elevated` (`TokenElevation`).
  - `pipe_owner`, `pipe_server_pid` and `process_user_sid`.
  - Test-only `PipeConnector::new` and `record_one_client`.
- `src/client.rs::platform_connector()` returns the pipe connector on Windows and `None` elsewhere. With the `test-support` feature it reads `AGENT_MAIL_TEST_PIPE` (served by the current user). `src/main.rs` calls it without any `cfg(windows)`, because `tests/selfcheck.rs` job024 forbids `cfg(windows)` in `main.rs`.
- `mandate create` on the Broker route (`create::run_via_broker`) runs its checks in this order: interactive terminal, sandbox indicator, then elevation, all before any prompt, file read or connection. It then runs the guided flow with the live login and sends one `create`. It reveals the returned token and makes the export offer under the Broker's ID. If the reveal fails, it sends `run`/`mandate_delete` on the same connection. The raw `create` result holding the token is wiped.
- `CoreError::BrokerUnavailable` (`broker_unavailable`) is new. Its message names `sc.exe start agent-mail` and `install.ps1`.
- `BrokerVersionMismatch { broker: Option<String> }` now names both versions (the Broker's only when it is short and plain) and `install.ps1`.
- `CoreError::AuditUnreadable` (code `audit_unavailable`) gives the read-side message for `audit`. The Broker's `audit` now uses it too.
- The `elevation_required` message now says to run `agent-mail mandate create` again in an elevated (Run as administrator) terminal.
- New `agent-mail audit [--text]` command:
  - In-process it reads `context.audit` and skips lines that are not JSON objects.
  - On Unix it refuses a symlinked log or log directory (`O_NOFOLLOW`, `symlink_metadata`).
  - On the Broker route it sends `{"audit":{}}`.
  - With `--text` it prints `time outcome operation key=value…`, escaping control characters, without `sid`.
- Help: the `audit` page is new, and `mandate create --help` gains the elevated-terminal sentence.
- README:
  - A **Windows named pipe** paragraph in "Sandbox and proxies".
  - The "Audit log" clause now points to `agent-mail audit`.
  - Three existing blocks in "Sandbox and proxies" were over 300 characters. I split them at sentence boundaries without changing a word (see notes).
- `CLAUDE.md` layout line names `src/client.rs`, `tests/client.rs` and the new test-only hooks. `Cargo.toml` comment updated (no new windows-sys feature in the end).
- Existing binary tests that run Agent commands on Windows now point the binary at an in-process `serve_on` test pipe (`support::broker::TestPipe`):
  - `tests/cli.rs` `run_list` (the `ac6`/`ac11`/audit-redirect tests);
  - `tests/selfcheck.rs` `ac9_debug_binary_reaches_the_store`.

  Their expected output is unchanged.

**Commits:** 304f80d

**Tests:** `cargo test` gave `test result: ok` for every suite: 0 failed. `tests/client.rs` 20 passed; `tests/broker.rs` 31 passed, 1 ignored; `tests/cli.rs` 30 passed. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean. I ran everything on Windows 11; the macOS `cfg` paths were not compiled locally (no macOS target installed), so check the first macOS CI run.

| AC | Test | Status |
|----|------|--------|
| AC1 | `tests/client.rs::ac1_broker_route_prints_exactly_what_the_in_process_run_prints` (all listed commands × JSON/`--text`, denied, invalid-token warning, no token; `CountingStore` 0 accesses), `ac1_each_command_sends_hello_then_one_run_with_the_token_in_it`, `windows::ac1_the_binary_forwards_list_to_the_pipe` | pass |
| AC2 | `ac2_every_tool_answers_the_same_on_the_broker_route`, `ac2_bad_arguments_fail_without_connecting`, `ac2_each_tool_call_opens_its_own_connection_starting_with_hello` | pass |
| AC3 | `ac3_not_elevated_refuses_before_any_prompt_connection_or_file_read` (also `--from` a missing file), `ac3_the_terminal_and_sandbox_refusals_still_come_first`, `ac3_elevated_create_stores_an_owned_mandate_and_reveals_its_token`, `ac3_from_file_prefills_the_flow_exactly_as_in_process`, `ac3_a_create_error_from_the_broker_is_printed_and_nothing_is_stored`, `ac3_a_failed_reveal_deletes_the_new_mandate_through_the_broker` | pass |
| AC4 | `ac4_no_pipe_gives_broker_unavailable_naming_the_fix`, `ac4_an_early_close_or_unreadable_reply_gives_broker_unavailable`, `ac4_a_version_mismatch_names_install_ps1_and_sends_nothing_more`, `ac4_the_same_codes_come_back_as_mcp_tool_errors`, `ac4_the_broker_is_expected_to_run_as_its_service_account`, `windows::ac4_a_pipe_served_by_another_account_gets_nothing`, `windows::ac4_a_client_expecting_this_user_connects_and_runs`, `windows::ac4_the_pipe_owner_and_server_process_name_the_serving_account` | pass |
| AC5 | `tests/audit.rs::audit_command::ac5_audit_prints_every_json_entry_in_file_order`, `ac5_without_a_log_audit_prints_no_entries`, `ac5_text_prints_one_escaped_line_per_entry_without_the_sid`, `ac5_a_symlinked_log_or_directory_is_refused` (Unix only, not run here), `ac5_on_the_broker_route_audit_shows_only_the_callers_entries`, `ac5_on_the_broker_route_the_local_log_is_not_read` | pass (symlink test: CI macOS) |
| AC6 | `tests/plugin.rs::job026_ac6_readme_records_whether_the_sandbox_reaches_the_pipe` | pass |
| AC7 | `tests/cli.rs::ac1_no_help_paragraph_is_longer_than_300_characters` (now 28 invocations), `ac2_help_keeps_the_facts_an_agent_depends_on` (`audit`: `your own Mandates`, `JSON`, `--text`; `mandate create`: `elevated`), `ac5_every_error_message_is_at_most_300_characters` (samples gain `BrokerUnavailable`, both `BrokerVersionMismatch` forms, `AuditUnreadable`), `ac7_top_level_help_lists_audit`, `ac7_audit_help_repeats_no_sentence_of_the_top_level_help` | pass |

**Review findings:** none (round 1).

**Notes for reviewers:**
- **AC4 server identity: the mechanism deviates from the spec, with evidence.** The spec asks the client to read the server's process with `GetNamedPipeServerProcessId` and compare that process's user SID. I tested this on this machine as a standard (medium-integrity) user against a running `LocalService` service process (Audiosrv, PID 4380):
  - `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` plus `OpenProcessToken` fails.
  - `WTSEnumerateProcessesExW` lists the process with a NULL `pUserSid`.
  - `Get-Process -IncludeUserName` needs elevation.

  So a non-elevated client cannot read a service process's user, and the check as written would refuse the real Broker for every Agent. `served_by` therefore requires the **pipe's owner SID** (`GetSecurityInfo`, `OWNER_SECURITY_INFORMATION`) to equal `S-1-5-80-…` for `NT SERVICE\agent-mail`. A user cannot make another account the owner of an object they create (only `SeRestorePrivilege`/`SeTakeOwnershipPrivilege` holders, that is administrators, can). In addition, `GetNamedPipeServerProcessId` is always called, and whenever the server process's token *can* be read (an elevated client, or a server of the same user), its user must match too.

  The check runs in `PipeConnector::open`, so it applies to every connection (`run`, `create`, `audit`, each MCP call), not only `create`. One assumption remains: the Broker's pipe owner is its default token owner, the service SID, because job 025's SDDL sets no `O:`. Confirm it with the opt-in elevated service test. `pipe_sddl` is unchanged, because its exact form is a job-025 test.
- **AC6 sandbox check not run.** Claude Code 2.1.310 on Windows 11. The sandbox is not enabled in this environment (`~/.claude/settings.json` and the project settings have no `sandbox` key), and I did not change settings or start a nested session with sandbox settings. The README therefore says the result is **not yet verified**, names `\\.\pipe\agent-mail`, `broker_unavailable` and the `agent-mail mcp` way around. To verify it:
  1. Enable `sandbox.enabled` in a Windows Claude Code project.
  2. Have the Agent run `agent-mail whoami` against a running Broker.
- **Existing tests changed by necessity:**
  - `tests/draft.rs::ac7_no_command_edits_or_deletes_a_draft` lists the top-level commands and now includes `audit`, which AC7 requires in `--help`. The spec's "passes unchanged" cannot hold for this one assertion.
  - `tests/audit.rs::ac6_no_command_reads_or_returns_the_audit_log` asserted that `audit` is an unknown command. It is renamed `ac6_no_command_but_audit_reads_or_returns_the_audit_log` and keeps the check for `log`.
- **README:** besides the new paragraph and the changed clause, three blocks in "Sandbox and proxies" (319, 332 and 335 characters) were split at sentence boundaries, so AC6's "no paragraph over 300 characters" test can hold. No word was changed.
- **Credentials:**
  - The `create` frame is serialised into a `Zeroizing` buffer pre-sized to 1 MiB, so it is not reallocated.
  - The `create` reply's raw result is wiped (`client::wipe`).
  - Tokens and proxy values travel as `Secret`. Local errors (`broker_unavailable`, `broker_version_mismatch`) carry no request data; `ac4_an_early_close…` checks that neither the token nor a proxy password appears.
- **Edge:** if the Broker stores a Mandate but the connection drops before the `create` reply arrives, the client says "Nothing was stored." while an unrevealed Mandate exists. It is listed by `mandate list` and can be deleted.

## Round 2
**Changed:** addresses the human note of 2026-10-25 ("set O: explicitly in pipe_sddl to the service SID and assert the owner in the elevated service test").
- `broker::pipe_sddl(broker_sid)` now starts with `O:<broker_sid>`: `O:<sid>D:P(A;;0x12019b;;;AU)(A;;GA;;;<sid>)`. The pipe's owner, which `served_by` checks on every client connection, no longer depends on the token's default owner. The DACL part is now `pipe_dacl_sddl`.
- `create_pipe` builds from `pipe_sddl`, so the Broker and `serve_on`/`create_test_pipe` set the owner explicitly. Only `create_test_pipe_for` (test-only) is different: it sets the owner to the current user and keeps the DACL for the given SID. The reason is that Windows refuses an owner other than the caller's own user (`ERROR_INVALID_OWNER`), and job 025's foreign-SID instance test needs that DACL.
- `tests/broker.rs::ac2_pipe_sddl_is_exact` now expects the `O:` prefix.
- The opt-in elevated `windows::ac1_service_starts_and_stops_with_exit_code_zero` now connects to `\.\pipe\agent-mail` while the test service runs. It asserts that `pipe_owner` equals `broker::service_sid("agent-mail-broker-test")` and that `served_by` accepts the pipe.
- The `pipe_owner` doc comment and the `CLAUDE.md` layout line now say the owner comes from `pipe_sddl`.

**Commits:** 952d736

**Tests:** `cargo test`: all 28 suites `ok`, 496 passed, 0 failed. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

The elevated service test is `#[ignore]` and was not run, because this shell is not elevated. It compiles, and its new owner assertion is what confirms the round-1 assumption. Run it elevated with `cargo test --test broker -- --ignored` while no real `agent-mail` Broker is running, since the test service serves `\.\pipe\agent-mail`.

| AC | Test | Status |
|----|------|--------|
| AC4 | `tests/broker.rs::ac2_pipe_sddl_is_exact` (owner in SDDL), `tests/client.rs::windows::ac4_the_pipe_owner_and_server_process_name_the_serving_account`, `windows::ac4_a_pipe_served_by_another_account_gets_nothing`, opt-in `tests/broker.rs::windows::ac1_service_starts_and_stops_with_exit_code_zero` (owner = service SID) | pass (opt-in test not run) |
| AC1–AC3, AC5–AC7 | unchanged from round 1 | pass |

**Review findings:** this round addresses only the human note. The coordinator did not point me at the `round-1` review files, so I have not acted on them yet.
