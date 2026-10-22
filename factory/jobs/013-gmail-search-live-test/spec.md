# Gmail search syntax and opt-in live Gmail test

## Goal
An Agent can run `agent-mail search --gmail "<query>"` against a Gmail Account and use Gmail's own search syntax (`from:alice has:attachment`, `newer_than:7d`). agent-mail passes the query to the server unchanged as `X-GM-RAW`. On servers that do not advertise `X-GM-EXT-1` the option fails with a structured `unsupported` error. A developer can also run an opt-in live test against a dedicated real Gmail account. It confirms LIST, standard search, Gmail search, `\Drafts` detection and peek-reading, and the Credential never touches disk, output or the repo.

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Ticket: `.scratch/agent-mail-v1/issues/12-gmail-search-live-test.md` (user stories 34 and 72 of `.scratch/agent-mail-v1/spec.md`). The spec says: "When the server advertises `X-GM-EXT-1`, a Gmail query string is passed through as `X-GM-RAW`."
- Files this feature touches:
  - `src/lib.rs`: `SearchArgs` gets `--gmail <QUERY>`. Its help text says the query is Gmail search syntax, that it only works on servers advertising Gmail's extensions, and that it cannot be combined with the standard criteria. `From<SearchArgs> for Query` carries the new field over.
  - `src/search.rs`: `Query` gets `gmail: Option<SearchText>` (`Query::all` sets it to `None`). When `gmail` is set, `imap_criteria()` returns `X-GM-RAW <quoted query>`, with `CHARSET UTF-8 ` in front when the query is not ASCII. Quoting uses the existing `SearchText::quoted`.
  - `src/imap.rs`: when the query is a Gmail query, `search` reads the capabilities after login (as `move_message` does) and fails with a new `ImapError` variant before `EXAMINE` when `X-GM-EXT-1` is missing. A new public `imap::drafts_mailbox(account, network) -> Result<String, ImapError>` returns the first mailbox, in server order, that LIST marks `\Drafts`. It is factored out of `draft_in`, which now uses it, so draft behaviour stays the same.
  - `src/core.rs`: a new `CoreError` variant with code `unsupported` and a next-step message. Add it to `every_error_message_names_a_next_step`.
  - `tests/support/mod.rs`: the fake IMAP server learns `X-GM-RAW`. A test scripts the UIDs it returns for an exact query string, for example `server.script_gmail_search("from:alice has:attachment", &[1])`. Unscripted queries match nothing. When the fake does not advertise `X-GM-EXT-1`, any `X-GM-RAW` gets `BAD`. `searches()` records the raw arguments as it does today.
  - `tests/search.rs`: the fake-server tests for AC1 to AC3 and AC6.
  - `tests/gmail_live.rs` (new): the live test (AC4, AC5). It is `#[ignore]`d and runs with `cargo test --test gmail_live -- --ignored`. Its test function name contains `gmail_live` and not `keychain`, so `cargo test -- --ignored keychain` still runs only the Keychain test.
  - `README.md`: the Search section documents `--gmail`, and a short developer note documents the live test.
  - `CLAUDE.md`: add `tests/gmail_live.rs` and its run command to the test layout list.

