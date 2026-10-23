# Agent-readable errors: refused APPEND, JSON usage errors, missing_token with argument mistakes, JSON `mcp` refusal

## Goal
An Agent can act on every error agent-mail reports without parsing free text. After this ships, a draft the server refuses fails with its own `draft_refused` code, which says no draft was created and what to do next. Usage errors on every command are one JSON line with the code `invalid_arguments`, the same code the MCP server uses. An MCP call with no Agent Token and bad arguments reports both problems at once. The in-process refusal of `mcp` is also JSON.

Follow-ups covered: F2, F4, F14, F15 in `factory/follow-ups.md`.

## Context
- Stack: Rust, one binary crate `agent-mail`, clap 4 (derive). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Files this feature touches:
  - `src/imap.rs`: `append` currently maps every refusal (a tagged `NO`/`BAD` before the continuation, or `NO`, `NO [TRYCREATE]` or `BAD` after the literal) to `ImapError::Protocol`. Add a variant, e.g. `ImapError::AppendRefused`, for those refusals only. Read errors, a dropped connection and missing responses stay `Protocol`/`Connection`.
  - `src/core.rs`: new `CoreError::DraftRefused { account, mailbox }` with code `draft_refused`. Map it in `connector_error` for the draft path. Its `ErrorBody` carries `operation`, `account` and `mailbox`.
  - `src/lib.rs`: `run` renders clap parse errors as the JSON usage error, except help and version output. The `CliCommand::Mcp` arm returns the JSON (or `--text`) error shape.
  - `src/mcp.rs`: in `call_tool`, the branch that handles no token plus invalid arguments adds the arguments message to the `missing_token` message.
  - `tests/support/mod.rs`: a scripted APPEND refusal on `FakeImap`. For example, `FakeImap::refuse_append(before_literal: bool, reply: &str)` answers the next `APPEND` with `<tag> <reply>`, either in place of `+ ready` or after reading the literal, and stores nothing.
  - Tests: `tests/draft.rs` (refused APPEND), `tests/cli.rs` (usage errors on every command, `mcp` in-process), `tests/mcp.rs` (missing_token with argument mistakes, `draft_refused` parity), `tests/plugin.rs` (SKILL.md and README wording).
  - Docs: `README.md` (draft errors list, audit `reason` list, usage-error paragraph) and `plugin/skills/agent-mail/SKILL.md` (the errors paragraph at line 49).

## Acceptance criteria

### Refused APPEND (F2)
1. AC1: Drafts mailbox refused with `TRYCREATE`. Set up a fake server with `start_with_list(Mode::Implicit, &["UIDPLUS"], &["Ghost"], Some("Ghost"))`. `Ghost` is listed and marked `\Drafts` but does not exist, so `APPEND` gets `NO [TRYCREATE]`. Then run `agent-mail draft --to carol@example.com --subject Status --body Hi` with a draft Grant:
   - Exit code 1, stdout empty.
   - stderr is one JSON line whose `error` has exactly the keys `account`, `code`, `mailbox`, `message`, `operation`, with `code` = `"draft_refused"`, `operation` = `"draft"`, `account` = the login and `mailbox` = `"Ghost"`.
   - `message` contains `no draft was created` and a next step (it contains `retry` or `ask the human`).
   - `server.appended()` is empty, and `LOGOUT` appears in `server.commands()`.
2. AC2: Other refusals. With the scripted refusal `refuse_append`, each of these fails with the same code `draft_refused`, the same keys, `mailbox` = `"Drafts"` and nothing appended:
   - (a) after the literal: `NO [OVERQUOTA] mailbox full zq7`
   - (b) after the literal: `BAD message rejected zq7`
   - (c) before the literal, in place of `+ ready`: `NO append not allowed zq7`

   In every case the `message` does not contain the server's text (`zq7`).
3. AC3: Audit. The refused draft of AC1 appends exactly one audit line. It names the operation `draft`, has `allowed` true, `reason` `"draft_refused"` and `mailbox` `"Ghost"`, and holds no Agent Token, Credential, recipient, subject or body.
4. AC4: MCP parity. The MCP `draft` tool, called with the same arguments against the AC1 server, returns `isError` true. Its `structuredContent` and text are equal to the CLI's stderr JSON for the same call, and it writes the same audit line apart from `time` (the existing `assert_same_error`/`assert_same_audit` helpers in `tests/mcp.rs`).

### JSON usage errors on every command (F4)
5. AC5: `draft --to` value refused. `agent-mail draft --to not-an-address-zq7 --subject S --body B`:
   - Exit code 2, stdout empty.
   - stderr is exactly one line, which parses as `{"error":{"code":"invalid_arguments","message":<string>}}` (only those two keys).
   - `message` contains `--to`, contains the validator's reason (`must be a bare address local@domain`) and ends with ``run `agent-mail draft --help` for usage``. It does not contain `not-an-address-zq7` and has no line break.
6. AC6: Every command. Each invocation below exits 2 with empty stdout. Its stderr is one JSON line with `code` `invalid_arguments`, whose `message` names the listed argument and ends with the listed help pointer:

   | args | names | help pointer |
   |---|---|---|
   | `list --account ""` | `--account` | `agent-mail list --help` |
   | `search --limit 0` | `--limit` | `agent-mail search --help` |
   | `read not-a-ref` | `REF` | `agent-mail read --help` |
   | `flag <valid ref>` (no `--set`/`--clear`) | `--set` | `agent-mail flag --help` |
   | `move <valid ref>` (no `--to`) | `--to` | `agent-mail move --help` |
   | `draft --to a@b.example --subject S --body B --reply-to <valid ref>` | `--subject` | `agent-mail draft --help` |
   | `whoami --full` | `--full` | `agent-mail whoami --help` |
   | `mandate delete` (no ID) | `<ID>` | `agent-mail mandate delete --help` |
   | `mandate create --bogus` | `--bogus` | `agent-mail mandate create --help` |
   | `mcp --bogus` | `--bogus` | `agent-mail mcp --help` |
   | `lsit` (unknown subcommand) | (none required) | `agent-mail --help` |

   The test also runs the `draft --to` case through the built binary (`CARGO_BIN_EXE_agent-mail`) and checks that its stderr and exit code match the in-process `run`.
