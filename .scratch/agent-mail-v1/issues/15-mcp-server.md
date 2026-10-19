# 15: MCP server with all Agent commands

**What to build:** `agent-mail mcp` runs as an MCP server over stdio, started by the host app (Claude Desktop, Cowork, or Claude Code's MCP config). It offers the Agent commands as tools (`whoami`, `list`, `search`, `read`, `flag`, `move`, `draft`) returning the same structures as the CLI's JSON. The Agent Token comes from the server process's environment. Mandate management is not exposed. Tested in-process as a thin adapter over the core.

**Blocked by:** 08, 09, 10, 11, 13

**Status:** ready-for-agent

- [ ] Listing tools returns exactly `whoami`, `list`, `search`, `read`, `flag`, `move` and `draft`, with input schemas matching the CLI options
- [ ] Each tool call returns the same structure as the equivalent CLI command's JSON output
- [ ] Denials and errors are returned as structured tool errors with the same codes as the CLI
- [ ] The token is read from the server process's environment; tool inputs cannot supply or override it
- [ ] No tool creates, lists or deletes Mandates
- [ ] Each tool call produces the same audit entry as the equivalent CLI command
