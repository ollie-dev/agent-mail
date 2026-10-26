VERDICT: PASS

## Findings
- [follow-up] install.ps1:349, uninstall.ps1:219 (and every cmdlet the elevated copy autoloads, e.g. `Get-FileHash` at install.ps1:140, `Get-Service`, `Get-CimInstance`): the elevated copy reuses the caller's host exe (`(Get-Process -Id $PID).Path`) and, with `-NoProfile` only, still autoloads modules through the user's `PSModulePath`, whose first entry (`Documents\WindowsPowerShell\Modules` / `Documents\PowerShell\Modules`) is user-writable. A same-user process, the threat this job closes for `%TEMP%`, could plant a module like `Microsoft.PowerShell.Utility` there, or swap a per-user `pwsh.exe`, and get code running as admin. This is the general weakness of any elevated PowerShell and is outside this spec. A later job could launch the elevated copy as `[Environment]::SystemDirectory\WindowsPowerShell\v1.0\powershell.exe`, reset `$env:PSModulePath` to the machine-wide value first thing in the elevated block, and use .NET calls (`[Security.Cryptography.SHA256]`) instead of cmdlets that autoload modules from user paths.

## Checked
- Read: the full diff `git diff main...factory/027-broker-installers` (ci.yml, release.yml, CLAUDE.md, install.ps1, scripts/verify-release.sh, src/audit.rs, src/broker.rs, src/broker/windows.rs, src/store.rs, tests/broker.rs, tests/installers.rs, tests/release.rs, uninstall.ps1), then spec.md, then build.md.
- install.ps1, all of it:
  - Relaunch: the elevated copy runs the in-memory script block text, so no file is read that another process could swap. The URL travels base64-encoded inside `-EncodedCommand`, with no quoting injection and nothing read from the environment (`AGENT_MAIL_DOWNLOAD_URL` is gone; the only `$env:` read is `PROCESSOR_ARCHITECTURE`, for the refusal). `-Relaunched` without admin rights refuses, and there is exactly one `RunAs`, behind `IsInRole`.
  - Paths: they come from `GetFolderPath('ProgramFiles')` (32-bit hosts are refused) and `[Environment]::SystemDirectory` for sc.exe and icacls.exe.
  - Staging and verification: downloads land in `.staging-<guid>` inside the install folder, after the folder DACL has been reset with `icacls /inheritance:r` to SYSTEM/Administrators full control and Users RX. The checksum regex is anchored and names the asset. Extraction comes after the checksum check and takes only the single `agent-mail.exe` entry. `--version` runs only after verification.
  - Service registration: `binPath= "\"<exe>\" broker"` passed through ProcessStartInfo, so it is quoted. `sc sdset` runs on both create and config. The SDDL gives RP (start) and WP (stop) only to SY and BA. It equals the SCM default, so the moment between create and sdset is not a gap.
  - Rollback and cleanup paths, and the error messages: sc.exe output only, no secrets.
- uninstall.ps1, all of it:
  - The confirmation comes before elevation, the NonInteractive refusal works, and `-KeepData` wins over `-Force`.
  - The elevated copy is told its choice explicitly, and the purge is run through `sc.exe start agent-mail purge` with the exit codes checked through Win32_Service.
  - Path removal keeps the value kind, and the folder is deleted via `-LiteralPath`.
- src/broker.rs:
  - `is_purge_start` matches exactly one argument, exactly `purge`, as the spec asks.
  - `purge` and `remove_audit_dir`: links are unlinked without being followed, and std `remove_dir_all` does not follow junctions.
- src/broker/windows.rs: the `start_arguments` pointer walk is bounded by argc and null-checked. The purge branch opens no pipe and reports code 2 on failure. Purging is reachable only with SERVICE_START. The delayed-auto start passes no arguments, and the control handler rejects user-defined controls.
- src/store.rs: `delete_unfiled_targets` touches only names under `agent-mail:` in the Broker's own (virtual account's) vault. It runs before `entries`, and `delete` handles records with missing parts. src/audit.rs: `broker_dir` uses `SHGetKnownFolderPath`, not the environment.
- tests/installers.rs:
  - The test CA is generated per run by rcgen (tests/support/mod.rs:659), and its private key exists only in memory. It is removed by a drop guard.
  - The only Credential is the placeholder `fake-app-password-123`, and panic messages print only Broker replies.
  - The `CleanMachine` guard purges, deletes the service and restores the Path.
- Dependencies: no change to Cargo.toml or Cargo.lock. Workflows add only a `cargo test` step and one `cp`. verify-release.sh compares assets byte for byte against `git show`.
- Ran: `cargo test --locked --test broker job027`: 2 passed.
- Ran: `cargo test --locked --test release job027`: 10 passed, including the parse and relaunch-length tests under powershell.exe.
- Ran: a grep of the diff for `fake-app-password`/`CREDENTIAL` (only the placeholder in a test fixture) and of both scripts for `$env:` (only `PROCESSOR_ARCHITECTURE`).
- Ran: `net session`. This shell is not elevated, so the ignored `tests/installers.rs` tests were not run here; CI runs them.
