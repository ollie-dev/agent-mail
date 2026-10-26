# Build: 027-broker-installers

## Round 1
**Changed:**
- `install.ps1` is rewritten for an elevated, system-wide install:
  - Without admin rights it runs one `Start-Process -Verb RunAs -Wait -PassThru` of the current host. The host gets `-EncodedCommand` with the script block's own text (`$MyInvocation.MyCommand.ScriptBlock`), gzip-compressed, so it also works through `irm | iex` and stays well under 32,767 characters. The URL is passed base64-encoded inside that command line. The parent exits with the child's code.
  - The download URL comes only from `-DownloadUrl`. `AGENT_MAIL_DOWNLOAD_URL` is no longer read.
  - The elevated copy (`-Relaunched`) never relaunches, and waits for Enter before closing.
  - Paths come from `GetFolderPath('ProgramFiles')` and `[Environment]::SystemDirectory`. The folder DACL is set explicitly with `icacls`: SYSTEM and Administrators full control, Users read and execute.
  - Download, checksum check and extraction happen in `.staging-<guid>` inside the install folder. Then the new binary's `--version` is checked.
  - Updates: the script stops the Broker, renames the old exe aside (this works while an MCP process runs it), moves the new one in, runs `sc create`/`sc config` with `binPath= "\"<exe>\" broker"`, `obj= "NT SERVICE\agent-mail"` and `start= delayed-auto`, then `sc sdset` with the AC1 SDDL.
  - It starts the Broker and waits 30 s for Running. On failure it restores the previous binary. Last, it appends `;<folder>` to the raw machine Path, keeping the value kind, and broadcasts `WM_SETTINGCHANGE`.
  - A failed first install removes the service and folder it created.
- `uninstall.ps1` (new) takes `-KeepData` (wins) and `-Force`:
  - It asks for confirmation before elevating; in a `-NonInteractive` host it refuses and exits 1.
  - It relaunches elevated the same way, passing `-Force` or `-KeepData` explicitly.
  - It stops the Broker, runs `sc.exe start agent-mail purge`, waits for STOPPED and checks the `Win32_Service` `ExitCode`/`ServiceSpecificExitCode` are 0.
  - It then runs `sc delete`, deletes the folder (with retries) and removes every matching Path entry, keeping the kind.
  - It prints `agent-mail was removed.` If nothing is installed it prints `agent-mail is not installed.`
- `src/broker.rs`:
  - `purge(store, audit_dir) -> Result<usize, PurgeError>` deletes every record, checks the store is empty, deletes `audit.log` and the folder, and unlinks a link without following it.
  - `is_purge_start(args)` is true only for exactly one argument `purge`.
- `src/broker/windows.rs`: `service_main` reads its start arguments. A purge start reports Running, runs `CredentialManagerStore::delete_unfiled_targets` (new in `store.rs`: later parts and undecodable names) and then `purge` on `AuditLog::broker_dir()` (new in `audit.rs`). It opens no pipe and stops with code 0, or service-specific code 2 (`PURGE_FAILED`).
- Workflows:
  - `ci.yml` and the `release.yml` `test` job have an `Installer tests` step guarded by `if: runner.os == 'Windows'`.
  - `publish` copies and uploads `uninstall.ps1`.
- `scripts/verify-release.sh` downloads `install.ps1` and `uninstall.ps1` and compares each with `git show "$tag:<name>"`. The header comment says so.
- `tests/release.rs`: the per-user `job024_ac4*` installer tests and `job024_ac4g_installer_never_elevates_...` are removed. Static checks and the parse and relaunch-length tests are added.
- `tests/installers.rs` (new): the elevated, ignored-by-default tests. `tests/broker.rs`: the purge tests.
- `CLAUDE.md`: the layout line now names `tests/installers.rs` and updates the `tests/release.rs` description. The spec defers this to Ticket 22, but the existing `tests/project.rs::claude_md_stack_section_records_layout` fails unless every test file is named.

**Commits:** ce7e32a

**Tests:** `cargo test` gave 500 passed, 0 failed, 13 ignored, every `test result: ok`. `cargo clippy -- -D warnings`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

**Not run here:** the ignored `tests/installers.rs` tests. This shell is not elevated, so `cargo test --locked --test installers -- --ignored --test-threads=1` will first run on Windows CI. The tests compile, and the scripts parse with zero errors under Windows PowerShell 5.1. pwsh is not installed here.

