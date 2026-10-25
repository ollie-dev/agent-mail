VERDICT: PASS

## Findings
- [follow-up] src/broker/windows.rs `create_pipe` / `pipe_sddl`: the explicit `O:<service SID>` only works if the service token lets that SID be an object owner (Windows returns `ERROR_INVALID_OWNER` otherwise, as the builder saw for foreign SIDs in `create_test_pipe_for`). Only the opt-in elevated `ac1_service_starts_and_stops_with_exit_code_zero` exercises this, and it was not run. Run it elevated (`cargo test --test broker -- --ignored`, no real Broker running) before Ticket 21 ships; if the pipe cannot be created as the service SID, the Broker would not start.
- [follow-up] Round 1 follow-ups still stand (startup race of the test pipe in `tests/cli.rs` under load; `ErrorBody` doc comment on field order).

## Checked
- Read: round-1 review, human-notes.md (O: in pipe_sddl, assert owner in service test), `git show 952d736` in full (`src/broker.rs`: `pipe_sddl`, `pipe_dacl_sddl`; `src/broker/windows.rs`: `create_pipe`, `create_pipe_with`, `create_test_pipe_for`, `pipe_owner`; `tests/broker.rs`: `ac2_pipe_sddl_is_exact`, the service test; CLAUDE.md line), and the round-2 section of build.md.
- Ran: `cargo test --no-fail-fast` in the worktree: every suite ok, 0 failed (broker 31 passed + 1 ignored). `cargo clippy --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.
- Criteria: the human note is met. The SDDL has an explicit owner, covered by the exact-string test, which fails without the change. The service test now asserts `pipe_owner == service_sid` and `served_by` accepts it, but it is ignored and not run here. `create_test_pipe_for` uses the current user as owner with the foreign-SID DACL, which keeps job 025's instance test valid. Round-1 had no blocking or fix-now items.
