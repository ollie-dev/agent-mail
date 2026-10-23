VERDICT: PASS

## Findings
None. The change only rewords clap doc comments and five `CoreError` message strings, adds tests and adds one reviewer bullet. No new interpolated values, no visibility changes, no new dependencies, and access policy is untouched.

## Checked
- Read: the full `git diff main...factory/019-help-error-text` (one commit, 9632ff3) across `src/core.rs`, `src/lib.rs`, `tests/cli.rs`, `tests/project.rs`, `tests/selfcheck.rs` and `.claude/agents/review-ui.md`.
- Read: `src/core.rs`, the `Uncovered`, `Unsupported`, `ProxyRefused`, `AmbiguousMandate` and `HardenedRuntimeMissing` messages. They still interpolate only account, mailbox, host, port, operation and Mandate IDs, the same as before, and never a token or Credential. `hardened_runtime_missing` still says that the Keychain was not accessed ("refused to access the Keychain") and still points to `RELEASES_URL`.
- Read: `src/core.rs` `Warning::invalid_tokens` (unchanged). It shows only token positions, never token values.
- Read: `src/lib.rs` help text. The security-relevant facts are kept: `whoami` "never shows a credential or token"; `read` says attachments are "never their content"; `mandate create` says it refuses without a terminal or in a sandbox, that "No option can supply a Credential", and that the export never includes Credentials or the Agent Token; `mcp` says no tool argument can supply or override the token and that Mandate management is not offered over MCP; `move` still needs a grant on both mailboxes (the long help says it, so dropping it from the `--to` line loses nothing); `draft` still requires a `read` grant for a reply. No `#[arg]` attributes, value parsers or `conflicts_with` changed, so input validation is the same.
- Read: `tests/cli.rs` new tests. Fixtures use `example.com` sample values and the existing `support::store_mandate` helper, with no real credentials. The `whoami` test builds `Context::trusting_only` with the in-memory store and does not touch the Keychain or the user's audit log. The test uses no new public API: `CoreError`, `RELEASES_URL` and `Warning::invalid_tokens` were already `pub`.
- Read: `tests/selfcheck.rs` assertion change (`not accessed` → `refused to access the Keychain`), which matches the new AC7 text.
- Read: `build.md`, then checked its claims against the code above.
- Ran: `git diff main...factory/019-help-error-text -- Cargo.toml Cargo.lock`. Empty, so no new dependencies.
- Ran: grep of the diff for `password|secret|app_password|credential` and token-like strings. Only help prose mentioning "Credential" matched; there are no literals.
- Ran: `cargo test --test cli --test project --test selfcheck`. cli 20 passed, project 6 passed, selfcheck 11 passed (4 ignored release-binary tests).
