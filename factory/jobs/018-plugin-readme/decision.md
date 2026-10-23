APPROVE
All four round-1 reviews pass, the security fix-now was fixed in the nit pass and verified, and all ten criteria are tested by a passing suite.

## Acceptance criteria
- AC1: pass (ac1_plugin_manifests_provide_the_skill_and_the_mcp_server). The check is structural only; no one ran a manual `/plugin install` in Claude Code, which the spec puts out of scope.
- AC2: pass (ac2_skill_documents_commands_output_denial_and_human_only_mandates; the nit pass extended it to reject any skill line that asks for a token)
- AC3: pass (ac3_skill_tells_the_human_to_install_a_missing_binary)
- AC4: pass (ac4_readme_shows_where_agent_tokens_go)
- AC5: pass (ac5_readme_lists_the_sandbox_allowed_domains)
- AC6: pass (ac6_readme_states_what_each_os_protects_against)
- AC7: pass (ac7_readme_recommends_one_gmail_app_password_per_mandate)
- AC8: pass (ac8_no_real_credential_or_agent_token_in_plugin_or_readme)
- AC9: pass (ac9_readme_covers_creating_a_mandate)
- AC10: pass (ac10_claude_md_names_the_plugin_files). I ran `cargo test` in the worktree: every suite ok, tests/plugin.rs 10 passed. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

Fix-now check: review-security's one [fix-now] (SKILL.md asked the human "for a valid token") is recorded under `## Nit pass 1` in build.md. Nit commit e0c87e2 changes only the two SKILL.md lines and adds a regression check in tests/plugin.rs. `round-1/verify-security-1.md` starts with VERIFIED. I ran the Done-when grep myself, and it prints only SKILL.md:17 ("Never print, echo, log or ask for a token"). `cargo test --test plugin` passes.

Scope: the diff touches only the marketplace and plugin files, README.md, CLAUDE.md and tests/plugin.rs, with no change under `src/`. It also clears ledger item F11 (the README no longer suggests putting a token in the project's `.mcp.json`).

## For the human
Nothing here needs your decision. The change adds no new way of storing credentials. The Agent Token still comes from `AGENT_MAIL_TOKEN`, and the README now points users to the git-ignored `.claude/settings.local.json` instead of a checked-in `.mcp.json`. One thing is worth a manual check after merge, but it is not needed to approve: install the plugin in a real Claude Code session and confirm the skill and the `agent-mail` MCP server appear. AC1 was checked only through the manifests and `claude plugin validate`. You can mark F11 in follow-ups.md as done.

## Follow-ups
- security · plugin/.mcp.json / README.md "Agent Tokens in Claude Code and Claude Desktop": no one has watched `AGENT_MAIL_TOKEN` from `.claude/settings.local.json` reach the plugin's MCP server through `${AGENT_MAIL_TOKEN:-}`, or confirmed that the expanded value is not shown in agent-visible places such as `/mcp` server details → install the plugin by hand once, confirm the server gets the token, and confirm the value is never shown.
- code · plugin/.claude-plugin/plugin.json: there is no `version`, so Claude Code tracks the git commit. The test enforces `version == Cargo.toml` only if a version is added → a later release job should decide whether to pin a version and bump it with each release.
- [pattern] ui/ux · README.md:114-416 (and some new paragraphs, such as Install and Creating a Mandate): many prose paragraphs run past the roughly 300-character guideline, some up to about 1000 characters. The same kind of finding (overlong help, error or doc text) is logged in 012 (F3), 015 (F9), 016 (F13) and 017 (F17) → split the README paragraphs in a docs pass. The CLAUDE.md convention exists now, so also add a test (for example in tests/release.rs) that fails on any README.md or SKILL.md prose paragraph over about 300 characters outside code blocks.
