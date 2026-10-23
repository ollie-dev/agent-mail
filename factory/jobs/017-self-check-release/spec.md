# Hardened-runtime self-check and release pipeline

## Goal
Pushing a release tag makes GitHub Actions build agent-mail for macOS arm64 and x86_64, ad-hoc sign it with hardened runtime and publish it on GitHub Releases with SHA-256 checksums and build attestations. Users can then install it with a one-line `curl | sh` installer that checks the checksum and needs no Rust toolchain. A release binary checks its own signature before any Keychain access, and if the hardened-runtime flag is missing it refuses to touch the Keychain with a clear error. An injectable build therefore fails safe. Debug builds skip the check.

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`. Repository: `github.com/ollie-dev/agent-mail` (the `origin` remote).
- Ticket: `.scratch/agent-mail-v1/issues/16-self-check-release.md` (user stories 52, 65, 66, 67 and 68 of `.scratch/agent-mail-v1/spec.md`). The spec's "macOS hardening" and "Distribution and hosts" sections apply. ADR-0002 explains why this matters: protection against a hijacked Agent rests on the Keychain binding the Mandate item to the agent-mail binary, and an unhardened binary could have code injected into it (for example via `DYLD_INSERT_LIBRARIES`) and use that binding.
- **Self-check (new `src/selfcheck.rs`)**:
  - On macOS, read the running process's own code-signing status with `csops(getpid(), CS_OPS_STATUS, ...)`, declared as `extern "C"` against libSystem. `CS_OPS_STATUS` = 0 and `CS_RUNTIME` = `0x0001_0000`. Using `SecCodeCopySelf` and `SecCodeCopySigningInformation` (`kSecCodeInfoFlags`) through `security-framework` is also fine. In both cases, use no new crate beyond what `Cargo.toml` already has.
  - The pass/fail decision is a pure function, `pub fn hardened_runtime_ok(status: Option<u32>) -> bool`. It is true only for `Some(flags)` with the `CS_RUNTIME` bit set. `None` means the status could not be read, and it fails.
  - The check is active only in builds with `cfg(all(target_os = "macos", not(debug_assertions)))`. In every other build it always passes, and on non-macOS there is no Keychain at all.
  - The result is computed once per process and cached. No environment variable, argument, config file or Cargo feature can skip it in a release build.
- **Where the check sits (`src/store.rs`, `src/core.rs`, `src/main.rs`)**:
  - A wrapper store, `GuardedStore<S: MandateStore>`, runs the self-check before it forwards any call to the inner store. This covers `get`, `put`, `delete` and `entries`, plus any Keychain enumeration the core uses. If the check fails, it returns a distinct store error without calling the inner store.
  - `StoreError` becomes an enum with two variants: `Unavailable` (today's meaning) and `HardenedRuntimeMissing`. The core maps `HardenedRuntimeMissing` to a new `CoreError::HardenedRuntimeMissing` with code `hardened_runtime_missing`, which the CLI reports with exit code 1.
  - `src/main.rs` wraps `RuntimeStore` in `GuardedStore` with the real check. Under `cfg(any(test, feature = "test-support"))` there is a constructor that takes a fixed status, `GuardedStore::with_status(inner, Option<u32>)`, so in-process tests can wrap `InMemoryStore` and choose the outcome.
  - `GuardedStore` also offers `check()`. `mandate create` calls it before its first prompt, so the human is never asked for Credentials that could not be stored.
  - The check runs lazily, before the first store access, not at process start. Commands that never touch the store keep working on an unhardened binary: `--help`, `--version`, usage errors and `missing_token`.
- **Error**. JSON on stderr: `{"error":{"code":"hardened_runtime_missing","message":"this agent-mail binary is not signed with the hardened runtime, so it refuses to access the Keychain; ask the human to reinstall agent-mail from https://github.com/ollie-dev/agent-mail/releases"}}`. The builder may reword the message, but it must say that the hardened runtime is missing and that the Keychain was not accessed, and it must point to reinstalling from the releases page. `--text` renders it like the other errors. The MCP server returns it as a tool error with the same code. An Agent command that fails this way is audited the way a `store_unavailable` failure is audited today, but with `reason` `hardened_runtime_missing`.
- **Release pipeline**: a hand-written workflow, `.github/workflows/release.yml`, with no `dist`. Reason: `dist` signs macOS binaries only with a Developer ID certificate. It cannot apply an ad-hoc signature with `--options runtime`, so it would need a custom signing step anyway, and the hand-written workflow keeps signing, checksums and attestation visible in one file.
  - Trigger: `push` of tags matching `v*`. The first step fails if the tag without its `v` differs from the `version` in `Cargo.toml`.
  - Runner: `macos-latest` (arm64). Run `cargo test` first, then `cargo build --release --locked` for `aarch64-apple-darwin` and `x86_64-apple-darwin`, with `rustup target add` for the second target.
  - Sign each binary with `scripts/sign-macos.sh <binary>`. The script runs `codesign --sign - --options runtime --force <binary>`, then `codesign --display --verbose=2 <binary>`, and exits non-zero unless the `flags=` line contains both `adhoc` and `runtime` (for example `flags=0x10002(adhoc,runtime)`); `--check-only <binary>` runs only the verification.
  - Assets per target:
    - `agent-mail-<target>.tar.gz`, holding the single file `agent-mail` at its root.
    - `agent-mail-<target>.tar.gz.sha256`, holding exactly `<64 lowercase hex>  agent-mail-<target>.tar.gz\n` (the `shasum -a 256` format).
    - `install.sh` is also uploaded as a release asset.
  - Attestation: `actions/attest-build-provenance` for both tarballs. Job permissions: `contents: write`, `id-token: write` and `attestations: write`.
  - Publish with `gh release create "$TAG" --notes-file .github/release-notes.md <assets>`.
  - A second job, `verify`, depends on the publish job and runs `scripts/verify-release.sh "$TAG"` on a fresh macOS runner. For each target it downloads the assets with `gh release download`, checks the checksum with `shasum -a 256 -c`, extracts the tarball, checks `codesign --display --verbose=2` for `adhoc` and `runtime`, runs `gh attestation verify <tarball> --repo ollie-dev/agent-mail`, and on the matching architecture runs `./agent-mail --version`. Reviewers can run the same script locally against any published tag.
- **Installer (`install.sh` at the repo root, POSIX `sh`)**:
  - The documented one-liner is `curl -fsSL https://github.com/ollie-dev/agent-mail/releases/latest/download/install.sh | sh`.
  - Platform: `uname -s` must be `Darwin`, else it fails with "agent-mail releases are available for macOS only" and exit 1. `uname -m` maps `arm64` to `aarch64-apple-darwin` and `x86_64` to `x86_64-apple-darwin`; anything else fails with exit 1.
  - Base URL: `${AGENT_MAIL_DOWNLOAD_URL:-https://github.com/ollie-dev/agent-mail/releases/latest/download}`. Tests point it at a `file://` directory.
  - Install steps:
    1. Download the tarball and its `.sha256` file into a fresh `mktemp -d` directory with `curl -fsSL`.
    2. Verify with `shasum -a 256 -c`, falling back to `sha256sum -c` if `shasum` is missing.
    3. Only after the check passes, extract the tarball and move the binary into place: `mkdir -p "$HOME/.local/bin"`, then `mv` to `$HOME/.local/bin/agent-mail` with mode 755.
    4. Print the installed path. If `$HOME/.local/bin` is not on `PATH`, also print a hint.
  - It never uses `sudo` and always removes the temporary directory.
