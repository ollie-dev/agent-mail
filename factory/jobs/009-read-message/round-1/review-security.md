VERDICT: PASS

## Findings
- [nit] src/core.rs:248: when the core gets a reference that fails `is_valid_reference` (`InvalidReference`), the audit entry still writes `mailbox` and `messages` from the raw reference. serde_json escapes CR, LF and NUL, so the log cannot be injected, and only non-CLI adapters can reach this path. Consider leaving `mailbox` and `messages` out for this reason so the log never records a name that was never checked.
- [nit] src/read.rs:770-788: header values (from, to, cc, subject, message_id, attachment names) go into the JSON output as the server sent them. serde_json escapes C0 controls but leaves DEL, C1 and bidi or format characters (for example U+202E) as they are. `search` already behaves the same way, and `--text` escapes them, so this is not a regression. Worth a shared decision when the MCP adapter (ticket 15) lands.
- [nit] src/imap.rs:429: the whole message is fetched and parsed in memory, and nothing bounds how deep mail-parser recurses into nested multiparts. The spec accepts this for this slice (Out of scope), and the 30 s TIMEOUT bounds the wall-clock time. Track it with the partial-fetch follow-up.

## Checked
- Policy comes before connection: core `read` matches the Account by exact login against the Mandate, then calls `allows_mailbox(Read, login, decoded mailbox)` (exact match or `*`) before `imap::read`. Denial is the default, and an uncovered Account can never borrow another Account's Credential. AC7 tests assert that no connection is made.
- Input validation: `MessageRef::parse` plus `is_valid_reference` (no CR/LF/NUL, non-empty, UID >= 1) run in clap (exit 2, no audit) and again in the core. The mailbox reaches the server only as modified UTF-7 through async-imap's quoted `EXAMINE`, and the UID only as a decimal `u32`.
- No state change: the code uses `EXAMINE` and `UID FETCH <uid> (UID BODY.PEEK[])` only. The UIDVALIDITY check runs after EXAMINE and before any fetch. An unsolicited FETCH for another UID is ignored. AC4 and AC6 cover this.
- Untrusted content: parse failures map to `server_error` (Protocol) with no panic, and a garbage-input unit test covers it. `html_to_text` is a pure string conversion: no fetches, no script execution. `--text` sends every header value, attachment name and body line through `escape_controls`, with a unit test using `\x1b[2J`. Attachment content never reaches `ReadResult`; only name, type and size do.
- Audit: the entry carries the reference only (`messages`) and never `results`. The audit is fail-closed. AC8 checks the log bytes for subjects, body words, names, addresses, the Credential, the token and the token hash.
- Credentials: none appear in new error messages, output or fixtures. The tests use the existing `support::CREDENTIAL` test constant. `log` is still compiled out.
- Dependencies: `mail-parser` 0.11.9 (pure parsing, no I/O, no unsafe network or fs access) and its build-time proc-macro `hashify`. Acceptable, and the feature spec names it.
- Tests: `cargo test --lib --test read` passes (43 + 10).