## Acceptance criteria
1. AC1: Against a fake server advertising `X-GM-EXT-1`, `search --gmail "from:alice has:attachment"` sends the query as the server's raw Gmail search and returns its results.
   - Setup: a fake started with `X-GM-EXT-1`, the standard fixture INBOX, and the query `from:alice has:attachment` scripted to UIDs `[1]`. The Mandate grants `search` on `INBOX`.
   - Input: `agent-mail search --gmail "from:alice has:attachment"`.
   - Expected: exit code 0. `server.searches()` is exactly `["X-GM-RAW \"from:alice has:attachment\""]`, with no `FROM` key and no translation of the query. The JSON has `mailbox` `INBOX`, `total` 1, `more` false and one message whose `ref` is `agent@example.test/INBOX/<INBOX_UIDVALIDITY>/1` and whose `subject` is `Quarterly report`. A standard `--from alice` search would return UIDs 3 and 1, so this result shows the raw query decided the outcome.
   - Also: `--gmail "von:jürgen"` sends `CHARSET UTF-8 X-GM-RAW "von:jürgen"`. `--gmail 'subject:"a\b"'` sends the query with `"` and `\` escaped (`X-GM-RAW "subject:\"a\\b\""`), so a quote cannot end the string and add search keys. `--gmail` with an empty value, or with a CR, LF or NUL, is a usage error: exit code 2, no connection (`server.connections() == 0`) and no audit line. `--gmail` together with any of `--from`, `--to`, `--subject`, `--text` (after the subcommand), `--since`, `--before`, `--unseen` or `--flagged` is also a usage error: exit code 2, no connection, no audit line. `--mailbox` and `--limit` may be combined with `--gmail`.
2. AC2: Against a fake server without `X-GM-EXT-1`, `--gmail` fails with a structured `unsupported` error.
   - Input: `agent-mail search --gmail "has:attachment"` against a fake started with no extra capabilities, with `search` granted on `INBOX`.
   - Expected: exit code 1. stderr is the structured error with `error.code` `unsupported`, `error.account` the login and `error.mailbox` `INBOX`. The message says the server does not support Gmail search and names the next step: use the standard criteria (`--from`, `--subject`, ...). It does not repeat the query. No `EXAMINE`, `SELECT` or `UID SEARCH` appears in `server.commands()`, and `server.searches()` is empty. The session ends with `LOGOUT`. With `--text` before the subcommand, the same failure prints the human-readable error and exits 1.
3. AC3: `--gmail` respects the same Grants, limit and audit behaviour as standard search.
   - Grants: a Mandate granting `search` on `INBOX` only, given `search --mailbox Archive --gmail "from:alice"`, fails with `denied` (exit code 1, `error.mailbox` `Archive`) and `server.connections() == 0`. A Mandate without a `search` Grant fails the same way for `--gmail "x"` on the default `INBOX`.
   - Limit: with `X-GM-EXT-1` and `label:all` scripted on `Archive` to all 25 UIDs, `search --mailbox Archive --gmail "label:all" --limit 5` returns UIDs 25, 24, 23, 22 and 21 in that order, with `total` 25 and `more` true. Without `--limit` it returns 20 messages (UIDs 25 to 6). `--limit 0` and `--limit 1001` are usage errors with no connection.
   - Audit: a successful Gmail search appends one line with the same keys, in the same order, as a standard search (`time`, `mandate`, `operation` `search`, `account`, `mailbox`, `messages`, `results`, `outcome` `allowed`). The denied call appends `outcome` `denied` with `reason` `denied`. The AC2 failure appends `outcome` `allowed`, `reason` `unsupported`, and no `messages` or `results`. No audit line contains the query text (for example, the substring `has:attachment` never appears in the log), the Credential or the Agent Token.
4. AC4: A live test, ignored by default and run only explicitly, reads the Gmail test account's address and app password from the developer's environment, and checks LIST, standard search, Gmail search, `\Drafts` detection and that reading leaves messages unread.
   - File and invocation: `tests/gmail_live.rs` holds one `#[ignore = "talks to a real Gmail account; run with `cargo test --test gmail_live -- --ignored`"]` test, `gmail_live_round_trip`. A plain `cargo test` does not run it (it is reported as ignored).
   - Environment: `AGENT_MAIL_LIVE_GMAIL_ADDRESS` (the login) and `AGENT_MAIL_LIVE_GMAIL_APP_PASSWORD` (the Credential).
   - Setup, all in memory: an `InMemoryStore` holds a Mandate for one Account on `imap.gmail.com:993` with `TlsMode::Implicit`. It grants `list`, `search` and `read` on `INBOX` and nothing else. The context is `Context::new(&store).with_audit_log(AuditLog::temporary())`: the platform certificate verifier and a temporary audit log, never the user's data directory. Every call runs in-process through `agent_mail::run` with the token in `AGENT_MAIL_TOKEN`.
   - Checks, in order; each failure names the step:
     (a) `list` exits 0, and its JSON names `INBOX`.
     (b) `search --mailbox INBOX --limit 5` exits 0 with `total` >= 1. Every `ref` parses with `MessageRef::parse` and names `INBOX`. Precondition: INBOX is not empty.
     (c) `search --mailbox INBOX --gmail "in:inbox" --limit 5` exits 0 with `total` >= 1, and every `ref` names `INBOX`. This also shows that Gmail advertises `X-GM-EXT-1` after login.
     (d) `imap::drafts_mailbox` on the same Account with `Network::platform()` returns `Ok(name)`, where `name` is non-empty and starts with `[Gmail]/` or `[Google Mail]/`, so localised names such as `[Gmail]/Entwürfe` pass.
     (e) `search --mailbox INBOX --unseen --limit 1` returns one message, ref R. If it returns none, the test fails with `precondition: INBOX of the live Gmail account needs at least one unread message`. Then `read R` exits 0 with a `ref` equal to R. A second `search --mailbox INBOX --unseen` still lists R, and R's `flags` do not contain `\Seen`.
   - No step creates a draft, sets a flag or moves a message, so the account is unchanged afterwards.