- **Release notes**: `.github/release-notes.md` is the body of every release. It contains the install one-liner and the Keychain note: "On macOS the first Keychain access after an update prompts once, because each release has a new code signature; allow it. A Keychain prompt at any other time is a red flag: deny it."
- **Files this feature touches**:
  - Rust sources:
    - `src/selfcheck.rs` (new).
    - `src/store.rs`: `GuardedStore` and the `StoreError` enum.
    - `src/core.rs`: the new error and its mapping.
    - `src/create.rs`: the early check.
    - `src/main.rs`: wraps the store.
    - `src/lib.rs`: `pub mod selfcheck`.
  - Release files:
    - `.github/workflows/release.yml`
    - `.github/release-notes.md`
    - `scripts/sign-macos.sh`
    - `scripts/verify-release.sh`
    - `install.sh`
  - Tests:
    - `tests/selfcheck.rs` (new). It holds the in-process guard tests and the opt-in release-binary tests, which are ignored by default and run with `cargo test --test selfcheck -- --ignored`.
    - `tests/release.rs` (new). It holds the installer tests and the release-file checks, which run by default on macOS.
    - Existing tests that match `StoreError` are updated to use the enum.
  - Docs: `README.md` gets an "Install" section and a "Releasing" paragraph under "Development". `CLAUDE.md` gets the new files in its layout list.
