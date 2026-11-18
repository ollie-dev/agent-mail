# `agent-mail setup` tracer bullet: Mandate step, "I'll configure it myself", final check

## Goal
A human runs `agent-mail setup`, or a bare `agent-mail` in a real terminal before any Mandate exists. They create a Mandate with the familiar `mandate create` guided flow and pick "I'll configure it myself". The wizard then shows the Agent Token once, with ready-to-paste snippets for Claude Code and Claude Desktop, checks the new token as the MCP server would, and says what to do next. This is the end-to-end path that later tickets add destinations to (ticket `.scratch/setup-wizard/issues/02-setup-tracer.md`, feature spec `.scratch/setup-wizard/spec.md`, user stories 3–8, 20–21, 32, 35–36, 59, 61–64).

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- New module `src/setup.rs`: the wizard flow (Mandate step, destination loop, final check, fallback on failure). New integration test file `tests/setup.rs`. Add both to the module and test layout in `CLAUDE.md` (`tests/release.rs` checks that file).
- `src/lib.rs`:
  - add `CliCommand::Setup` with a one-line doc comment;
  - decide what a bare `agent-mail` does: `run` / `parse` today return clap's help with `arg_required_else_help`;
  - route `setup` in `run_cli`.
- `src/main.rs`: the bare call follows the same decision as `agent_mail::run`, since the binary calls `parse` and then `run_cli`.
- `src/create.rs`: the guided flow (`guided`, the refusals, `SANDBOX_INDICATORS`, the Provider note, the export offer) is reused. Factor out a step that seals and stores the Mandate and hands the `AgentToken` back to the wizard instead of revealing it. `mandate create` keeps its exact behaviour and transcript.
- `src/core.rs`: `whoami` (opening the presented tokens as the MCP server does), `CoreError` codes `interactive_terminal_required`, `sandbox_detected` and `aborted`, and `MandateList` for the "store lists no Mandate" check.
- `src/lib.rs` `render_whoami`: the `whoami --text` lines shown by the final check.
- `src/machine.rs`: `Machine::current_exe()` gives the absolute binary path for the Claude Desktop snippet. The install step and the install path arrive in ticket 07.
- `src/terminal.rs`: every prompt and message goes through the Terminal. The token and snippets go through `Terminal::reveal` only.
- Test seam: `agent_mail::run` in-process with `ScriptedTerminal`, `InMemoryStore`, the fake IMAP server in `tests/support/` (`with_route` plus the test TLS roots, as `tests/create.rs` does) and `ScriptedMachine` via `Context::with_machine`. Placeholder Credentials and tokens only.

## Acceptance criteria
Carried over from the ticket in order. Concrete inputs and outputs are added where needed.

1. AC1: `agent-mail setup` exists, with a one-line description in `--help`. `run(["agent-mail", "--help"])` exits 0, and stdout has a line whose trimmed text starts with `setup` and whose description fits on that one line (for example "Set up agent-mail: create a Mandate and connect it to an Agent App"). `setup` is not hidden.
2. AC2: a bare `agent-mail` starts the wizard only when stdin and stdout are terminals, no Agent-session indicator from the `mandate create` list is set, and the store lists no Mandate. Otherwise it prints help exactly as today. One test per condition:
   - (a) interactive `ScriptedTerminal`, empty env, empty `InMemoryStore`: the first transcript entry is the wizard's first prompt (the Mandate name prompt of the guided flow).
   - (b) `ScriptedTerminal::non_interactive()`, empty env, empty store: output equals today's bare-call output. That is a `RunOutput` with empty stdout, stderr holding the `Usage: agent-mail` line and `--help`, and code 2. The transcript is empty.
   - (c) interactive terminal, an empty store, and each name in `SANDBOX_INDICATORS` set (for example `CLAUDECODE=""`): help as in (b), empty transcript.
   - (d) interactive terminal, empty env, a store holding one Mandate: help as in (b), empty transcript, and the store is unchanged.
   - (e) the existing binary tests `bare_call_prints_usage_hint_on_stderr` and `bare_call_is_a_usage_error_with_empty_stdout` in `tests/cli.rs` stay green unchanged.
   - (f) on the Broker route (`Context::with_broker`), a bare call prints help as in (b) and sends no request.
