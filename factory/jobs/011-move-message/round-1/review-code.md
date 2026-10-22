VERDICT: PASS

## Findings
- [nit] src/imap.rs (move_in): when the server advertises MOVE, `capabilities()` is read but the fallback `UID STORE` result is collected with a bare `Vec<Fetch>` type annotation; fine, but a short comment on why the STORE responses are discarded would help.
- [nit] src/core.rs (Command::Move audit): a `new_ref` is appended to `messages` only on success, as specified; no issue, just noting it matches AC7.

Checked: Policy checks on both mailboxes and exact-login Account match happen before any connection; capability check precedes SELECT; UIDVALIDITY check precedes fetch/move/copy; fallback uses UID EXPUNGE of that one UID only; COPYUID accepted only for the single source UID; target sent as quoted modified UTF-7. Full `cargo test` green, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean. Each AC has a test in tests/move_message.rs.
