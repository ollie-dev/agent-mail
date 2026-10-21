# Search messages in a granted mailbox

## Goal
An Agent runs `agent-mail search` to find messages in one mailbox its Policy grants `search` on, using the standard criteria from, to, subject, text, since, before, unseen and flagged, combined with AND. It gets back at most 20 results by default (adjustable with `--limit`), newest first. Each result has date, from, subject, flags and a message reference (Account, mailbox, UIDVALIDITY, UID) that later operations (read, flag, move) will accept. Every search call appends an audit entry with the mailbox and the number of results.

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Ticket: `.scratch/agent-mail-v1/issues/07-search.md`. Feature spec: `.scratch/agent-mail-v1/spec.md`, sections "Policy and Grants", "Mail operations" and "Audit log". Terms as in `GLOSSARY.md` (Account, Agent, Mandate, Agent Token, Policy, Grant).
- Files this feature touches:
  - `src/lib.rs`: new `search` subcommand and its `--text` rendering.
  - `src/core.rs`: `Command::Search`, `Outcome::Messages`, the Policy check, the audit entry, `CoreError::Denied` gains an optional mailbox, and new error `mailbox_not_found`.
  - `src/search.rs` (new, or a name of the builder's choice): search criteria, the result types, and the message reference with its string form (format and parse).
  - `src/imap.rs`: a search call: log in, `EXAMINE` the mailbox, `UID SEARCH`, `UID FETCH` of the selected UIDs only, logout. It shares the connect, TLS and login code that `list_mailboxes` uses now.
  - `src/utf7.rs`: an `encode` function (UTF-8 to modified UTF-7) for the mailbox name sent to the server, round-trip tested against `decode`.
  - `src/audit.rs`: `Entry` gains `results` (number of results returned), serialised after `messages` and before `outcome`.
  - `tests/search.rs` (new) and `tests/support/mod.rs`: the fake IMAP server gains the message fixtures below and handles `EXAMINE`, `UID SEARCH` (evaluating the criteria itself) and `UID FETCH`, and it records each fetch's UID count. It also records `SELECT` and `STORE` if it receives them, and sets `\Seen` on a non-peek `BODY[...]`/`RFC822`/`RFC822.TEXT` fetch, as RFC 3501 says a server must.
  - `README.md`: the Audit log section names `search` and documents `mailbox`, `messages` and `results` for it. A short "Search" section gives the options, the output fields and the message reference format.
  - `CLAUDE.md`: the Stack layout line names `tests/search.rs` and any new `src/` module (checked by `tests/project.rs`).

### Command line
`agent-mail search [--mailbox NAME] [--from S] [--to S] [--subject S] [--text S] [--since YYYY-MM-DD] [--before YYYY-MM-DD] [--unseen] [--flagged] [--limit N]`, plus the global `--text`.
- `--mailbox` is the decoded UTF-8 mailbox name, as `list` prints it. Default `INBOX`.
- No criterion means every message in the mailbox (`ALL`).
- Criteria map to IMAP `FROM`, `TO`, `SUBJECT`, `TEXT`, `SINCE`, `BEFORE`, `UNSEEN` and `FLAGGED`, sent together in one `UID SEARCH`, so they combine with AND. Under RFC 3501, string criteria are case-insensitive substring matches, `SINCE` includes its date, `BEFORE` excludes it, and both compare the message's internal date (ignoring time of day and timezone).
- `--limit N`: an integer from 1 to 1000. Default 20.
- Usage errors are caught by the argument parser (clap): exit code 2, a usage message on stderr, no connection and no audit entry, the same as other usage errors today. They are: a malformed date (`2026-13-01`, `10/05/2026`), `--limit 0`, `--limit 1001`, `--limit x`, and any criterion or mailbox value containing CR, LF or NUL.
- String values are sent as IMAP quoted strings with `"` and `\` escaped. Non-ASCII values are sent as quoted UTF-8 with `CHARSET UTF-8`.

### Result (JSON, one line on stdout)
```json
{"account":"agent@example.test","mailbox":"INBOX","total":2,"more":false,"messages":[
  {"ref":"agent@example.test/INBOX/1700000001/3","date":"2026-10-10T09:00:00Z","from":"alice@example.com","subject":"Re: Quarterly report","flags":["\\Flagged"]}]}
```
- `messages`: newest first, meaning by UID descending, with at most `limit` entries. They are the `limit` highest matching UIDs, and only those UIDs are fetched.
- `total`: how many messages matched. `more`: `true` when `total` > number returned.
- `date`: the message's internal date in RFC 3339 UTC (`core::rfc3339_utc` format).
- `from`: the first From address, as `Name <addr>` when it has a display name, else `addr`. `""` when absent.
- `subject`: RFC 2047-decoded. `""` when absent.
- `flags`: the server's flags as strings, e.g. `"\\Seen"`, in server order.
- `ref`, the message reference: `<account>/<mailbox>/<uidvalidity>/<uid>`. In the account and mailbox, `%`, `/`, and every byte outside printable ASCII `0x21`–`0x7E` are percent-encoded as `%XX` (upper-case hex) over the UTF-8 bytes. `uidvalidity` and `uid` are decimal. Example: `agent@example.test/%5BGmail%5D%2FEntw%C3%BCrfe/1700000003/7`. The module gives a parse function that returns `(account, mailbox, uidvalidity, uid)`, so tickets 08 to 10 can accept references.

### Fake server fixtures (used by the criteria below)
Account `agent@example.test` (`support::LOGIN`). `INBOX` has UIDVALIDITY `1700000001`:

| UID | From | To | Subject | Body | Internal date | Flags |
|---|---|---|---|---|---|---|
| 1 | `Alice Example <alice@example.com>` | agent@example.test | Quarterly report | numbers attached | 01-Oct-2026 09:00:00 +0000 | `\Seen` |
| 2 | `bob@example.com` | agent@example.test | Lunch? | pizza today | 05-Oct-2026 09:00:00 +0000 | (none) |
| 3 | `alice@example.com` | team@example.com | Re: Quarterly report | pizza on friday | 10-Oct-2026 09:00:00 +0000 | `\Flagged` |
| 4 | `Carol <carol@example.com>` | agent@example.test | `=?UTF-8?Q?Rechnung_f=C3=BCr_Oktober?=` | please pay | 15-Oct-2026 11:00:00 +0200 | `\Seen \Flagged` |

`Archive` has UIDVALIDITY `1700000002` and 25 messages, UIDs 1 to 25, all from `news@example.com` with subject `Newsletter <uid>`. `Spam` exists and has no messages. A mailbox named `Missing` does not exist (`EXAMINE` answers `NO`).

## Acceptance criteria
1. AC1: Searching INBOX by `--from alice@example.com` returns only matching messages from the fake server, newest first, with date, from, subject, flags and message reference. Test: a Mandate (ID `aaaa1111`) granting `search` on `INBOX`. `agent-mail search --mailbox INBOX --from alice@example.com` exits 0, and stdout parses as `{"account":"agent@example.test","mailbox":"INBOX","total":2,"more":false,"messages":[M3,M1]}`, where M3 = `{"ref":"agent@example.test/INBOX/1700000001/3","date":"2026-10-10T09:00:00Z","from":"alice@example.com","subject":"Re: Quarterly report","flags":["\\Flagged"]}` and M1 = `{"ref":"agent@example.test/INBOX/1700000001/1","date":"2026-10-01T09:00:00Z","from":"Alice Example <alice@example.com>","subject":"Quarterly report","flags":["\\Seen"]}`. Each message object has exactly these five keys. Also, with no criteria, the first result is UID 4 with `"date":"2026-10-15T09:00:00Z"` and `"subject":"Rechnung für Oktober"`.
2. AC2: Each criterion (from, to, subject, text, since, before, unseen, flagged) narrows results as expected, and criteria combine with AND. Test, against INBOX, listing the UIDs of `messages` in order: none → `[4,3,2,1]`; `--from alice@example.com` → `[3,1]`; `--to team@example.com` → `[3]`; `--subject quarterly` → `[3,1]`; `--text pizza` → `[3,2]`; `--since 2026-10-05` → `[4,3,2]`; `--before 2026-10-10` → `[2,1]`; `--unseen` → `[3,2]`; `--flagged` → `[4,3]`; `--from alice@example.com --unseen` → `[3]`; `--text pizza --flagged` → `[3]`; `--since 2026-10-05 --before 2026-10-15` → `[3,2]`; `--from bob@example.com --flagged` → `[]` with `"total":0,"more":false`. In every case `total` equals the number of UIDs listed.
3. AC3: Without `--limit`, at most 20 results are returned and the result says more exist; `--limit 5` returns at most 5. Test, against `Archive` (25 messages, Grant on `Archive`): with no `--limit`, `messages` has 20 entries with UIDs 25 down to 6, `"total":25` and `"more":true`, and the fake server's fetches cover 20 UIDs in total. With `--limit 5`, UIDs `[25,24,23,22,21]`, `"total":25`, `"more":true`, and fetches cover 5 UIDs. With `--limit 30`, 25 entries and `"more":false`. Against INBOX with `--limit 5`, 4 entries and `"more":false`. `--limit 0`, `--limit 1001` and `--limit x` each exit 2 and the fake server sees no connection.
4. AC4: Searching a mailbox without a `search` Grant is denied with a structured error and audited. Test, each case against a fresh audit log: (a) Grants `search` on `INBOX` and `list` on `*`, then `search --mailbox Archive`; (b) Grants only `list` and `read` on `INBOX`, then `search --mailbox INBOX`; (c) an empty Policy, then `search` (default mailbox `INBOX`). Each exits 1, stdout is empty, and stderr parses as `{"error":{"code":"denied","operation":"search","account":"agent@example.test","mailbox":<the mailbox>,"message":...}}`, with a message that names a next step (the existing `every_error_message_names_a_next_step` test covers the new variant). The fake server sees no connection. The log holds one line `{"time":T,"mandate":"aaaa1111","operation":"search","account":"agent@example.test","mailbox":<the mailbox>,"outcome":"denied","reason":"denied"}` with no `messages` or `results` key. `--text` prints `error: denied: ...` naming `search` and the mailbox. A `list` denial still has no `mailbox` key.
5. AC5: Each search appends an audit entry with the mailbox and the number of results. Test: AC1's search appends exactly one line `{"time":T,"mandate":"aaaa1111","operation":"search","account":"agent@example.test","mailbox":"INBOX","messages":["agent@example.test/INBOX/1700000001/3","agent@example.test/INBOX/1700000001/1"],"results":2,"outcome":"allowed"}`, with keys in this order. A search with no matches (`--from bob@example.com --flagged`) logs `"results":0` and `"messages":[]`. AC3's default-limit search logs `"results":20` and 20 references. A granted search of `Missing` logs `"mailbox":"Missing","outcome":"allowed","reason":"mailbox_not_found"` with no `results`. The log bytes never contain a subject, a body word (`pizza`, `numbers attached`), the Credential, the Agent Token or its token hash.
6. AC6: Message references include UIDVALIDITY and UID. Test: every `ref` from AC1 to AC3 parses with the module's parse function into the Account `agent@example.test`, the searched mailbox, that mailbox's fixture UIDVALIDITY (`1700000001` or `1700000002`) and the message's UID. A unit test checks that formatting then parsing returns the original values for the mailbox `[Gmail]/Entwürfe`, the account `a/b%c@example.test`, UIDVALIDITY `4294967295` and UID `1`. The formatted reference is `a%2Fb%25c@example.test/%5BGmail%5D%2FEntw%C3%BCrfe/4294967295/1` and contains exactly three unencoded `/`. Parsing `INBOX/1/2`, `a@b/INBOX/x/2`, `a@b/INBOX/1/0` and `a@b/INBOX/1/2/3` fails.

## Out of scope
- Gmail raw search (`--gmail`, `X-GM-RAW`) and the live Gmail test: ticket 12.
- `--account` and several tokens: ticket 13. A Mandate covering several Accounts gets the existing `account_required` error, as `list` does.
- Reading, flagging and moving by reference (tickets 08 to 10). This job only defines the reference format and its parser. The UIDVALIDITY mismatch error (`stale_reference`) belongs to ticket 08.
- Searching several mailboxes in one call, sorting other than newest first (UID descending), paging or offsets, and `SORT`/`ESEARCH` server extensions.
- Message sizes, To/Cc in results, snippets or any body text in results.
- The MCP `search` tool: ticket 15.
- Literal (`{n}`) encoding of search strings. Non-ASCII values go as quoted UTF-8 only.

## Notes for reviewers
- Security:
  - Search reads mail through the Credential, so the Policy check (`search` on this Account and this decoded mailbox) must happen before any connection (AC4 asserts no connection). `MailboxPattern::matches` compares decoded UTF-8 names exactly. The mailbox sent to the server is the Agent's name encoded with modified UTF-7, never a server-supplied name.
  - The Agent supplies untrusted input: mailbox name and criteria. CR, LF and NUL are rejected by the parser, and `"`/`\` are escaped, so no value can end the `UID SEARCH` command or inject another (builder adds a test: `--subject 'x" ALL "y'` sends one `UID SEARCH` and matches nothing in INBOX). The core should validate again, not only clap, because the MCP adapter (ticket 15) will call the core directly. Reviewers should confirm this.
  - The server supplies untrusted output: subjects, addresses, flags and mailbox names. JSON escapes control characters. `--text` output goes through `escape_controls`. Mailbox and account text in references is percent-encoded, so a crafted name cannot forge extra reference segments or audit lines.
  - Searching must not change mailbox state. Use `EXAMINE` (read-only), never `SELECT`, and fetch headers with `BODY.PEEK` or `ENVELOPE`. Builder adds a test that every fixture's flags are unchanged after a search and that the fake saw no `SELECT` or `STORE`.
  - The audit log gains message references and a count, never subjects, addresses or bodies (AC5). The new `results` field is an addition to the 06 entry format. The README field list must match.
  - The "number of results" in AC5 is read as the number returned (equal to `messages` length), not `total`. Reviewers should confirm that `total` stays out of the log.
  - The limit cap of 1000 is this spec's choice, to bound fetch size and output. The ticket only says "adjustable".
  - Non-ASCII criteria are sent as quoted UTF-8 with `CHARSET UTF-8`. Gmail and Dovecot accept this, but strict RFC 3501 servers may reply `BAD`. That surfaces as `server_error`, which is acceptable for this slice.
- UX: The user is an Agent calling the CLI, usually via the skill. Typical flow: `agent-mail list`, then `agent-mail search --mailbox INBOX --from x --unseen`. It reads `more`/`total` to decide whether to narrow the search or raise `--limit`, then passes a `ref` to `read` (ticket 08). The human sees the outcome only in the audit log. Denials name operation, Account and mailbox, so the Agent can tell the human exactly which Grant to add. `--help` for `search` lists every option, the date format, the default mailbox and the default limit.
- UI: Terminal output only. JSON by default: one line on stdout as shown under "Result"; errors as JSON on stderr. With `--text`: a header line `<account> <mailbox>: showing <n> of <total> messages`, then one line per message, `<date>  <from>  <subject>  [<flags joined by space>]  <ref>`, with every server-supplied value passed through `escape_controls`. When `more` is true, a final line `more messages match; narrow the search or raise --limit`. Builder adds a unit test that a subject containing `\n` and `\x1b` stays on one escaped line.
