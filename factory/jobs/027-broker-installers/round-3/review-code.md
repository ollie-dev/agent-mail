VERDICT: PASS

## Findings
- [follow-up] install.ps1:36-44, uninstall.ps1:28-36: the elevated path imports the modules by `$PSHOME\Modules\<name>\<name>.psd1`. This is checked on Windows PowerShell 5.1 only in this review; the elevated `--ignored` installer tests under `pwsh.exe` (where the path layout differs between 7.x versions) run first on Windows CI. Watch that run.

## Checked
- Read: the full round-2 review (no blocking or fix-now items to re-verify); the Round 3 section of build.md; the round-3 commit 6d52102 for `install.ps1` and `uninstall.ps1` in full (module hardening block, `$IsAdministrator`, the .NET-only `-EncodedCommand` bootstrap, the fixed system `powershell.exe` relaunch host, the .NET SHA256, restoring `PSModulePath` before `exit`). `tests/release.rs` is changed in the same commit; I read its diff stat but not every line of the new tests.
- The change does what the human note asked. Nothing user-writable can run before the hardening, because only .NET and built-ins run there. Elevation always goes through the system host. Both scripts handle the non-elevated path and the `iex` path the same way (`PSModulePath` restored).
- Ran: `cargo test` in the worktree: every `test result` line ok, 0 failed. The installers suite shows 1 passed and 9 ignored, which are the elevated tests that run on CI.
- Criteria: AC1 and AC3 are the ones this round touches. The static test `job027_elevated_scripts_load_only_the_modules_powershell_ships_with` and the planted-module test (with a control that shows the planted commands win without the hardening) cover it, and both pass. The other criteria are unchanged since round 2.
