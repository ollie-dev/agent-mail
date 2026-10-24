VERDICT: PASS

## Findings
- None. The new README text is clear, placeholder-only for tokens, and each paragraph is under about 300 characters. The installer error line is unchanged in wording.

## Checked
- Read: `git diff main...factory/023-token-arm64-selfcheck` for README.md, install.sh, tests/selfcheck.rs (stat for the test files), and spec.md.
- Ran: measured line lengths of the `## MCP server` section of README.md: longest prose line about 260 characters. Read the rendered order: scope-local command, `.mcp.json` block with `${AGENT_MAIL_TOKEN}`, env source sentence, a separate never-commit sentence. Read the Release-binary tests paragraph: now says nothing is written to the audit log. Read the install.sh `uname -m` case: `arm64` and `x86_64` only, error text names the seen value and both supported ones.
