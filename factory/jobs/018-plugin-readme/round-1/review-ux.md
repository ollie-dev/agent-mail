VERDICT: PASS

## Findings
- [follow-up] README.md (lines 114-416, existing sections): many paragraphs exceed the 300-character guideline in CLAUDE.md (up to about 1000 characters). They predate this job, which keeps them out of scope → a later docs job should split them.
- Note: no manual `/plugin install` was done in an interactive Claude Code session (the builder says the same). AC1 rests on the structural test and `claude plugin validate`.

## Checked
- Read: spec.md, the full diff of factory/018-plugin-readme against main (marketplace.json, plugin.json, plugin/.mcp.json, SKILL.md, README.md changes, CLAUDE.md), build.md after the diff.
- Ran (worktree): `cargo build`; `cargo test --test plugin` gave 10 passed.
- Ran: `agent-mail search -h` and `draft -h`. Every option the skill lists (`--gmail`, `--text`, `--limit`, `--account`, `--reply-to`) exists, and the skill's rules match: `--account` is not allowed with `--reply-to`, and `--subject` is not allowed with `--reply-to`.
- Ran: `agent-mail --text whoami` and `whoami` with an empty token. Both gave `missing_token` with exit 1, in the text and JSON forms the skill describes.
- Checked in src: `mandate create` refuses to run in an agent sandbox (src/create.rs sandbox_indicator), and `proxy_refused` carries a host (src/core.rs). Both match the README and the skill.
- Read the skill as a prompt-injected Agent would: it never tells the Agent to run mandate create or delete, or to run the installer. Both are directed to the human.
- Read the README's new sections: Creating a Mandate, Agent Tokens, Claude Code plugin, Gmail app passwords, What each OS protects against, Sandbox allowed domains. All use placeholders and short paragraphs.
