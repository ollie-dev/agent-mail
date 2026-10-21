# Read a message by its reference

## Goal
An Agent runs `agent-mail read <REF>` with a message reference from `search` and gets back the message's headers, its plain-text body (from the text part, or the HTML part converted to text) and a list of its attachments (name, type, size), without the message being marked as seen. Bodies longer than 20,000 characters are cut with a truncation marker unless `--full` is given. A reference whose UIDVALIDITY no longer matches the mailbox fails with `stale_reference` instead of reading the wrong message. Every read call appends an audit entry with the message reference and no body content.

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Ticket: `.scratch/agent-mail-v1/issues/08-read.md`. Feature spec: `.scratch/agent-mail-v1/spec.md`, sections "Policy and Grants", "Mail operations" and "Audit log". Terms as in `GLOSSARY.md` (Account, Agent, Mandate, Agent Token, Policy, Grant).
- Files this feature touches:
  - `Cargo.toml` / `Cargo.lock`: add `mail-parser` (named in the feature spec's dependency list) for MIME parsing, charset and transfer decoding and RFC 2047/2231 names. HTML-to-text may use mail-parser's own conversion or a small HTML-to-text crate, whichever the builder picks, as long as AC2 holds.
  - `src/lib.rs`: new `read` subcommand and its `--text` rendering.
  - `src/core.rs`: `Command::Read { reference: MessageRef, full: bool }`, `Outcome::Message(ReadResult)`, the Policy check, the audit entry, and new errors `stale_reference` and `message_not_found` (both with `operation`, `account`, `mailbox`, and a message that names a next step).
  - `src/read.rs` (new): the result types, MIME-to-result conversion (headers, body selection, HTML to text, attachment list), line-ending normalisation and truncation.
  - `src/imap.rs`: a read call: log in, `EXAMINE` the mailbox, compare its UIDVALIDITY with the reference's, then `UID FETCH <uid> (UID BODY.PEEK[])`, logout. It shares the connect, TLS and login code with `list_mailboxes` and `search`.
  - `src/search.rs`: no change expected. `MessageRef::parse` and `format` from job 008 are reused.
  - `tests/read.rs` (new) and `tests/support/mod.rs`: `FakeMessage` gains an optional full raw MIME source that `raw()` returns when set, and the fake gains the `Read` mailbox below. It already sets `\Seen` on a non-peek body fetch and records `SELECT`/`STORE`.
  - `README.md`: a short "Read" section (command, output fields, truncation, `--full`, `stale_reference`), and the Audit log section names `read` and documents `mailbox` and `messages` for it.
  - `CLAUDE.md`: the Stack layout line names `src/read.rs` and `tests/read.rs` (checked by `tests/project.rs`).

### Command line
`agent-mail read <REF> [--full]`, plus the global `--text`.
- `<REF>` is a message reference as `search` prints it: `<account>/<mailbox>/<uidvalidity>/<uid>`, parsed with `MessageRef::parse`. A reference that does not parse, or whose decoded account or mailbox contains CR, LF or NUL, is a usage error caught by clap: exit code 2, a usage message on stderr, no connection, no audit entry.
- The Account used is the Mandate's Account whose login equals the reference's account. If the Mandate covers no such Account, the read is denied (AC7).
- `--full`: return the whole body, untruncated.

### Result (JSON, one line on stdout)
```json
{"ref":"agent@example.test/Read/1700000006/1","account":"agent@example.test","mailbox":"Read",
 "date":"2026-10-06T08:15:00Z","from":"Alice Example <alice@example.com>","to":["agent@example.test"],"cc":["Carol <carol@example.com>"],
 "subject":"Project update","message_id":"<update-1@example.com>",
 "body":"Hi,\n\nthe project is on track.\n\nAlice","truncated":false,"attachments":[]}
```
Keys exactly these, in this order.
- `date`: the `Date:` header in RFC 3339 UTC (`core::rfc3339_utc` format); `""` when absent or unparseable.
- `from`: the first From address, formatted as in `search` (`Name <addr>` or `addr`); `""` when absent. `to`, `cc`: every address in the header, same format, in header order; `[]` when absent.
- `subject`: RFC 2047-decoded; `""` when absent. `message_id`: the `Message-ID` header with its angle brackets; `""` when absent.
- `body`: the text of the first `text/plain` body part. If there is none, the first `text/html` body part converted to text. `""` if neither exists. Decoded from its transfer encoding and charset. Line endings normalised to `\n`, and trailing line breaks removed.
- Truncation counts Unicode scalar values (Rust `char`s) of the normalised body. If it has more than 20,000 and `--full` is not given, `body` is its first 20,000 chars followed by the marker `\n\n[truncated: showing 20000 of <N> characters; read again with --full for the complete body]`, and `truncated` is `true`. Otherwise `body` is complete and `truncated` is `false`.
- `attachments`: every leaf MIME part that is not the selected body part or its `multipart/alternative` sibling, in message order, each `{"name":...,"type":...,"size":...}`. `name` comes from the Content-Disposition `filename` or else the Content-Type `name`, RFC 2047/2231-decoded, `""` when neither exists. `type` is the lower-case `type/subtype` without parameters. `size` is the decoded size in bytes. No attachment content is ever returned.
- A UID that does not exist in the mailbox (no FETCH response) fails with `message_not_found`, exit 1. A mailbox the server refuses to open fails with the existing `mailbox_not_found`.

### Fake server fixtures (used by the criteria below)
A new mailbox `Read`, UIDVALIDITY `1700000006`. No message has `\Seen`. Each message's raw source is given as a full MIME message with CRLF line endings:

| UID | Content | Key values |
|---|---|---|
| 1 | `text/plain; charset=utf-8`, 7bit | `From: Alice Example <alice@example.com>`, `To: agent@example.test`, `Cc: Carol <carol@example.com>`, `Subject: Project update`, `Date: Tue, 06 Oct 2026 10:15:00 +0200`, `Message-ID: <update-1@example.com>`, body `Hi,\r\n\r\nthe project is on track.\r\n\r\nAlice\r\n` |
| 2 | `text/html; charset=utf-8` only, quoted-printable | `From: news@example.com`, `Subject: HTML only`, body `<html><head><style>p{color:red}</style></head><body><p>Hello <b>Bob</b>,</p><p>See&nbsp;the <a href="https://example.com/x">report</a> &amp; reply.</p><script>alert(1)</script></body></html>` |
| 3 | `multipart/mixed`: a `text/plain` part `See the attached files.`, then `report.pdf` (`application/pdf`, `Content-Disposition: attachment; filename="report.pdf"`, base64 of the 1024 bytes `0x00..=0xFF` repeated four times), then `notes.txt` (`text/plain`, `Content-Disposition: attachment; filename="notes.txt"`, 7bit, content `ATTACHMENT-CONTENT-7f3a`, 23 bytes) | `From: bob@example.com`, `Subject: Files` |
| 4 | `text/plain; charset=utf-8`, 8bit, one line of 25,000 chars: 19,999 × `x`, then `ü`, then 5,000 × `y` | `From: alice@example.com`, `Subject: Long` |
| 5 | `multipart/alternative`: `text/plain` `Plain version`, then `text/html` `<p>HTML version</p>` | `From: alice@example.com`, `Subject: Both` |

Unless a criterion says otherwise, tests use the Mandate `aaaa1111` with the single Account `agent@example.test` (`support::LOGIN`) and Grants `read` on `Read` and `search` on `*`. References are written `R(n)` = `agent@example.test/Read/1700000006/<n>`.

## Acceptance criteria
1. AC1: Reading a plain-text message returns its headers and body. Test: `agent-mail read R(1)` exits 0 and stdout parses as exactly the JSON object shown under "Result" (`"date":"2026-10-06T08:15:00Z"`, `"cc":["Carol <carol@example.com>"]`, `"body":"Hi,\n\nthe project is on track.\n\nAlice"`, `"truncated":false`, `"attachments":[]`). A reference taken from `search --mailbox Read` output for UID 1 gives the same result.
2. AC2: Reading an HTML-only message returns readable text without markup. Test: `read R(2)` exits 0. `body` contains `Hello Bob,`, `report` and `& reply.`, and `See` and `the` separated by a space or no-break space. It does not contain `<`, `>`, `&amp;`, `&nbsp;`, `color:red` or `alert(`. `attachments` is `[]`. Also `read R(5)` (both parts) returns `"body":"Plain version"` and `"attachments":[]`.
3. AC3: A multipart message with attachments lists each attachment's name, type and size, and returns no attachment content. Test: `read R(3)` exits 0 with `"body":"See the attached files."` and `"attachments":[{"name":"report.pdf","type":"application/pdf","size":1024},{"name":"notes.txt","type":"text/plain","size":23}]`. Neither stdout nor `--text` output contains `ATTACHMENT-CONTENT-7f3a` or `AAECAwQFBgcICQoL` (the start of the PDF's base64).
4. AC4: After reading, the message is still unread on the fake server. Test: read `R(1)`, `R(2)`, `R(3)` and, with `--full`, `R(4)`. Afterwards the fake's `all_flags()` show no `\Seen` on any `Read` message, and its `commands()` contain no `SELECT`, `STORE` or `UID STORE`.
5. AC5: A body longer than 20,000 characters is cut at 20,000 with a truncation marker; `--full` returns it complete. Test: `read R(4)` gives `"truncated":true` and a `body` equal to 19,999 × `x` + `ü` + `\n\n[truncated: showing 20000 of 25000 characters; read again with --full for the complete body]`. `read R(4) --full` gives `"truncated":false` and a `body` of exactly 25,000 chars ending in 5,000 × `y`. `read R(1)` (short) is unaffected by `--full`.
6. AC6: A reference with a UIDVALIDITY different from the server's fails with a structured `stale_reference` error. Test: `read agent@example.test/Read/1700000099/1` exits 1, stdout is empty, and stderr parses as `{"error":{"code":"stale_reference","operation":"read","account":"agent@example.test","mailbox":"Read","message":...}}`. The message tells the Agent to search the mailbox again for a fresh reference, and the existing `every_error_message_names_a_next_step` test covers the new variants. The fake sees an `EXAMINE` but no `UID FETCH`. `--text` prints `error: stale_reference: ...`.
7. AC7: Reading from a mailbox without a `read` Grant is denied and audited. Test, each case against a fresh audit log: (a) Grants `read` on `INBOX` and `search` on `Read`, then `read R(1)`; (b) an empty Policy, then `read R(1)`; (c) the default Grants, then `read other@example.test/Read/1700000006/1` (an Account the Mandate does not cover). Each exits 1, stdout is empty, and stderr parses as `{"error":{"code":"denied","operation":"read","account":<the reference's account>,"mailbox":"Read","message":...}}`. The fake server sees no connection. The log holds one line `{"time":T,"mandate":"aaaa1111","operation":"read","account":<the reference's account>,"mailbox":"Read","messages":[<the reference>],"outcome":"denied","reason":"denied"}`.
8. AC8: Each read appends an audit entry with the message reference and no body content. Test: `read R(3)` appends exactly one line `{"time":T,"mandate":"aaaa1111","operation":"read","account":"agent@example.test","mailbox":"Read","messages":["agent@example.test/Read/1700000006/3"],"outcome":"allowed"}`, keys in this order, with no `results` key. AC6's read logs `"outcome":"allowed","reason":"stale_reference"` with the same `messages` form, and reading `agent@example.test/Read/1700000006/99` logs `"reason":"message_not_found"`. A malformed reference (`read not-a-ref`) exits 2 and logs nothing. After AC1 to AC5's reads, the log bytes never contain a subject (`Project update`, `Files`), a body word (`on track`, `Hello`, `xxxxxxxx`), an attachment name (`report.pdf`), `ATTACHMENT-CONTENT-7f3a`, an address from a message (`alice@example.com`), the Credential, the Agent Token or its token hash.

