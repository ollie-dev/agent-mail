VERDICT: PASS

## Findings
- [follow-up] src/core.rs (draft audit entry): a core call with an invalid `--to` logs `"recipients":[]`, a deliberate deviation. When the MCP job (ticket 15) lands, confirm that this is the wanted behaviour for rejected input.
- [follow-up] src/imap.rs (`append`): a server `NO [TRYCREATE]` or `BAD` on APPEND maps to a generic protocol error. A later job could give it a clearer message.

Tests: `cargo test` is green, and `cargo clippy -- -D warnings` and `cargo fmt --check` are clean. I read every changed line in `src/draft.rs`, `src/core.rs`, `src/imap.rs`, `src/audit.rs` and `src/lib.rs`. I did not read all of `tests/draft.rs` and `tests/support/mod.rs`, which are mostly fixtures; the suite passing covers them.
- The Policy checks run before any connection, and the account is matched by exact login.
- The header-injection validation is repeated in the core.
- The original is fetched with EXAMINE and BODY.PEEK, with the UIDVALIDITY check before the fetch.
- The Drafts name comes only from the server's LIST.
- The audit entry holds references and recipients only.
- The deviations the builder listed are reasonable. In particular, the AC2 `ref` follows the `MessageRef::format` percent-encoding, which `read` can actually parse.
