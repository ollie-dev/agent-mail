# 04: Claude Code destination: the plugin

**What to build:** after writing the token to a Claude Code project, the wizard installs the agent-mail Claude Code plugin for that project, pinned to the running binary's release, so the skill and the MCP server match the binary. See `../spec.md`, user stories 43–46, 60, and Further Notes (Q5b).

**Blocked by:** 03

**Status:** ready-for-agent

- [ ] `claude` is looked up on PATH, then in the usual install places (macOS: `~/.local/bin/claude`, `~/.claude/local/claude`, `/opt/homebrew/bin/claude`, `/usr/local/bin/claude`, npm's global bin; Windows: `%USERPROFILE%\.local\bin\claude.exe`, `%APPDATA%\npm\claude.cmd`); the first that runs `claude --version` is used (tests for the order)
- [ ] The marketplace is added at user scope as `ollie-dev/agent-mail#v<binary version>`; when it is already registered at that pin, the add is skipped
- [ ] The plugin is installed with `claude plugin install agent-mail@agent-mail --scope local`, run in the project folder
- [ ] The marketplace is never removed (removing uninstalls its plugins everywhere)
- [ ] When no `claude` is found or a `claude` command fails, the token stays written, and the wizard prints the two commands to run later; the final check shows "plugin: not installed" as a warning, not a failure
- [ ] The final check runs `claude plugin list` and confirms `agent-mail@agent-mail` is enabled
- [ ] The git-risk stop of ticket 03 also skips the plugin step and prints the two commands with the snippet
- [ ] Tests assert the exact commands the test `Machine` recorded
- [ ] `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass

## Comments
