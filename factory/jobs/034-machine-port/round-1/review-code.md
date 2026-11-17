VERDICT: PASS

## Findings
- [follow-up] src/audit.rs:`local_app_data`: still calls `SHGetKnownFolderPath` itself (the comment says this is so a source check can see the audit log's rule), so the known-folder call exists twice. A later job could route it through `known_folder_path` and update that source check.

## Checked
- Read: the full diff `git diff main...factory/034-machine-port` for `src/` and `CLAUDE.md`. That covers `src/machine.rs` (trait, `PlatformMachine`, `absolute_entries`, the platform modules, `scripted`), `src/core.rs` (`Context`, `new`, `with_machine`, `machine()`, `execute_reporting`), `src/broker.rs` (`context_for`), `src/broker/windows.rs`, and `src/audit.rs` (`known_folder_path`, `home_from_user_database`). I also listed the test names in `tests/machine.rs`.
- Ran: `cargo test`: all suites green, including `tests/machine.rs` with 22 passed. `cargo clippy --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean. `cargo build --release`: succeeds. `git diff main --stat -- tests/`: only the new `tests/machine.rs`.
- Criteria:
  - AC1: the `ac1_*` tests cover `Context::new`, `with_machine`, builder order and `trusting_only`. They would fail without the field.
  - AC2: the probe test with the environment overrides, the macOS audit-path and install-dir test, the Windows known-folder test, `claude_dirs` and the current dir/exe tests. `absolute_entries` also has a unit test, and it drops relative entries.
  - AC3: the `ac3_*` tests cover exit code and output, the working folder, and `NotStarted`. `run` uses `Command` directly with stdin null, piped output and lossy decoding.
  - AC4: the `ac4_*` tests cover fresh roots, distinct folders, ordered replay and recording, the two `should_panic` cases, the suffixes, removal on drop, and the cfg-gated scripted module.
  - AC5: `ac5_context_runs_through_the_scripted_machine`.
  - AC6: `ac6_claude_md_names_the_machine_port`.
  - AC7: lint, format and release build are clean, and `machine()` is public, so there is no dead-code warning.
