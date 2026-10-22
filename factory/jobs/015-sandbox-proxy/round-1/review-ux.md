VERDICT: PASS

## Findings
- [follow-up] src/core.rs (execute_reporting): an invalid proxy variable is only reported when a command actually connects, so `agent-mail whoami` and `mandate list` never flag it. That fits "nothing connected", but a later job could add a hint in `whoami` if agents get confused by it.

## Checked
- Read: the full diff of main...factory/015-sandbox-proxy for README.md, CLAUDE.md, src/core.rs, src/create.rs and src/imap.rs. Read src/proxy.rs lines 1-200 (URL parsing, redacting Debug, variable selection). Read spec.md.
- Ran: `cargo test --test proxy` in the worktree: 15 passed. These cover AC1 to AC6 (CONNECT, SOCKS5, credentials not shown, refusal, direct mode, built-in roots).
- Ran: `HTTPS_PROXY=ftp://u:p@x agent-mail list` with no token: stops at `missing_token`, so the proxy value is not echoed. Ran `agent-mail --help`: unchanged, no proxy flags are needed.
- Checked the new error messages. `proxy_refused` carries `account` and `host`, names host:port, and tells the Agent to ask the human to allow the host in the sandbox's allowed domains. `invalid_proxy` names the variable, does not echo the value, and says to fix or unset it. The `mandate create` login check gives matching reasons. The README section "Sandbox and proxies" matches this behaviour and lists both new codes.
