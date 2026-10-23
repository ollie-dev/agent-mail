VERDICT: PASS

## Findings
- [follow-up] plugin/.claude-plugin/plugin.json: no `version`, so Claude Code tracks the git commit. Fine now (the test enforces equality with Cargo.toml if one is added); a later release job may decide whether to pin one.

## Checked
- Read: full `git diff main...factory/018-plugin-readme` (marketplace.json, plugin.json, plugin/.mcp.json, SKILL.md, README.md sections, CLAUDE.md, all of tests/plugin.rs), then build.md.
- Read for the skill's claims: src/create.rs (sandbox refusal of `mandate create`), src/core.rs error codes (login_failed, message_not_found, unsupported_flag, store_unavailable, connection_failed), src/flag.rs (seen/flagged only).
- Ran: `cargo test` in the worktree: every suite ok, tests/plugin.rs 10 passed. `cargo clippy --all-targets -- -D warnings` clean. `cargo fmt --check` clean. `find plugin -type f -perm +111`: nothing executable.
- Criteria: AC1 to AC10 each map to ac1..ac10 tests in tests/plugin.rs. Each asserts on files that did not exist or text that was absent before, so each would fail without the change. AC8 also self-tests its scanners. The `.mcp.json` env holds only `${AGENT_MAIL_TOKEN:-}`; the skill never tells the Agent to run `mandate create/delete` or the installer; the README no longer advises a token in the project's `.mcp.json`; the OS section matches the ADR-0002 statements.
- Not done: no manual `/plugin install` in interactive Claude Code (the builder reports only `claude plugin validate` passing).