What I checked by hand:
- The non-elevated paths: the ARM64 refusal, and `uninstall.ps1` with nothing installed, both from a file and through `iex`.
- The `-EncodedCommand` gzip relaunch mechanics, run without `RunAs`: the exit code and an awkward URL came through intact.

| AC | Test | Status |
|----|------|--------|
| AC1 | installers::job027_ac1_install_registers_and_starts_the_broker | ignored (elevated, CI) |
| AC1 | installers::job027_ac1_failed_verification_installs_nothing | ignored (elevated, CI) |
| AC1 | release::job027_ac1_install_elevates_once_behind_the_administrators_check | pass |
| AC1 | release::job027_ac1_install_verifies_before_extracting_and_stages_in_the_install_folder | pass |
| AC1 | release::job027_ac1_install_registers_the_broker_with_a_quoted_path_and_an_explicit_access_list | pass |
| AC1 | release::powershell::job027_ac1_the_relaunch_command_line_fits | pass |
| AC2 | installers::job027_ac2_rerun_updates_in_place_and_keeps_the_mandates | ignored (elevated, CI) |
| AC2 | installers::job027_ac2_a_binary_that_does_not_run_restores_the_previous_version | ignored (elevated, CI) |
| AC3 | broker::job027_ac3_purge_deletes_every_mandate_and_the_audit_folder | pass |
| AC3 | broker::job027_ac3_purge_runs_only_for_the_single_start_argument_purge | pass |
| AC3 | release::job027_ac3_uninstall_asks_before_elevating_and_purges_through_the_broker | pass |
| AC3 | installers::job027_ac3_uninstall_without_an_install_changes_nothing | ignored (elevated, CI) |
| AC3 | installers::job027_ac3_uninstall_without_a_confirmation_or_switch_changes_nothing | ignored (elevated, CI) |
| AC4 | release::powershell::job027_ac4_both_scripts_parse_under_each_shell | pass |
| AC4 | release::job027_ac4_publish_uploads_both_installers | pass |
| AC4 | release::job027_ac4_scripts_are_ascii_with_lf_line_endings | pass |
| AC5 | release::job027_ac5_verify_release_checks_the_installer_assets | pass |
| AC6 | release::job027_ac6_windows_ci_runs_the_installer_tests | pass |
| AC6 | installers::job027_ac6_broker_runs_as_its_virtual_account_and_keeps_mandates_over_a_restart | ignored (elevated, CI) |
| AC6 | installers::job027_ac6_uninstall_with_force_leaves_nothing_behind | ignored (elevated, CI) |
| AC6 | installers::job027_ac6_uninstall_with_keep_data_keeps_the_mandates_and_the_audit_log | ignored (elevated, CI) |

**Review findings:** none (round 1).

**Choices reviewers can check:**
- **Purge return value.** I read "`purge` ... returns 2" in AC3 as the number of Mandates deleted, so `purge` returns `Ok(2)`. Service-specific code 2 is the failure exit of the service, `PURGE_FAILED`.
- **`sdshow` check.** The test compares the `D:` part of `sc sdshow` exactly with the AC1 string. `sdshow` may append an `S:` audit part that the installer does not set.
- **Storing a Mandate (AC6).** The Mandate is stored by sending `hello` and `create` frames over a pipe opened with the client's own `PipeConnector::broker()`, which checks the pipe owner and server. The client `Session` type is `pub(crate)`. The live login goes to the fake IMAP server over TLS on `localhost`, and its CA is added to `Cert:\LocalMachine\Root` behind a drop guard.
- **Virtual account (AC6).** The test reads the service process's SID with `win::process_user_sid`. If an elevated admin cannot open the service token, it falls back to WMI `Win32_Process.GetOwnerSid`.
- **-KeepData test.** It passes `-KeepData -Force`, which shows both that the data is kept and that `-KeepData` wins. An audited `list` with no token runs first, so `audit.log` exists before removal.
- **Test cleanup.** The test guard (`CleanMachine`) also runs the purge before deleting the service, so Mandates stored by one test do not leak into later tests.
- **Not covered by automated tests.** The relaunch through UAC, and the "service does not reach Running within 30 s" rollback.

