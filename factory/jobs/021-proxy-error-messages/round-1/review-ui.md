VERDICT: PASS

## Findings
- None. The new `proxy_auth_failed` message (about 210 characters) names the variable, says "ask the human" and never shows a value. The `whoami` `--text` lines and help paragraph are short and consistent with the existing output.

## Checked
- Read: the spec, and the diff of `src/`, `README.md` and `plugin/` against main (proxy.rs, imap.rs, core.rs, mcp.rs, whoami text, docs).
- Ran: `agent-mail whoami --text` with no proxy (no proxy line), with `ALL_PROXY=ftp://u:p@127.0.0.1:1` (`proxy ALL_PROXY is not a usable proxy URL; ask the human to fix or unset it`) and with `HTTPS_PROXY=http://u:p@127.0.0.1:1` (`proxy HTTPS_PROXY`). Also `agent-mail whoami --help`: the new paragraph is one short line and the value stays hidden.
