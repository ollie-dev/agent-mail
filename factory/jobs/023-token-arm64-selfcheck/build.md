## Round 1
**Changed:**
- `README.md` `## MCP server`: `claude mcp add agent-mail --scope local --env AGENT_MAIL_TOKEN=<your agent token> -- agent-mail mcp`, with prose that the local scope stores the entry only for you, in this project, outside the repository (`~/.claude.json`); a project `.mcp.json` alternative whose `env` holds only `${AGENT_MAIL_TOKEN}`, with the token coming from the `env` of `.claude/settings.local.json` (links "Agent Tokens in Claude Code and Claude Desktop"); the warning "Never put a literal token in `.mcp.json` or any other file you commit."
- `README.md` **Release-binary tests**: now says they run no audited command and write nothing to your audit log.
- `install.sh`: `arm64 | aarch64)` narrowed to `arm64)`; the error wording is unchanged.
- `tests/selfcheck.rs` `release::ac2_...`: loop is `whoami`, `mandate list`, `mandate delete deadbeef`; the audit-line comment is removed. `src/` and `ac8_refused_list_is_audited_like_store_unavailable` are untouched.
- New tests in `tests/plugin.rs` (job023_ac1-ac3) and `tests/release.rs` (job023_ac4-ac7, with an `install_on` helper that puts a fake `uname` first on `PATH`).

**Commits:** 46d4023
**Tests:** `cargo test`: every suite `test result: ok` (0 failed); `cargo clippy -- -D warnings` and `cargo clippy --all-targets -- -D warnings`: clean; `cargo fmt --check`: clean. Opt-in `cargo test --test selfcheck -- --ignored`: `test result: ok. 4 passed; 0 failed`.

| AC | Test | Status |
|----|------|--------|
| AC1 | `tests/plugin.rs` `job023_ac1_mcp_server_section_offers_claude_mcp_add_in_local_scope` | pass |
| AC2 | `tests/plugin.rs` `job023_ac2_mcp_server_section_shows_a_project_mcp_json_with_a_reference` | pass |
| AC3 | `tests/plugin.rs` `job023_ac3_mcp_server_section_warns_never_to_commit_a_token`, plus existing `ac8_no_real_credential_or_agent_token_in_plugin_or_readme` and `ac4_readme_shows_where_agent_tokens_go` | pass |
| AC4 | `tests/release.rs` `installer::job023_ac4_aarch64_is_an_unsupported_architecture` (macOS), `job023_ac4_installer_no_longer_accepts_aarch64` (static) | pass |
| AC5 | `tests/release.rs` `installer::job023_ac5_arm64_and_x86_64_install_their_targets` | pass |
| AC6 | `tests/release.rs` `job023_ac6_release_binary_tests_run_no_audited_command` (not ignored); opt-in `release::ac2_unhardened_release_binary_refuses_every_keychain_access` also run and passing | pass |
| AC7 | `tests/release.rs` `job023_ac7_readme_says_release_binary_tests_write_nothing_to_the_audit_log` | pass |
| AC8 | `cargo test`, clippy, fmt; `ac8_refused_list_is_audited_like_store_unavailable` unchanged | pass |

**Review findings:** none (round 1).
