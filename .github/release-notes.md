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

It verifies the zip's SHA-256 checksum and installs `%USERPROFILE%\.local\bin\agent-mail.exe`. Verify its build attestation with:

```sh
gh attestation verify agent-mail-x86_64-pc-windows-msvc.zip --repo ollie-dev/agent-mail
```

On Windows, Mandates live in Windows Credential Manager, where any program running as you, including a hijacked Agent, can read the Credentials. The Windows build protects only against honest Agents.

## Keychain prompt after an update

On macOS the first Keychain access after an update prompts once, because each release has a new code signature; allow it. A Keychain prompt at any other time is a red flag: deny it.
