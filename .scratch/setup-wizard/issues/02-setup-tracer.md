# 02: `agent-mail setup` tracer bullet: Mandate step, "I'll configure it myself", final check

**What to build:** a human runs `agent-mail setup` (or a bare `agent-mail` with no Mandate yet), creates a Mandate with the familiar guided flow, picks "I'll configure it myself", and gets the Agent Token once with ready-to-paste snippets for Claude Code and Claude Desktop, then a final check and the next step. This is the end-to-end path every later ticket adds destinations to. See `../spec.md`, user stories 3–8, 20–21, 32, 35–36, 59, 61–64.

**Blocked by:** 01

**Status:** done

- [ ] `agent-mail setup` exists, with a one-line description in `--help`
- [ ] A bare `agent-mail` starts the wizard only when stdin and stdout are terminals, no Agent-session indicator from the `mandate create` list is set and the store lists no Mandate; otherwise it prints help exactly as today (tests for each condition)
- [ ] Inside an Agent session `setup` refuses as `mandate create` does, before any prompt, and stores nothing
- [ ] The Mandate step is the `mandate create` guided flow, unchanged: Provider picker and note before the Credential prompt, live login check, Grants, export offer; `mandate create` itself behaves as before
- [ ] "Connect it where?" offers "I'll configure it myself" (the other choices arrive in later tickets); it prints the token once with the exact `env` snippet for `.claude/settings.local.json` and the `mcpServers` snippet for `claude_desktop_config.json` (`command` the binary's absolute path, `args` `["mcp"]`)
- [ ] After each destination the wizard asks "Connect it somewhere else too?" and loops until the answer is no
- [ ] The final check opens the Mandate with the new token as the MCP server would and shows its `whoami --text` lines, then one next-step line per destination
- [ ] Once the Mandate is stored, any later failure or Ctrl-C prints the token once with the snippets; cancelling there offers to delete the Mandate (default No). A test asserts the token appears exactly once in the transcript on every path and never in stdout or stderr otherwise
- [ ] All new messages follow the CLAUDE.md text convention; no Credential appears anywhere in the output
- [ ] Tests drive it in-process with `agent_mail::run`, the scripted terminal, the in-memory store, the fake IMAP server and the test `Machine`
- [ ] `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass

## Comments
