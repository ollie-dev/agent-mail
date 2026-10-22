VERDICT: PASS

## Findings
- [follow-up] src/core.rs:~700: the `proxy_refused` message is a single very long line (about 300 characters), like the other long messages in this file; a later job could shorten the error messages across the file.

## Checked
- Read: spec.md, the diff of README.md and src/core.rs (new error variants, `host` field, codes, message text, `--text` fallthrough), the README "Sandbox and proxies" section.
- Ran: built the worktree and ran `HTTPS_PROXY=ftp://u:p@127.0.0.1:1 agent-mail list` with and without `--text`. With no token, it stops at `missing_token` before the proxy is looked at, in both JSON and `error: code: message` form. So I did not see the proxy errors rendered by the binary. Their shape comes from the code: JSON `code`, `account`, `host` (skipped when absent) and `message`, naming `host:port` and the allowed-domains action. The `--text` form is the existing generic `error: <code>: <message>` line.
- The `invalid_proxy` message names the variable and does not echo the URL value. The shape matches the existing errors.