- **Opt-in release-binary harness**:
  - The ignored tests run `cargo build --release` and copy `target/release/agent-mail` into a temporary directory.
  - `<unknown token>` is the string from `AgentToken::mint().expose()` for a freshly minted token that no store holds. With it, `whoami` makes exactly one Keychain lookup, which finds nothing and raises no prompt. Its outcome shows whether the Keychain was reached: `invalid_token` means it was reached, `hardened_runtime_missing` means it was refused.

## Acceptance criteria
1. AC1: A release binary signed ad-hoc with hardened runtime passes the self-check and can access the Keychain.
   - Opt-in macOS test in `tests/selfcheck.rs`. Input: the copied release binary, signed with `scripts/sign-macos.sh`, run as `agent-mail whoami` with `AGENT_MAIL_TOKEN=<unknown token>`. Expected: exit code 1, and the stderr JSON `error.code` is `invalid_token`, which proves the Keychain lookup ran. The code is never `hardened_runtime_missing`.
   - Same binary, `agent-mail --version`: exit 0.
2. AC2: A release binary without the hardened-runtime flag refuses any Keychain access with a clear error (opt-in macOS test, ignored by default).
   - Input: the copied release binary, re-signed with `codesign --sign - --force` and no runtime option. This gives a deterministic unhardened signature on both architectures.
   - Run with `AGENT_MAIL_TOKEN=<unknown token>`: `whoami`, `list`, `mandate list` and `mandate delete deadbeef`. Expected for each: exit code 1, stderr JSON `error.code` `hardened_runtime_missing`, and a `message` containing `hardened runtime` and `Keychain`.
   - The same binary still answers `--version` with exit 0, and `whoami` with no `AGENT_MAIL_TOKEN` gives `missing_token`.
   - The test is `#[ignore]`d and `#[cfg(target_os = "macos")]`, and plain `cargo test` reports it as ignored.
3. AC3: Pushing a release tag produces macOS arm64 and x86_64 binaries on GitHub Releases, each with a SHA-256 checksum and a build attestation.
   - Automated in CI: the `verify` job of `.github/workflows/release.yml` (via `scripts/verify-release.sh <tag>`) passes only if both `agent-mail-aarch64-apple-darwin.tar.gz` and `agent-mail-x86_64-apple-darwin.tar.gz` and both `.sha256` files are on the release, both pass `shasum -a 256 -c`, and both pass `gh attestation verify --repo ollie-dev/agent-mail`.
   - Locally, `tests/release.rs` checks the workflow text:
     - It triggers on `push` with `tags: ['v*']` (either quote style).
     - It names both targets.
     - It calls `scripts/sign-macos.sh`.
     - It uses `actions/attest-build-provenance` and grants `id-token: write` and `attestations: write`.
     - It runs `scripts/verify-release.sh`.
