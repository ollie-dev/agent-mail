VERDICT: PASS

## Findings
- [nit] src/export.rs: `from_toml` now has two parsers (the span-aware walk and the strict serde cross-check). The duplication is deliberate and documented, so it is acceptable. If the two ever disagree, the user sees only the generic "does not match the Mandate file format" message.
- [nit] src/create.rs: the Gmail warning says "The credential you type next goes to ...". That is accurate, but it could also say how to fix it (Ctrl-C and correct the host).

Round 1 had no blocking items. The round 2 changes match the human notes:
- The Credential prompt names `host:port`, with the host escaped.
- The Gmail host warning is covered by a unit test and an integration test.
- File errors give a line only and repeat no key name.
- `--from` is refused unless it is a regular file, checked before the open and again on the opened handle.

`cargo test` is green and `cargo clippy --all-targets -- -D warnings` is clean.
