## Install

```sh
curl -fsSL https://github.com/ollie-dev/agent-mail/releases/latest/download/install.sh | sh
```

The installer downloads the binary for your Mac (Apple silicon or Intel), verifies its SHA-256 checksum and installs it to `~/.local/bin/agent-mail`. No Rust toolchain is needed. To check where a download was built, verify its build attestation:

```sh
gh attestation verify agent-mail-aarch64-apple-darwin.tar.gz --repo ollie-dev/agent-mail
```

## Keychain prompt after an update

On macOS the first Keychain access after an update prompts once, because each release has a new code signature; allow it. A Keychain prompt at any other time is a red flag: deny it.
