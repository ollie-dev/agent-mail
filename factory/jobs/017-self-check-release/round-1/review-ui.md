VERDICT: PASS

## Findings
- [follow-up] src/core.rs (HardenedRuntimeMissing message): "refused to access the Keychain and the Keychain was not accessed" says the same thing twice; trim to "so it refused to access the Keychain" in a later wording pass.

## Checked
- Read: full diff of src/core.rs, create.rs, main.rs, install.sh, scripts/sign-macos.sh, scripts/verify-release.sh, .github/release-notes.md, README.md changes.
- Ran: install.sh against a file:// dir (success: progress, "Checksum verified.", installed path, PATH hint with copyable line, exit 0; malformed checksum: one-line "agent-mail installer: ... nothing was installed" on stderr, exit 1). Output is plain text, no colour, lines fit near 80 columns except the path-bearing lines. The error message goes through the existing CoreError rendering, so --text and JSON match other errors.