4. AC4: `codesign` on the published binaries shows an ad-hoc signature with the runtime flag.
   - In CI, `scripts/verify-release.sh` fails unless `codesign --display --verbose=2` on each extracted binary prints a `flags=` line containing both `adhoc` and `runtime`.
   - Opt-in local test: after `scripts/sign-macos.sh` on the copied release binary, `codesign --display --verbose=2` stderr contains `flags=0x10002(adhoc,runtime)`, and `scripts/sign-macos.sh` exits 0.
   - `scripts/sign-macos.sh --check-only <binary>` (verify without signing) exits non-zero on the AC2 binary (ad-hoc signature without runtime) and exits 0 on the binary signed above.
5. AC5: The installer downloads the binary for the current architecture to `~/.local/bin`, verifies its checksum, and aborts without installing on a mismatch.
   - `tests/release.rs`, macOS. Fixture: a temporary download directory with `agent-mail-aarch64-apple-darwin.tar.gz` and `agent-mail-x86_64-apple-darwin.tar.gz`, each holding a shell script `agent-mail` that prints its own target name, plus correct `.sha256` files.
   - Input: `sh install.sh` with `HOME=<temp home>` and `AGENT_MAIL_DOWNLOAD_URL=file://<download dir>`. Expected: exit 0, and `<temp home>/.local/bin/agent-mail` exists with mode 755. Running it prints the target for the host's `uname -m` (`arm64` maps to `aarch64-apple-darwin`, `x86_64` to `x86_64-apple-darwin`).
   - Mismatch: same input, but the host target's `.sha256` file holds 64 zeros and `<temp home>/.local/bin/agent-mail` already holds the text `old`. Expected: non-zero exit, stderr contains `checksum`, the file still holds exactly `old`, and no other file appears in `<temp home>/.local/bin`.
   - A missing asset (empty download directory) gives a non-zero exit and installs nothing.
6. AC6: The installed binary runs without any Rust toolchain present.
   - Opt-in macOS test on the copied, signed release binary.
   - `otool -L <binary>`: every linked library path starts with `/usr/lib/` or `/System/Library/`.
   - The binary run with a cleared environment except `HOME=<temp dir>` and `PATH=/usr/bin:/bin`, so no `CARGO_HOME`, `RUSTUP_HOME` or `~/.cargo/bin`: `--version` exits 0 and prints `agent-mail <Cargo.toml version>`, and `whoami` with no token gives exit 1 with `missing_token`.
   - In CI, `scripts/verify-release.sh` runs `--version` on the downloaded binary for the runner's architecture.
7. AC7: Release notes include the note that the first Keychain access after an update prompts once.
   - `tests/release.rs`: `.github/release-notes.md` contains `first Keychain access after an update prompts once` and the install one-liner `curl -fsSL https://github.com/ollie-dev/agent-mail/releases/latest/download/install.sh | sh`.
   - `.github/workflows/release.yml` passes `--notes-file .github/release-notes.md` to `gh release create`.
   - `README.md`'s "Install" section contains the same Keychain sentence.
8. AC8: The guard refuses every store access in-process when the hardened-runtime flag is missing, and passes through when it is present.
   - Unit tests of `hardened_runtime_ok`:
     - `Some(0x0001_0003)` gives true.
     - `Some(0x0002_0003)` (adhoc, linker-signed, no runtime) gives false.
     - `Some(0)` gives false.
     - `None` gives false.
   - In-process (`tests/selfcheck.rs`), with `GuardedStore::with_status(InMemoryStore holding Mandate W, Some(0x0002_0003))`, where W covers `agent@example.test` with `list` on `INBOX` and the fake IMAP server is running:
     - `whoami` and `list` with W's token each return `hardened_runtime_missing` with exit code 1, the fake server sees 0 connections, and the inner store's entries are unchanged.
     - `mandate list` and `mandate delete <W id>` return `hardened_runtime_missing`, and W is still stored.
     - `mandate create` on a scripted terminal returns `hardened_runtime_missing` before the first prompt: the scripted terminal records no prompt.
     - An MCP `tools/call` of `whoami` returns `isError` true with `structuredContent.error.code` `hardened_runtime_missing`.
     - The `list` call writes one audit line with the same `outcome` a `store_unavailable` failure records today and `reason` `hardened_runtime_missing`. The line contains no token.
   - With `Some(0x0001_0003)`, the same `whoami` and `list` calls succeed and match the output of the unwrapped store.
