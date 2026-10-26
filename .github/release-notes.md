## Install

On macOS:

```sh
curl -fsSL https://github.com/ollie-dev/agent-mail/releases/latest/download/install.sh | sh
```

The installer downloads the binary for your Mac (Apple silicon or Intel), verifies its SHA-256 checksum and installs it to `~/.local/bin/agent-mail`. No Rust toolchain is needed. To check where a download was built, verify its build attestation:

```sh
gh attestation verify agent-mail-aarch64-apple-darwin.tar.gz --repo ollie-dev/agent-mail
```

On Windows x64, from PowerShell:

```powershell
irm https://github.com/ollie-dev/agent-mail/releases/latest/download/install.ps1 | iex
```

It asks for administrator rights once (UAC), verifies the zip's SHA-256 checksum and installs `%ProgramFiles%\agent-mail\agent-mail.exe`. It registers and starts the Broker, the service `agent-mail`, and adds the folder to the machine Path. Rerun it to update; your Mandates are kept.

Verify its build attestation with:

```sh
gh attestation verify agent-mail-x86_64-pc-windows-msvc.zip --repo ollie-dev/agent-mail
```

To uninstall, download `uninstall.ps1` from this release and run `powershell -ExecutionPolicy Bypass -File uninstall.ps1`. It asks before deleting your Mandates and audit log; `-KeepData` keeps them, `-Force` deletes them without asking.

## Windows

On Windows the Broker holds the Mandates and makes every mail call, so a hijacked Agent cannot extract a Credential, as on macOS. Run `agent-mail mandate create` in an elevated terminal. Administrators are out of scope on both platforms.

- `broker_unavailable`: the service could not be reached. Start it with `sc.exe start agent-mail` in an elevated terminal, or rerun `install.ps1`.
- `broker_version_mismatch`: the binary and the service are different versions. Rerun `install.ps1`.

## Keychain prompt after an update

On macOS the first Keychain access after an update prompts once, because each release has a new code signature; allow it. A Keychain prompt at any other time is a red flag: deny it.
