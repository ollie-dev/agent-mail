VERDICT: PASS

## Findings
- [follow-up] src/lib.rs (Mcp doc comment): the `mcp --help` long text is one dense paragraph of about 700 characters; clap wraps it, but splitting it into short paragraphs (host setup, tools, errors) would scan better. A later job may reword it.

## Checked
- Read: `git diff main...factory/016-mcp-server` for README.md, src/lib.rs and src/main.rs (the rendered surfaces); skimmed the stat for src/mcp.rs and tests.
- Ran: `agent-mail --help` (the `mcp` row is aligned and in the same style as the other rows); `agent-mail mcp -h` (one-line summary, usage, options) and `agent-mail mcp --help` (long text, usage, options; both read well at 80 columns).
- Ran: a piped session with `initialize`, `tools/list`, a `tools/call` with no token, and a garbage line. Stdout held only one JSON object per line. The error result carried `isError` true and the CLI-shaped `{"error":{"code":"missing_token",...}}`. The garbage line got a JSON-RPC -32700 error. Output has no colour, so `NO_COLOR` makes no difference.
- Read: the README "MCP server" section. It matches the existing README style and gives config snippets, the tools, the result and error shapes, and the token rule.
