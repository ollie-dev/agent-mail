# 05: Claude Desktop destination

**What to build:** "Connect it where?" gains "Claude Desktop". The wizard adds one `agent-mail` MCP server entry with the token to `claude_desktop_config.json`, after backing the file up, and tells the human to restart Claude Desktop. See `../spec.md`, user stories 48–58.

**Blocked by:** 03 (reuses the token add/replace handling)

**Status:** ready-for-agent

- [ ] Config location: `~/Library/Application Support/Claude/claude_desktop_config.json` on macOS, `%APPDATA%\Claude\claude_desktop_config.json` on Windows, plus the Microsoft Store package's redirected location when present. Confirm on Windows whether the Store version redirects it and record the finding as a comment on this ticket
- [ ] Before any change, `claude_desktop_config.json.bak` is written with the old content
- [ ] The `mcpServers.agent-mail` entry gets `command` (the installed binary's absolute path), `args` `["mcp"]` and `env.AGENT_MAIL_TOKEN`; every other key and server is kept, in order
- [ ] A missing file (and folder) is created
- [ ] A file that is not valid JSON is left untouched and the wizard falls back to "configure it myself"
- [ ] An existing token in the entry gets the same add/replace/delete-offer handling as ticket 03
- [ ] The final check reads the token back from the file and opens the Mandate with it
- [ ] Next step: "Quit and restart Claude Desktop, then ask it to list your mailboxes."
- [ ] `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass

## Comments