9. AC9: Debug builds skip the self-check.
   - On macOS, the debug binary that `cargo test` builds (`env!("CARGO_BIN_EXE_agent-mail")`) is linker-signed or unsigned without the runtime flag. Run as `whoami` with `AGENT_MAIL_TOKEN=<unknown token>`, it exits 1 with `invalid_token`, never `hardened_runtime_missing`.
   - Unit test: under `cfg(debug_assertions)`, `selfcheck::check()` returns pass regardless of the process's status.
10. AC10: Docs are updated and the suite is green.
    - `README.md` has an "Install" section with the one-liner, the `~/.local/bin` location, the checksum check, how to verify an attestation (`gh attestation verify <file> --repo ollie-dev/agent-mail`) and the Keychain note. It also has a "Releasing" paragraph: bump `Cargo.toml`, tag `vX.Y.Z` and push the tag, and the `verify` job checks the result.
    - `CLAUDE.md` lists `src/selfcheck.rs`, `tests/selfcheck.rs`, `tests/release.rs`, `install.sh`, `scripts/` and the workflow.
    - `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass.

## Out of scope
- Developer ID signing, notarisation and a Homebrew tap (spec Out of Scope).
- Windows release builds and the PowerShell installer (ticket 18).
- The Claude Code plugin and skill that point to the installer (ticket 17).
- Installing a specific version (the installer always takes `latest`), self-update and uninstall commands.
- Linux builds.
- Actually pushing a release tag from this job. AC3 and AC4 are proven by the first real tag push running the `verify` job; this branch only adds the workflow and scripts.

## Notes for reviewers
- Security:
  - This job is the macOS line of defence from ADR-0002: Keychain items trust only the agent-mail binary, which holds only if the binary cannot have code injected into it.
  - Check that the guard sits in front of every Keychain call, including `keys()` enumeration and the `mandate create` path. Check that a failed check never falls through to the inner store, and that nothing at runtime (environment, flag, file, Cargo feature present in release) can switch the guard off.
  - The skip depends on `debug_assertions`, so a release profile built with `debug-assertions = true` would also skip it. Consider pinning `[profile.release] debug-assertions = false` in `Cargo.toml`.
  - `csops` is declared by hand. Check the buffer size (`u32`) and that a failed call gives `None`, which fails the check.
  - The installer runs untrusted network data: it must verify the checksum before extracting or moving anything, and never execute what it downloaded. The checksum comes from the same release as the binary, so it guards against corruption and mirror tampering, not against a compromised release; the attestation covers that, and the README should say so.
  - `AGENT_MAIL_DOWNLOAD_URL` is controlled by the user who runs the installer.
  - Workflow permissions should be the minimum listed, and third-party actions should be pinned to a version or SHA.
  - No Credential or token appears in any test fixture. The opt-in tests use only a freshly minted unknown token and never read existing Keychain items, so they raise no prompt.
- Testability:
  - AC3 and AC4 need GitHub (a tag push and a published release), so they cannot run under `cargo test`. They are made checkable by the CI `verify` job and `scripts/verify-release.sh`, plus local structural checks of the workflow file and an opt-in signing test.
  - AC1, AC2, AC4 (local part) and AC6 are opt-in, ignored by default, because they build a release binary and touch the real Keychain (lookup only).
  - AC5 relies on `curl` handling `file://` URLs, which macOS's system `curl` does.
- UX:
  - Operator: the human installs with the one-liner, sees the install path and a PATH hint if needed, and after each update expects exactly one Keychain prompt, which the release notes and README explain.
  - Maintainer: bumps the version, pushes a `v*` tag and watches the `verify` job.
  - Agent: on an unhardened build, every mail command fails with `hardened_runtime_missing` and a message telling it to ask the human to reinstall.
- UI: terminal output only. That is the installer's progress and error lines, the new `hardened_runtime_missing` error in JSON and `--text`, and the release notes page on GitHub Releases. There are no other pages.
