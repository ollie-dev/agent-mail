# Several Mandates per Agent and `whoami`

## Goal
An Agent can hold several Mandates at once. It puts several Agent Tokens in `AGENT_MAIL_TOKEN`, comma-separated, and names the Account a command works on with `--account` (or leaves it out when only one Account is reachable). agent-mail picks the one Mandate covering that Account and enforces only that Mandate's Policy. It refuses with `ambiguous_mandate` when two presented Mandates cover the Account. `agent-mail whoami` shows, for each presented token, its Mandate's short ID, name, Accounts and Grants, so the Agent knows what it may do before it tries.

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Ticket: `.scratch/agent-mail-v1/issues/13-multiple-tokens-whoami.md` (user stories 27 to 31 of `.scratch/agent-mail-v1/spec.md`). The spec says: "`AGENT_MAIL_TOKEN` holds one or more tokens, comma-separated. Each command targets one Account (optional when unambiguous). The Mandate covering that Account is used. If two presented Mandates cover it, that is an error. Grants never combine across Mandates." It also says: "`whoami`: the Policies of the presented tokens."
- Terms used here:
  - **Presented tokens**: the fields of `AGENT_MAIL_TOKEN` split on `,`. Each field is trimmed of ASCII whitespace. Empty or whitespace-only fields are ignored. A field's **position** is its 1-based field number in the split string, counting ignored fields, so in `A,,B` token B is at position 3. A token that appears more than once counts once.
  - **Opened Mandate**: a presented token that parses, is found in the Mandate store and whose record opens (`seal::open`). Any other presented token is an **invalid token**.
  - **Reachable Accounts**: the logins of the Accounts covered by the opened Mandates. They are deduplicated and ordered by token position, then by the order of the Accounts inside each Mandate.
  - **Target Account**: the Account a command works on. For `read`, `flag`, `move` and `draft --reply-to` it is the message reference's account, as today. For `list`, `search` and `draft` without `--reply-to` it is the `--account` value, or, when that is omitted, the only reachable Account.
  - **Selected Mandate**: the one opened Mandate with an Account whose login equals the target Account exactly. Every Grant check, the `list` mailbox filter, the Credential used and the audit line's `mandate` come from the selected Mandate only.
- Files this feature touches:
  - `src/core.rs`:
    - Replace `open_mandate` with a step that opens every presented token and records the positions of invalid tokens. A store read failure still fails the whole call with `store_unavailable`.
    - Replace `single_account` with target-Account resolution and Mandate selection.
    - `Command::List`, `Command::Search` and `Command::Draft` gain an optional account (for example `account: Option<String>`). The builder chooses the shape.
    - New `Command::Whoami` and `Outcome::Whoami(WhoamiResult)`.
    - New `CoreError::AmbiguousMandate { account, mandates }` with code `ambiguous_mandate`. `ErrorBody` gains an optional `mandates` field holding short IDs.
    - The `account_required` message changes to name `--account`.
    - The invalid-token positions are reported next to the result or error, for example in a small report struct returned by `execute`, so the MCP adapter (ticket 15) can reuse them.
    - Add the new error to `every_error_message_names_a_next_step`.
  - `src/lib.rs`:
    - `--account <LOGIN>` on `list`, `search` and `draft`. On `draft` it conflicts with `--reply-to`. The value must be non-empty and hold no CR, LF or NUL; anything else is a usage error (exit code 2).
    - A new `whoami` subcommand that takes `--text`.
    - Rendering of the invalid-token warning on stderr, and the `whoami` JSON and text output.
    - Help text on `AGENT_MAIL_TOKEN` saying it takes one or more comma-separated tokens.
  - `tests/tokens.rs` (new): several tokens, `--account`, `ambiguous_mandate`, invalid tokens and `whoami`. `tests/support/mod.rs` gains a helper that runs with a raw `AGENT_MAIL_TOKEN` string, and possibly a fixture for a Mandate with a chosen ID.
  - Existing tests that assert the old `account_required` message or the single-token behaviour (`tests/list.rs`, `tests/search.rs`, `tests/draft.rs`, `tests/audit.rs`) are updated wherever the message changed.
  - `README.md`: a new section on several Mandates and `whoami`. The Draft section's `account_required` sentence and the Audit log section's `reason` list are updated.
  - `CLAUDE.md`: add `tests/tokens.rs` to the test layout list.
- Test fixtures: "work" is the fake Account `agent@example.test` (`LOGIN`) and "private" is `other@example.test` (`OTHER_LOGIN`). The fake IMAP server accepts both logins. Both Accounts point at the same fake server. Each test builds its Mandates with distinct short IDs, for example `aaaa1111`, `bbbb2222` and `cccc3333`.