5. AC5: The live test never writes the Credential to disk, output or the repo, and skips with a clear message when its environment is missing.
   - Skip: the environment is read through a helper in `tests/gmail_live.rs`, `live_config(lookup: impl Fn(&str) -> Option<String>) -> Result<LiveConfig, String>`. The helper is covered by ordinary, non-ignored tests in the same file:
     - Both variables unset gives `Err("gmail_live skipped: set AGENT_MAIL_LIVE_GMAIL_ADDRESS and AGENT_MAIL_LIVE_GMAIL_APP_PASSWORD to run it")`.
     - Only the address set, or the password set to `""`, gives an `Err` that names the missing or empty variable `AGENT_MAIL_LIVE_GMAIL_APP_PASSWORD` and does not contain the address's value.
     - Both set (`live@example.test` and `fake-app-password-123`) gives `Ok`, with the login `live@example.test` and the password held as a `Credential`, so `format!("{config:?}")` does not contain `fake-app-password-123`.
     When `live_config` returns `Err`, `gmail_live_round_trip` prints the message to stderr and returns without failing, and makes no connection.
   - No Credential written: in the live run, after each step, stdout and stderr of every `RunOutput` are checked not to contain the app password or the Agent Token *before* any assertion that could print them. Every panic or assert message in the file prints step names and error codes only, never a `RunOutput`, `Mandate`, `Account` or environment value with `{:?}`. At the end, the temporary audit log is read and checked not to contain the app password or the token. The test writes no file except that temporary audit log, and the repo gains no fixture holding a real address or password (the non-ignored tests use only the fake values above).
6. AC6: The README documents `--gmail` and the live test.
   - `README.md`'s Search section lists `--gmail QUERY` in the usage block and explains:
     - The query is passed to Gmail unchanged as its raw search, for example `agent-mail search --gmail "has:attachment newer_than:7d"`.
     - It works only on servers advertising `X-GM-EXT-1` and fails with `unsupported` elsewhere.
     - It cannot be combined with the standard criteria, but works with `--mailbox` and `--limit`.
     - Grants, the limit, the result shape and the audit line are the same as for standard search, and the query is never logged.
   - A developer note says how to run the live test: the two environment variables, the run command, and the preconditions (a dedicated test account with an app password, INBOX not empty, at least one unread message).
   - Checked by a test in `tests/search.rs` that reads the Search section (reusing the existing `readme_section` helper) and asserts it contains `--gmail`, `X-GM-EXT-1` and `unsupported`. The test also asserts that `README.md` contains `AGENT_MAIL_LIVE_GMAIL_ADDRESS`, `AGENT_MAIL_LIVE_GMAIL_APP_PASSWORD` and `cargo test --test gmail_live -- --ignored`. `search --help` lists `--gmail`.

## Out of scope
- Combining `--gmail` with the standard criteria (AND-ing `X-GM-RAW` with `FROM` and others). It is a usage error in this job.
- Gmail-specific result fields (`X-GM-MSGID`, `X-GM-THRID`, `X-GM-LABELS`), thread grouping and label editing.
- Searching across mailboxes (Gmail's `[Gmail]/All Mail`) beyond what an explicit `--mailbox` with a Grant allows.
- Running the live test in CI, and live tests for `flag`, `move` or `draft`. The live test never writes to the account.
- Exposing Drafts detection as an agent-facing command. `imap::drafts_mailbox` is a library function the live test and `draft` use.
- Multiple Accounts per Mandate for search (ticket 13), and the MCP adapter (ticket 15), which will reuse `Query.gmail`.

## Notes for reviewers
- Security:
  - The Gmail query is untrusted Agent input that reaches the IMAP command line. It goes through `SearchText` (no CR, LF or NUL) and `SearchText::quoted`, so it cannot end the quoted string or add keys. AC1 checks this with quote and backslash input.
  - The capability check runs after login and before `EXAMINE`, so a failed `--gmail` opens no mailbox.
  - The query is never written to the audit log or echoed in errors. This matches standard search, which does not log criteria.
  - The live test is the only code here that handles a real Credential. It comes from the environment into a `Credential` (redacted `Debug`, zeroized on drop). It lives only in an `InMemoryStore`. It is checked not to appear in output or in the temporary audit log, and no assert message formats it.
  - Watch for accidental `{:?}` of `RunOutput`, `Mandate` or the config in `tests/gmail_live.rs`. Also watch that async-imap logging stays compiled out (`log` `max_level_off`), since its traces would contain `LOGIN`.
- UX: the user of `--gmail` is an Agent. It chooses `--gmail` when `list` or an earlier error tells it the Account is Gmail. The `unsupported` error's next step points it back to the standard criteria. The live test's user is a developer. They export the two variables for a dedicated test account (one app password, which the README already recommends per Mandate) and run `cargo test --test gmail_live -- --ignored`. Without the variables they get a one-line skip message instead of a failure. A precondition failure (empty INBOX, no unread message) says what to fix in the account.
- UI:
  - Terminal output only. `--gmail` results print exactly as standard search does, JSON by default and the existing text form with `--text` before the subcommand. The new `unsupported` error uses the existing structured error shape and the text error form.
  - `search --help` gains the `--gmail` line. The live test prints only its skip message or its step names.
  - Ticket 12's criteria are carried over unchanged as AC1 to AC5. AC6 was added for the README and help text. "Skips" (ticket AC5) is not native in Rust's test harness, so it is put into practice as print-and-return-without-failing, and the env handling is tested through the `live_config` helper. The live run itself (AC4) can only be checked by a developer with a real account, not in CI.