## Out of scope
- `--account` and several tokens (ticket 13). The Account is taken from the reference. Mandate selection across tokens is not part of this job.
- Attachment download or content, inline image rendering, and returning raw MIME or arbitrary headers beyond those listed.
- Reading several messages in one call, or reading by sequence number or Message-ID.
- Flag changes, including an explicit "mark as read" (ticket 09).
- The MCP `read` tool (ticket 15).
- Gmail-specific fetch items (`X-GM-MSGID`, `X-GM-LABELS`) and the live Gmail test (ticket 12).
- Partial fetches (`BODY.PEEK[]<0.N>`) to bound download size. The whole message is fetched and truncated locally.

## Notes for reviewers
- Security:
  - Reading fetches mail through the Credential, so the Policy check (`read` on the reference's Account and decoded mailbox) must happen before any connection (AC7 asserts no connection). The Account is matched by exact login against the Mandate's Accounts, so a reference naming an uncovered Account can never use another Account's Credential.
  - The reference is untrusted Agent input. It is parsed by `MessageRef::parse`, which rejects anything other than four segments with numeric UIDVALIDITY and a UID ≥ 1. CR, LF and NUL in the decoded account or mailbox are rejected. The mailbox goes to the server only as the modified UTF-7 encoding of the decoded name, and the UID only as a decimal number. The core should validate again, not only clap, because the MCP adapter (ticket 15) calls the core directly.
  - The UIDVALIDITY check must happen after `EXAMINE` and before any `UID FETCH` (AC6 asserts no fetch), so a renumbered mailbox never yields another message's content.
  - The message is untrusted server data: headers, body and attachment names may contain control characters, escape sequences, enormous parts or malformed MIME. JSON output escapes control characters. In `--text` output, every header value and attachment name goes through `escape_controls`. The body keeps its `\n` line breaks, but every other control or format character is escaped. Builder adds a unit test with `\x1b[2J` in the subject, body and an attachment name. A parse failure maps to `server_error`, never a panic. HTML conversion must not fetch remote resources or execute anything.
  - Reading must not change mailbox state. Use `EXAMINE` and `BODY.PEEK[]`, never `SELECT`, `BODY[]` or `RFC822` (AC4; the fake sets `\Seen` on a non-peek fetch, as RFC 3501 requires).
  - The audit entry carries the reference, never subjects, addresses, bodies or attachment names (AC8). Including `messages` on a denied read records what the Agent tried; the reference holds only the account, mailbox and numbers, which the log already contains for searches.
  - The `stale_reference` and `message_not_found` errors, the CRLF-to-LF normalisation, trailing-line-break trimming and the exact marker text are this spec's concrete choices, made so the ticket's criteria are testable. The ticket only names `stale_reference`.
  - The whole message is fetched before truncation, so a very large message costs memory and time. This is accepted for this slice and listed under Out of scope.
- UX: The user is an Agent calling the CLI, usually through the skill. Typical flow: `agent-mail search --mailbox INBOX --unseen`, then `agent-mail read <ref>` for each result worth reading. It sees `truncated` and decides whether to rerun with `--full`, and it reports attachments to the human by name. A `stale_reference` error tells it to search again. A denial names operation, Account and mailbox, so it can tell the human which Grant to add. `--help` for `read` explains the reference argument (as printed by `search`), `--full` and the 20,000-character default.
- UI: Terminal output only. JSON by default: one line on stdout as shown under "Result"; errors as JSON on stderr. With `--text`: header lines `From: `, `To: ` (comma-joined), `Cc: ` (only when non-empty), `Date: `, `Subject: `, `Ref: `, then a blank line, then the body (with the truncation marker when cut), then, if there are attachments, a blank line, `Attachments:` and one line per attachment `  <name> (<type>, <size> bytes)`.
