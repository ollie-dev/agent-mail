VERDICT: PASS

## Findings
- [nit] src/lib.rs (Cli.text): the spec's global `--text` is no longer global, because clap cannot have it alongside `search --text S`. The builder's workaround is sound and documented in `search --help`, the README and `text_flag_still_works_after_list_and_mandate`. One cost: `agent-mail search ... --text` now filters on the TEXT criterion, not the output format. Worth a line in the skill docs when ticket 15 or the skill is touched.
- [nit] src/core.rs (`connector_error`): `ImapError::MailboxNotFound` with `mailbox: None` maps to `Protocol`. That is unreachable today, but it could be made unrepresentable later.
- [nit] The reference encoding follows the ACs (RFC 3986 pchar, so `[` and `]` are encoded) rather than the spec prose. That is the right call, and the deviation is flagged in build.md.

Verification: `cargo test` is green in the worktree. The suites that ran all report ok, and 1 test is ignored (the keychain opt-in). I did not scan the output for a missing suite. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

- The Policy check runs before any connection.
- `EXAMINE` is used with `UID FETCH` of the top-`limit` UIDs only, and the fetch asks for ENVELOPE, not a body.
- The audit entry logs references and `results` (the number returned), never `total`.
- The core's `Query` is built from validated types, so CR, LF and NUL cannot reach it.
- `--text` output goes through `escape_controls`.