## Round 2
**Changed:**
- Fixed the problem in the human note of 2026-10-26: the elevated AC2 and AC6 installer tests failed with `tls_certificate`.
  - Cause: the fake IMAP server's test CA and its leaf certificate both used rcgen's default DN. Windows' chain engine therefore took the leaf as self-signed and never chained it to the CA in `LocalMachine\Root`.
  - Fix: in `tests/support/mod.rs` the CA is now `CN=agent-mail test CA` (`TEST_CA_NAME`) and the leaf is `CN=localhost` (`TEST_LEAF_NAME`).
  - Certificate generation moved into `pub fn test_certificates()` (CA, leaf, key) so a test can inspect the leaf. `tls_config()` uses it.
- New test `installers::job027_fake_server_certificate_chains_to_the_test_ca_on_windows`. It is not ignored, since it needs no rights and changes nothing.
  - It builds the leaf's chain with .NET `X509Chain` (Windows' chain engine), with the CA only in `ExtraStore`, and asserts two elements ending at the CA's thumbprint, plus the leaf subject `CN=localhost`.
  - Checked against the old code: with the default DNs restored, it fails with `the leaf does not chain to the CA: 1 ... CN=rcgen self signed cert`. That is the failure the human saw.
  - The header of `tests/installers.rs` now says this one test runs by default.

**Commits:** 8acc80e

**Tests:** `cargo test` gave 501 passed, 0 failed, 13 ignored, every `test result: ok`. `cargo clippy -- -D warnings`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean. The elevated `--ignored` installer tests still run only on Windows CI, because this shell is not elevated.

| AC | Test | Status |
|----|------|--------|
| AC2, AC6 (fixture) | installers::job027_fake_server_certificate_chains_to_the_test_ca_on_windows | pass |
| AC1 to AC6 | unchanged from Round 1 | as in Round 1 |

**Review findings:** no review files this round. Human note 2026-10-26 → fixed as described above.

## Round 3
**Changed:** fixed the PSModulePath risk from the human note of 2026-10-26 (the round-1 security `[follow-up]` at install.ps1:349 and uninstall.ps1:219).
- **Elevated copy's host.** It is now always `[Environment]::SystemDirectory\WindowsPowerShell\v1.0\powershell.exe`, which only administrators can replace. Before, it was the caller's own host (`(Get-Process -Id $PID).Path`), which might be a per-user `pwsh.exe`.
- **Bootstrap.** The `-EncodedCommand` bootstrap now uses only .NET (`[IO.Compression.GZipStream]::new`, `[IO.StreamReader]::new`), so nothing autoloads before the script block runs. Before, it used `New-Object`, which autoloads `Microsoft.PowerShell.Utility`.
- **Module hardening.** The first statements of both script blocks are .NET and built-in commands only.
  - `$IsAdministrator` is computed with `[WindowsPrincipal]::new(...)`; this replaces the `Test-Administrator` function, which used `New-Object`.
  - When elevated, the block sets `$env:PSModulePath` to `$PSHOME\Modules`, imports what it needs from there by full `.psd1` path, and sets `$PSModuleAutoLoadingPreference = 'None'`. `install.ps1` imports Management and Utility; `uninstall.ps1` also imports CimCmdlets.
  - A module planted in the user-writable `Documents\...\Modules` can therefore no longer supply a command.
  - `PSModulePath` is restored before the end, so an operator's shell running the script through `iex` gets it back.
- **Checksum.** `Get-FileHash` is replaced by `[Security.Cryptography.SHA256]`.

New tests in `tests/release.rs`:
- `job027_elevated_scripts_load_only_the_modules_powershell_ships_with` is a static check. Everything in the block before autoloading is turned off must be exactly the .NET and built-in lines. The bootstrap must have no `New-Object`, the scripts no `Get-FileHash`, and `PSModulePath` must be restored before `exit`.
- `powershell::job027_a_module_planted_on_psmodulepath_never_loads_after_the_hardening` runs on Windows under each shell, needs no rights and changes nothing.
  - It puts a module first on `PSModulePath` that exports a planted `Get-Service`, `Get-CimInstance` and `Write-Host`.
  - Without the hardening, the planted commands win; the test asserts this as its control.
  - With each script's hardening block, the real commands run.
- `assert_elevates_once` now requires the system Windows PowerShell as the relaunch host and no `Get-Process`.

