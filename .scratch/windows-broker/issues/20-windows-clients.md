# 20: CLI and MCP server as Broker clients on Windows

**What to build:** on Windows, every CLI command and every `agent-mail mcp` tool call is forwarded to the Broker (ADR-0004) instead of running in-process; the client only parses arguments, does terminal I/O and renders the reply, so output, `--text` lines and MCP results stay exactly as they are. `mandate create` must run elevated. A new `agent-mail audit` command shows the caller's audit entries on both macOS and Windows. macOS keeps running everything in-process.

**Blocked by:** 19

**Status:** done

- [ ] On Windows, all Agent commands and `mandate list`/`delete` go through the pipe, with results and errors identical to the in-process ones (shared tests compare them)
- [ ] The MCP server forwards each tool call to the Broker and renders results as today
- [ ] `mandate create` and `mandate create --from` on Windows refuse to start when not elevated, with a short error saying to rerun from an elevated terminal; when elevated, the client runs the guided flow, sends the Credential to the Broker, shows the returned Agent Token via `CONOUT$` and makes the export offer
- [ ] A client that cannot reach the pipe fails with `broker_unavailable`, and a version mismatch with `broker_version_mismatch`; both messages name the fix (start the service, rerun `install.ps1`)
- [ ] `agent-mail audit` prints the caller's audit entries, as JSON by default and with `--text`, on macOS from the user's own log and on Windows from the Broker
- [ ] Check whether an Agent in the Claude Code sandbox on Windows can open the named pipe, and record the result in the README's "Sandbox and proxies" section
- [ ] Help text follows the CLAUDE.md convention
