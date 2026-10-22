VERDICT: PASS

## Findings
- [nit] src/core.rs (`CoreError::message`): `nothing_done` treats every non-Read operation as "no flag was changed". That is correct today, but a future operation such as move would need its own branch.
- [nit] src/core.rs (`flag`): the audit `change` is computed by parsing the flag name a second time (`Flag::parse(name)` in `execute`). Carrying the parsed flag in `Trail` would avoid the repeat. This is harmless.

Checked: the full diff was read. `cargo test` is green, `cargo clippy --all-targets -- -D warnings` is clean, and `cargo fmt --check` is clean.
- The Policy check and the flag-name mapping both happen before any connection. Only the canonical `\Seen` or `\Flagged` reaches the server.
- The connector never sends CLOSE, EXPUNGE or a body fetch. It checks UIDVALIDITY before any fetch or store, and checks existence before the store.
- The rejected flag name is neither echoed nor logged.
- `StaleReference`, `MessageNotFound` and `InvalidReference` now carry `operation`, and `read` still reports `read`.
- Each acceptance criterion has a test in `tests/flag.rs`.
