VERDICT: PASS

## Findings

No findings. I read every changed file looking for security problems:

- **Policy:** checks finish before any connection. In reply mode, an Account the Mandate does not cover is denied, and the Account is matched by exact login through `mandate.account(login)`. Next comes `draft` on that Account (`grants_for`, exact account match), then `read` on the original's mailbox. Without a reference the call uses `single_account`. Every path denies by default, and `trail.allowed` is set only after all checks pass.
- **Header injection:** the core checks `to`, `subject` and `body` again (`src/core.rs` `draft`), not only the clap value parsers. Addresses must be bare printable ASCII with no CR, LF, `<`, `>`, `,`, `;` or `:`. A subject that is not plain ASCII, or that contains `=?`, is RFC 2047 encoded. The body goes after the blank line and is quoted-printable encoded, so the whole literal is ASCII and `message.len()` is the correct octet count. `From:` and the Message-ID domain use the login only when it passes `is_valid_address`.
- **Server data:** Message-ID and References from the original are stripped and then filtered by `is_safe_message_id`: printable ASCII, no space or angle brackets, at most 900 characters. The original's subject is decoded, its CR, LF and NUL are replaced, and it is re-encoded. The returned subject shows original content only to a caller that holds `read` on that mailbox.
- **APPEND target:** the mailbox name comes only from the server's `LIST` `\Drafts` entry. A name with CR, LF or NUL is refused, and the name is sent through `quoted()`. The Agent cannot pick the target mailbox.
- **Reply fetch:** the original's mailbox is opened with `EXAMINE`, UIDVALIDITY is checked before the fetch, and the fetch uses `BODY.PEEK[HEADER]`. The connector never sends SELECT, STORE, COPY, MOVE or EXPUNGE, and the `Drop` check in `tests/draft.rs` asserts this for every test.
- **Audit:** entries hold recipients and references only. If any `--to` value is invalid, `recipients` is logged as `[]`, so a rejected value is never written. `InvalidDraft` messages name only the field. AC8 asserts that the log never contains a subject, body, Message-ID, Credential, Agent Token or token hash, and `assert_no_secrets` runs on every CLI call in the tests.
- **Errors and output:** connector errors map to enum codes, so neither the Credential nor server text can appear in them. In `--text` mode, values go through `escape_controls`.
- **Dependencies:** none added. `ring` and `base64` were already dependencies, and `Cargo.toml` and `Cargo.lock` are unchanged.
- **Tests:** `cargo test --test draft` passes, 11 of 11.