3. AC3: inside an Agent session, `setup` refuses as `mandate create` does, before any prompt, and stores nothing. With any `SANDBOX_INDICATORS` variable set, `run(["agent-mail", "setup"])` exits 1, and stderr is one JSON error line with code `sandbox_detected`, or `error: sandbox_detected: …` with `--text`. The transcript is empty and the store is empty. In the same way, a non-interactive terminal gives `interactive_terminal_required` with an empty transcript and store. On the Broker route, `setup` exits 1 before any prompt and before any request. Its short stderr message points to `agent-mail mandate create` in an elevated terminal until the Windows wizard lands (ticket 08). Nothing is stored.
4. AC4: the Mandate step is the `mandate create` guided flow, unchanged: Provider picker and note before the Credential prompt, live login check, Grants, export offer. `mandate create` itself behaves as before.
   - (a) A `setup` script choosing a Provider that has a `note` (for example Gmail, routed to the fake IMAP server) shows the same prompts in the same order as the matching `mandate create` script, up to and including the export offer. That includes the "Mail provider:" picker, the note as a message before the hidden Credential prompt, the live login against the fake IMAP server, the mailbox and operation checklists, the summary confirm and the export offer. The only difference is that the token reveal entry is missing.
   - (b) After the step, the store holds exactly one Mandate whose Accounts and Grants are those ticked.
   - (c) A wrong Credential followed by Abort ends the run with exit 1, code `aborted`, "Nothing was stored." in the transcript, and an empty store.
   - (d) Every existing test in `tests/create.rs` passes unchanged.
5. AC5: "Connect it where?" offers "I'll configure it myself". The other choices arrive in later tickets. The choice prompt "Connect it where?" has exactly the option `I'll configure it myself`. Picking it reveals one `Reveal` transcript entry containing:
   - the token;
   - the exact Claude Code snippet for `.claude/settings.local.json`: `{"env":{"AGENT_MAIL_TOKEN":"<token>"}}`, pretty-printed or compact as the builder picks, valid JSON either way;
   - the exact Claude Desktop snippet for `claude_desktop_config.json`: `{"mcpServers":{"agent-mail":{"command":"<abs>","args":["mcp"],"env":{"AGENT_MAIL_TOKEN":"<token>"}}}}`. Here `<abs>` is `Machine::current_exe()` (with `ScriptedMachine`, `<root>/downloads/agent-mail`), escaped as a JSON string so Windows backslashes stay valid.

   A test parses each snippet with `serde_json` and compares it to the expected value.
6. AC6: after each destination, the wizard asks "Connect it somewhere else too?" and loops until the answer is no. Script: configure-myself, Yes, configure-myself, No. The transcript shows "Connect it where?" twice and "Connect it somewhere else too?" twice, then the final check. The second configure-myself does not reveal the token again. Instead it shows a message that the token and snippets are shown above, and adds no second next-step line. The confirm's default is No.
7. AC7: the final check opens the Mandate with the new token as the MCP server would and shows its `whoami --text` lines, then one next-step line per destination. After the loop, one message holds exactly the output of `render_whoami` for `whoami` run with `AGENT_MAIL_TOKEN=<new token>`: the `<id>  <name>` line plus its `  account …` and `  grant …` lines. That output equals what `run(["agent-mail", "whoami", "--text"])` prints with the same env and store. It is followed by exactly one next-step message for the configure-myself destination, for example "Paste the snippet into your Agent App's config, restart it, then ask it to list your mailboxes." The run exits 0 with empty stdout and stderr. If the final check cannot open the Mandate (a test store whose reads fail after the put), the wizard shows one short failure message and exits 1. The token is not shown again.
8. AC8: once the Mandate is stored, any later failure or Ctrl-C prints the token once with the snippets. Cancelling there offers to delete the Mandate (default No). A test asserts the token appears exactly once in the transcript on every path, and never in stdout or stderr otherwise. Paths, each with its own test:
   - (a) the happy path of AC5;
   - (b) Cancel at "Connect it where?": one `Reveal` with token and both snippets, then the confirm "Delete this Mandate?" (wording up to the builder) with default No. Accepting the default keeps the Mandate in the store and exits 0.
   - (c) the same as (b), answering Yes: the Mandate is gone from the store, a message says so, and the run exits 1 with code `aborted`.
   - (d) Cancel at "Connect it somewhere else too?" after the reveal: no second reveal, then the delete offer with default No.
   - (e) Cancel at the delete offer counts as No.
   - (f) `ScriptedTerminal::failing_reveal`: the token cannot be shown, so the Mandate is deleted as `mandate create` does, "Nothing was stored." is shown, the run exits 1, and the store is empty.

   On every path, the token string occurs in exactly one transcript entry, that entry is a `Reveal` (or there is none for (f)), and the token occurs in neither `RunOutput.stdout` nor `RunOutput.stderr`. In the wizard, cancelling the export offer behaves as in `mandate create` (no export, continue).
