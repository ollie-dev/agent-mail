# MCP server with all Agent commands

## Goal
A host app (Claude Desktop, Cowork, or Claude Code's MCP config) can start `agent-mail mcp` as an MCP server over stdio, with the Agent Token in the server process's environment. The Agent then calls `whoami`, `list`, `search`, `read`, `flag`, `move` and `draft` as MCP tools. Each tool behaves exactly like the matching CLI command: the same result structures, the same error codes and the same audit entries. Mandate management stays CLI-only.

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Ticket: `.scratch/agent-mail-v1/issues/15-mcp-server.md` (user stories 49 and 64 of `.scratch/agent-mail-v1/spec.md`). The spec says: "The CLI and MCP adapters are thin translations into and out of [the core]". It also says: "MCP tools mirror the agent commands (`whoami`, `list`, `search`, `read`, `flag`, `move`, `draft`) and return the same structures. Mandate management is CLI-only." ADR-0002: the MCP server is a process the host app starts, and it enforces the Policy in-process on every call, like the CLI does.
- Transport and protocol: MCP over stdio. Messages are newline-delimited JSON-RPC 2.0, one JSON object per line, read from stdin and written to stdout. Requests are handled one at a time, in order. Only JSON-RPC messages go to stdout; diagnostics, if any, go to stderr. Supported methods:
  - `initialize`
  - `notifications/initialized` (a notification, so no reply)
  - `ping`
  - `tools/list`
  - `tools/call`
- Implementation choice: a small hand-written, synchronous JSON-RPC loop in a new `src/mcp.rs`, using the existing `serde_json`, with no new dependency. Reason: the core is blocking (`imap.rs` builds its own tokio runtime and calls `block_on`, which panics inside another runtime). `Context<'a>` borrows a non-`Send` `&dyn MandateStore`, so an async framework such as `rmcp` would need `spawn_blocking` plus a restructured `Context`. The builder may use `rmcp` instead only if every criterion below still holds and the entry point keeps the shape given next.
- Entry point for tests: `agent_mail::mcp::serve(input: impl BufRead, output: impl Write, env: &HashMap<String, String>, context: &Context<'_>) -> std::io::Result<()>`. It runs until EOF on `input`, then returns `Ok(())`. The builder may adjust the exact signature, but tests must be able to drive a whole session in-process with an in-memory input and output. `env` is the server process's environment, captured once at start. It is the only source of `AGENT_MAIL_TOKEN` and of the proxy variables.
- Tool mapping: each `tools/call` builds the same `core::Command` the CLI builds and runs it through `core::execute_reporting(env, &command, context)`. Argument checks reuse the CLI's validators in `src/lib.rs`: `account_login`, `reference`, `search_text`, `gmail_query`, `date`, `limit`, `address`, `subject`, `body` and `mailbox_name`. Move these into a shared place if needed, so the CLI and MCP checks cannot drift apart. Tools have no `text` output option: results are always JSON.
- Tool results:
  - Success: `{"content":[{"type":"text","text":<J>}],"structuredContent":<O>,"isError":false}`. `O` is the result object and `J` is exactly the CLI's JSON stdout line for the same command, without its trailing `\n`. `O` is `J` parsed.
  - With invalid tokens: when the core reports invalid-token positions, `content` gets a second text item. That item is exactly the CLI's stderr warning line `{"warning":{"code":"invalid_token",...}}`, without its `\n`.
  - Error: `{"content":[{"type":"text","text":<E>}],"structuredContent":<EO>,"isError":true}`. `E` is exactly the CLI's JSON stderr error line `{"error":{...}}`, without its `\n`, and `EO` is `E` parsed. The warning item, if any, comes first, as on the CLI's stderr.
- Files this feature touches:
  - `src/mcp.rs` (new): the JSON-RPC loop, the tool definitions with their input schemas, argument parsing into `core::Command`, and rendering of results.
  - `src/lib.rs`:
    - `pub mod mcp`.
    - A new `mcp` subcommand: "Run as an MCP server over stdio for a host app". It takes no options besides `--help`.
    - Shared argument validators, and shared JSON rendering of outcomes, errors and warnings, so both adapters print identical JSON.
  - `src/main.rs`: when the parsed command is `mcp`, call `mcp::serve` with locked stdin and stdout, the process environment and the same `Context` as today, then exit 0. Every other command goes through `run` as today.
  - `tests/mcp.rs` (new). `tests/support/mod.rs` gains a helper that feeds a list of JSON-RPC lines to `mcp::serve` with a given `AGENT_MAIL_TOKEN`, store, fake-server CA and audit log path, and returns the parsed response lines.
  - `README.md`: a new "MCP server" section. `CLAUDE.md`: add `src/mcp.rs` and `tests/mcp.rs` to the layout list.
- Fixtures used below:
  - Mandate W, short ID `aaaa1111`, covers `agent@example.test` (`LOGIN`) on the fake server and grants `list`, `search`, `read`, `flag`, `move` and `draft` on `*`. `<tW>` is its token.
  - `<REF>` is `agent@example.test/INBOX/<INBOX_UIDVALIDITY>/1`.
  - "Session" means: an `initialize` request, then `notifications/initialized`, then the requests listed.

## Acceptance criteria
1. AC1: Listing tools returns exactly `whoami`, `list`, `search`, `read`, `flag`, `move` and `draft`, with input schemas matching the CLI options.
   - Input: a session with `{"jsonrpc":"2.0","id":2,"method":"tools/list"}`. Expected: `result.tools` holds exactly seven tools, named `whoami`, `list`, `search`, `read`, `flag`, `move` and `draft`, in this order. Each tool has a non-empty `description` and an `inputSchema` with `"type":"object"` and `"additionalProperties":false`.
   - Each schema's `properties` key set and `required` set (absent or empty means none required) are exactly:
     - `whoami`: properties none; required none.
     - `list`: `account` (string); required none.
     - `search`: `account`, `mailbox`, `from`, `to`, `subject`, `text`, `since`, `before`, `gmail` (strings), `unseen`, `flagged` (booleans) and `limit` (integer, `minimum` 1, `maximum` 1000, `default` 20); `mailbox` has `default` `"INBOX"`; required none.
     - `read`: `ref` (string), `full` (boolean); required `ref`.
     - `flag`: `ref`, `set`, `clear` (strings); required `ref`.
     - `move`: `ref`, `to` (strings); required `ref`, `to`.
     - `draft`: `to` (array of strings, `minItems` 1), `subject`, `body`, `reply_to`, `account` (strings); required `to`, `body`.
   - No property is named `token`, `text_output` or anything that selects output format. In `search`, `text` is the TEXT criterion, as on the CLI.
   - Argument rules match the CLI. Each of these `tools/call` inputs returns an error result (`isError` true) whose `structuredContent` is `{"error":{"code":"invalid_arguments","message":...}}`. The message names the offending argument. There is no connection (`server.connections() == 0`) and no audit line, as for a CLI usage error:
     - `flag` with both `set` and `clear`, and with neither.
     - `draft` with both `subject` and `reply_to`, with neither, and with both `account` and `reply_to`.
     - `search` with `gmail` plus `from`.
     - `search` with `limit` 0, 1001 or `"20"` (a string).
     - `search` with `since` `"2026-13-01"`.
     - `read` with `ref` `"not-a-ref"` or with no `ref`.
     - `move` with `to` `""`.
     - `list` with `account` `"a\nb"`.
     - `draft` with `to` `["Bob <bob@example.com>"]`.
     - Any tool given an unknown property, such as `{"verbose":true}`.
2. AC2: Each tool call returns the same structure as the equivalent CLI command's JSON output.
   - For each pair below, run the CLI command with `agent_mail::run` and the tool call with `mcp::serve`. Use separate fresh fake servers with the same fixtures and the same Mandate W. Expected: the tool result has `isError` false, and its `content[0].text` equals the CLI stdout with the trailing `\n` removed. `structuredContent` equals that JSON parsed. Fields that hold the server's port are compared after replacing the port.
     - `whoami` / `whoami {}`.
     - `list` / `list {}`.
     - `search --mailbox INBOX --limit 2` / `search {"mailbox":"INBOX","limit":2}`.
     - `search` with no options / `search {}`, which defaults to `INBOX` and limit 20.
     - `read <REF>` / `read {"ref":"<REF>"}`. Also with `--full` / `"full":true` on the long-body fixture, where `truncated` is false.
     - `flag <ref in Triage> --set seen` / `flag {"ref":...,"set":"seen"}`.
     - `move <REF> --to Archive` / `move {"ref":"<REF>","to":"Archive"}`.
     - `draft --to carol@example.com --subject S --body B` / `draft {"to":["carol@example.com"],"subject":"S","body":"B"}`.
     - `draft --reply-to <ref in Thread> --to carol@example.com --body B` / `draft {"reply_to":...,"to":["carol@example.com"],"body":"B"}`.
   - The fake server state after each mutating tool call matches the state after the CLI call: the message's flags after `flag`, the source and target contents after `move`, and the appended message's flags and headers, except Date and Message-ID, after `draft`.
   - Invalid-token warning: with `AGENT_MAIL_TOKEN` `garbage,<tW>`, `list {}` succeeds with `isError` false. `content[1].text` equals the CLI's stderr warning line `{"warning":{"code":"invalid_token","positions":[1],"message":...}}`, without its `\n`. With only `<tW>`, `content` has exactly one item.
3. AC3: Denials and errors are returned as structured tool errors with the same codes as the CLI.
   - For each case, compare the tool result with the CLI run of the same command. Expected: the tool result has `isError` true. Its `content[0].text` (or `content[1].text` when a warning comes first) equals the CLI's stderr error line, without its `\n`. Its `structuredContent` equals that JSON parsed. The response is a JSON-RPC `result`, not a JSON-RPC `error`. Cases:
     - `missing_token`: `AGENT_MAIL_TOKEN` unset, `list {}`.
     - `invalid_token`: `AGENT_MAIL_TOKEN` `garbage`, `list {}`.
     - `denied`: a Mandate granting only `list` on `INBOX`, then `search {"mailbox":"Archive"}`. Expected `operation` `search`, `account` `agent@example.test`, `mailbox` `Archive`.
     - `account_required`: a Mandate covering `agent@example.test` and `other@example.test`, then `list {}`. Expected `accounts` with both logins.
     - `ambiguous_mandate`: two Mandates covering `agent@example.test`, then `list {}`.
     - `stale_reference`: `read` with `<REF>` whose UIDVALIDITY is off by one.
     - `unsupported_flag`: `flag {"ref":...,"set":"answered"}`.
     - `same_mailbox`: `move {"ref":"<REF>","to":"INBOX"}`.
     - `unsupported`: `search {"gmail":"has:attachment"}` on a server without `X-GM-EXT-1`.
     - `drafts_not_found`: `draft` on a server with no `\Drafts` mailbox.
     - Warning before error: with `garbage,<tW>`, `list {"account":"other@example.test"}`. Expected: `content[0]` is the warning and `content[1]` is the `denied` error.
   - Protocol-level failures use JSON-RPC errors, not tool errors: an unknown tool name gives JSON-RPC error code `-32602` (see AC5).
4. AC4: The token is read from the server process's environment; tool inputs cannot supply or override it.
   - Setup: Mandate W, and Mandate P (`bbbb2222`) covering only `other@example.test` with `list` on `Archive`. The server's `env` holds `AGENT_MAIL_TOKEN` `<tW>` only.
   - `list {"token":"<tP>"}` and `list {"AGENT_MAIL_TOKEN":"<tP>"}` each return `invalid_arguments` (unknown property). They make no connection and write no audit line.
   - A `tools/call` whose `params._meta` is `{"AGENT_MAIL_TOKEN":"<tP>"}`, calling `list {}`, returns W's result `{"account":"agent@example.test","mailboxes":[...]}`, and its audit line has `mandate` `aaaa1111`. `_meta` is ignored for token purposes.
   - With `AGENT_MAIL_TOKEN` unset in `env`, every tool, including `whoami {}`, returns `missing_token`, whatever the arguments hold.
   - The token is captured when the server starts. A session where the test changes nothing but sends two `list {}` calls gets W's result both times.
   - No secret leaves: across a whole session that calls every tool, plus one denial and one error, the bytes written to `output` contain neither `<tW>`, `<tP>`, the fake Credential (`CREDENTIAL`) nor any token hash.
5. AC5: No tool creates, lists or deletes Mandates.
   - `tools/list` names no tool containing `mandate`, `create` or `delete` (covered by AC1's exact list).
   - `tools/call` with each name `mandate`, `mandate_create`, `mandate_list`, `mandate_delete`, `create` and `delete` returns a JSON-RPC error with code `-32602` and a message naming the unknown tool. The message points to `tools/list` and says Mandates are managed with the `agent-mail mandate` CLI in the human's terminal.
   - After those calls the in-memory store holds exactly the records it held before. No audit line is written, and no Terminal prompt is issued: a scripted terminal given to the `Context` records zero prompts.
   - `agent-mail mcp --help` exits 0 and its text says Mandate management is not offered over MCP.
6. AC6: Each tool call produces the same audit entry as the equivalent CLI command.
   - For each CLI/tool pair in AC2 (other than `whoami`) and each error case in AC3, run the CLI with a fresh audit log and the MCP session with another fresh audit log. Use separate fresh fake servers and Mandates with the same short IDs. Expected: each run appends exactly one line. The two lines are equal once the `time` key is removed, with the same keys, values and key order.
   - `whoami {}` appends no audit line, matching the CLI.
   - `initialize`, `ping`, `tools/list`, `invalid_arguments` results and unknown-tool errors append no audit line.
   - An `audit_unavailable` failure behaves as on the CLI: with the audit log path pointing at a directory, `list {}` returns a tool error with code `audit_unavailable` and no result.
7. AC7: The server speaks MCP over stdio and survives bad input.
   - `initialize` with `params.protocolVersion` `"2025-06-18"` returns:
     `{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"agent-mail","version":<CARGO_PKG_VERSION>}}}`
     A result may also carry an `instructions` string that tells the Agent to start with `whoami`. A client requesting `"2025-03-26"` gets that version back. Any other requested version gets `"2025-06-18"`.
   - `notifications/initialized` produces no output line. `ping` with id 7 returns `{"jsonrpc":"2.0","id":7,"result":{}}`.
   - An unknown method returns error code `-32601`, with the request's id.
   - A line that is not JSON returns error code `-32700` with `"id":null`, and the next valid request on the following line is still answered.
   - `tools/call` without `params.name` returns `-32602`.
   - Response ids echo request ids, both numbers and strings. Every output line is a single JSON object followed by `\n`, and nothing else is written to `output`.
   - EOF on `input` makes `serve` return `Ok(())`.
   - Binary wiring: spawn the built binary (`env!("CARGO_BIN_EXE_agent-mail")`) with the arguments `mcp`. Write the `initialize` and `tools/list` lines to its stdin and close it. Expected: exit code 0, two JSON response lines on stdout, and the seven tool names.
8. AC8: README, help and project docs describe the MCP server, and the suite runs green.
   - `README.md` gains an "MCP server" section. It shows a Claude Desktop `claude_desktop_config.json` entry (`"command": "agent-mail"`, `"args": ["mcp"]`, `"env": {"AGENT_MAIL_TOKEN": "<your token>"}`) and a Claude Code `.mcp.json` / `claude mcp add` equivalent. It lists the seven tools and says they return the same JSON as the CLI. It explains that errors are tool errors with the CLI's codes plus `invalid_arguments`, that the token comes only from the server's environment, and that Mandate management is CLI-only.
   - A test asserts that `README.md` contains `agent-mail mcp`, `AGENT_MAIL_TOKEN`, `invalid_arguments` and each of the seven tool names. It also asserts that `agent-mail --help` lists `mcp`.
   - `CLAUDE.md` lists `src/mcp.rs` and `tests/mcp.rs`.
   - `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass.

## Out of scope
- MCP resources, prompts, sampling, logging, progress notifications, cancellation and pagination of `tools/list`.
- Streamable HTTP or SSE transports; stdio only.
- Handling requests in parallel; requests are processed one at a time.
- A `text` output mode for tools; MCP results are always JSON.
- The Claude Code plugin with its skill and MCP entry (user story 61), and a `.mcpb` package for Claude Desktop.
- Any tool for `mandate create`, `mandate list` or `mandate delete`.
- An audit line for `whoami`, which the CLI also does not write.
- Re-reading `AGENT_MAIL_TOKEN` during a session. A new token needs a server restart by the host app.

## Notes for reviewers
- Security:
  - The MCP server is a long-lived process holding the Agent Tokens from its environment. Every `tools/call` must go through `core::execute_reporting`, so Mandate selection, Grant checks and the audit entry happen per call, exactly as on the CLI. Nothing opened (Mandates, Credentials) may be cached across calls in the adapter.
  - Tool arguments are untrusted Agent input, and possibly prompt-injected. `additionalProperties: false` plus strict parsing means no argument can name a token, a store or a Mandate (AC4). `_meta` and any other JSON-RPC field are never read as a token source.
  - The arguments must pass the same validators as the CLI before any `Command` is built. Watch for type confusion (strings where integers or booleans are expected, arrays where strings are expected) and for line breaks or NUL in account, mailbox, subject and reference values.
  - JSON-RPC error messages for unknown tools or methods echo the requested name. Escape control characters, or cap the name's length, so a hostile client cannot write arbitrary bytes back. The output is JSON-encoded anyway.
  - stdout is the protocol channel. A stray `println!` or log line would corrupt it. `log` is compiled out (`max_level_off`), which keeps async-imap's LOGIN trace out.
  - Mandate management is excluded by construction: the adapter never builds `Command::MandateCreate`, `MandateList` or `MandateDelete`, and `serve` has no Terminal use (AC5). This matters because `mandate create` must never be drivable by an Agent.
  - The ticket's dependency list names `rmcp`. This spec prefers a hand-written loop with no new crate, for the runtime and `Send` reasons given in Context. Reviewers may challenge this choice.
- UX:
  - The user is an Agent in Claude Desktop, Cowork or Claude Code. The human adds `agent-mail mcp` with `AGENT_MAIL_TOKEN` in the host's MCP `env`. The Agent then follows this flow:
    1. Call `whoami` to see its Mandates and Grants.
    2. Call `list`.
    3. Call `search` and take `ref` values from the results.
    4. Call `read`, `flag`, `move` or `draft` with those `ref` values.
  - Argument names mirror the CLI options: `reply_to` for `--reply-to`, and `ref` for the positional REF. The CLI docs therefore carry over. Tool descriptions should reuse the CLI help text and say where `ref` comes from.
  - `invalid_arguments` is a new, MCP-only error code. The CLI reports usage errors through clap with exit code 2 instead. Ticket AC3 ("same codes as the CLI") is put into practice for every core error. Argument errors are the one place where the CLI has no JSON code to copy.
  - Ticket criteria 1 to 6 are carried over unchanged as AC1 to AC6. AC7 (protocol behaviour) and AC8 (docs and green suite) were added.
- UI:
  - No new terminal UI beyond the `mcp` subcommand and its `--help` text.
  - The rendered surface is the JSON-RPC stream on stdout: the tool list with its schemas, tool results whose `content[0].text` is the CLI's JSON line, and `structuredContent` for hosts that read it.
