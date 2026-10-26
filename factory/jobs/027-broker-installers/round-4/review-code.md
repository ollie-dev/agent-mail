VERDICT: PASS

## Findings
- [follow-up] install.ps1:199, uninstall.ps1:124: the WM_SETTINGCHANGE broadcast now relies on .NET broadcasting after deleting a missing machine variable. It is not testable without elevation; watch the elevated Windows CI run (the Path assertions in AC2/AC6) once.

## Checked
- Read: `git show 168b359` in full (`install.ps1`, `uninstall.ps1`: the Add-Type/SendMessageTimeout P/Invoke replaced by `[Environment]::SetEnvironmentVariable('AGENT_MAIL_INSTALLER_UNUSED', $null, 'Machine')` in a try/catch; `tests/release.rs`: new `job027_elevated_scripts_compile_no_code`); the round-3 review and the Round 4 section of build.md.
- The change does what the security finding asked: no runtime C# compilation remains, the raw Path write with its kind preserved is untouched, and the static test forbids reintroducing it (forbidden tokens, only `Add-Type -AssemblyName System.IO.Compression...`, exactly one SetEnvironmentVariable line).
- Ran: `cargo test` in the worktree: every `test result` line ok, 0 failed (installers suite 1 passed, 9 ignored elevated tests that run on CI).
- Criteria: only AC1/AC3 security hardening touched this round; covered by `job027_elevated_scripts_compile_no_code`, which would fail on the old Add-Type code. Other criteria unchanged since round 3.