9. AC9: all new messages follow the CLAUDE.md text convention, and no Credential appears anywhere in the output. Across the AC4–AC8 transcripts, every line of every new `Message`, prompt and error has at most 300 characters. The placeholder Credential (`tests/support` `CREDENTIAL`) never occurs in the transcript text, stdout or stderr.
10. AC10: tests drive it in-process with `agent_mail::run`, the scripted terminal, the in-memory store, the fake IMAP server and the test `Machine`. `tests/setup.rs` builds every context with `ScriptedTerminal`, `InMemoryStore`, the fake IMAP route and `ScriptedMachine` (`Context::with_machine`), and calls `agent_mail::run`. No test spawns a process for the wizard or touches the user's home, Keychain or audit log.
11. AC11: `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass.

## Out of scope
- The install and upgrade step, and the install path in the Desktop snippet (tickets 07 and 09). This ticket uses `current_exe()`.
- The Claude Code project destination, git-risk check, token merge, add/replace and delete-old offer (ticket 03), and the Claude Code plugin and marketplace (ticket 04).
- The Claude Desktop destination that writes the config file (ticket 05).
- "Connect an existing Mandate" (ticket 06).
- The Windows elevated child and token hand-back (ticket 08). On the Broker route, `setup` only refuses (AC3) and a bare call prints help (AC2f).
- README, plugin skill and installer hand-off changes (ticket 10). Only `CLAUDE.md`'s layout lines are touched here.
- Any change to `mandate create`'s behaviour, the Mandate model or the Mandate file format.

## Notes for reviewers
- Security:
  - Untrusted input: the wizard is the second place a Credential is typed, and it reuses `create.rs`'s hidden prompt and refusal checks.
  - Mailbox access: the live login check is the only mailbox access.
  - Ordering: the Agent-session and non-terminal refusals must run before any prompt or store access.
  - Token visibility: the Agent Token must only reach `Terminal::reveal` (the controlling terminal), never stdout, stderr or a `Message`. Check that the refactor of `create::run` doesn't leave a second reveal and doesn't change `mandate create`'s transcript.
  - Snippet text: the Desktop `command` path is put into JSON, so it must be JSON-escaped, not formatted by hand.
  - Broker route: on Windows the in-process store must never be used by `setup`. That is why it refuses on the Broker route.
- Ticket wording:
  - "Token appears exactly once" next to "prints the token once with the `env` snippet and the `mcpServers` snippet" is contradictory if read literally, since both snippets contain the token. This spec reads it as exactly one transcript entry (one `Reveal`) containing the token, which holds the token line and both snippets. If the reviewers want a single literal occurrence, the snippets would need a placeholder and would no longer be ready to paste.
  - "Cancelling there offers to delete the Mandate" is read as: any Cancel after the Mandate is stored. Cancelling the export offer is kept as `mandate create` has it (treated as No) so the Mandate step stays unchanged.
  - Exit codes chosen here: 0 when the wizard ends with the Mandate kept, including after a cancel answered No; 1 with `aborted` when the Mandate was deleted or never stored.
- UX: the operator (the human) runs this in their own terminal. The flow is name → Provider picker (and note) → Credential → live login → Grants → summary → store → export offer → "Connect it where?" → reveal with snippets → "Connect it somewhere else too?" → final check → next step. An Agent can't drive it: it is refused in sessions, and a bare call prints help when not on a terminal.
- UI: terminal-only. Prompts and messages go through the Terminal (inquire on stderr, messages and the token on `/dev/tty` or `CONOUT$`), and `--help` gains one `setup` line. stdout and stderr stay empty on success and carry only the usual one-line errors on failure.
