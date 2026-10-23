VERDICT: PASS

## Findings
- [fix-now] src/lib.rs (usage_message, `InvalidSubcommand` arm): the unknown-command message reads "unknown command; did you mean `list`?; run `agent-mail --help` for usage", with a stray "?;" joint → drop the semicolon inside, e.g. "unknown command, did you mean `list`; run `agent-mail --help` for usage". Done when: `agent-mail lsit` stderr has no "?;" and still ends with "run `agent-mail --help` for usage"; the test in tests/cli.rs for `lsit` still passes.

## Checked
- Read: full diff `main...factory/020-agent-readable-errors` for src/lib.rs, src/core.rs, src/mcp.rs, README.md and SKILL.md; skimmed the file list for tests.
- Ran (debug binary): `draft --to not-an-address-zq7 ...`, `list --bogus=hunter2zq7`, `read a b`, `lsit`, `mandate delete`, `mcp --bogus`, `--text list --limit 0`, `whoami --full`, `search --limit 0`, `list --account ''`, `draft ... --reply-to ... --subject`, bare `mandate`.
- Saw: each prints one JSON line on stderr with exit 2, names the argument, ends with the correct per-command help pointer and echoes no caller value. Longest message is about 230 characters. Bare `mandate` still prints plain help on stderr with exit 2. The `draft_refused` message (src/core.rs) is one sentence saying no draft was created and giving a next step; it does not echo the server's text.
