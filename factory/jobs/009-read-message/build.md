## Round 1
**Changed:**
- New `agent-mail read <REF> [--full]` (plus global or trailing `--text`): `src/read.rs` holds `ReadResult`/`Attachment`, MIME-to-result conversion with `mail-parser` 0.11 (body = first non-attachment `text/plain` leaf, else first `text/html` leaf through mail-parser's `html_to_text`, which drops `<head>`, `<style>`, `<script>` and decodes entities without fetching anything; attachments = every other leaf except direct siblings in the body's `multipart/alternative`), CRLF/CR-to-LF normalisation, trailing-break trim and char-based truncation at 20,000 with the spec's marker.
- `src/imap.rs`: `imap::read` logs in, `EXAMINE`s, compares UIDVALIDITY before any fetch (`StaleReference`), then `UID FETCH <uid> (UID BODY.PEEK[])`; no FETCH for the UID gives `MessageNotFound`. `examine` is shared with `search`.
- `src/core.rs`: `Command::Read { reference, full }`, `Outcome::Message`; core re-validates the reference (`read::is_valid_reference`), picks the Mandate Account whose login equals the reference's account, checks `read` on the decoded mailbox before connecting, and appends an audit entry with `mailbox` and `messages: [<ref>]` (never `results`). New errors `stale_reference` and `message_not_found` (with `operation`, `account`, `mailbox`, next-step message), added to `every_error_message_names_a_next_step`.
- `src/lib.rs`: clap value parser `read::parse_reference` (MessageRef::parse plus CR/LF/NUL check, exit 2), `--help` text naming REF, `--full` and 20,000; `--text` rendering per the spec's UI notes with `escape_controls` on every header value and attachment name and per-line escaping of the body (unit test with `\x1b[2J` in subject, body and attachment name).
- Fake server: `FakeMessage.source` (full raw MIME returned by `raw()`), new `Read` mailbox (UIDVALIDITY 1700000006, UIDs 1-5 as in the spec). README "Read" section and Audit log updates; CLAUDE.md layout names `src/read.rs` and `tests/read.rs`.
- Choice to check: I added a third error, `invalid_reference` (operation `read`), returned only when a non-CLI adapter hands the core a reference with CR/LF/NUL, an empty segment or UID 0. The CLI never reaches it (clap rejects those with exit 2). The spec asks the core to validate again but names no code for it.
- Note: error bodies serialise through `serde_json::json!`, so their keys come out in alphabetical order (this was already true before this job). AC6 checks the parsed object, not key order, so the test asserts the key set.

**Commits:** cc38ff6
**Tests:** `cargo test` → every suite `test result: ok` (lib 43 passed, tests/read.rs 10 passed, 0 failed overall); `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | tests/read.rs `ac1_plain_text_message_returns_headers_and_body` | pass |
| AC2 | tests/read.rs `ac2_html_only_message_returns_readable_text` | pass |
| AC3 | tests/read.rs `ac3_attachments_are_listed_without_content` | pass |
| AC4 | tests/read.rs `ac4_reading_leaves_messages_unread` | pass |
| AC5 | tests/read.rs `ac5_long_bodies_are_truncated_unless_full` | pass |
| AC6 | tests/read.rs `ac6_stale_reference_fails_before_any_fetch` (+ src/core.rs `every_error_message_names_a_next_step`) | pass |
| AC7 | tests/read.rs `ac7_read_without_a_read_grant_is_denied_and_audited` | pass |
| AC8 | tests/read.rs `ac8_each_read_is_audited_with_its_reference_only` | pass |

Extra tests: src/read.rs (normalisation, char-based truncation, reference refusal, malformed MIME never panics, nested alternative + RFC 2047 name, RFC 2231 filename), src/lib.rs (`read_text_output_escapes_controls_but_keeps_body_line_breaks`, `read_refuses_malformed_references`), tests/read.rs (`--text` layout, `--help` content).

**Review findings:** none (round 1).
