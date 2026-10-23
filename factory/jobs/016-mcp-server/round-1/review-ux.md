VERDICT: PASS

## Findings
- [follow-up] src/mcp.rs:444: with no token and invalid arguments the call returns `missing_token` (spec-mandated by AC4), so an Agent that also made an argument mistake only learns about it after the human fixes the token; acceptable, but a later job could mention both in the message.
- [follow-up] src/lib.rs (run, Mcp arm): in-process `run` refuses `mcp` with a plain-text stderr line; only reachable from tests, no user impact.

## Checked
- Read: spec.md, build.md, full diff (src/mcp.rs in full, src/lib.rs, src/main.rs, src/core.rs, README.md MCP section, CLAUDE.md).
- Ran (worktree binary): `agent-mail mcp --help` exits 0 and says Mandate management is not offered over MCP; `agent-mail --help` lists `mcp`; `agent-mail mcp extra` gives a clap usage error, exit 2; piped `initialize` then `list {"x":1}` and `list {}` with AGENT_MAIL_TOKEN unset: initialize answered with instructions pointing to whoami, both calls returned a tool error `missing_token` with matching content text and structuredContent, one JSON object per line.
- Tool descriptions and schemas reviewed against the CLI options and spec AC1; error messages for invalid_arguments name the argument and what to give. README matches behaviour (config snippets, seven tools, error codes, token source).
