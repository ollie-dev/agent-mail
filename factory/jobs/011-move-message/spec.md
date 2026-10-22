# Move a message between granted mailboxes

## Goal
An Agent runs `agent-mail move <REF> --to <MAILBOX>` with a message reference from `search` to file or archive a message. The call needs a `move` Grant on both the source mailbox (the one the reference names) and the target mailbox. agent-mail uses IMAP `MOVE` when the server advertises it. Otherwise it falls back to `UID COPY`, marking that one UID `\Deleted`, and `UID EXPUNGE` of that UID only (UIDPLUS). On a server with neither, it refuses. The result returns the message's new reference in the target mailbox when the server reports it, and every call appends an audit entry.

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Ticket: `.scratch/agent-mail-v1/issues/10-move.md`. Feature spec: `.scratch/agent-mail-v1/spec.md`, sections "Policy and Grants" ("`move` requires the operation on both the source and the target mailbox") and "Mail operations" ("Move: IMAP MOVE where advertised, otherwise copy, mark deleted and expunge that UID only (UIDPLUS)"). Terms as in `GLOSSARY.md` (Account, Agent, Mandate, Agent Token, Policy, Grant). `Operation::Move` (`"move"`) already exists in `src/mandate.rs`.
- Files this feature touches. `move` is a Rust keyword, so the module and test file are named `move_message`.
  - `src/move_message.rs` (new): the `MoveResult` type and the `--text` line helper.
  - `src/lib.rs`: new `move` subcommand, its `--help`, and its `--text` rendering.
  - `src/core.rs`: `Command::Move { reference: MessageRef, target: SearchText }` and `Outcome::Moved(MoveResult)`. Adds the Policy check on both mailboxes, the audit entry, and two new errors: `move_unsupported` and `same_mailbox`. Both get a message that names a next step, so `every_error_message_names_a_next_step` covers them. `StaleReference`, `MessageNotFound` and `InvalidReference` are reused with `operation: Operation::Move`, and their messages for `move` say that nothing was moved.
  - `src/audit.rs`: `Entry` gains an optional `target` key, serialised after `mailbox` and before `messages`.
  - `src/imap.rs`: a move call. Log in, then read the capabilities with `CAPABILITY` after login. If neither `MOVE` nor `UIDPLUS` is advertised, return a new `ImapError::MoveUnsupported` before any `SELECT` that could change state. Otherwise, `SELECT` the source, compare its UIDVALIDITY with the reference's, and `UID FETCH <uid> (UID FLAGS)` to confirm the message exists. Then:
    - With `MOVE`, send `UID MOVE <uid> <target>`.
    - Without `MOVE` but with `UIDPLUS`, send `UID COPY <uid> <target>`, then `UID STORE <uid> +FLAGS.SILENT (\Deleted)`, then `UID EXPUNGE <uid>`.
    - Read the new UID and the target's UIDVALIDITY from a `COPYUID` response code (RFC 4315), untagged or tagged. async-imap 0.12's `uid_mv`/`uid_copy` return `()` and drop response codes, so the connector has to read the responses itself, for example with `run_command` and the unsolicited channel.
    - Then `LOGOUT`. Never send `CLOSE`, plain `EXPUNGE`, or any body fetch. The target name is sent as a quoted modified UTF-7 string (`utf7::encode`).
    - A `NO` to `UID MOVE` or `UID COPY` (e.g. `[TRYCREATE]`, target missing) maps to `mailbox_not_found` with `mailbox` = the target.
  - `tests/move_message.rs` (new) and `tests/support/mod.rs`. Fake server changes:
    - New `FakeImap::start_with_capabilities(mode, &[&str])` appends the given atoms (e.g. `MOVE`, `UIDPLUS`) to the advertised capabilities. `start(mode)` is unchanged and advertises neither.
    - `UID MOVE` (only when `MOVE` is advertised, else `BAD`) and `UID COPY`/`COPY` (always) are implemented. A copy keeps the message's flags and gets UID = the target's last UID + 1. When `UIDPLUS` is advertised, the fake reports `[COPYUID <target uidvalidity> <old uid> <new uid>]`: in an untagged `OK` before the `EXPUNGE` responses for `MOVE`, and in the tagged `OK` for `COPY`. Both `MOVE` and `COPY` to a missing mailbox answer `NO [TRYCREATE]`.
    - `EXPUNGE` removes every `\Deleted` message of the selected mailbox. `UID EXPUNGE <set>` (only when `UIDPLUS` is advertised, else `BAD`) removes only `\Deleted` messages in the set. Both answer `* n EXPUNGE`.
    - A new accessor `messages_in(name) -> Vec<(u32, String, Vec<String>)>` returns UID, subject and flags in UID order.
    - The fake gains the `Filing` mailbox below.
    - Existing tests that assert no `COPY`, `MOVE` or `EXPUNGE` keep passing.
  - `README.md`: a "Move" section (command, Grants needed, result, errors, MOVE/UIDPLUS behaviour). The Audit log section names `move` and documents `mailbox`, `target` and `messages` for it.
  - `CLAUDE.md`: the Stack layout line names `src/move_message.rs` and `tests/move_message.rs` (checked by `tests/project.rs`).

