# 21: Installer and uninstaller for the Broker

**What to build:** `install.ps1` installs agent-mail system-wide and registers the Broker (ADR-0004), and a new `uninstall.ps1` removes it. The binary must live where only administrators can write, or a hijacked Agent could replace the Broker's binary, so the installer now asks for elevation once.

**Blocked by:** 20

**Status:** ready-for-agent

- [ ] `install.ps1` requests elevation (UAC) once, downloads and verifies the binary as today, installs it to `%ProgramFiles%\agent-mail`, registers the Broker as a service under `NT SERVICE\agent-mail` with delayed automatic start, starts it and adds the folder to the machine PATH
- [ ] Rerunning `install.ps1` updates in place: it stops the Broker, replaces the binary, starts it again and checks `--version`; stored Mandates are untouched
- [ ] `uninstall.ps1` requests elevation, stops and removes the Broker, deletes the binary and the PATH entry, and deletes the stored Mandates and the audit log after a confirmation; `-KeepData` keeps them
- [ ] `install.ps1` and `uninstall.ps1` both work in Windows PowerShell 5.1 and PowerShell 7, and are uploaded as release assets
- [ ] `scripts/verify-release.sh` checks the new asset
- [ ] Windows CI runs `install.ps1` against a `file://` download directory and `uninstall.ps1` for real, and checks that the service runs under the virtual account, that a Mandate stored under it survives a Broker restart, and that removal leaves nothing behind (with and without `-KeepData`)
