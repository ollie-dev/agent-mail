VERDICT: PASS

## Findings
- [follow-up] src/create.rs:479: the `proxy_refused` line of `mandate create` still says "allow that host in the proxy" and is only reached for allowlist refusals now; fine, but the sandbox-oriented wording differs from the Agent error. A later job could align the human-facing wording.

## Checked
- Read: spec.md, full `git diff main...factory/021-proxy-error-messages` for src/, README.md and SKILL.md (tests only skimmed by name and result).
- Ran: `cargo test` (all suites pass, including the new ac2-ac8, ac14 and unit tests), `cargo clippy -- -D warnings` (clean), `cargo fmt --check` (clean).
- Ran: `agent-mail whoami` and `whoami --text` with ALL_PROXY / HTTPS_PROXY set but no token: missing_token error as before (the proxy line itself is covered by the passing tokens.rs and mcp.rs tests).
- Ran: `whoami --help` shows the new proxy paragraph; `-h` is the one-line summary, like other commands.
- Read the new error message (core.rs): names the variable, says "ask the human", no host, no value, no "allowed domains", under 300 characters. README and SKILL.md bullets match behaviour (proxy_refused narrowed, proxy_auth_failed added, connection_failed for others).
