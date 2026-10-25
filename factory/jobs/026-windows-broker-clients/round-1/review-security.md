VERDICT: PASS

## Findings
- [follow-up] src/broker.rs:56 (`pipe_sddl`, job 025): the client's server-identity check (`served_by`, src/broker/windows.rs) anchors on the pipe's owner SID, but the Broker's SDDL sets no `O:`. The owner is therefore the service token's default owner, assumed but not shown to be `S-1-5-80-…` for `NT SERVICE\agent-mail`. If the default owner is different, the check fails closed: every Windows client gets `broker_unavailable`, with no leak. The same reliance makes test pipes created from an elevated (admin) process owned by `BUILTIN\Administrators`, so `AGENT_MAIL_TEST_PIPE` runs from an elevated shell would be refused → a later job should set the owner explicitly (`O:{broker_sid}` in `pipe_sddl`, update the job-025 SDDL test) and check `pipe_owner` against the service SID in the opt-in elevated service test.
- [follow-up] src/create.rs:1010-1023: if the Broker stores the Mandate and the connection then drops before the `create` reply arrives, the client prints "Nothing was stored." while an unrevealed, unusable Mandate exists in the Broker's store. No secret is exposed: the token was never shown, and the Mandate can be listed and deleted. But the message is false → reword it for this case (for example "may have been stored; check `agent-mail mandate list`"), or have the Broker take the Mandate back when it cannot write the reply.

Notes, no finding:
- The AC4 identity mechanism differs from the spec, which compared the server process's user SID. The client checks the pipe owner SID always, plus the server process's user whenever it can read the token. I accept this. A non-admin cannot set another account as the owner of an object it creates, and job 025's pipe DACL (`0x12019b` for AU) leaves out `FILE_CREATE_PIPE_INSTANCE`, so a squatter can create neither a pipe nor an extra instance owned by the service SID. The check sits in `PipeConnector::open`, so it covers every connection (`run`, `create`, `audit`, each MCP call), not only `create`. The client also connects at identification level (job 025), so a rogue server could not impersonate it.
- The pipe override `AGENT_MAIL_TEST_PIPE` and `PipeConnector::new` exist only under `cfg(any(test, feature = "test-support"))`, and `test-support` is not in `default`.

## Checked
- Read: the full `git diff main...factory/026-windows-broker-clients` for src/ and Cargo.toml.
  - `src/client.rs`, all of it: `platform_connector`, `Route::elevated`, `Session::open`/`exchange`/`call`, `wipe`, `shown_version`, `run_request`, `run_command`, `render_warning`, `tool_lines`, `audit`.
  - `src/broker/windows.rs`: `process_elevated`, `process_user_sid`, `pipe_server_pid`, `pipe_owner`, `served_by`, `PipeConnector`, `connect_waiting`, `record_one_client`.
  - `src/broker.rs`: `Secret::expose`, `RunRequest.text`, `handle_run` text path, `handle_audit`, `service_sid` and its TrustedInstaller vector, plus the existing `pipe_sddl`, `write_frame`/`read_frame` (Zeroizing, 1 MiB cap) and `CreateAccount`.
  - `src/create.rs::run_via_broker`: check order, Credential in the `create` request only, reply wiping, reveal-failure delete.
  - `src/core.rs`: new error variants and messages, checked for request data in the messages.
  - `src/lib.rs`: the `run_cli` Broker branch, `audit_command`, `render_audit`, `audit_value`.
  - `src/audit.rs::open_existing`: symlink and `O_NOFOLLOW`/`O_NONBLOCK`.
  - `src/mcp.rs::call_tool`/`render_lines`: a malformed reply gives an error, not a panic.
  - `src/main.rs`, `README.md` diff, `spec.md`, then `build.md`, with its claims checked against the code.
  - tests/client.rs: the AC3 secret assertions (lines 461-545) and the AC4 tests (lines 704-800).
- Ran:
  - `cargo test --test client --test audit`: 20 and 17 passed, 0 failed.
  - Grepped the added diff lines for `Cred-`, `secret` and `passw`. Only test placeholders (`proxy-secret-9`, `Cred-7f3a-secret` constant use) and README wording turned up, with no real Credentials.
  - Grepped `src/broker/windows.rs`, `src/broker.rs` and ADR-0004 for the service account and the pipe SDDL. The service runs as the virtual account `NT SERVICE\agent-mail`, and the pipe SDDL has no owner set.
