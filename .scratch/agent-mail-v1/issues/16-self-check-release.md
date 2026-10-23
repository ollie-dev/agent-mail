# 16: Hardened-runtime self-check and release pipeline

**What to build:** Release binaries are built in CI, ad-hoc signed with hardened runtime, and installed without a Rust toolchain. At startup, before any Keychain access, a release binary checks its own signature for the hardened-runtime flag and refuses to touch the Keychain if it is missing, so an injectable build fails safe. Debug builds used during development skip the check.

GitHub Actions builds macOS arm64 and x86_64 on each release tag, signs them, and publishes them to GitHub Releases with SHA-256 checksums and GitHub build attestations. A `curl | sh` installer downloads the right binary to `~/.local/bin` and verifies its checksum. `dist` (formerly cargo-dist) is the candidate tool; add a custom signing step if it cannot sign ad-hoc with hardened runtime.

**Blocked by:** 03

**Status:** done

- [ ] A release binary signed ad-hoc with hardened runtime passes the self-check and can access the Keychain
- [ ] A release binary without the hardened-runtime flag refuses any Keychain access with a clear error (opt-in macOS test, ignored by default)
- [ ] Pushing a release tag produces macOS arm64 and x86_64 binaries on GitHub Releases, each with a SHA-256 checksum and a build attestation
- [ ] `codesign` on the published binaries shows an ad-hoc signature with the runtime flag
- [ ] The installer downloads the binary for the current architecture to `~/.local/bin`, verifies its checksum, and aborts without installing on a mismatch
- [ ] The installed binary runs without any Rust toolchain present
- [ ] Release notes include the note that the first Keychain access after an update prompts once
