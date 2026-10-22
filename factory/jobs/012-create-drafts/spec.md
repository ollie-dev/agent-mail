# Create drafts, including replies

## Goal
An Agent runs `agent-mail draft --to <ADDR> --subject <TEXT> --body <TEXT>` to prepare a message for the human to review and send. The draft is appended, with the `\Draft` flag, to the Account's Drafts mailbox. agent-mail finds that mailbox by its `\Drafts` special-use attribute, not by its name, so localised names such as `[Gmail]/Entwürfe` work. With `--reply-to <REF>`, the draft gets In-Reply-To and References from the original message and a `Re:` subject, so it threads correctly. Each call appends an audit entry that records the recipients and the new message's reference, and never the subject or body. Agents cannot edit or delete drafts.

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Ticket: `.scratch/agent-mail-v1/issues/11-draft.md`. Feature spec: `.scratch/agent-mail-v1/spec.md`, sections "Policy and Grants" ("`draft` takes no mailbox list and always targets the `\Drafts` special-use mailbox") and "Mail operations" ("Draft: build a message and APPEND it to the `\Drafts` mailbox with the `\Draft` flag. Reply mode sets In-Reply-To and References from the original message and prefixes the subject"). Terms are as in `GLOSSARY.md` (Account, Agent, Mandate, Agent Token, Policy, Grant, Credential). `Operation::Draft` (`"draft"`) already exists in `src/mandate.rs`, and `mandate create` and the export already store `draft` Grants with an empty mailbox list.
- Files this feature touches:
  - `src/draft.rs` (new):
    - the `DraftResult` type and its `--text` line;
    - recipient and subject validation;
    - building the RFC 5322 message;
    - the reply helpers: the reply subject (`Re:` added once) and the References value.
  - `src/lib.rs`: the new `draft` subcommand, its `--help`, and its `--text` rendering.
  - `src/core.rs`:
    - `Command::Draft { to: Vec<String>, subject: Option<String>, body: String, reply_to: Option<MessageRef> }` and `Outcome::Drafted(DraftResult)`.
    - The Policy checks (`draft` on the Account; in reply mode also `read` on the original's mailbox), done before any connection.
    - The audit entry.
    - New errors `drafts_not_found` and `invalid_draft`. Each gets a message that names a next step, so `every_error_message_names_a_next_step` covers them.
    - `StaleReference`, `MessageNotFound` and `InvalidReference` are reused with `operation: Operation::Draft`. Their messages for `draft` say that no draft was created.
  - `src/audit.rs`: `Entry` gains an optional `recipients` key (a list of addresses), serialised after `messages` and before `change`.
  - `src/imap.rs`: a draft call that runs in one session:
    1. Log in.
    2. In reply mode: `EXAMINE` the original's mailbox, compare its UIDVALIDITY with the reference's, and fetch the original's headers with a `.PEEK` fetch only, so the original is not marked `\Seen`.
    3. `LIST "" "*"`. The Drafts mailbox is the first entry, in server order, whose attributes include `\Drafts` (case-insensitive). With no such entry, return a new `ImapError::DraftsNotFound` before any `APPEND`.
    4. `APPEND <raw drafts name> (\Draft) {n}` with the message. The raw modified UTF-7 name the server listed is sent back unchanged, as a quoted string.
    5. Read the new UID and the mailbox's UIDVALIDITY from an `[APPENDUID <uidvalidity> <uid>]` response code (RFC 4315). async-imap 0.12's `append` returns `()` and drops response codes, so the connector may have to send the command and the literal itself, as `copy_or_move` does for `COPYUID`.
    6. `LOGOUT`.

    It never sends `SELECT`, `STORE`, `COPY`, `MOVE`, `EXPUNGE` or `CLOSE`.
  - `tests/draft.rs` (new) and `tests/support/mod.rs`. Fake server changes:
    - New `FakeImap::start_with_list(mode, capabilities: &[&str], extra_listed: &[&str], drafts: Option<&str>)`:
      - `extra_listed` holds raw names that `LIST` returns after `SERVER_MAILBOXES`.
      - The `LIST` line for the raw name `drafts` carries `(\HasNoChildren \Drafts)`, in a plain `LIST`, as Gmail sends it.
      - `start` and `start_with_capabilities` mark no mailbox `\Drafts` and list exactly as today.
    - New empty fixture mailbox `Drafts` (UIDVALIDITY `1700000010`). It is not in `SERVER_MAILBOXES`.
    - New fixture mailbox `Thread` (UIDVALIDITY `1700000009`), not in `SERVER_MAILBOXES`. It holds two messages, given below.
    - `APPEND`:
      - It handles a synchronising literal: it answers `+ ready`, reads `n` bytes, then reads the rest of the line.
      - It stores the message in the named mailbox with UID = last UID + 1 and the given flags. The stored message's `subject` is the raw Subject header value, so `messages_in` works.
      - It answers `OK [APPENDUID <uidvalidity> <uid>]` when `UIDPLUS` is advertised, and plain `OK` otherwise.
      - A missing mailbox gets `NO [TRYCREATE]`.
    - New accessor `appended() -> Vec<(String, Vec<String>, String)>`. Each entry is the raw mailbox name, the flags and the raw message, in order.
    - Existing tests keep passing.
  - `README.md`: a "Draft" section covering:
    - the command and the Grants it needs;
    - how the Drafts mailbox is found;
    - reply mode;
    - the result;
    - the errors;
    - that drafts cannot be edited or deleted.

    The Audit log section names `draft` and documents `mailbox`, `messages` and `recipients` for it.
  - `CLAUDE.md`: the Stack layout line names `src/draft.rs` and `tests/draft.rs`. This is checked by `tests/project.rs`.

### Command line
`agent-mail draft --to <ADDR> [--to <ADDR>...] (--subject <TEXT> | --reply-to <REF>) --body <TEXT>`, plus `--text` (global or after the subcommand).
- `--to` is required and repeatable, and recipients keep their order. Each value is a bare address `local@domain`:
  - It has exactly one `@`, with a non-empty part on each side.
  - It is printable ASCII only, with no space and none of `<>()[]\,;:"`.
  - It is at most 254 characters long.

  Anything else is a usage error: exit 2, no connection, no audit entry.
- `--subject` is required without `--reply-to` and conflicts with `--reply-to` (usage error). It may hold any UTF-8 except CR, LF and NUL, and must not be empty.
- `--body` is required. It may hold any UTF-8 except NUL, including newlines.
- `--reply-to <REF>` takes a message reference as `search` prints it, parsed with `read::parse_reference`. A reference that does not parse is a usage error (exit 2, no audit entry).
- Without `--reply-to`, the Account is the Mandate's only Account; several Accounts fail with `account_required`, as for `list`. With `--reply-to`, the Account is the Mandate's Account whose login equals the reference's account, as for `read`.
- The core validates `to`, `subject` and `body` again, because the MCP adapter will call the core directly. A violation fails with `invalid_draft`, with `"operation":"draft"`, before any connection. Its message names the field (`to`, `subject` or `body`) and never echoes the value.

### The message appended
- Headers:
  - `From:` is the Account login, only when the login contains exactly one `@`.
  - `To:` lists the recipients, comma-separated, in order.
  - `Subject:` is RFC 2047-encoded when it is not ASCII.
  - `Date:` is the current time.
  - `Message-ID:` is `<32 lowercase hex characters from a CSPRNG @ the login's domain>`, or `@agent-mail.invalid` when the login has no `@`.
  - `MIME-Version: 1.0` and `Content-Type: text/plain; charset=utf-8`.
  - In reply mode, `In-Reply-To:` and `References:` as defined below.
- The header section is ASCII only. No line of the message exceeds 998 octets, so a long or non-ASCII body uses a transfer encoding such as quoted-printable.
- Flags: exactly `(\Draft)`. It never carries `\Seen`.
- Reply mode, from the original's headers:
  - `In-Reply-To:` is the original's Message-ID, in angle brackets.
  - `References:` is the original's References ids, in order, each in angle brackets, followed by the original's Message-ID, all separated by single spaces.
  - The subject is the original's decoded subject with `Re: ` put in front. If the subject, after trimming leading whitespace, already starts with `re:` (case-insensitive), it is left unchanged.
  - Message ids from the original are untrusted server data. An id containing anything other than printable ASCII without space, `<` or `>` is dropped from References.
  - If the original has no Message-ID, the draft has neither In-Reply-To nor References, but still gets the reply subject.

### Result (JSON, one line on stdout)
```json
{"ref":"agent@example.test/Drafts/1700000010/1","account":"agent@example.test","mailbox":"Drafts","to":["carol@example.com"],"subject":"Status","original":null}
```
The keys are exactly these, in this order:
- `ref`: the draft's reference built from `APPENDUID`, or `null` when the server reported none.
- `mailbox`: the decoded Drafts mailbox name, for example `[Gmail]/Entwürfe`.
- `to`: the recipients, as given.
- `subject`: the subject as sent, which is the reply subject in reply mode.
- `original`: the `--reply-to` reference, or `null`.

With `--text`, the output is one line, with every value passed through `escape_controls`:
- `draft created: <ref>`, or
- `draft created in <mailbox> (new reference not reported)`.

### Audit entry
- Shape: `{"time":T,"mandate":"aaaa1111","operation":"draft","account":...,"mailbox":<decoded Drafts name, only once found>,"messages":[<original ref, in reply mode>, <new ref, if reported>],"recipients":[...],"outcome":...,"reason":...}`. Absent keys are omitted, and `messages` is omitted when it would be empty.
- `recipients` is present on every `draft` entry, denials included.
- The entry never holds the subject, the body, the original's headers, the Credential, the Agent Token or the token hash.

### Fake server fixture (used by the criteria below)
`Thread` (UIDVALIDITY `1700000009`). Both messages have no flags and full MIME sources:

| UID | From | Subject | Message-ID | References |
|---|---|---|---|---|
| 1 | `alice@example.com` | `Re: Budget` | `<thread-2@example.com>` | `<thread-0@example.com>` and `<thread-1@example.com>`, folded over two lines |
| 2 | `bob@example.com` | `RE: Agenda` | none | none |

The existing `Read` UID 1 has Subject `Project update`, Message-ID `<update-1@example.com>` and no References.

Unless a criterion says otherwise:
- Tests use the Mandate `aaaa1111` with the single Account `agent@example.test` (`support::LOGIN`). Its Grants are `draft` (no mailboxes), `read` on `Thread` and `Read`, and `list` on `*`.
- The fake is started with `start_with_list(Mode::Implicit, &["UIDPLUS"], &["Drafts"], Some("Drafts"))`.
- `T(n)` = `agent@example.test/Thread/1700000009/<n>`, and `R(n)` = `agent@example.test/Read/1700000006/<n>`.
- "The appended message" means the raw message of the single `appended()` entry, parsed with `mail_parser`.

## Acceptance criteria
1. AC1: Creating a draft with to, subject and body appends one message with the `\Draft` flag to the mailbox marked `\Drafts` on the fake server.
   - Test: `draft --to carol@example.com --to dave@example.com --subject Status --body "Line one\nLine two"` exits 0. stdout is exactly `{"ref":"agent@example.test/Drafts/1700000010/1","account":"agent@example.test","mailbox":"Drafts","to":["carol@example.com","dave@example.com"],"subject":"Status","original":null}`.
   - `appended()` is exactly one entry with mailbox `Drafts` and flags `["\\Draft"]`. `messages_in("Drafts")` is `[(1, "Status", ["\\Draft"])]`.
   - The appended message has:
     - From `agent@example.test`;
     - To `carol@example.com` and `dave@example.com`, in order;
     - Subject `Status`;
     - a Date that parses;
     - a Message-ID matching `^[0-9a-f]{32}@example\.test$` once the brackets are stripped;
     - no In-Reply-To or References;
     - a text body equal to `Line one\nLine two` (line endings compared after normalising CRLF to LF).
   - `commands()` contain `LIST` and `APPEND`. They contain no `SELECT`, `STORE`, `UID STORE`, `COPY`, `UID COPY`, `MOVE`, `UID MOVE`, `EXPUNGE`, `UID EXPUNGE` or `CLOSE`.
   - With `--text`, the output is the line `draft created: agent@example.test/Drafts/1700000010/1`.
   - A second identical call yields `ref` `.../Drafts/1700000010/2` and a different Message-ID.
2. AC2: With the Drafts mailbox named `[Gmail]/Entwürfe` and marked `\Drafts`, the draft lands there.
   - Test: fake started with `start_with_list(Mode::Implicit, &["UIDPLUS"], &["Drafts"], Some("[Gmail]/Entw&APw-rfe"))`, so an unmarked mailbox named `Drafts` also exists. `draft --to carol@example.com --subject Status --body Hi` exits 0, with `"mailbox":"[Gmail]/Entwürfe"` and `"ref":"agent@example.test/[Gmail]/Entwürfe/1700000003/1"`.
   - `appended()` is one entry with raw mailbox `[Gmail]/Entw&APw-rfe` and flags `["\\Draft"]`. `messages_in("Drafts")` is empty.
   - The new `ref` works as a reference: `read` with it, with `read` granted on `[Gmail]/Entwürfe`, returns subject `Status`.
3. AC3: When no mailbox is marked `\Drafts`, the command fails with a structured error and creates nothing.
   - Test: fake started with `start_with_list(Mode::Implicit, &["UIDPLUS"], &["Drafts"], None)`, so `Drafts` is listed but not marked. `draft --to carol@example.com --subject Status --body Hi` exits 1 with empty stdout. stderr parses as `{"error":{"code":"drafts_not_found","operation":"draft","account":"agent@example.test","message":...}}`. The message says that no mailbox is marked as the Drafts mailbox, that no draft was created, and that the human should create or mark a Drafts folder.
   - `commands()` contain `LIST` but no `APPEND`. `appended()` is empty, and every mailbox's `messages_in` is unchanged.
   - The audit line is exactly `{"time":T,"mandate":"aaaa1111","operation":"draft","account":"agent@example.test","recipients":["carol@example.com"],"outcome":"allowed","reason":"drafts_not_found"}`.
4. AC4: A reply draft carries In-Reply-To equal to the original's Message-ID, References extended with it, and the subject prefixed once with `Re:`.
   - Test: `draft --to alice@example.com --reply-to T(1) --body Thanks` exits 0, with `"subject":"Re: Budget"` and `"original":"agent@example.test/Thread/1700000009/1"`. The appended message has:
     - In-Reply-To `<thread-2@example.com>`;
     - the raw References header, unfolded, equal to `<thread-0@example.com> <thread-1@example.com> <thread-2@example.com>`;
     - Subject `Re: Budget`, not `Re: Re: Budget`.
   - `draft --to alice@example.com --reply-to R(1) --body Thanks` gives Subject `Re: Project update`, In-Reply-To `<update-1@example.com>` and References `<update-1@example.com>`.
   - `draft --to bob@example.com --reply-to T(2) --body Ok` exits 0 with Subject `RE: Agenda` and neither In-Reply-To nor References.
   - After these calls, the originals' flags (`messages_in("Thread")`, `messages_in("Read")`) are unchanged, with no `\Seen`. Every `UID FETCH` in `fetches()` uses `BODY.PEEK`, and `commands()` contain `EXAMINE` and no `SELECT`.
   - `draft --to a@example.com --reply-to T(1) --subject X --body Y` is a usage error (exit 2) and makes no connection.
5. AC5: Creating a reply draft requires `read` on the original's mailbox in addition to `draft`; without it the call is denied.
   - Test: Grants `draft` and `read` on `Read` only, then `draft --to alice@example.com --reply-to T(1) --body Thanks`. It exits 1 with empty stdout. stderr parses as `{"error":{"code":"denied","operation":"read","account":"agent@example.test","mailbox":"Thread","message":...}}`.
   - The fake sees no connection, and `appended()` is empty.
   - The audit line is exactly `{"time":T,"mandate":"aaaa1111","operation":"draft","account":"agent@example.test","messages":["agent@example.test/Thread/1700000009/1"],"recipients":["alice@example.com"],"outcome":"denied","reason":"denied"}`.
   - A reply whose reference names `other@example.test`, an Account the Mandate does not cover, is denied the same way, with error `account` `other@example.test` and no connection.
6. AC6: Without a `draft` Grant on the Account, the call is denied and audited.
   - Test: Grants `read` and `list` on `*` and no `draft`, then `draft --to carol@example.com --subject Status --body Hi`. It exits 1 with empty stdout. stderr parses as `{"error":{"code":"denied","operation":"draft","account":"agent@example.test","message":...}}` with no `mailbox` key.
   - The fake sees no connection, and `appended()` is empty.
   - The audit log holds exactly one line: `{"time":T,"mandate":"aaaa1111","operation":"draft","account":"agent@example.test","recipients":["carol@example.com"],"outcome":"denied","reason":"denied"}`.
   - The same call in reply mode (`--reply-to T(1)`), with `read` granted on `Thread` but no `draft`, fails with `"operation":"draft"` and no connection.
7. AC7: No command exists to edit or delete an existing draft.
   - Test: the top-level `--help` lists no subcommand other than the existing ones and `draft`.
   - `draft --help` names no option to replace, edit, update or delete a draft, and none of the words `edit` or `delete` appears as an option.
   - `agent-mail draft delete`, `agent-mail draft edit <REF>` and `agent-mail draft --to a@example.com --subject X --body Y --replace agent@example.test/Drafts/1700000010/1` each exit 2, with no connection and no audit entry.
   - Across every test in `tests/draft.rs`, the fake's `commands()` never contain `STORE`, `UID STORE`, `EXPUNGE`, `UID EXPUNGE`, `COPY`, `UID COPY`, `MOVE`, `UID MOVE` or `SELECT`.
8. AC8: Each draft appends an audit entry with recipients and the new message's reference, and no body content.
   - Test: after AC1's first call, the audit line is exactly `{"time":T,"mandate":"aaaa1111","operation":"draft","account":"agent@example.test","mailbox":"Drafts","messages":["agent@example.test/Drafts/1700000010/1"],"recipients":["carol@example.com","dave@example.com"],"outcome":"allowed"}`, keys in this order, with no `target`, `change` or `results` key.
   - After AC4's first call, on a freshly started fake, the line has `"messages":["agent@example.test/Thread/1700000009/1","agent@example.test/Drafts/1700000010/1"]`: the original reference, then the new draft reference.
   - With the fake started without `UIDPLUS` (`start_with_list(Mode::Implicit, &[], &["Drafts"], Some("Drafts"))`), the call exits 0 with `"ref":null`, and the audit line has no `messages` key. `--text` prints `draft created in Drafts (new reference not reported)`.
   - After all these calls, the log bytes never contain:
     - a body (`Line one`, `Thanks`) or a subject (`Status`, `Budget`);
     - an original's Message-ID (`thread-2@example.com`);
     - the Credential, the Agent Token or its token hash.

     Entries for `list`, `search`, `read`, `flag` and `move` never carry a `recipients` key.

### Other behaviour (tested in `tests/draft.rs`, no ticket criterion)
- `--to "Carol <carol@example.com>"`, `--to "a@b.c\r\nBcc: x@y.z"`, `--to "a@b@c"` and `--to ""` are each usage errors (exit 2, no connection, no audit entry). So are `--subject "Hi\r\nBcc: x@y.z"` and `--subject ""`. A core call with such a value fails with `invalid_draft`, makes no connection, and its error and audit line do not contain the rejected value.
- `--subject Grüße --body "für dich"` produces an appended message whose header section is ASCII only and that parses back to Subject `Grüße` and body `für dich`. A body with one 2,000-character line produces no raw line longer than 998 octets and parses back unchanged.
- A stale reply reference (`--reply-to agent@example.test/Thread/1700000099/1`) fails with `stale_reference`, `"operation":"draft"`, `"mailbox":"Thread"`, and a message saying that no draft was created. A missing UID (`T(99)`) fails with `message_not_found`. Neither sends `APPEND`. Their audit lines have `"outcome":"allowed"` and the matching `reason`.

## Out of scope
- Editing, replacing or deleting drafts (feature spec, Out of Scope), and sending mail.
- Cc, Bcc, display names in recipients, internationalised (SMTPUTF8) addresses, attachments, HTML bodies, and reading the body from a file or stdin.
- Defaulting `--to` to the original's From or Reply-To in reply mode, reply-all, and quoting the original's body in the reply.
- Using the original's In-Reply-To when it has no References (RFC 5322 §3.6.4 fallback).
- Finding the new draft's UID by searching the Drafts mailbox when no `APPENDUID` is reported; `ref` is then `null`.
- `LIST ... RETURN (SPECIAL-USE)`, XLIST, and choosing a Drafts mailbox by name.
- A size limit on the body.
- `--account` and several tokens (ticket 13), the MCP `draft` tool (ticket 15), and the live Gmail test (ticket 12).

## Notes for reviewers
- Security:
  - This is the first command that writes new content into a mailbox through the Credential. The Policy checks happen before any connection: `draft` on the Account, and in reply mode `read` on the original's mailbox, as AC5 and AC6 assert. The Account is matched by exact login for replies, so a reference naming an uncovered Account never uses another Account's Credential.
  - `to`, `subject` and `body` are untrusted Agent input and are the header-injection surface. Addresses are restricted to a bare `local@domain` with no CR, LF, space, `<`, `>` or `,`. The subject must not contain CR, LF or NUL. Both are validated in the CLI and again in the core. Bodies are only ever placed after the header section, with a transfer encoding.
  - The original's Message-ID, References and Subject are untrusted server data copied into the draft's headers. Message ids are re-wrapped in brackets and filtered to printable ASCII with no space or angle brackets. The subject is decoded and re-encoded, never copied raw.
  - Fetching the original uses `EXAMINE` and a `.PEEK` fetch, so replying never marks the original `\Seen` (AC4). The UIDVALIDITY check comes before the fetch.
  - The Drafts mailbox name comes from the server's `LIST` and is sent back as a quoted string. It never comes from the Agent, so an Agent cannot write into another mailbox.
  - Audit entries record recipients and references only, never subject, body or original headers (AC8). Recipients are logged because the ticket asks for them. They are addresses the Agent supplied, not message content.
  - These are this spec's concrete choices to make the ticket testable; the ticket names none of them:
    - `recipients`, `drafts_not_found` and `invalid_draft`;
    - the result shape;
    - the `operation` `read` in AC5's denial;
    - `--subject` conflicting with `--reply-to`.
  - AC7 ("no command exists") is a negative criterion. It is tested through `--help` output, usage errors for plausible edit and delete spellings, and the absence of state-changing IMAP verbs.
  - No conflict with ADR-0001 to ADR-0003.
- UX: The user is an Agent calling the CLI, usually through the skill. Typical flows:
  - New draft: `agent-mail draft --to x@y --subject ... --body ...`, then tell the human that a draft is waiting in their Drafts folder.
  - Reply:
    1. `agent-mail search --mailbox INBOX ...`
    2. `agent-mail read <ref>` to see the sender and the content.
    3. `agent-mail draft --to <sender> --reply-to <ref> --body ...`

  A denial names the operation and mailbox that lack a Grant, so the Agent can tell the human exactly what to grant: `draft` on the Account, or `read` on the original's mailbox. `drafts_not_found` tells the Agent to ask the human to set up a Drafts folder. `draft --help` explains:
  - that `--to` is repeatable and takes bare addresses;
  - that `--reply-to` takes a `search` reference and derives the subject;
  - the Grants needed;
  - that the Drafts mailbox is found by its special-use flag;
  - the result keys, and that `ref` may be `null`;
  - that drafts cannot be edited or deleted afterwards.

  The human reviews and sends the draft in their own mail client.
- UI: Terminal output only.
  - JSON by default: one line on stdout, as shown under "Result". Errors are JSON on stderr.
  - With `--text`, one line: `draft created: <ref>`, or `draft created in <mailbox> (new reference not reported)`.