## Acceptance criteria
1. AC1: With two tokens for Mandates covering different Accounts, `list --account work` uses the Mandate covering `work` and `list --account private` the other.
   - Setup: Mandate W (`aaaa1111`) covers only `agent@example.test` and grants `list` on `INBOX`. Mandate P (`bbbb2222`) covers only `other@example.test` and grants `list` on `Archive`. `AGENT_MAIL_TOKEN` is `<tW>,<tP>`.
   - Input: `agent-mail list --account agent@example.test`. Expected: exit code 0, stdout `{"account":"agent@example.test","mailboxes":[{"name":"INBOX"}]}`, and stderr empty. The audit line has `mandate` `aaaa1111`, `account` `agent@example.test` and `outcome` `allowed`.
   - Input: `agent-mail list --account other@example.test`. Expected: exit code 0, stdout `{"account":"other@example.test","mailboxes":[{"name":"Archive"}]}`, and the audit line has `mandate` `bbbb2222`.
   - The token order does not matter. With `<tP>, <tW>` (a space after the comma), both calls give the same results.
   - Reference commands use the same selection. With W also granting `read` on `INBOX`, `read agent@example.test/INBOX/<INBOX_UIDVALIDITY>/1` exits 0 and its audit line has `mandate` `aaaa1111`. `search --account other@example.test --mailbox Archive` needs P's Grants, and with P granting `search` on `Archive` it exits 0 with `account` `other@example.test`.
   - `--account ""` and `--account $'a\nb'` are usage errors: exit code 2, no connection (`server.connections() == 0`) and no audit line. `draft --account agent@example.test --reply-to <REF> ...` is also a usage error (exit code 2).
2. AC2: With one reachable Account, `--account` can be omitted. With several, omitting it returns a structured error listing the choices.
   - One reachable Account: with only `<tW>` presented, `agent-mail list` gives the same output as `list --account agent@example.test` in AC1. Two presented Mandates covering that one Account are not this case: that is ambiguous (AC3). With `<tW>,garbage`, `list` also succeeds, since the invalid token reaches nothing (see AC5).
   - Several reachable Accounts: with `<tW>,<tP>`, `agent-mail list` exits 1. stderr is `{"error":{"code":"account_required","accounts":["agent@example.test","other@example.test"],"message":...}}`. The message names `--account` as the next step and no longer says choosing is "not supported yet". The same holds for `search` and for `draft --to carol@example.com --subject S --body B`. A single Mandate covering both Accounts gives the same error, with the same `accounts` list.
   - In both cases no connection is made (`server.connections() == 0`). One audit line is appended with `outcome` `denied` and `reason` `account_required`, and without `mandate` or `account`.
3. AC3: When two presented Mandates cover the same Account, any command on it returns a structured `ambiguous_mandate` error.
   - Setup: Mandates X (`aaaa1111`) and Y (`bbbb2222`) both cover `agent@example.test`. Each grants `list`, `search`, `read`, `flag`, `move` and `draft` on `*`. Mandate P (`cccc3333`) covers `other@example.test` and grants `list` on `Archive`. `AGENT_MAIL_TOKEN` is `<tX>,<tY>,<tP>`.
   - Each of these exits 1 with stderr `{"error":{"code":"ambiguous_mandate","account":"agent@example.test","mandates":["aaaa1111","bbbb2222"],"message":...}}`. The `mandates` list is in token order. The message says agent-mail never picks one and names the next step: present only one of these tokens, or ask the human to delete one of the Mandates.
     - `list --account agent@example.test`
     - `search --account agent@example.test`
     - `read agent@example.test/INBOX/<INBOX_UIDVALIDITY>/1`
     - `flag <same ref> --set seen`
     - `move <same ref> --to Archive`
     - `draft --account agent@example.test --to carol@example.com --subject S --body B`
     - `draft --reply-to <same ref> --to carol@example.com --body B`
   - None of them connects (`server.connections() == 0`). Each appends one audit line with `account` `agent@example.test`, `outcome` `denied` and `reason` `ambiguous_mandate`, and without `mandate`.
   - The ambiguity is reported even when only one of the two Mandates grants the operation. With Y's Grants emptied, `list --account agent@example.test` still fails with `ambiguous_mandate`.
   - Other Accounts are unaffected. `list --account other@example.test` exits 0 with `mailboxes` `[{"name":"Archive"}]`.
   - The same token presented twice (`<tX>,<tX>`) is not ambiguous. `list` exits 0 using X.
