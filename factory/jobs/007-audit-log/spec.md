# Audit log for agent calls

## Goal
Every `list` call an Agent makes leaves one metadata-only entry in an append-only audit log under the mail owner's per-user data directory, so the human can see which Mandate did what, on which Account, and whether the Policy allowed or denied it. No Credential, Agent Token, token hash or message body is ever written, and no agent-mail command reads the log back. Later operation tickets (search, read, flag, move, draft) add their entries through the same mechanism.

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Ticket: `.scratch/agent-mail-v1/issues/06-audit-log.md` (feature spec section "Audit log" in `.scratch/agent-mail-v1/spec.md`).
- Files this feature touches:
  - `src/audit.rs` (new): the audit entry type, the append-only log writer and the platform path rule.
  - `src/core.rs`: `Context` gains the audit log destination; `execute` appends one entry for every `Command::List` call, whatever the result.
  - `src/lib.rs`: `pub mod audit;`. No new CLI subcommand.
  - `src/main.rs`: the real `Context` writes to the platform path (via `Context::new`).
  - `Cargo.toml`: a `libc` dependency (macOS target) if needed to read the home directory from the user database.
  - `tests/audit.rs` (new) and `tests/support/mod.rs`: test contexts write to a fresh temporary file per test, never to the user's real data directory; a helper reads the log's lines.
  - `README.md`: an "Audit log" section that names the location and the entry fields.
  - `CLAUDE.md`: the Stack layout line lists `src/audit.rs` and `tests/audit.rs` (checked by `tests/project.rs`).

### Entry format (used by the criteria below)
The log is a UTF-8 text file of JSON Lines: one JSON object per line, each line ending in `\n`. Keys, in this order, with absent keys omitted:
- `time`: RFC 3339 UTC to the second, e.g. `2026-10-21T09:30:15Z` (format of `core::rfc3339_utc`).
- `mandate`: the Mandate short ID (8 lowercase hex digits); omitted when no Mandate was opened.
- `operation`: the operation name as in `Operation::as_str`, here `"list"`.
- `account`: the Account's login; omitted when no single Account was chosen.
- `mailbox`: a mailbox name; omitted for `list` (used by later tickets).
- `messages`: message references; omitted for `list` (used by later tickets).
- `outcome`: `"allowed"` if the Policy granted the operation, otherwise `"denied"`.
- `reason`: the stable error `code` (as in `CoreError::code`) when the call failed, e.g. `"denied"`, `"invalid_token"`, `"missing_token"`, `"login_failed"`; omitted on success.

### Location
- macOS: `<home>/Library/Application Support/agent-mail/audit.log`, where `<home>` is the user's home directory from the user database (`getpwuid_r`), not from `$HOME`.
- Other platforms (until the Windows milestone): `$XDG_DATA_HOME/agent-mail/audit.log`, falling back to `<home>/.local/share/agent-mail/audit.log`.
- The directory is created on first write. On Unix, the directory is created with mode `0700` and the file with mode `0600`.

