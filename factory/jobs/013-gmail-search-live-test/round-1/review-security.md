VERDICT: PASS

## Findings

None. Every changed file was read for security issues:

- Injection: the `--gmail` query is a `SearchText` (CR/LF/NUL rejected; empty rejected by `gmail_query`), and `imap_criteria()` sends it only through `SearchText::quoted` (`"` and `\` escaped). So the query cannot end the quoted string or add keys/commands. The unit test covers quote and backslash input, and so does the AC1 integration test.
- Capability gate: `search_async` reads `CAPABILITY` after login and returns `GmailUnsupported` before `EXAMINE`, then logs out. No mailbox is opened.
- Access policy: `--gmail` goes through the same core `search` path, so Grant checks run before any connection. The AC3 tests show `denied` with `connections() == 0` for an ungranted mailbox and for a Mandate with no search Grant.
- Leakage: the `unsupported` message names the account and mailbox only, never the query. Audit lines carry no query text (the AC3 test asserts this). `ImapError` variants carry no data, so `{e:?}` in the live test cannot print a secret.
- Live test (`tests/gmail_live.rs`): the password goes into a `Credential` (redacted `Debug`) inside an `InMemoryStore`. The plain-text copy used for the leak checks is `Zeroizing`. stdout and stderr are checked for the password and the token before any assertion that could show them. The audit log is checked at the end. No assert message formats a `RunOutput`, `Mandate`, `Account`, config or environment value. Skip messages name variables, never values. The fixtures are fake (`live@example.test`, `fake-app-password-123`, README placeholder `you.test@gmail.com`). The README uses `read -rs`, which keeps the password out of shell history.
- Dependencies: none added. `log` stays `max_level_off`/`release_max_level_off`, so async-imap's `LOGIN` traces remain compiled out.
- `drafts_mailbox` refactor: same LIST parsing and CR/LF/NUL rejection of the server-given name as before. The `draft` behaviour is unchanged (the draft tests pass).
