VERDICT: PASS

## Findings
- [follow-up] src/broker/windows.rs `serve_pipe`: connections and threads are unbounded and have no idle timeout, so a local user can hold many idle connections open. A later job should bound them and add an idle timeout.
- [follow-up] src/broker/windows.rs `run_service`: the `match` has two arms with identical results. Merge them (cosmetic).

## Checked
- Read: the round 1 review, `build.md` (Round 2), and the diff from the round 1 commit to the round 2 commit (`git diff 12b3eba 5802f95`). That covers `src/broker/windows.rs` (`open_audit_log` sharing flags, `only_broker_writes`, `FOLDER_WRITE_RIGHTS`, `create_test_pipe_for`), `src/broker.rs` (the new pipe SDDL), `src/store.rs`, `CLAUDE.md` and `tests/broker.rs`.
- Previous blocking item (`FILE_SHARE_READ` only on `audit.log`): resolved. Both modes now open with `FILE_SHARE_READ | FILE_SHARE_WRITE`, with no `FILE_SHARE_DELETE`, and appends still use `FILE_APPEND_DATA` only. The new test `ac8_concurrent_appends_and_reads_of_the_broker_log_all_succeed` runs 8 threads that append and read at once, then a read and an append while another append handle is held. It would have failed under the old flags.
- The new folder DACL check (`only_broker_writes`) requires a protected DACL and allows no other SID write-like rights. It runs after the reparse-point and owner checks. The test `ac8_a_folder_with_an_inherited_dacl_is_refused` covers it.
- Ran: `cargo test --locked`: every suite ok, 0 failed (`tests/broker.rs`: 31 passed, 1 ignored, the elevated SCM test). `cargo clippy --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.
- Criteria: AC2 now has the exact new SDDL, the mask read-back and the refused non-first create (`ac2_pipe_sddl_is_exact`, `ac2_pipe_dacl_allows_authenticated_users_and_the_broker_only`, `ac2_authenticated_users_cannot_add_an_instance_of_the_pipe`). AC8 is covered by the concurrency test, the inherited-DACL test and the earlier junction, link and owner tests. The other ACs are unchanged and their tests are green.
