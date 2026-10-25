VERDICT: PASS

## Findings
- [follow-up] src/create.rs:1010-1023: (carried from round 1, unchanged) if the Broker stores the Mandate and the connection then drops before the `create` reply arrives, the client prints "Nothing was stored." while an unrevealed Mandate exists in the Broker's store. No secret is exposed, but the message is false → reword this case (for example "may have been stored; check `agent-mail mandate list`"), or have the Broker roll back the Mandate when it cannot write the reply.
- [follow-up] tests/broker.rs:954-965: the owner assertion added this round lives in the opt-in elevated service test, which was not run (build.md says so). Until someone runs it, the claim that the real service's pipe is owned by `NT SERVICE\agent-mail` rests on code reading, not an observed run. If it is wrong, the failure is closed (`broker_unavailable`), not a leak → run `cargo test --test broker -- --ignored` elevated before the Ticket 21 release and record the result.

Notes, no finding:
- Round 1 had no `[blocking]` or `[fix-now]` items. Its `pipe_sddl` owner follow-up, which the human asked for in human-notes.md, is resolved in 952d736. `pipe_sddl` now emits `O:<broker_sid>`. `serve_pipe` passes `current_user_sid()` as `broker_sid` (src/broker/windows.rs:1006). Under the service, that is the virtual account SID, and a token's own user SID is always a valid owner, so the Broker cannot fail with `ERROR_INVALID_OWNER`. `PipeConnector::broker()` expects `service_sid(SERVICE_NAME)`, which is that same SID when the service runs as `NT SERVICE\agent-mail`.
- The squatter guard holds as before. A non-admin process can name only its own user (or an owner-flagged group) as `O:`, so its pipe can never carry the service SID. `served_by` still runs in `PipeConnector::open` for every connection.
- `create_test_pipe` and `create_test_pipe_for` stay test-only. They now set the owner to the current user. That also clears round 1's note that test pipes from an elevated shell would be owned by Administrators and refused.

## Checked
- Read: the full `git diff main...factory/026-windows-broker-clients` (stat), with a focus on commit 952d736, the only change since round 1. Through the security lens:
  - `src/broker.rs`: `pipe_sddl` and `pipe_dacl_sddl`.
  - `src/broker/windows.rs`: `create_pipe`, `create_pipe_with`, `create_test_pipe`, `create_test_pipe_for`, `pipe_owner`, `served_by`, `PipeConnector`, and `serve_pipe`, including where `broker_sid` comes from.
  - `tests/broker.rs`: `ac2_pipe_sddl_is_exact` and the elevated service test's new owner and `served_by` assertions.
  - The `CLAUDE.md` layout line.
  - From the job folder: spec.md, human-notes.md, round-1/review-security.md, decision.md, and the build.md Round 2 section, read last and checked against the code.
- Ran:
  - `cargo test --test broker --test client` on Windows: broker 31 passed, 1 ignored (the elevated service test); client 20 passed; 0 failed.
  - Grepped for `pipe_sddl` and `create_pipe(` callers across src/ and tests/: every pipe the Broker creates goes through `pipe_sddl`, and only the test-support helpers use another owner.
