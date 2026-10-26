VERDICT: PASS

## Findings
- [follow-up] tests/installers.rs: the elevated tests (AC1, AC2, AC6) could not be run in this review; the shell is not elevated. The first real run happens on Windows CI. Watch that first CI run for timing issues (30 s Running wait, `sc start ... purge` wait) and the pwsh leg.
- [follow-up] CLAUDE.md: the diff edits the layout line, which the spec defers to Ticket 22. It is needed so `tests/project.rs::claude_md_stack_section_records_layout` stays green, so I accept it. Ticket 22 should add `uninstall.ps1` and the new flow to the "Release files" line.

## Checked
- Read: full `git diff main...factory/027-broker-installers --stat`; `install.ps1` and `uninstall.ps1` in full; the diffs of `src/broker.rs` (`purge`, `is_purge_start`, `remove_audit_dir`), `src/broker/windows.rs` (`start_arguments`, `run_purge`, `service_main`), `src/store.rs` (`delete_unfiled_targets`), `src/audit.rs`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `scripts/verify-release.sh`, `CLAUDE.md`, `tests/broker.rs`; `tests/installers.rs` helpers, the CleanMachine guard and the AC1 test, plus a grep of all test names and assertions; `build.md`.
- Security: paths come from `GetFolderPath` and `SystemDirectory`; staging is `.staging-<guid>` inside the install folder with an explicit icacls DACL; the service `binPath` is quoted; the SDDL is set with `sdset`; the URL comes only from `-DownloadUrl`, not the environment; extraction comes after the checksum; `purge` runs only for exactly one argument `purge`, and the unit test covers the near-misses. The elevated copy never relaunches, and `Start-Process -Verb RunAs` appears once behind the Administrators check. The `iex` case relaunches with the block's own text, gzip-compressed, to stay under the command line limit.
- Ran: `cargo test` in the worktree: every `test result` line is ok, 0 failed; the 13 elevated installer tests are ignored by design. `cargo clippy --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.
- Criteria:
  - AC1: the static checks in `tests/release.rs` (single RunAs, checksum before extract, quoted path, SDDL) and the ignored `job027_ac1_*` tests (service config, DACL, Path, `whoami`, version line, each verification failure).
  - AC2: the ignored `job027_ac2_*` tests (update with an MCP process running, restore on a broken binary).
  - AC3: `broker::job027_ac3_purge_*` (in-process purge and argv parsing, would fail without `purge`), the static uninstall check, and the ignored uninstall tests.
  - AC4: the parse test under each shell and the publish static test.
  - AC5: the verify-release static test.
  - AC6: the CI step static test, plus the ignored virtual-account, restart, `-Force` and `-KeepData` tests.
  - The rollback when the service does not reach Running within 30 seconds is untested, as `build.md` states; the code path matches the spec.
