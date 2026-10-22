VERDICT: PASS

## Findings
- [nit] src/imap.rs:661: an untagged FETCH for the UID that carries no FLAGS item (a server sending only `UID`, or a mix of unsolicited responses) is taken as the result and reported as an empty flag set. This is not a security issue, but it could tell the Agent "no flags" when the message has some. Use the store's FETCH only when it actually carries FLAGS; otherwise fall back to the follow-up `UID FETCH (UID FLAGS)`.
- [nit] src/core.rs:234: `change` is computed by parsing the untrusted name a second time, separately from `flag()`. It is correct today: only the canonical literal is logged, never the Agent's text. Returning the parsed `Flag` through `Trail` would keep one source of truth.

## Checked
- Credentials: there are no new log, error or output paths that carry the Credential or the Agent Token. Errors reuse `connector_error`. AC6 asserts the audit bytes never contain the Credential, the token or the token hash. No secrets in fixtures or the commit (99214f1).
- Injection: the flag name never reaches the server. `Flag::parse` maps it to `\Seen` or `\Flagged`, and the STORE item is built only from those literals. CR/LF and other values are rejected before any connection, and the rejected value is neither echoed nor logged. The reference is re-checked with `is_valid_reference` in the core. The mailbox goes through `utf7::encode` and async-imap's quoted `SELECT`, the same as `EXAMINE` in `read`. The UID is a validated u32.
- Access policy: the order is reference check, flag parse, exact-login Account match, then `allows_mailbox(Flag, login, mailbox)`. All of these happen before `imap::flag`, so the default is deny. An uncovered Account is denied without using another Account's Credential. AC4 covers a wrong-mailbox Grant, an empty Policy and an uncovered Account, each with no connection.
- Mailbox safety: the server receives `SELECT`, then the UIDVALIDITY check, then `UID FETCH (UID FLAGS)`, `UID STORE` and `LOGOUT`. No `CLOSE`, `EXPUNGE` or body fetch is sent, and AC1 asserts this. The stale check comes before any fetch or store (AC5).
- Output: server-supplied flag names are JSON-escaped, and in `--text` mode passed through `escape_controls`.
- Dependencies: none added (Cargo.toml and Cargo.lock unchanged).
- `cargo test --test flag`: 7 passed.