### Command line
`agent-mail move <REF> --to <MAILBOX>`, plus `--text` (global or after the subcommand).
- `<REF>`: a message reference as `search` prints it, parsed with `read::parse_reference`. A reference that does not parse is a usage error: exit 2, no connection, no audit entry.
- `--to <MAILBOX>` is required. It takes the decoded UTF-8 name as `list` prints it (e.g. `Archive`, `[Gmail]/Entwürfe`). An empty value or one containing CR, LF or NUL is a usage error (exit 2, no audit entry).
- The Account is the Mandate's Account whose login equals the reference's account, as for `read` and `flag`. The target is in the same Account.

### Result (JSON, one line on stdout)
```json
{"ref":"agent@example.test/INBOX/1700000001/2","account":"agent@example.test","mailbox":"INBOX","target":"Archive","new_ref":"agent@example.test/Archive/1700000002/26"}
```
Keys exactly these, in this order. `ref` is the reference as given, `mailbox` is the source and `target` is the target as given. `new_ref` is the reference built from the `COPYUID` the server reported (the target's UIDVALIDITY and the new UID), or `null` when the server reported none. With `--text`, the output is one line `<ref> -> <new_ref>`, or `<ref> -> <target> (new reference not reported)`, with every value passed through `escape_controls`.

### Other behaviour (tested in `tests/move_message.rs`, no ticket criterion)
- `--to` equal to the reference's mailbox (exact string match) fails with `same_mailbox` (`operation` `move`, `account`, `mailbox`) before any connection. The audit entry is `outcome` `denied`, `reason` `same_mailbox`.
- If the target does not exist on the server (`--to Missing`, with `move` granted on `*`), the call fails with `mailbox_not_found`, `"mailbox":"Missing"`. The source is unchanged.

### Fake server fixture (used by the criteria below)
- Existing `INBOX` (UIDVALIDITY `1700000001`, UIDs 1 to 4; UID 2 is `Lunch?` from `bob@example.com`, no flags) and `Archive` (UIDVALIDITY `1700000002`, UIDs 1 to 25).
- New mailbox `Filing`, UIDVALIDITY `1700000008`, not added to `SERVER_MAILBOXES`. Its three simple messages (`source: None`):

| UID | From | Subject | Flags |
|---|---|---|---|
| 1 | `alice@example.com` | `Filing one` | none |
| 2 | `bob@example.com` | `Filing two` | `\Deleted` (marked by the human, not yet expunged) |
| 3 | `carol@example.com` | `Filing three` | `\Seen` |

Unless a criterion says otherwise:
- Tests use the Mandate `aaaa1111` with the single Account `agent@example.test` (`support::LOGIN`) and Grants `move` on `INBOX`, `Archive` and `Filing`, and `search` on `*`.
- The fake is started with `start_with_capabilities(Mode::Implicit, &["MOVE", "UIDPLUS"])`.
- `I(n)` = `agent@example.test/INBOX/1700000001/<n>`, and `F(n)` = `agent@example.test/Filing/1700000008/<n>`.
- "Unchanged" means `messages_in` for every mailbox equals its value before the call.

## Acceptance criteria
1. AC1: With `move` granted on INBOX and Archive, moving a message leaves it in Archive and removes it from INBOX on the fake server.
   - Test: `move I(2) --to Archive` exits 0 and stdout is exactly `{"ref":"agent@example.test/INBOX/1700000001/2","account":"agent@example.test","mailbox":"INBOX","target":"Archive","new_ref":"agent@example.test/Archive/1700000002/26"}`.
   - `messages_in("INBOX")` holds UIDs `[1, 3, 4]` with their flags unchanged. `messages_in("Archive")` holds UIDs 1 to 26, and UID 26 is `(26, "Lunch?", [])`.
   - The fake's `commands()` contain `SELECT` and `UID MOVE`. They contain no `UID COPY`, `COPY`, `STORE`, `UID STORE`, `EXPUNGE`, `UID EXPUNGE` or `CLOSE`. No fetch item other than `UID` and `FLAGS` is requested.
   - With `--text`, the output is the line `agent@example.test/INBOX/1700000001/2 -> agent@example.test/Archive/1700000002/26`.
2. AC2: With `move` granted only on the source, the move is denied, nothing changes, and the denial is audited.
   - Test: Grants `move` on `INBOX` only, then `move I(2) --to Archive`. It exits 1 with empty stdout. stderr parses as `{"error":{"code":"denied","operation":"move","account":"agent@example.test","mailbox":"Archive","message":...}}`, naming the target that lacks the Grant.
   - The fake sees no connection, and every mailbox is unchanged.
   - The audit log holds exactly one line `{"time":T,"mandate":"aaaa1111","operation":"move","account":"agent@example.test","mailbox":"INBOX","target":"Archive","messages":["agent@example.test/INBOX/1700000001/2"],"outcome":"denied","reason":"denied"}`.
   - The same holds for an empty Policy (error `mailbox` is `INBOX`), and for a reference naming `other@example.test` (an Account the Mandate does not cover; error `account` is `other@example.test`). In both cases there is no connection and the audit line has `"outcome":"denied"`.
3. AC3: With `move` granted only on the target, the move is denied and nothing changes.
   - Test: Grants `move` on `Archive` and `read` and `search` on `INBOX`, then `move I(2) --to Archive`. It exits 1 with empty stdout. stderr parses as `{"error":{"code":"denied","operation":"move","account":"agent@example.test","mailbox":"INBOX","message":...}}`.
   - The fake sees no connection, and every mailbox is unchanged.
   - The audit line is as in AC2, with `"outcome":"denied","reason":"denied"`.
4. AC4: On a server without MOVE but with UIDPLUS, the fallback moves the message and leaves every other message in the source untouched.
   - Test: fake started with `&["UIDPLUS"]`, then `move F(1) --to Archive`. It exits 0 with `"new_ref":"agent@example.test/Archive/1700000002/26"`.
   - `messages_in("Filing")` is exactly `[(2, "Filing two", ["\\Deleted"]), (3, "Filing three", ["\\Seen"])]`, so the message the human marked deleted is not expunged. `messages_in("Archive")` UID 26 is `(26, "Filing one", [])`, without `\Deleted`.
   - `commands()` contain `UID COPY`, `UID STORE` and `UID EXPUNGE`, in that order. They contain no `UID MOVE`, `MOVE`, plain `EXPUNGE` or `CLOSE`.
   - `stores()` is exactly one entry: `1 +FLAGS.SILENT (\Deleted)` or `1 +FLAGS (\Deleted)`.
5. AC5: On a server with neither MOVE nor UIDPLUS, the move fails with a structured error rather than expunging other messages.
   - Test: fake started with `start(Mode::Implicit)`, then `move F(1) --to Archive`. It exits 1 with empty stdout. stderr parses as `{"error":{"code":"move_unsupported","operation":"move","account":"agent@example.test","mailbox":"Filing","message":...}}`. The message says that the server supports neither MOVE nor UIDPLUS and that nothing was moved.
   - `commands()` contain none of `UID MOVE`, `MOVE`, `UID COPY`, `COPY`, `UID STORE`, `STORE`, `UID EXPUNGE`, `EXPUNGE` or `CLOSE`, and every mailbox is unchanged.
   - The audit line has `"outcome":"allowed","reason":"move_unsupported"`.
   - With `--text`, the output is `error: move_unsupported: ...`.
6. AC6: A stale message reference fails without moving anything.
   - Test: `move agent@example.test/INBOX/1700000099/2 --to Archive` exits 1 with empty stdout. stderr parses as `{"error":{"code":"stale_reference","operation":"move","account":"agent@example.test","mailbox":"INBOX","message":...}}`. The message says nothing was moved and tells the Agent to search again for a fresh reference.
   - The fake sees `SELECT`, but no `UID FETCH`, `UID MOVE`, `UID COPY`, `UID STORE` or `UID EXPUNGE`, and every mailbox is unchanged.
   - The audit line has `"outcome":"allowed","reason":"stale_reference"`.
   - A missing UID (`move agent@example.test/INBOX/1700000001/99 --to Archive`) fails with `message_not_found` and `"operation":"move"`. It sends no `UID MOVE` or `UID COPY`, and every mailbox is unchanged.
7. AC7: The result returns the message's new reference in the target mailbox where the server reports it.
   - Test with `&["MOVE", "UIDPLUS"]`: `new_ref` is as in AC1, and the audit line is exactly `{"time":T,"mandate":"aaaa1111","operation":"move","account":"agent@example.test","mailbox":"INBOX","target":"Archive","messages":["agent@example.test/INBOX/1700000001/2","agent@example.test/Archive/1700000002/26"],"outcome":"allowed"}`, keys in this order, with no `results` or `change` key.
   - `new_ref` works as a reference: `read` with it, with `read` granted on `Archive`, returns subject `Lunch?`.
   - Test with `&["MOVE"]` only (the fake then sends no `COPYUID`): `move I(2) --to Archive` exits 0 with `"new_ref":null`, and the message is in Archive as UID 26. The audit `messages` is `["agent@example.test/INBOX/1700000001/2"]`. `--text` prints `agent@example.test/INBOX/1700000001/2 -> Archive (new reference not reported)`.
   - After these calls, the log bytes never contain a subject (`Lunch?`), an address from a message (`bob@example.com`), the Credential, the Agent Token or its token hash. `list`, `search`, `read` and `flag` entries never carry a `target` key.

## Out of scope
- Moving several messages, UID ranges, or messages selected by search criteria in one call.
- Moving between Accounts, creating the target mailbox, and Gmail label editing (`X-GM-LABELS`).
- Rolling back a half-finished fallback (e.g. a `UID COPY` that succeeded followed by a failed `UID STORE` or `UID EXPUNGE`). The call reports the server error, and the human may see a duplicate or a `\Deleted` source copy.
- Discovering the new UID by searching the target (e.g. by Message-ID) when no `COPYUID` is reported. `new_ref` is then `null`.
- Permanent delete or moving to Trash by special-use flag.
- `--account` and several tokens (ticket 13), the MCP `move` tool (ticket 15), and the live Gmail test (ticket 12).

## Notes for reviewers
- Security:
  - This is the first command that removes a message from a mailbox through the Credential. Both Policy checks (`move` on the source and on the target, on the reference's Account) happen before any connection (AC2 and AC3 assert no connection). The Account is matched by exact login, so an uncovered Account's reference never uses another Account's Credential.
  - The target mailbox is untrusted Agent input. It is validated as `SearchText` (no CR, LF or NUL; the CLI also rejects empty), compared exactly against the Grants, and sent only as a quoted, modified-UTF-7 string. The reference gets the same `is_valid_reference` check in the core as for `read` and `flag`, because the MCP adapter will call the core directly.
  - Expunge is the dangerous step. Plain `EXPUNGE` and `CLOSE` would also remove messages the human marked `\Deleted`. The fallback therefore uses `UID EXPUNGE` of that one UID only, and refuses outright without UIDPLUS (AC4 and AC5 use a pre-existing `\Deleted` message to catch a plain `EXPUNGE`). The capability check comes before any change, so an unsupported server never gets a `COPY` (no duplicates).
  - The UIDVALIDITY check comes after `SELECT` and before any fetch, move or copy (AC6). The existence check stops a missing UID being reported as moved, since `UID MOVE` of a missing UID answers `OK`.
  - Capabilities must be read after login (Gmail advertises more after authentication). The fake advertises the same set before and after, so this is a review point rather than a test.
  - Audit entries record references and mailbox names only, never subjects or addresses (AC7). The `target` key, the `new_ref` key, `move_unsupported`, `same_mailbox` and the result shape are this spec's concrete choices to make the ticket testable. The ticket names none of them. The ticket's AC3 does not mention auditing, but the denial is audited the same way as in AC2.
  - No conflict with ADR-0001 to ADR-0003.
- UX: The user is an Agent calling the CLI, usually through the skill. Typical flow:
  1. `agent-mail list` to see the granted mailboxes.
  2. `agent-mail search --mailbox INBOX ...` and then `agent-mail read <ref>`.
  3. `agent-mail move <ref> --to Archive`.

  The Agent uses `new_ref` for follow-up calls, such as flagging the filed message. When `new_ref` is `null`, it searches the target instead. A denial names the mailbox that lacks the Grant, so the Agent can tell the human exactly which Grant to add. `--help` for `move` explains the reference argument, that `--to` takes a name as `list` prints it, that `move` must be granted on both mailboxes, the result keys, `new_ref` possibly being `null`, and `stale_reference` and `move_unsupported`.
- UI: Terminal output only. JSON by default, one line on stdout as shown under "Result". Errors are JSON on stderr. With `--text`, one line: `<ref> -> <new_ref>`, or `<ref> -> <target> (new reference not reported)`.
