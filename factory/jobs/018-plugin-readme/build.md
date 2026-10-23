## Round 1
**Changed:**
- Added the plugin marketplace `.claude-plugin/marketplace.json` (marketplace `agent-mail`, one plugin `agent-mail`, `source` `./plugin`) and the plugin under `plugin/`: `.claude-plugin/plugin.json`, `.mcp.json` (`agent-mail mcp`, `env` only `"${AGENT_MAIL_TOKEN:-}"`), `skills/agent-mail/SKILL.md`. No binary, nothing executable.
- The skill covers every Agent command and the MCP tools, the JSON output and stderr errors (exit codes 1 and 2), a `denied` example, what to do for each error code, human-only Mandates, and `## If agent-mail is missing` (ask the human to run the installer; never run it).
- README: new sections `## Creating a Mandate`, `## Agent Tokens in Claude Code and Claude Desktop`, `## Claude Code plugin`, `## Gmail app passwords`, `## What each OS protects against` (matching ADR-0002). `## MCP server` no longer recommends a token in the project's `.mcp.json` and points to the new section. `## Sandbox and proxies` gained an allowed-domains block.
- CLAUDE.md: layout names `tests/plugin.rs` and a new "Claude Code plugin" line naming `.claude-plugin/marketplace.json`, `plugin/` and `plugin/skills/agent-mail/SKILL.md`.
- Host details checked against the current Claude Code docs (code.claude.com/docs, fetched 2026-10-23), not guessed:
  - The sandbox allow-list is `sandbox.network.allowedDomains`. An entry without a port allows every port. The sandbox covers shell commands only, so the plugin's MCP server connects directly; the README says so.
  - Settings `env` reaches every subprocess Claude Code starts, stdio MCP servers included, once the workspace is trusted. With `CLAUDE_CODE_MCP_ALLOWLIST_ENV=1` only a server's configured `env` gets through, so `plugin/.mcp.json` keeps the `${AGENT_MAIL_TOKEN:-}` reference.
  - Claude Code adds `.claude/settings.local.json` to the global git excludes when it creates the file. The README tells users who create it by hand to git-ignore it themselves.
  - Marketplace fields: `name`, `owner` and `plugins[].name`/`source`. Plugin fields: `name` (required), `description` and `author`.
- `claude plugin validate .` and `claude plugin validate ./plugin` both pass. One warning is left on purpose, "no version": `plugin.json` has no `version`, so Claude Code tracks the git commit and nobody has to bump a second version file at release time. The test still enforces `version == Cargo.toml` if one is ever added. The other warning is a missing icon.
- No manual `/plugin install` was run inside an interactive Claude Code session. AC1 is checked structurally and with `claude plugin validate`.

**Commits:** bf3f985
**Tests:** `cargo test` → every suite `test result: ok` (tests/plugin.rs: `ok. 10 passed; 0 failed`). `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | ac1_plugin_manifests_provide_the_skill_and_the_mcp_server | pass |
| AC2 | ac2_skill_documents_commands_output_denial_and_human_only_mandates | pass |
| AC3 | ac3_skill_tells_the_human_to_install_a_missing_binary | pass |
| AC4 | ac4_readme_shows_where_agent_tokens_go | pass |
| AC5 | ac5_readme_lists_the_sandbox_allowed_domains | pass |
| AC6 | ac6_readme_states_what_each_os_protects_against | pass |
| AC7 | ac7_readme_recommends_one_gmail_app_password_per_mandate | pass |
| AC8 | ac8_no_real_credential_or_agent_token_in_plugin_or_readme | pass |
| AC9 | ac9_readme_covers_creating_a_mandate | pass |
| AC10 | ac10_claude_md_names_the_plugin_files | pass |

**Review findings:** none (round 1).

## Nit pass 1
**Commits:** e0c87e2
**Tests:** `cargo test` → all 24 suites `test result: ok`, none failed (tests/plugin.rs: `ok. 10 passed; 0 failed`). `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean.
**Findings:**
- review-security [fix-now], "ask the human for a valid token" in SKILL.md contradicted "never ask for a token" → fixed.
  - `plugin/skills/agent-mail/SKILL.md` line 19 now says: "Ask the human to set a valid token in `AGENT_MAIL_TOKEN` themselves and restart the host. Never have them paste a token into the chat."
  - The `missing_token`/`invalid_token` bullet (old line 66) now says "ask the human to set a valid token in `AGENT_MAIL_TOKEN` and restart the host, as above".
  - I searched README.md for the same wording. The only near match is the CLI's own `invalid_token` warning text ("ask the human for a new token"), which is quoted output, out of scope, and not matched by the check.
  - Done-when grep `grep -niE "ask (the human )?for (a|the|an|your)?( valid| agent)? ?token" plugin/skills/agent-mail/SKILL.md README.md` prints only SKILL.md:17 ("Never print, echo, log or ask for a token").
  - Regression test: `ac2_skill_documents_commands_output_denial_and_human_only_mandates` now fails on any skill line that asks the human (or asks) for a token without "never". It also requires the "set a valid token" wording.
