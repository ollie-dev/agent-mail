## Round 1
**Changed:**
- New `src/draft.rs`: `DraftResult` and its `--text` line, address/subject/body validation, reply subject (`Re: ` added once), References builder with message-id filtering, original-header parsing, CSPRNG Message-ID (ring `SystemRandom`), RFC 5322 date, and the message builder (ASCII headers, RFC 2047 `B` subject folded at 45 octets per word, folded `To:`/`References:`, quoted-printable body, all lines <= 76/78 octets).
- `src/imap.rs`: `draft()` runs one session: login, optional `EXAMINE` + UIDVALIDITY check + `UID FETCH <uid> (UID BODY.PEEK[HEADER])`, `LIST "" "*"` picking the first `\Drafts` entry (`ImapError::DraftsNotFound` otherwise), then a hand-driven `APPEND "<raw>" (\Draft) {n}` that reads `APPENDUID` from the tagged OK; `LOGOUT`. No SELECT/STORE/COPY/MOVE/EXPUNGE/CLOSE.
- `src/core.rs`: `Command::Draft`, `Outcome::Drafted`, `CoreError::DraftsNotFound` / `InvalidDraft { field }`; core re-validates to/subject/body before anything else; Policy checks before any connection (reply: covered Account by exact login, else `denied` op `read`; then `draft` grant; then `read` on the original's mailbox). Stale/not-found messages for `draft` say "no draft was created". Audit entry with `mailbox` (once found), `messages` (original, then new ref), `recipients`.
- `src/audit.rs`: optional `recipients` key between `messages` and `change`.
- `src/lib.rs`: `draft` subcommand with help text, value parsers (usage error exit 2), `--subject` conflicts with/required unless `--reply-to`, `--text` rendering.
- `tests/support/mod.rs`: `start_with_list`, `\Drafts` LIST marking, `Thread` and `Drafts` fixtures, `APPEND` with synchronising literal / `APPENDUID` / `TRYCREATE`, `appended()`, `raw_header()` helper.
- README "Draft" section and Audit log updates; CLAUDE.md layout names `src/draft.rs` and `tests/draft.rs`.

**Commits:** e3d55df
**Tests:** `cargo test` -> all suites `ok` (lib 59 passed, tests/draft.rs 11 passed, no failures); `cargo clippy -- -D warnings` and `cargo fmt --check` clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | `ac1_draft_is_appended_with_the_draft_flag_to_the_drafts_mailbox` | pass |
| AC2 | `ac2_drafts_mailbox_is_found_by_special_use_not_by_name` | pass |
| AC3 | `ac3_no_drafts_mailbox_fails_with_a_structured_error_and_creates_nothing` | pass |
| AC4 | `ac4_reply_draft_threads_with_the_original_and_prefixes_re_once` | pass |
| AC5 | `ac5_reply_needs_read_on_the_original_mailbox` | pass |
| AC6 | `ac6_without_a_draft_grant_the_call_is_denied_and_audited` | pass |
| AC7 | `ac7_no_command_edits_or_deletes_a_draft` (+ `Drop for Setup` asserting no forbidden verb in every test) | pass |
| AC8 | `ac8_audit_entry_records_recipients_and_references_only` | pass |
| Other | `header_injection_values_are_refused_by_the_cli_and_the_core`, `non_ascii_subjects_and_long_bodies_are_encoded`, `stale_or_missing_reply_references_create_nothing` | pass |

**Deviations from the spec, for the reviewer to check:**
- AC2's `ref`: the spec writes `agent@example.test/[Gmail]/Entwürfe/1700000003/1`, but that is not a valid message reference (it has five `/` segments and `read::parse_reference` would refuse it, which contradicts AC2's own "the new `ref` works as a reference" bullet). The ref is built with `MessageRef::format`, as `search` and `move` do: `agent@example.test/%5BGmail%5D%2FEntw%C3%BCrfe/1700000003/1`. The test asserts that exact string and that `read` with it returns subject `Status`.
- `recipients` when a `--to` value is invalid (only reachable through the core): logged as `[]`, so a rejected value is never written to the log, while the key stays present on every `draft` entry.
- `From:` and the Message-ID domain use the login only when it passes the same bare-address check as `--to` (stricter than "contains exactly one `@`"), so a malformed login cannot inject header text; otherwise no `From:` and `@agent-mail.invalid`.
- Message ids from the original longer than 900 characters are also dropped (keeps the 998-octet line limit). An original whose Message-ID is unsafe is treated as having none (no In-Reply-To/References, reply subject kept).
- ASCII subjects are also RFC 2047-encoded when they contain `=?` or exceed 900 characters (fidelity and line length); control characters in the original's decoded subject (CR, LF, NUL) are replaced by spaces.
- A core call with both `subject` and `reply_to`, or neither, fails with `invalid_draft` field `subject`.
- An `APPEND` the server refuses (NO/BAD) maps to `server_error`.

**Review findings:** none (round 1).

## Nit pass
**Commits:** 2d0e904
**Tests:** `cargo test` -> every suite `ok`, 0 failed (lib 59 passed, tests/draft.rs 11 passed); `cargo clippy -- -D warnings` -> Finished, no warnings; `cargo fmt --check` -> clean.
**Findings:**
- review-ux.md [fix-now] draft help does not say --subject and --reply-to cannot be combined -> applied: the `draft` help now reads "or --reply-to with a reference exactly as `agent-mail search` prints it, but not both".