## Acceptance criteria
1. AC1: A successful `list` call appends one entry with time, Mandate short ID, operation, Account and outcome `allowed`. Test: a Mandate with ID `aaaa1111` over Account `agent@example.test`, granting `list` on `INBOX` and `Archive`, against the fake IMAP server; `agent-mail list` exits 0, and the log then holds exactly one line that parses as `{"time": T, "mandate": "aaaa1111", "operation": "list", "account": "agent@example.test", "outcome": "allowed"}`, with no other keys. `T` matches `^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$` and lies between the UTC seconds read just before and just after the call.
2. AC2: A denied call appends an entry with outcome `denied` and the reason. Test: the same Mandate granting only `read` on `INBOX` (no `list` Grant); `agent-mail list` exits 1 with error code `denied`, the fake server sees no connection, and the log holds exactly one line `{"time": T, "mandate": "aaaa1111", "operation": "list", "account": "agent@example.test", "outcome": "denied", "reason": "denied"}`.
3. AC3: A call with an unknown token appends an entry without any Mandate ID and without the token. Test, three cases each against a fresh log: (a) a well-formed token minted with `AgentToken::mint()` but never stored; (b) the malformed text `not-a-token`; (c) no `AGENT_MAIL_TOKEN` at all. Each `list` call exits 1, and the log holds exactly one line with keys `time`, `operation` (`"list"`), `outcome` (`"denied"`) and `reason` (`"invalid_token"` for a and b, `"missing_token"` for c), and no `mandate` or `account` key. The log bytes contain neither the presented token text nor its token hash.
4. AC4: No entry ever contains a Credential, Agent Token, token hash or message body (asserted across allowed, denied and error cases). Test: in one log, run `list` for (i) the allowed case of AC1, (ii) the denied case of AC2, (iii) an error case where the fake server rejects the login (Mandate whose Credential differs from the server's, reason `login_failed`, outcome `allowed`), (iv) an unreachable server (reason `connection_failed`), and (v) the unknown-token cases of AC3. The log then holds one line per call, and its bytes contain none of: the Credential `fake-app-password-123` or the wrong Credential used in (iii), any presented Agent Token, any token's hash (`support::token_hash`), any token's store key in hex, or any text matching the Agent Token prefix. (`list` reads no message bodies; later tickets extend this test with body fixtures.)
5. AC5: Entries are appended, never rewritten; earlier entries survive later calls. Test: pre-create the log with the line `{"time":"2026-01-01T00:00:00Z","operation":"list","outcome":"denied","reason":"missing_token"}\n`; run `list` three times (allowed, denied, allowed). The log then holds four lines, the first byte-identical to the pre-existing line, and after each call the file's previous content is a byte-for-byte prefix of its new content. Mailbox or login text containing `\n` or `\r` (e.g. an Account login `evil\n{"outcome":"allowed"}@example.test`) is JSON-escaped, so each call still adds exactly one line.
6. AC6: No agent-facing command reads or returns the audit log. Test: after a `list` call has written an entry, run `agent-mail --help`, `agent-mail mandate --help`, `list`, `mandate list` and `mandate delete ffffffff`, each in JSON and `--text` mode; no stdout or stderr contains the audit log's path, the substring `audit.log`, or the entry's `time` value. `agent-mail audit` and `agent-mail log` both fail as unknown subcommands (exit code 2) and print nothing from the log.
7. AC7: The log location follows the platform's per-user data directory convention and is documented. Test: a unit test of the path rule gives `/Users/alice/Library/Application Support/agent-mail/audit.log` for home `/Users/alice` on macOS, and on other Unix gives `/x/data/agent-mail/audit.log` for `XDG_DATA_HOME=/x/data` and `/home/alice/.local/share/agent-mail/audit.log` when `XDG_DATA_HOME` is unset or empty (the rule takes home and `XDG_DATA_HOME` as inputs, so tests set no process environment). `README.md` has an `## Audit log` section that contains the string `~/Library/Application Support/agent-mail/audit.log` and names every entry field listed under Entry format.

## Out of scope
- Entries for `mandate create`, `mandate list` and `mandate delete`. This slice logs the one Agent mail operation that exists (`list`). Logging the Mandate management commands is a follow-up ticket; see Notes for reviewers.
- Entries for search, read, flag, move and draft: each operation ticket (07 to 11) adds its own entries, including `mailbox` and `messages`, through this mechanism.
- Log rotation, size limits, retention and a human-facing viewer command.
- Windows location (`%LOCALAPPDATA%`), which comes with the Windows milestone (ticket 18).
- Tamper-proofing the log against a process running as the same user (signing, hash chains).
- A setting to turn the log off.

## Notes for reviewers
- Security:
  - The log is written in-process by every call (ADR-0002), so any process of the same user, including an Agent, can read or delete the file directly. The ticket only promises that no agent-mail *command* exposes it (AC6). Mode `0600`/`0700` keeps out other OS users only. This is a known limit, not a defect.
  - Untrusted input reaches the log: Account logins and, in later tickets, server-supplied mailbox names and Agent-supplied arguments. JSON encoding escapes control characters, so a crafted value cannot forge a second entry (AC5).
  - The path must not be redirectable by the Agent. On macOS the home directory comes from the user database, not `$HOME`. On other Unix the XDG convention is environment-based and an Agent could redirect it; that platform is not a v1 target.
  - No entry may be built from `Mandate`, `AgentToken` or `Account` wholesale: the entry type has only the fields above, and none of them can hold a `Credential` or token.
  - Write failure: if the entry cannot be appended (directory not creatable, disk full), the `list` call should fail closed with a new error code `audit_unavailable` (message names a next step, as the existing `every_error_message_names_a_next_step` test requires) and print no mailboxes. The builder adds this behaviour and its test. It is not in the ticket's ACs, so reviewers should confirm it.
  - Test contexts must never write to the developer's real data directory. `Context::trusting_only` (test-support only) points at a per-test temporary file, and any constructor that changes the audit path stays behind `cfg(any(test, feature = "test-support"))`.
  - Open question: the ticket says "every call", but agents may also run `mandate list` and `mandate delete`. Deleting a Mandate revokes an Agent Token, and the human may want that on record. This spec leaves it to a follow-up ticket to keep the slice small.
- UX: The writer is the Agent's call, which is silent: no change to `list` output or exit codes except the new fail-closed error. The reader is the human (mail owner), who opens the file named in the README with a pager or `jq`. The README section gives the path and explains each field and the `allowed`/`denied` meaning (the Policy's decision; `reason` gives the error code even for allowed calls that later failed).
- UI: No new terminal output on success. The only UI surfaces are the README section and the `audit_unavailable` error body (JSON and `--text`, via the existing renderers).