4. AC4: A Grant from one Mandate never allows an operation on an Account covered only by another Mandate.
   - Setup: Mandate W (`aaaa1111`) covers only `agent@example.test`. Besides `list` on `INBOX`, its Policy holds Grants naming `other@example.test`: `list`, `search`, `read` and `draft` on `*`. Such a record can be built and sealed in a test even though `mandate create` never produces it. Mandate P (`bbbb2222`) covers only `other@example.test` and grants only `list` on `Archive`. `AGENT_MAIL_TOKEN` is `<tW>,<tP>`.
   - `list --account other@example.test` exits 0 with `mailboxes` exactly `[{"name":"Archive"}]`. W's `list` Grant on `*` for that login adds nothing.
   - `search --account other@example.test --mailbox INBOX` exits 1 with `error.code` `denied`, `error.operation` `search`, `error.account` `other@example.test` and `error.mailbox` `INBOX`.
   - `read other@example.test/INBOX/<INBOX_UIDVALIDITY>/1` exits 1 with `denied` (`operation` `read`, `mailbox` `INBOX`).
   - `draft --account other@example.test --to carol@example.com --subject S --body B` exits 1 with `denied` (`operation` `draft`).
   - None of the denied calls connects to the server. Each appends an audit line with `mandate` `bbbb2222` and `outcome` `denied`.
   - An Account no opened Mandate covers is denied too. With only `<tW>` presented, `list --account other@example.test` exits 1 with `denied`, `operation` `list` and `account` `other@example.test`, and makes no connection. The audit line has `account` `other@example.test` and no `mandate`.
5. AC5: An invalid token in the list is reported without blocking the valid ones.
   - Invalid tokens covered: `garbage` (malformed), a well-formed token minted with `AgentToken::mint()` but never stored (unknown), and a token whose stored record was tampered with (does not open).
   - Input: `AGENT_MAIL_TOKEN` = `garbage,<tW>` and `agent-mail list`. Expected: exit code 0 and stdout exactly as in AC1. stderr is exactly one line, `{"warning":{"code":"invalid_token","positions":[1],"message":...}}`. The message says the token at that position of `AGENT_MAIL_TOKEN` is not valid and was ignored, and names the next step: check it, or ask the human for a new token. stderr does not contain `garbage` or any token text.
   - With `<tW>,<unknown>,<tampered>` the warning's `positions` is `[2,3]`. With `<tW>,,garbage` it is `[3]`.
   - With `--text` (`agent-mail --text list`) the warning is the single stderr line `warning: invalid_token: <message>`.
   - The warning also comes with an error. With `<tW>,garbage`, `list --account other@example.test` exits 1. stderr holds the warning line first, then the `denied` error.
   - If every presented token is invalid, the result is the existing `invalid_token` error (exit code 1) with no separate warning line. If only commas and spaces are presented (`" , ,"`), the result is the existing `missing_token` error.
   - The audit line for a call with invalid tokens has the same keys as without them, and never contains token text.
   - The same reporting applies to `search`, `read`, `flag`, `move` and `draft`. One test per command with `garbage,<valid>` that asserts exit code 0 and the warning line is enough.
6. AC6: `whoami` returns, for each presented Mandate, its short ID, name, Accounts and Grants, and never Credentials.
   - Setup: Mandate W (`aaaa1111`, name `work-inbox`) covers `agent@example.test` on `localhost:<port>` with implicit TLS and grants `list` on `INBOX` and `Archive`. Mandate P (`bbbb2222`, name `other-archive`) covers `other@example.test` and has no Grants. `AGENT_MAIL_TOKEN` is `<tW>,<tP>,garbage`.
   - Input: `agent-mail whoami`. Expected: exit code 0, stderr empty, and stdout one JSON line with the keys in this order:
     `{"mandates":[{"id":"aaaa1111","name":"work-inbox","accounts":[{"login":"agent@example.test","host":"localhost","port":<port>,"tls":"implicit"}],"grants":[{"operation":"list","account":"agent@example.test","mailboxes":["INBOX","Archive"]}]},{"id":"bbbb2222","name":"other-archive","accounts":[{"login":"other@example.test","host":"localhost","port":<port>,"tls":"implicit"}],"grants":[]}],"invalid_tokens":[3]}`
     - Mandates appear in token order. Each Mandate's ID, name, Accounts and Grants come from its opened (authenticated) record. `invalid_tokens` is `[]` when every token opened.
   - Never secrets: stdout and stderr do not contain the fake Credential, either token, either token hash (`token_hash`) or `garbage`.
   - No mail access: `server.connections() == 0`, and no Grant is needed (P, with no Grants, is still shown).
   - Ambiguity is not an error for `whoami`. With two Mandates covering `agent@example.test`, both are listed and the exit code is 0.
   - With `AGENT_MAIL_TOKEN` unset, `whoami` fails with `missing_token` (exit code 1). With only invalid tokens it exits 0 with `"mandates":[]` and their positions in `invalid_tokens`.
   - `agent-mail --text whoami` (or `whoami --text`) prints, for each Mandate, a line `<id>  <name>`, then `  account <login> (<host>:<port>, <tls>)` lines and `  grant <operation> on <account>: <mailboxes>` lines. For each invalid token it prints the line `token <position> of AGENT_MAIL_TOKEN is not valid and was ignored`. Control characters in names are escaped, as in `mandate list --text`.
