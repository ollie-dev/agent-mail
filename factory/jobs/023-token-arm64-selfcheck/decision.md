APPROVE
All four round-1 reviews pass with no fix-now findings, every criterion has a passing test, and the diff stays inside the README, install.sh and test files the spec names.

## Acceptance criteria
- AC1: pass (tests/plugin.rs `job023_ac1_mcp_server_section_offers_claude_mcp_add_in_local_scope`)
- AC2: pass (tests/plugin.rs `job023_ac2_mcp_server_section_shows_a_project_mcp_json_with_a_reference`)
- AC3: pass (tests/plugin.rs `job023_ac3_mcp_server_section_warns_never_to_commit_a_token`, plus the existing `ac8_no_real_credential_or_agent_token_in_plugin_or_readme` and `ac4_readme_shows_where_agent_tokens_go`)
- AC4: pass (tests/release.rs `installer::job023_ac4_aarch64_is_an_unsupported_architecture` and the static check `job023_ac4_installer_no_longer_accepts_aarch64`)
- AC5: pass (tests/release.rs `installer::job023_ac5_arm64_and_x86_64_install_their_targets`)
- AC6: pass (tests/release.rs `job023_ac6_release_binary_tests_run_no_audited_command`, not ignored; the selfcheck loop is now whoami, mandate list, mandate delete deadbeef)
- AC7: pass (tests/release.rs `job023_ac7_readme_says_release_binary_tests_write_nothing_to_the_audit_log`)
- AC8: pass (I ran `cargo test` at 46d4023: every suite ok, 0 failed. `cargo clippy -- -D warnings` and `cargo fmt --check` are clean. Nothing changed in `src/`, and the only selfcheck.rs change is the 3 removed lines in `mod release`, so `ac8_refused_list_is_audited_like_store_unavailable` is unchanged.)

## For the human
Nothing needs your decision. All new README token values are placeholders (`<your agent token>`, `${AGENT_MAIL_TOKEN}`). The installer change only narrows the architecture match. Download and checksum handling are unchanged.

## Follow-ups
- [pattern] ux · README.md (`## MCP server`, the `claude mcp add` and "from the environment" paragraphs): both are close to the 300-character limit (about 250 and about 300 characters). This adds to the open F22, and the same kind of finding is logged in 012 (F3), 015 (F9), 016 (F13), 017 (F17), 018 (F22) and 019 (F25) → include them in the F22 README paragraph pass, and add the test F22 proposes: fail on any README.md or SKILL.md prose paragraph over about 300 characters outside code blocks.
