VERDICT: PASS

## Findings
None. Every token value in the new README text is a placeholder: `<your agent token>` in the `claude mcp add --scope local` line, and `${AGENT_MAIL_TOKEN}` in the `.mcp.json` block. The README suggests no project or user scope that would write a literal token into a shared file. The "never commit" warning now covers any committed file, not just `.mcp.json`. The linked "Agent Tokens in Claude Code and Claude Desktop" section already says that `.claude/settings.local.json` is git-ignored, and what to do if you create it by hand.

The installer change only narrows the architecture match. Download, checksum, extraction and install are unchanged, and the error line names only `uname -m` output. The opt-in release tests no longer run `list`. Nothing left in `mod release` is audited (`whoami`, `--version`, `mandate list`, `mandate delete`), so they no longer write to the developer's real audit log. They still present only a freshly minted `AgentToken::mint()` that no Keychain item matches. The new test fixtures (a fake `uname`, fake binaries that echo their target) hold no credentials. No new dependencies.

## Checked
- Read: the full diff `git diff main...factory/023-token-arm64-selfcheck` (README.md, install.sh, tests/plugin.rs, tests/release.rs, tests/selfcheck.rs).
- Read: README.md `## Agent Tokens in Claude Code and Claude Desktop` (where the new text links), to confirm the advice to keep `settings.local.json` out of git is there.
- Read: install.sh in full, to confirm that only the `case` arm changed and the checksum and extraction checks are intact.
- Read: tests/selfcheck.rs `mod release`, every `run(`/`clean(` call and argument array, plus a grep for `TEST_LOG_VAR`/`audit`. Only `whoami`, `--version`, `mandate list` and `mandate delete deadbeef` are run, and there is no log redirect.
- Read: tests/release.rs `install_on`, `downloads` and `temp_dir`. The fake `uname` and fake assets sit in fresh temp dirs and hold no secrets.
- Read: build.md after the diff, and checked its claims against the code and test runs.
- Ran: `cargo test --test plugin --test release`: 15 and 21 passed, 0 failed.
- Ran: `grep -nE 'amt_[A-Za-z0-9]{6,}|AGENT_MAIL_TOKEN=[^<$ ]' README.md`. The only hit is the existing `amt_...work...` placeholder example, so there is no literal token.
- Ran: `cargo clippy --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.