7. AC7: Caller-supplied values are never echoed.
   - `list --bogus=hunter2zq7`: the message contains `--bogus` but not `hunter2zq7`, because an unknown option is named only up to its first `=`.
   - `read <valid ref> extra-word-zq7`: the message does not contain `extra-word-zq7`, because unexpected arguments not starting with `-` are not echoed; it says an unexpected argument was given.
   - `search --subject "a\nb-zq7"`: the message names `--subject` and does not contain `zq7`.
8. AC8: Help and version are unchanged. `--help`, `-h`, `--version`, `draft --help` and `help draft` still print clap's plain text on stdout with exit code 0. Running with no arguments, and `mandate` with no subcommand, still print the plain help on stderr with exit code 2, as today. None of these outputs parse as JSON.
9. AC9: No side effects. A usage error reads no store, makes no connection and writes no audit line. Take `draft --to not-an-address --subject S --body B` with a valid Agent Token in `AGENT_MAIL_TOKEN` and the audit log redirected to a temporary file: the store's access count is 0, `server.connections()` is 0 and the audit file has no lines.

### In-process `mcp` refusal (F15)
10. AC10: The in-process `agent_mail::run` with args `["agent-mail", "mcp"]` returns exit code 2 and empty stdout. stderr is the one JSON line `{"error":{"code":"invalid_arguments","message":"`agent-mail mcp` runs as a server on stdin and stdout; start the binary from an MCP host"}}`. With `["agent-mail", "--text", "mcp"]`, stderr is the single line ``error: invalid_arguments: `agent-mail mcp` runs as a server on stdin and stdout; start the binary from an MCP host``.

### missing_token with argument mistakes over MCP (F14)
11. AC11: No token, bad arguments. With no `AGENT_MAIL_TOKEN`, the MCP call `read` with arguments `{}` returns `isError` true and `structuredContent` `{"error":{"code":"missing_token","message":M}}`:
    - M starts with `no agent token presented; set AGENT_MAIL_TOKEN`.
    - M also contains the message the same call returns as `invalid_arguments` when a valid token is present (`missing argument `ref`: it is required`).
    - No audit line is written and no connection is made.
12. AC12: No token, valid arguments. With no `AGENT_MAIL_TOKEN`, the call `read` with `{"ref": <valid ref>}` returns `missing_token` with the message exactly `no agent token presented; set AGENT_MAIL_TOKEN`, unchanged.
13. AC13: Token-like values stay hidden. With no `AGENT_MAIL_TOKEN`, the calls `list` with `{"token": T}` and `draft` with `{"AGENT_MAIL_TOKEN": T}`, where T is a real Agent Token from the store, return `missing_token`. Each message names the unknown argument (`token` or `AGENT_MAIL_TOKEN`), and neither the text content nor `structuredContent` contains T.

### Docs and build
14. AC14: Docs.
    - `README.md` lists `draft_refused` among the `draft` errors and among the audit `reason` examples.
    - `README.md` and `plugin/skills/agent-mail/SKILL.md` both say that a usage error is a JSON error with code `invalid_arguments` and exit code 2.
    - A test in `tests/plugin.rs` checks these strings.
    - Every new or changed paragraph stays at or under about 300 characters.
15. AC15: `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` run green.

## Out of scope
- A `--text` form of clap usage errors. The output mode is unknown when parsing fails, so usage errors are always JSON, even when `--text` is among the arguments. Only the `mcp` refusal (AC10), which parses, honours `--text`.
- A structured `argument` field in `invalid_arguments` errors. The shape stays `code` plus `message`, matching the MCP server.
- Reporting argument mistakes alongside `missing_token` on the CLI. Clap rejects arguments before the token is read, so the CLI already reports the usage error first.
- Retrying or creating the Drafts mailbox after `TRYCREATE`. agent-mail never creates mailboxes.
- Other follow-ups: F22, F24 and F25 (overlong text, root `-h`), F23 (audit test race).

## Notes for reviewers
- Security:
  - Usage-error messages are built from clap's error context, not from clap's rendered text, so they never echo a value the caller passed. The only exception is an unknown option's name, cut at the first `=` (AC7). This matters because draft values are a header-injection surface and an Agent may paste a token by mistake. Today clap's plain text echoes such values, so this tightens current behaviour.
  - The `draft_refused` message never quotes the server's response text, which is untrusted (AC2).
  - The combined `missing_token` message reuses the MCP argument messages, which name argument keys and never values, so an Agent Token passed as an argument is not reflected (AC13).
  - The usage error is produced before any store access, so no Keychain prompt and no audit line (AC9).
- UX:
  - Mostly Agents. They act on `code`:
    - `draft_refused` tells them nothing was created and to retry later or ask the human.
    - `invalid_arguments` now means "fix your call" on both the CLI and MCP, always with exit code 2.
    - `missing_token` over MCP no longer hides a second mistake.
  - Operators running `mandate create`/`delete` with a typo now see a JSON line instead of clap's text. It is readable and points to the right `--help`.
  - The existing `invalid_draft` code stays for the core-level check reached through MCP.
  - Exit codes are unchanged: 2 for usage, 1 for other errors.
- UI: Terminal output only: the stderr error lines described above, plus wording in README.md and SKILL.md. No pages.
