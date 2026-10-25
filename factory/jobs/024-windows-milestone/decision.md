ESCALATE
All tests and four reviews pass, but the change adds a new place where Credentials are stored, Windows Credential Manager, which any same-user program can read, so a human should sign off.

## Acceptance criteria
I ran these in the worktree on Windows 11: `cargo test --locked` (every suite ok, 0 failed, only the 3 opt-in tests ignored), `cargo test --locked --test credential_manager -- --ignored` (2 passed, and `cmdkey /list` showed no `agent-mail` item afterwards), `cargo clippy --all-targets --locked -- -D warnings` (clean) and `cargo fmt --check` (clean).

- AC1a: pass (`tests/credential_manager.rs` `job024_ac1a_mandate_create_then_list_with_the_printed_token`, opt-in; I ran it with `--ignored`, and by default it is reported as ignored)
- AC1b: pass (`tests/credential_manager.rs` `job024_ac1b_store_round_trip_is_add_only_and_leaves_nothing`, opt-in; I ran it with `--ignored`)
- AC1c: pass (`tests/credential_layout.rs` `job024_ac1c_*`, 6 tests)
- AC1d: pass (`tests/audit.rs` `job024_ac1d_windows_log_is_under_local_app_data`, `job024_ac1d_windows_base_is_the_known_folder_not_an_environment_variable`, `job024_ac1d_platform_path_on_windows_ends_in_agent_mail_audit_log`)
- AC1e: pass (`tests/create.rs` `job024_ac1e_controlling_terminal_is_conout_on_windows_and_dev_tty_on_unix`, `ac11_real_terminal_writes_the_token_to_dev_tty_only`)
- AC2a: pass (`tests/selfcheck.rs` `job024_ac2a_self_check_passes_on_windows_and_the_guard_forwards`)
- AC2b: pass (`tests/selfcheck.rs` `job024_ac2b_self_check_is_macos_release_only_and_main_always_guards`)
- AC2c: pass (`tests/release.rs` `job024_ac2c_readme_says_the_hardened_runtime_check_is_macos_only`)
- AC3a: pass (`tests/release.rs` `job024_ac3a_build_windows_job_packages_and_checksums_the_exe`)
- AC3b: pass (`tests/release.rs` `job024_ac3b_publish_attests_and_releases_the_windows_zip_and_installer`)
- AC3c: pass (`tests/release.rs` `job024_ac3c_verify_release_checks_the_windows_zip`; the `sh -n` part runs on Unix only, so not on this machine)
- AC3d: pass (`tests/release.rs` `job024_ac3d_test_job_runs_on_macos_and_windows`)
- AC4a–f: pass (`tests/release.rs` `windows_installer::job024_ac4a_*` through `job024_ac4f_*`, run under Windows PowerShell 5.1; PowerShell 7 is not installed here)
- AC4g: pass (`job024_ac4g_installer_never_elevates_runs_the_binary_or_changes_path`, `windows_installer::job024_ac4g_non_x64_is_refused_with_a_message_naming_x64`)
- AC5a: pass (`tests/release.rs` `job024_ac5a_readme_windows_paragraph_says_a_hijacked_agent_can_read_credentials`)
- AC5b: pass (`tests/release.rs` `job024_ac5b_readme_and_release_notes_give_the_windows_install`)
- AC6a: pass (`tests/release.rs` `job024_ac6a_ci_workflow_tests_on_macos_and_windows`)
- AC6b: pass (`tests/release.rs` `job024_ac6b_gitattributes_checks_out_lf_everywhere`)
- AC6c: pass (full `cargo test --locked` on Windows: 0 failed, only the opt-in tests ignored)
- AC6d: pass on Windows only. No one ran clippy on macOS, because cross-checking needs a C compiler for `ring`. The new macOS/Windows CI matrix would catch a macOS failure.

## For the human
The work matches the spec and stays inside its scope (20 files, all listed in the spec). All four reviews in round 1 say `VERDICT: PASS` with no `[fix-now]`. Every criterion has a test that I saw pass. I am escalating because of what the change is, not because of its quality.

- **New Credential store.** Sealed Mandates, with Credentials inside, now go into Windows Credential Manager (`CRED_PERSIST_LOCAL_MACHINE`). There, DPAPI lets any program running as the same user decrypt them, including a hijacked Agent. ADR-0002 already accepts this, and the README says so plainly, but it is the first store outside the macOS Keychain's protection. Look at `src/store.rs` (the `credential_manager` module, about lines 280–520) and the README `**Windows.**` paragraph.
- **Accepted security trade-offs.**
  - Add-only `put` (ADR-0003) is "read every target name, then `CredWriteW`". Credential Manager has no create-only write, so a same-user race could overwrite a record.
  - The Windows audit log follows symlinks and junctions, unlike the Unix `O_NOFOLLOW` rule.
  - The security reviewer accepted both under ADR-0002.
- **Not verified on this machine.** macOS clippy, PowerShell 7, and the release workflow itself (`build-windows` and `publish` only run on a tag push). The new CI workflow will exercise macOS and PowerShell 7 on its first run.

If you approve, the job merges and the next `v*` tag publishes a Windows x64 zip, its `.sha256` and `install.ps1`. Consider pushing a pre-release tag first and running `scripts/verify-release.sh` on it. If you send it back for rework, say which trade-off is not acceptable (for example, the Windows build should not ship at all under an honest-Agents-only model, or the audit log must refuse reparse points before release).

## Follow-ups
- code · src/store.rs:447 (`CredentialManagerStore::put`): add-only is read-then-`CredWriteW`, so a same-user race can overwrite an item, because Credential Manager has no create-only write. Accepted under ADR-0002 → revisit only if the Windows threat model changes.
- code · src/terminal.rs (`reveal` doc comment): the reflowed comment leaves a short orphan line ("fd 1 or fd 2") → tidy the wrapping when the file is next touched.
- security · src/audit.rs:158-200: on Windows the audit log is opened with plain create+append, so a symlink or junction at `%LOCALAPPDATA%\agent-mail` or `audit.log` is followed → open with `FILE_FLAG_OPEN_REPARSE_POINT` and refuse a reparse point or a non-regular file, matching the Unix behaviour.
- ui · install.ps1:128: run through `irm ... | iex`, a failure only writes the error to stderr and the shell carries on with no non-zero status → add a short "nothing was installed" line on stdout in that mode, or a `$LASTEXITCODE` hint.
- ux · install.ps1:109: when `%USERPROFILE%\.local\bin` is not on PATH, the hint only says to add it to the user Path variable → print the exact one-line PowerShell command that adds it.
