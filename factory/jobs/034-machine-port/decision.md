APPROVE
All four round-1 reviews pass with no fix-now findings; every criterion has a test, and the suite, clippy, fmt and the release build pass on macOS.

## Acceptance criteria
- AC1: pass (`ac1_context_new_uses_the_platform_machine`, `ac1_with_machine_swaps_the_machine`, `ac1_builder_order_does_not_matter`, `ac1_trusting_only_uses_the_platform_machine`)
- AC2: pass on macOS (`ac2_environment_does_not_redirect_home_or_app_data` with `ac2_probe_prints_platform_folders`, `ac2_macos_app_data_matches_the_audit_log_and_install_dir_matches_install_sh`, `ac2_claude_dirs_end_with_the_usual_folders`, `ac2_current_dir_and_current_exe_are_the_process_values`). The Windows variant `ac2_windows_app_data_and_install_dir_come_from_known_folders` is first run in Windows CI. I checked `FOLDERID_Profile`, `FOLDERID_ProgramFiles`, `FOLDERID_RoamingAppData`, `windows_sys::core::GUID` and `SHGetKnownFolderPath` against the windows-sys 0.61.2 source.
- AC3: pass on macOS (`ac3_run_returns_exit_code_and_output`, `ac3_run_uses_the_working_folder`, `ac3_a_missing_program_is_not_started`). The Windows `cmd.exe` variant runs in CI.
- AC4: pass (`ac4_each_scripted_machine_gets_a_fresh_root_under_temp`, `ac4_every_folder_is_distinct_and_under_root`, `ac4_replies_play_back_in_order_and_commands_are_recorded`, `ac4_running_out_of_replies_panics_naming_the_program`, `ac4_a_program_other_than_claude_or_git_panics`, `ac4_platform_suffixes_of_claude_and_git_are_accepted`, `ac4_dropping_the_machine_removes_its_root`, `ac4_scripted_machine_is_test_only`). `cargo build --release` succeeds, and the release binary has no `agent-mail-scripted-machine` marker (grep count 0).
- AC5: pass (`ac5_context_runs_through_the_scripted_machine`). `git diff main...factory/034-machine-port --stat -- tests/ src/main.rs Cargo.toml Cargo.lock` lists only the new `tests/machine.rs`, and every existing suite passes.
- AC6: pass (`tests/project.rs::claude_md_stack_section_records_layout`, `ac6_claude_md_names_the_machine_port`)
- AC7: pass on macOS (`cargo test` with all suites ok and `tests/machine.rs` 22 passed, `cargo clippy -- -D warnings` clean, `cargo fmt --check` clean)

## For the human
Nothing needs a decision. This is a prefactor with no visible change. It adds no new dependencies, and nobody calls `run` yet. The Windows code paths have not been compiled locally, so the first Windows CI run is their first build. Watch that run.

## Follow-ups
- code · src/audit.rs (`local_app_data`): the function still calls `SHGetKnownFolderPath` itself so that the `tests/audit.rs` source check can see it, so the known-folder call exists twice → route it through `known_folder_path` and update that source check.
- security · tests/project.rs:181: the release marker `agent-mail-scripted-machine` (src/machine.rs:244) is not in `TEST_ONLY_MARKERS`, so `ac11_release_binary_has_no_test_only_code` does not cover `ScriptedMachine` → add `("agent-mail-scripted-machine", "src/machine.rs")` to the list.
- security · src/machine.rs:92: `PlatformMachine::run` passes the whole environment to the child, including any `AGENT_MAIL_TOKEN` → ticket 04 should `env_remove` the token variable before running `claude` or `git`, and test that.
- security · src/machine.rs:92: on Windows, `claude.cmd` arguments are escaped by Rust's batch-file handling, and the fix for CVE-2024-24576 needs Rust 1.77.2 or later, but the crate pins no `rust-version` → ticket 04 should pass only fixed arguments to `.cmd`/`.bat` targets, or set a `rust-version` floor.
- security · src/broker/windows.rs:993: the Broker session's `Context` gets `&PlatformMachine`, so a future handler calling `run` would run the program as `NT SERVICE\agent-mail` → when tickets 08 and 09 touch the Broker, give the session an inert `Machine` that returns `NotStarted` and `None`, unless a request really needs it.
