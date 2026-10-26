VERDICT: CHANGES

## Findings
- [blocking] install.ps1:199, uninstall.ps1:124: the elevated copy still compiles C# from the user-writable `%TEMP%`. Round 3 now always relaunches Windows PowerShell 5.1. There, `Add-Type -MemberDefinition` (the `SendMessageTimeout` broadcast) goes through CodeDom: it writes `<random>.0.cs` and `<random>.tmp` into `%TEMP%`, runs `csc.exe` on them and loads the result into the admin process. Under UAC admin-approval elevation, the elevated copy's `%TEMP%` is the operator's own `AppData\Local\Temp`. Those files inherit that folder's ACL, which gives the user full control. So a same-user, non-elevated process watching the folder can swap the `.cs` source, or the compiled output, between write and load, and its code then runs as administrator. This is the same threat class the human asked to close for `PSModulePath`, and the one AC1 closes for downloads ("never under `%TEMP%`"). The commit message's "uses only .NET" does not hold here. I confirmed the temp files: with `TEMP` pointed at a watched scratch folder, the exact `Add-Type` line from install.ps1 created `joy15uy5.tmp` and `joy15uy5.0.cs` there. With `TEMP` set to a missing folder, `Add-Type` fails with a path error under `C:\nonexistent-x\`.
  - Expected fix: no runtime C# compilation in either elevated script. Remove both `Add-Type -MemberDefinition` calls and get the `WM_SETTINGCHANGE` broadcast without compiling. For example, after the raw registry write, call `[Environment]::SetEnvironmentVariable('<an unused name>', $null, 'Machine')`: .NET Framework deletes the missing value as a no-op and broadcasts `Environment` itself. Pointing `$env:TEMP`/`$env:TMP` at an admin-only folder before compiling is a weaker fallback. In that case the uninstaller must not use the install folder it is about to delete.
  - Add a static test in `tests/release.rs` that fails if either script contains `-MemberDefinition`, `-TypeDefinition` or `Add-Type -Path`. (`Add-Type -AssemblyName` for `System.IO.Compression` only loads from the GAC and is fine.)
  - Check: `grep -nE 'Add-Type +-(MemberDefinition|TypeDefinition|Path|LiteralPath)' install.ps1 uninstall.ps1` prints nothing, and the AC2/AC6 elevated tests still pass (the Path entry is still applied).

## Checked
- Read: `spec.md`, `human-notes.md` (the new note "fix the psmodulepath risk"), `round-2/review-security.md` (no `[blocking]` or `[fix-now]` items to re-verify), and the For-the-human and Follow-ups sections of `decision.md`.
- Read: `git log main..HEAD`. The new commit is 6d52102. I read its full diff in `install.ps1`, `uninstall.ps1` and `tests/release.rs`. Earlier rounds covered the rest of `main...factory/027-broker-installers`, and it is unchanged.
- install.ps1 / uninstall.ps1, the `PSModulePath` hardening:
  - The relaunch host is now `[Environment]::SystemDirectory\WindowsPowerShell\v1.0\powershell.exe`, not the caller's own host. Admin-only, and not derived from the environment.
  - The bootstrap `$boot` uses only `[X]::new` .NET calls, with no cmdlets.
  - The hardening runs first in the block: `$IsAdministrator` comes from .NET, then `PSModulePath` is set to `$PSHOME\Modules`, the modules are imported by full path from `$PSHOME`, and `$PSModuleAutoLoadingPreference = 'None'` is set. Only `Set-StrictMode`, `Import-Module` and `ForEach`, which need no module, run before it.
  - `Get-FileHash` is replaced by .NET `SHA256` over a stream that is disposed.
  - `PSModulePath` is restored for an `iex` caller.
  - Every cmdlet both scripts use comes from Core, Management, Utility or (uninstall) CimCmdlets, so nothing needs autoloading. I listed them with a grep.
  - Remaining code-loading paths in the elevated copy: `Add-Type -AssemblyName` loads from the GAC (fine). `Add-Type -MemberDefinition` compiles via `%TEMP%` (the finding).
- tests/release.rs: `assert_loads_only_shipped_modules` and `job027_a_module_planted_on_psmodulepath_never_loads_after_the_hardening`. The planted module goes to a test temp dir and holds no secrets.
- Ran: `cargo test --locked --test release`: 34 passed, including the new planted-module test.
- Ran: a PowerShell 5.1 probe with a `FileSystemWatcher` on a scratch `TEMP` and the scripts' exact `Add-Type -MemberDefinition` line. It showed `.tmp` and `.0.cs` files created there. A second probe with `TEMP` pointed at a missing folder made `Add-Type` fail on a path under it.
- Ran: `icacls` on `%LOCALAPPDATA%\Temp`. The user has `(OI)(CI)(F)`, inherited by files created there.
