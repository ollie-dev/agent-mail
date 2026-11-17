# Build: 034-machine-port

## Round 1
**Changed:**
- New `src/machine.rs`: the `Machine` trait, `RunOutcome` / `ProgramOutput`, the unit struct `PlatformMachine` (macOS, other Unix, Windows and a stub for other OSes), and the `scripted` module with `ScriptedMachine`, `Reply` and `RecordedCommand` behind `cfg(any(test, feature = "test-support"))`. The module carries a release marker `agent-mail-scripted-machine`, like the scripted terminal.
- `PlatformMachine`: home from `audit::home_from_user_database` (Unix) or `FOLDERID_Profile`; app data `<home>/Library/Application Support` (macOS) or `FOLDERID_RoamingAppData`; install dir `<home>/.local/bin` or `FOLDERID_ProgramFiles\agent-mail`; `claude_dirs` = absolute `PATH` entries (relative and empty dropped), then the usual folders from the spec table. `run` uses `std::process::Command` directly with `current_dir(dir)`, stdin null, piped stdout/stderr, `from_utf8_lossy`. A spawn error gives `NotStarted { reason }`.
- `ScriptedMachine`: root `temp_dir/agent-mail-machine-<pid>-<nanos>-<n>`, removed on drop. Folders are `home`, `app-data`, `project` (created), `downloads/agent-mail[.exe]`, `install`, and `claude_dirs` = `[path-bin, home/.local/bin]`. `run` accepts only file stems `claude` and `git` (so `claude.exe`, `claude.cmd` and `git.exe` work), records every call, and panics naming the program when the script runs out. It uses `Mutex`, so it is `Sync`.
- `Context` gained `machine: &'a dyn Machine`. `Context::new` sets `&PlatformMachine`. Added `with_machine` and the public `machine()` accessor. `execute_reporting` and broker `context_for` carry `context.machine`, and the Windows Broker session uses `&PlatformMachine`.
- `src/audit.rs`: `home_from_user_database` is now `pub(crate)`. There is a new `pub(crate) known_folder_path(&GUID)`, which `program_data()` and `machine.rs` use. `local_app_data()` keeps its spelled-out `SHGetKnownFolderPath(&FOLDERID_LocalAppData, ...)` call, because the existing `tests/audit.rs::job024_ac1d_windows_base_is_the_known_folder_not_an_environment_variable` greps for that text and AC5 forbids changing existing tests. It still shares the `known_folder` decode-and-free helper. The audit log's behaviour is unchanged.
- `CLAUDE.md`: the layout line names `machine.rs` (the `Machine` port, `PlatformMachine` and `ScriptedMachine`) and `tests/machine.rs`. The Test-only code line names `ScriptedMachine`.

**Commits:** 98d2df6
**Tests:** `cargo test` gave all suites `ok` (`tests/machine.rs`: `test result: ok. 22 passed; 0 failed`). `cargo clippy -- -D warnings` and `cargo clippy --all-targets -- -D warnings` were clean, and `cargo fmt --check` was clean. `cargo build --release` succeeded, and the release binary does not contain `agent-mail-scripted-machine`, while the `tests/machine` test binary does. Run on macOS only. The Windows code paths (`known_folder_path` with `FOLDERID_Profile` / `RoamingAppData` / `ProgramFiles`, and the Windows tests) were checked against the `windows-sys` 0.61.2 source but not compiled here, because no Windows target is installed. CI on Windows is their first run.

| AC | Test | Status |
|----|------|--------|
| AC1 | `ac1_context_new_uses_the_platform_machine`, `ac1_with_machine_swaps_the_machine`, `ac1_builder_order_does_not_matter`, `ac1_trusting_only_uses_the_platform_machine` | pass |
| AC2 | `ac2_environment_does_not_redirect_home_or_app_data` (re-runs `ac2_probe_prints_platform_folders` with `HOME`/`USERPROFILE`/`APPDATA`/`PATH` overridden), `ac2_macos_app_data_matches_the_audit_log_and_install_dir_matches_install_sh` (macOS), `ac2_windows_app_data_and_install_dir_come_from_known_folders` (Windows, not run here), `ac2_claude_dirs_end_with_the_usual_folders` (macOS and Windows variants), `ac2_current_dir_and_current_exe_are_the_process_values` | pass (macOS) |
| AC3 | `ac3_run_returns_exit_code_and_output` (Unix and Windows variants), `ac3_run_uses_the_working_folder`, `ac3_a_missing_program_is_not_started` | pass (macOS) |
| AC4 | `ac4_each_scripted_machine_gets_a_fresh_root_under_temp`, `ac4_every_folder_is_distinct_and_under_root`, `ac4_replies_play_back_in_order_and_commands_are_recorded`, `ac4_running_out_of_replies_panics_naming_the_program` (should_panic "git"), `ac4_a_program_other_than_claude_or_git_panics` (should_panic "npm"), `ac4_platform_suffixes_of_claude_and_git_are_accepted`, `ac4_dropping_the_machine_removes_its_root`, `ac4_scripted_machine_is_test_only` (source check of the cfg gate). Manual: `cargo build --release` succeeded without the feature. | pass |
| AC5 | `ac5_context_runs_through_the_scripted_machine`. Check: `git diff main --stat -- tests/ src/main.rs Cargo.toml Cargo.lock` lists only the new `tests/machine.rs` | pass |
| AC6 | `tests/project.rs::claude_md_stack_section_records_layout`, `ac6_claude_md_names_the_machine_port` | pass |
| AC7 | `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check` (macOS) | pass |

**Review findings:** none (round 1).

**Notes for reviewers:**
- The new marker `agent-mail-scripted-machine` is not in `tests/project.rs`'s `TEST_ONLY_MARKERS`, because AC5 forbids editing existing test files. A follow-up could add it there.
- Other Unix (not a target) gets `app_data` = `<home>/.config` and `claude_dirs` without `/opt/homebrew/bin`. It only needs to compile.
