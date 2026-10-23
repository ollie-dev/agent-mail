VERDICT: PASS

## Findings
- [follow-up] install.sh:25: `arm64 | aarch64` accepts aarch64 although the spec maps only arm64; harmless, no change needed beyond noting it. (Skip if you disagree.)

## Checked
- Read: spec.md, the diff stat, install.sh, scripts/sign-macos.sh and verify-release.sh (usage), release-notes.md, README Install/Releasing/tests sections, diffs of src/core.rs, create.rs, main.rs, Cargo.toml, src/selfcheck.rs.
- Ran: `cargo test --test release --test selfcheck`: 11 passed, 4 release-binary tests ignored as specified. `sh install.sh` with an empty file:// download dir: prints "could not download <url>", exit 1, nothing installed. Debug `agent-mail whoami --text` with invalid token: normal warning, no guard error (debug skip). `--help` works. `sign-macos.sh` and `verify-release.sh` with no args print a usage line.
- Error message names the missing hardened runtime, says the Keychain was not accessed and points to the releases page; README documents the new code and the attestation caveat.
