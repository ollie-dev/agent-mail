# Broker installers: elevated install.ps1 and a new uninstall.ps1

## Goal
On Windows the operator runs `install.ps1` once, confirms one UAC prompt, and ends up with `agent-mail.exe` in `%ProgramFiles%\agent-mail` on the machine PATH and the Broker (ADR-0004) running as the service `agent-mail` under `NT SERVICE\agent-mail`. Rerunning it updates the binary in place and keeps the stored Mandates. A new `uninstall.ps1` removes the Broker, the binary and the PATH entry, and deletes the stored Mandates and the audit log after a confirmation unless `-KeepData` is given.

## Context
- Stack: Rust, one crate (`cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`), plus PowerShell scripts that must run under Windows PowerShell 5.1 and PowerShell 7. Ticket: `.scratch/windows-broker/issues/21-installers.md`; ADR: `docs/adr/0004-windows-broker.md`. There is no `.scratch/windows-broker/spec.md`; the ticket and the ADR are the source.
- Test command: `cargo test` (both OSes). The tests that really install, register a service and change the machine PATH need an elevated shell and change the machine, so they are opt-in and ignored by default: `cargo test --locked --test installers -- --ignored --test-threads=1`, run on Windows CI (GitHub's Windows runners are elevated).
- Files touched:
  - `install.ps1`: rewritten for the elevated, system-wide install and update.
  - `uninstall.ps1` (new, repo root).
  - `src/broker/windows.rs`: `service_main` reads its start arguments; a start with the single argument `purge` runs the purge instead of the pipe server.
  - `src/broker.rs`: the OS-independent purge (`purge(store, audit_dir)`), so it can be tested in-process with the in-memory store.
  - `.github/workflows/release.yml`: copies and uploads `uninstall.ps1`; its Windows test job also runs the opt-in installer tests.
  - `.github/workflows/ci.yml`: a Windows-only step running the opt-in installer tests.
  - `scripts/verify-release.sh`: checks the `install.ps1` and `uninstall.ps1` assets.
  - `tests/installers.rs` (new): the elevated, ignored-by-default Windows tests of both scripts against a `file://` download directory.
  - `tests/release.rs`: the per-user `job024_ac4*` installer tests and `job024_ac4g_installer_never_elevates_...` no longer hold and are replaced by the tests here. The static checks of scripts, workflows and `verify-release.sh` live here.
  - `tests/broker.rs`: the in-process purge test.
  - `tests/support/`: reuses the fake IMAP server and its test CA.

## Acceptance criteria
Terms: the **install folder** is `[Environment]::GetFolderPath('ProgramFiles')` + `\agent-mail`. Never use `$env:ProgramFiles`, which the caller controls. The **binary** is `<install folder>\agent-mail.exe`. The **machine Path** is the raw `Path` value under `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment`, read with `DoNotExpandEnvironmentNames`. The **data** is the Broker's stored Mandates plus `%ProgramData%\agent-mail` (the audit log and its folder). Unless stated otherwise, the Windows tests below are in `tests/installers.rs`, run elevated, and use a download directory with a zip of the real test-built `agent-mail.exe` (`CARGO_BIN_EXE_agent-mail`) and its correct `.sha256`. Each test first asserts that no service `agent-mail` and no install folder exist. If either does, it panics with "agent-mail is installed on this machine; the installer tests would replace it". A drop guard stops and deletes the service, deletes the install folder and restores the machine Path, so a failed test leaves nothing behind.

1. AC1: `install.ps1` requests elevation (UAC) once, downloads and verifies the binary as today, installs it to `%ProgramFiles%\agent-mail`, registers the Broker as a service under `NT SERVICE\agent-mail` with delayed automatic start, starts it and adds the folder to the machine PATH.
   - Not elevated: the script starts exactly one elevated copy of itself (`Start-Process -Verb RunAs -Wait`), passing the download URL and its own arguments as arguments, not through the environment. It then exits with that copy's exit code. The elevated copy never relaunches. If UAC is declined, it prints `agent-mail installer: administrator rights are needed; nothing was installed` to stderr and exits 1. This is checked statically, since a test cannot click UAC: the script calls `Start-Process` with `-Verb RunAs` exactly once, behind a check of the `Administrators` role (`WindowsPrincipal.IsInRole`).
   - Elevated, with a correct download directory, on a clean machine: exit 0. Afterwards:
     - the binary exists, and the install folder holds only `agent-mail.exe`;
     - `Get-CimInstance Win32_Service -Filter "Name='agent-mail'"` has `StartName` `NT SERVICE\agent-mail`, `StartMode` `Auto`, `DelayedAutoStart` `True`, `State` `Running` and `PathName` exactly `"<binary>" broker` (quoted);
     - `sc.exe sdshow agent-mail` prints exactly `D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLOCRRC;;;IU)(A;;CCLCSWLOCRRC;;;SU)`, set explicitly by the installer, so only SYSTEM and Administrators can start, stop or reconfigure it;
     - the machine Path ends with the install folder exactly once, every earlier entry is unchanged, and the registry value kind (`ExpandString`) is preserved;
     - the install folder's DACL grants no write, modify or full-control right to `BUILTIN\Users`, `NT AUTHORITY\Authenticated Users`, `Everyone` or `NT AUTHORITY\INTERACTIVE`;
     - `<binary> whoami`, run from the test process with no token, gets a reply from the Broker (not `broker_unavailable`);
     - stdout contains `Installed agent-mail <version>`, where `<version>` is the binary's `--version` output.
   - Verification is unchanged from today, with these expected outputs:
     - checksum mismatch: `checksum mismatch for agent-mail-x86_64-pc-windows-msvc.zip; nothing was installed`;
     - malformed `.sha256`: `checksum file for ... is malformed; nothing was installed`;
     - missing zip: `could not download ...; nothing was installed`;
     - a zip holding anything other than exactly `agent-mail.exe`: `... must hold exactly agent-mail.exe; nothing was installed`;
     - a `PROCESSOR_ARCHITECTURE` other than `AMD64`: refused with a message naming x64.

     Each of these exits 1 and leaves no install folder, no service and an unchanged machine Path. Downloads and extraction happen in a fresh directory inside the install folder, which is removed afterwards, never under `%TEMP%`. Nothing is extracted before the checksum passes (static check: `checksum mismatch` comes before `ExtractToFile` in the script).
2. AC2: Rerunning `install.ps1` updates in place: it stops the Broker, replaces the binary, starts it again and checks `--version`; stored Mandates are untouched.
   - With the Broker installed and holding one Mandate (stored as in AC6), rerun `install.ps1` with a download directory whose binary differs from the installed one (for example the same exe with bytes appended). Exit 0, and:
     - the binary's SHA-256 equals the new one;
     - the service is `Running` with the same configuration as in AC1;
     - the machine Path still holds the install folder exactly once;
     - `mandate list` from the test user still shows the Mandate, with the same id.
   - While an `agent-mail.exe mcp` process started from the binary is still running, the rerun still exits 0 and installs the new binary. Renaming the in-use binary aside is allowed; a leftover renamed copy is deleted by the next run or by `uninstall.ps1`.
   - If the new binary's `--version` does not print `agent-mail <x.y.z>` with exit 0 (test: a zip of a non-executable `agent-mail.exe`), or the service does not reach `Running` within 30 seconds, the installer puts the previous binary back, starts the Broker again, prints `agent-mail installer: the new binary did not start; the previous version was restored` and exits 1. Afterwards the binary's hash equals the old one and the service is `Running`.
3. AC3: `uninstall.ps1` requests elevation, stops and removes the Broker, deletes the binary and the PATH entry, and deletes the stored Mandates and the audit log after a confirmation; `-KeepData` keeps them.
   - Parameters: `-KeepData` and `-Force`. `-Force` deletes the data without asking; `-KeepData` wins over `-Force`.
   - The confirmation is asked before elevation, in the operator's own console: `Delete all stored Mandates and the audit log? Agents lose access and this cannot be undone. [y/N]`. Only `y` or `yes` (case-insensitive) deletes the data. Any other answer keeps it and prints `Keeping the stored Mandates and the audit log.` The elevated copy is then passed `-Force` or `-KeepData` explicitly and never asks.
   - When no confirmation can be read (a `-NonInteractive` host) and neither switch is given, it prints `uninstall.ps1: cannot ask for confirmation; rerun with -KeepData or -Force` and exits 1 before any change.
   - Not elevated: it relaunches itself elevated once, as in AC1, with the same refusal text naming "nothing was removed".
   - Deleting the data: the uninstaller stops the Broker, then runs `sc.exe start agent-mail purge` and waits for `STOPPED` with exit code 0. On a start with the single argument `purge`, the Broker opens no pipe. It deletes every record in its own Credential Manager store, deletes `%ProgramData%\agent-mail\audit.log` and the folder, and stops with exit code 0, or with service-specific code 2 on any failure. If the purge fails, the uninstaller prints `uninstall.ps1: could not delete the stored Mandates; nothing else was removed`, leaves the service registered and exits 1.
   - In-process test (`tests/broker.rs`, every OS): `purge` over an in-memory store holding two Mandates and a temporary audit folder with a log returns 2. Afterwards the store's `entries()` is empty and the folder no longer exists.
   - Then it removes the service (`sc.exe delete`), deletes the install folder, and removes from the machine Path every entry equal to the install folder (case-insensitive, trailing `\` ignored). All other entries stay in order with their raw values and the value kind.
   - Exit 0, and stdout ends with `agent-mail was removed.`
   - Running it when nothing is installed prints `agent-mail is not installed.` and exits 0 without changes.
4. AC4: `install.ps1` and `uninstall.ps1` both work in Windows PowerShell 5.1 and PowerShell 7, and are uploaded as release assets.
   - The AC1 to AC3 and AC6 installer tests run under `powershell.exe`, and under `pwsh.exe` too where it is installed (it is on GitHub's Windows runners), each with `-NoProfile -NonInteractive -ExecutionPolicy Bypass -File`.
   - A non-elevated test in `tests/release.rs` (Windows only) parses both scripts with `[System.Management.Automation.Language.Parser]::ParseFile` under each available shell and asserts zero parse errors.
   - Static test: `release.yml`'s `publish` job contains `cp uninstall.ps1 dist/uninstall.ps1`, and its `gh release create` file list contains both `dist/install.ps1` and `dist/uninstall.ps1`.
5. AC5: `scripts/verify-release.sh` checks the new asset.
   - For each of `install.ps1` and `uninstall.ps1`, it downloads the asset with `gh release download "$tag" --pattern <name>`. It fails with `missing asset <name>` if the asset is absent, and with `<name> differs from <tag>` unless the asset is byte-identical to `git show "<tag>:<name>"`.
   - The header comment says so.
   - Static test in `tests/release.rs`: the script contains `uninstall.ps1`, `install.ps1`, `missing asset` and `git show`, and `sh -n` still parses it (the existing executable-and-parse test).
6. AC6: Windows CI runs `install.ps1` against a `file://` download directory and `uninstall.ps1` for real, and checks that the service runs under the virtual account, that a Mandate stored under it survives a Broker restart, and that removal leaves nothing behind (with and without `-KeepData`).
   - `ci.yml`, and the Windows leg of `release.yml`'s `test` job, have a step guarded by `if: runner.os == 'Windows'` that runs `cargo test --locked --test installers -- --ignored --test-threads=1`. A static test asserts it.
   - Virtual account: after install, the process of service `agent-mail` runs as the SID of `NT SERVICE\agent-mail` (`broker::service_sid("agent-mail")`), checked via `win::process_user_sid` on the service's PID. The Broker's pipe is owned by that SID.
   - A Mandate survives a restart. The test:
     1. starts the fake IMAP server over TLS on `localhost` and adds its test CA to `Cert:\LocalMachine\Root`, removed again by a drop guard;
     2. stores a Mandate (placeholder Credential) through the installed Broker by sending a `create` request through the Broker client session;
     3. runs `sc.exe stop agent-mail` and then `sc.exe start agent-mail`;
     4. asserts `mandate list` from the binary shows that Mandate's id.
   - Removal with `-Force` leaves nothing behind:
     - no service `agent-mail`;
     - no install folder;
     - the machine Path is byte-identical to before the install;
     - no `%ProgramData%\agent-mail`;
     - after a fresh install, `mandate list` shows no Mandates.
   - Removal with `-KeepData`: the service, install folder and Path entry are gone as above, but `%ProgramData%\agent-mail\audit.log` still exists. After a fresh install, `mandate list` shows the earlier Mandate's id. The test ends with `-Force` to clean up.

## Out of scope
- README, release notes, skill and `CLAUDE.md` updates for the new install, update and uninstall flow, and listing `tests/installers.rs` and `uninstall.ps1` in `CLAUDE.md` (Ticket 22). Existing README assertions in `tests/release.rs` stay as they are until then.
- Removing an old per-user `%USERPROFILE%\.local\bin\agent-mail.exe` left by the previous installer.
- Deleting the virtual account's profile folder (`C:\Windows\ServiceProfiles\agent-mail`), which Windows keeps after the service is deleted. "Nothing behind" is defined by the checks in AC6.
- Code-signing the Windows binary or the scripts; service failure-recovery actions; an MSI or winget package.
- macOS: `install.sh` is unchanged.

## Notes for reviewers
- Security:
  - The point of this ticket is that only administrators can write the Broker's binary. Check that:
    - downloads, extraction and staging happen only inside the admin-only install folder, never in the user-writable `%TEMP%`, where a same-user process could swap the verified file before the elevated copy moves it;
    - paths come from `GetFolderPath`, not environment variables;
    - the service `PathName` is quoted (the unquoted-service-path flaw);
    - the service DACL is set explicitly.
  - The `purge` start argument deletes every Mandate. It is safe only because starting the service needs `SERVICE_START`, which the DACL in AC1 gives only to SYSTEM and Administrators. Review the argv parsing: exactly one argument `purge`, anything else runs the normal Broker.
  - `install.ps1` now elevates, runs the verified binary (`--version`) and changes the machine PATH, which the old script and its test forbade. Make sure the relaunch passes no environment-derived URL and cannot be pointed at another script.
  - Removing a CA from `LocalMachine\Root` is test-only, in CI, behind a drop guard; no fixture holds a real Credential.
  - Testability concern: when run through `irm | iex` there is no script file to relaunch. The builder has to relaunch the script's own text, for example with `-EncodedCommand`, which must stay under the 32,767-character command line limit after UTF-16 and base64 encoding. Otherwise they must state another approach. The relaunch path itself cannot be tested automatically, because of UAC.
- UX: the operator runs the scripts once per install, update or removal, in their own PowerShell window, and sees one UAC prompt. Output from the elevated copy appears in a new window, which should stay open until Enter is pressed when it was relaunched, so the result is not lost. The parent then prints the outcome. The uninstall confirmation is asked in the operator's console before elevation. A new terminal is needed to see the PATH change, and the installer should say so. Agents never run these scripts; a client that cannot reach the Broker keeps failing with `broker_unavailable`, which names rerunning `install.ps1`.
- UI: terminal output only: the installer's progress lines, `Installed agent-mail <version>`, the uninstall confirmation prompt and `agent-mail was removed.`, plus the error lines quoted above, each one short line in the CLAUDE.md style. No pages.
