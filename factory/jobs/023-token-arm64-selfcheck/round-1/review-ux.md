VERDICT: PASS

## Findings
- [follow-up] README.md (MCP server section): the first `claude mcp add` paragraph is about 250 characters and the "from the environment" paragraph is near 300; fine for this job, but the README paragraph-length pass (F22) should look at them.

## Checked
- Read: spec.md, the full diff of factory/023-token-arm64-selfcheck against main (README.md, install.sh, tests/plugin.rs, tests/release.rs, tests/selfcheck.rs), and tests/selfcheck.rs lines 478-500.
- Ran: `cargo test --test plugin --test release`: all 36 pass, including the job023 AC1-AC7 tests (fake-uname arm64 and x86_64 install their targets, aarch64 gives "unsupported architecture aarch64" with nothing installed). `cargo clippy -- -D warnings` clean. `cargo fmt --check` clean. `grep aarch64 install.sh`: only the arm64 arm and its target remain.
- README flow: the MCP server section shows `claude mcp add agent-mail --scope local --env AGENT_MAIL_TOKEN=<your agent token> -- agent-mail mcp` with prose on local scope, a `.mcp.json` with `${AGENT_MAIL_TOKEN}` only, the token source (`.claude/settings.local.json` env, linked), and a "Never put a literal token in `.mcp.json` or any other file you commit." sentence. Only placeholders appear. The Release-binary tests paragraph now says nothing is written to the audit log.
- selfcheck loop is exactly whoami, mandate list, mandate delete deadbeef; the audit comment is gone.