7. AC7: The README and the help text document several tokens, `--account`, `ambiguous_mandate` and `whoami`.
   - `README.md` gains a section (for example "Several Mandates and whoami") explaining:
     - `AGENT_MAIL_TOKEN` takes one or more comma-separated tokens.
     - `--account` on `list`, `search` and `draft`, and that reference commands take the Account from the reference.
     - The `account_required` error with its `accounts` list.
     - The `ambiguous_mandate` error and what to do about it.
     - That Grants never combine across Mandates.
     - The `invalid_token` warning with its positions.
     - `whoami` and its output.
   - The Draft section no longer says a Mandate covering several Accounts simply fails; it points to `--account`. The Audit log section's `reason` examples include `account_required` and `ambiguous_mandate`.
   - A test in `tests/tokens.rs` asserts that `README.md` contains `whoami`, `--account`, `ambiguous_mandate`, `account_required` and `comma-separated`. It also asserts that `agent-mail --help` lists `whoami`, and that `list --help`, `search --help` and `draft --help` each list `--account`.

## Out of scope
- An audit line for `whoami`. It reads only the Mandate store and touches no mail. Auditing it, which would need an entry naming several Mandates, is left for later.
- The MCP adapter (ticket 15). It will reuse the core's target-Account resolution and the reported invalid-token positions.
- A cap on the number of presented tokens, and any token selection by Mandate ID or name. Selection is by Account only.
- `--account` on `read`, `flag`, `move` and `draft --reply-to`. Their Account always comes from the message reference.
- Changing `mandate create`, `mandate list` or `mandate delete`, which read no Agent Token.
- Matching Accounts by anything other than the exact login (no host comparison, no case folding).

## Notes for reviewers
- Security:
  - This job changes which Credential is used for a call, so Mandate selection is the critical path. Check these points:
    - A Grant is only ever evaluated against the selected Mandate's own Policy (AC4).
    - An Account that no opened Mandate covers is denied before any connection.
    - Ambiguity is detected before any Grant check or connection (AC3), so agent-mail never silently picks between two Credentials for the same login.
  - Invalid tokens must never be echoed. The warning, the errors and the audit log carry positions only. Tests assert that `garbage` and token text are absent.
  - `whoami` exposes the same metadata as `mandate list` (logins, hosts, ports, TLS mode, Grants) for the presented tokens only, taken from authenticated records. It must not expose a Credential, token, token hash or store key. Every presented token costs one store lookup, which on macOS can mean one Keychain prompt per token. This is acceptable, but a reviewer may want a cap; it is listed as out of scope.
  - `--account` is untrusted Agent input. It is compared exactly against logins and echoed in errors and the audit log, so it must be refused when empty or when it holds CR, LF or NUL (AC1).
  - The `ambiguous_mandate` error reveals Mandate short IDs to the Agent. `whoami` reveals them anyway.
- UX:
  - The user is an Agent whose host config holds several tokens, for example a Claude Code project's `.claude/settings.local.json`. Its flow:
    1. Run `whoami` to see its Mandates and their Grants.
    2. Run commands with `--account <login>`, or without it when only one Account is reachable.
    3. Follow references from `search` into `read`, `flag`, `move` and `draft --reply-to` without naming the Account again.
  - Each new error names its next step: `account_required` lists the choices, `ambiguous_mandate` says to drop a token or ask the human to delete a Mandate, and the invalid-token warning gives positions so the human can find the bad token.
  - The ticket's "work" and "private" are put into practice as the fake logins `agent@example.test` and `other@example.test`. The Account is identified by its login, as in Grants and output.
  - Ticket 13's criteria are carried over unchanged as AC1 to AC6. AC7 was added for the README and help text.
  - "Reported without blocking" (ticket AC5) is put into practice as a stderr warning line next to an unchanged result, and as `invalid_tokens` in `whoami`.
- UI:
  - Terminal output only.
  - New: the `whoami` subcommand with JSON and `--text` output, the `--account` option on `list`, `search` and `draft`, the `ambiguous_mandate` error, a reworded `account_required` message, and a `{"warning":...}` / `warning: ...` line on stderr when some tokens are invalid.
  - Existing result shapes on stdout are unchanged.
