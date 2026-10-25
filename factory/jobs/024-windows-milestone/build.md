# Build: 024-windows-milestone

## Round 1
**Changed:**
- `.gitattributes` (`* text=auto eol=lf`), worktree renormalized; every CRLF-sensitive test now passes on a Windows checkout.
- `src/store.rs`: new `credential_layout` module, pure and compiled on every OS. It covers target names `agent-mail:<base64url key>` and splitting over the 2,560-byte blob limit. The first part starts with a big-endian `u16` part count, and later parts go under `#1`, `#2`, and so on. `classify` sorts names into Record, Part and Foreign. New `CredentialManagerStore` (cfg(windows), `windows-sys` 0.61, already in the lock tree): generic credentials, `CRED_PERSIST_LOCAL_MACHINE`. `put` is add-only: it reads every target name before writing, writes the later parts first and the first part last, and removes what it wrote on failure. `entries()` counts undecodable names and records it cannot read with an empty value. `RuntimeStore = CredentialManagerStore` on Windows, fixed at compile time; `EmptyStore` stays for other non-macOS targets.
- `src/audit.rs`: `Platform::Windows`; `platform_path()` on Windows uses `SHGetKnownFolderPath(FOLDERID_LocalAppData)`, not an environment variable. The Unix-only XDG test and the `sample` helper are gated `#[cfg(unix)]`.
- `src/terminal.rs`: `TTY_PATH` is `/dev/tty` under `cfg(unix)` and `CONOUT$` under `cfg(windows)`. The Windows `write_tty` writes UTF-16 with `WriteConsoleW`, so non-ASCII names show correctly whatever the console code page. It never touches stdout or stderr.
- `install.ps1`: arch check (`AMD64` only, the message names x64). Downloads with `WebClient` (https and file:// on 5.1 and 7). The `.sha256` format is checked with a case-sensitive regex before use, then `Get-FileHash` is compared. Nothing is extracted before the checksum passes, and the zip must hold exactly one entry, `agent-mail.exe`. The binary is staged in the bin directory and then renamed in place (`File.Replace`/`File.Move`); the temporary directory is removed. No elevation, nothing executed, PATH only hinted. It runs in its own `& { }` scope, so `irm | iex` does not leak settings or close the shell; `-File` runs exit 1 on failure.
- `.github/workflows/release.yml`: the `test` job runs on a matrix of macOS and Windows (version check `shell: bash`). New read-only `build-windows` job: build, `Compress-Archive`, `.sha256` via `printf '%s  %s\n'`, `upload-artifact` (v7.0.2, SHA pinned). `publish` needs `[test, build-windows]`, downloads the artifact (`download-artifact` v8.0.2, SHA pinned), attests the zip and publishes the zip, its `.sha256` and `install.ps1`. New `.github/workflows/ci.yml`. `scripts/verify-release.sh` checks the Windows zip (format, checksum, `unzip -Z1` contents, attestation). There is no signature check and nothing is run.
- README (Install: Windows one-liner, attestation command, the hardened-runtime check being macOS-only; the Windows paragraph; Windows audit log path; Releasing/CI; Credential Manager tests), release notes, CLAUDE.md (targets, new files and tests).
- Tests made to compile and pass on Windows: `tests/release.rs` `ac3_scripts_are_executable_and_parse` is gated `#[cfg(unix)]`. In `tests/export.rs`, the symlink part of `export_never_overwrites_or_follows_a_symlink` and the FIFO part of `from_must_name_a_regular_file_and_is_never_opened_otherwise` are gated `#[cfg(unix)]`. The FIFO part failed on Windows because `mkfifo` from Git Bash creates nothing usable. `ac11_real_terminal_writes_the_token_to_dev_tty_only` now checks both `write_tty`s.

**Commits:** a252588, 5c3e2ef, a506ef2, b711af0

**Tests:** run on Windows 11 (x86_64-pc-windows-msvc), Windows PowerShell 5.1:
- `cargo test --locked` passed: 424 tests, 0 failed. The only 3 ignored tests are the opt-in ones: `gmail_live` and the 2 in `credential_manager`.
- `cargo test --locked --test credential_manager -- --ignored`: `test result: ok. 2 passed; 0 failed` against the real Credential Manager. `cmdkey /list` shows no `agent-mail` item afterwards.
- `cargo clippy --all-targets --locked -- -D warnings`, `cargo clippy --release --locked -- -D warnings` (no `test-support`) and `cargo fmt --check` are clean on Windows. macOS was not checked locally: cross-checking needs a C compiler for `ring`. The macOS cfg paths are unchanged apart from code that compiles on every OS, and `WINDOWS_ASSET` was moved into the cfg(windows) module so it is not dead code on macOS.

| AC | Test | Status |
|----|------|--------|
| AC1a | `tests/credential_manager.rs` `job024_ac1a_mandate_create_then_list_with_the_printed_token` (opt-in) | pass (with `--ignored`); ignored by default |
| AC1b | `tests/credential_manager.rs` `job024_ac1b_store_round_trip_is_add_only_and_leaves_nothing` (opt-in) | pass (with `--ignored`); ignored by default |
| AC1c | `tests/credential_layout.rs` `job024_ac1c_*` (6 tests: target name, split/join for 0/1/2560/2561/6000, 3 ordered names, missing part, foreign names, persist/add-only source check) | pass |
| AC1d | `tests/audit.rs` `job024_ac1d_windows_log_is_under_local_app_data`, `job024_ac1d_windows_base_is_the_known_folder_not_an_environment_variable`, `job024_ac1d_platform_path_on_windows_ends_in_agent_mail_audit_log` | pass |
| AC1e | `tests/create.rs` `job024_ac1e_controlling_terminal_is_conout_on_windows_and_dev_tty_on_unix`, `ac11_real_terminal_writes_the_token_to_dev_tty_only` | pass |
| AC2a | `tests/selfcheck.rs` `job024_ac2a_self_check_passes_on_windows_and_the_guard_forwards` | pass |
| AC2b | `tests/selfcheck.rs` `job024_ac2b_self_check_is_macos_release_only_and_main_always_guards` | pass |
| AC2c | `tests/release.rs` `job024_ac2c_readme_says_the_hardened_runtime_check_is_macos_only` | pass |
| AC3a | `tests/release.rs` `job024_ac3a_build_windows_job_packages_and_checksums_the_exe` | pass |
| AC3b | `tests/release.rs` `job024_ac3b_publish_attests_and_releases_the_windows_zip_and_installer` | pass |
| AC3c | `tests/release.rs` `job024_ac3c_verify_release_checks_the_windows_zip` (`sh -n` part Unix only) | pass |
| AC3d | `tests/release.rs` `job024_ac3d_test_job_runs_on_macos_and_windows` | pass |
| AC4a | `tests/release.rs` `windows_installer::job024_ac4a_matching_checksum_installs_agent_mail_exe` | pass |
| AC4b | `windows_installer::job024_ac4b_existing_install_is_replaced` | pass |
| AC4c | `windows_installer::job024_ac4c_checksum_mismatch_installs_nothing` | pass |
| AC4d | `windows_installer::job024_ac4d_malformed_checksum_file_installs_nothing` | pass |
| AC4e | `windows_installer::job024_ac4e_missing_zip_installs_nothing` | pass |
| AC4f | `windows_installer::job024_ac4f_zip_without_only_agent_mail_exe_installs_nothing` | pass |
| AC4g | `job024_ac4g_installer_never_elevates_runs_the_binary_or_changes_path` (every OS), `windows_installer::job024_ac4g_non_x64_is_refused_with_a_message_naming_x64` | pass |
| AC5a | `tests/release.rs` `job024_ac5a_readme_windows_paragraph_says_a_hijacked_agent_can_read_credentials` | pass |
| AC5b | `tests/release.rs` `job024_ac5b_readme_and_release_notes_give_the_windows_install` | pass |
| AC6a | `tests/release.rs` `job024_ac6a_ci_workflow_tests_on_macos_and_windows` | pass |
| AC6b | `tests/release.rs` `job024_ac6b_gitattributes_checks_out_lf_everywhere` | pass |
| AC6c | the full `cargo test --locked` on Windows: 0 failures, only the opt-in tests ignored | pass |
| AC6d | `cargo clippy --all-targets --locked -- -D warnings` on Windows | pass (Windows only, see Tests) |

**Notes for reviewers:**
- The installer tests run under `powershell.exe` (5.1) always, and also under `pwsh.exe` when it is installed. PowerShell 7 is not installed on this machine, so 7 is exercised only on the GitHub Windows runner.
- AC1a deletes through `mandate delete <id>`. `mandate delete` treats any undecodable `agent-mail:*` item in the user's Credential Manager as unreadable (`revocation_incomplete`), as `KeychainStore` does. A Credential Manager holding such junk would make that opt-in test fail.
- Credential Manager has no create-only write, so add-only means "read every target name, then `CredWriteW`". A same-user race between the read and the write is possible, but ADR-0002 already accepts that any same-user process can read and write these items.

**Review findings:** none (round 1).