Manual check: with the hardening, a planted `Get-FileHash` and `Get-Service` did not load, and the real cmdlets came from `C:\Windows\System32\WindowsPowerShell\v1.0`. The ARM64 refusal and `uninstall.ps1` through `iex` still work, and `PSModulePath` is unchanged afterwards. The new .NET-only bootstrap decodes and passes the exit code through.

**Commits:** 6d52102

**Tests:** `cargo test` gave 503 passed, 0 failed, 13 ignored, every `test result: ok`. `cargo clippy -- -D warnings`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean. Both scripts parse with 0 errors under Windows PowerShell 5.1. The elevated `--ignored` installer tests still run only on Windows CI.

| AC | Test | Status |
|----|------|--------|
| AC1, AC3 (security) | release::job027_elevated_scripts_load_only_the_modules_powershell_ships_with | pass |
| AC1, AC3 (security) | release::powershell::job027_a_module_planted_on_psmodulepath_never_loads_after_the_hardening | pass |
| AC1, AC3 | release::job027_ac1_install_elevates_once_behind_the_administrators_check, release::job027_ac3_uninstall_asks_before_elevating_and_purges_through_the_broker (updated) | pass |
| others | unchanged from Rounds 1 and 2 | as before |

**Review findings:** round 2 had no `[blocking]` or `[fix-now]` findings. Human note 2026-10-26 ("fix the psmodulepath risk") → fixed as described above.

**Scope note:** only an elevated run is hardened. A non-elevated parent still uses its own host and modules, because it gains no rights. An operator who runs the script from a shell they already elevated keeps whatever that shell has loaded before.

## Round 4
**Changed:** fixed the one round-3 finding, `[blocking]` security at install.ps1:199 and uninstall.ps1:124. The elevated copy compiled C# through the user-writable `%TEMP%`.
- **Cause.** The `WM_SETTINGCHANGE` broadcast used `Add-Type -MemberDefinition` (a `SendMessageTimeout` P/Invoke). Under Windows PowerShell 5.1 that writes `.cs` and `.tmp` files to `%TEMP%`, runs `csc.exe` on them and loads the result into the admin process.
- **Fix.** Both `Add-Type -MemberDefinition` calls are removed. After the raw registry write, both scripts now call `[Environment]::SetEnvironmentVariable('AGENT_MAIL_INSTALLER_UNUSED', $null, 'Machine')`. Deleting that missing value changes nothing, and .NET then sends the `WM_SETTINGCHANGE` "Environment" broadcast itself, without compiling anything.
  - The Path itself is still written raw, with its value kind kept, as before.
  - The `Add-Type` calls left are only `-AssemblyName System.IO.Compression(.FileSystem)`, which load from the GAC.
- **Test.** New static test `release::job027_elevated_scripts_compile_no_code`. It fails if either script contains:
  - `-MemberDefinition`, `-TypeDefinition`, `Add-Type -Path`/`-LiteralPath`, `DllImport`, `SendMessageTimeout` or a CodeDom compiler;
  - any `Add-Type` other than `-AssemblyName System.IO.Compression...`;
  - any `SetEnvironmentVariable` other than the single broadcast line.

**Commits:** 168b359

**Tests:**
- `cargo test` gave 504 passed, 0 failed, 13 ignored, every `test result: ok`.
- `cargo clippy -- -D warnings`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.
- Both scripts parse with 0 errors under Windows PowerShell 5.1.

| AC | Test | Status |
|----|------|--------|
| AC1, AC3 (security) | release::job027_elevated_scripts_compile_no_code | pass |
| others | unchanged from Rounds 1 to 3 | as before |

**Review findings:**
- security `[blocking]` install.ps1:199, uninstall.ps1:124 (runtime C# compilation through `%TEMP%`) → fixed as above.
- The reviewer's `Done when:` check, `grep -nE 'Add-Type +-(MemberDefinition|TypeDefinition|Path|LiteralPath)' install.ps1 uninstall.ps1`, prints nothing (exit 1).
- The other half of that check, "the AC2/AC6 elevated tests still pass", needs an elevated shell, so it waits for Windows CI. Those tests check the Path value itself, which is still written as before; the broadcast is the only change.
- Non-elevated, the same call with target `User` on the missing name is a no-op and creates no value.
- No other `[blocking]` or `[fix-now]` findings in round 3: the code, UI and UX reviews had none.
