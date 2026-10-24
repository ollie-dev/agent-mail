VERDICT: PASS

## Findings
(none)

## Checked
- Read: full `git diff main...factory/023-token-arm64-selfcheck` (README.md MCP server and Release-binary tests paragraphs; install.sh arch case; tests/plugin.rs job023 tests; tests/release.rs install_on, job023 ac4-ac7 tests; tests/selfcheck.rs loop); tests/selfcheck.rs ac2 loop in the worktree.
- Ran: `cargo test` all suites ok, 0 failed (ignored opt-in tests skipped); `cargo clippy -- -D warnings` clean; `cargo fmt --check` clean.
- Criteria: AC1-AC3 covered by job023_ac1..ac3 in tests/plugin.rs (fail on old README: no --scope local, no json block, old "checked-in" wording); README tokens are only placeholders/`${AGENT_MAIL_TOKEN}`. AC4/AC5 by job023_ac4/ac5 installer tests with fake uname plus static check in tests/release.rs (aarch64 now falls to the unsupported arm). AC6 by job023_ac6 scanning `mod release` (loop is whoami, mandate list, mandate delete; no TEST_LOG_VAR; comment removed). AC7 by job023_ac7. AC8: suite green, ac8_refused_list_is_audited test untouched.
