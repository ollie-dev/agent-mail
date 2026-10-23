VERDICT: PASS

## Findings
- No blocking or fix-now issues. Every help paragraph is split into short single-topic paragraphs, the first lines of `-h` still stand alone, and the trimmed error messages keep their next step (retry, search again, ask the human).

## Checked
- Read: spec.md, `git diff main...factory/019-help-error-text` for src/lib.rs, src/core.rs and .claude/agents/review-ui.md (test diffs skimmed by running them).
- Ran: `target/debug/agent-mail -h` and `--help` (root; the AC3 `--text` line and the split after_help read well), `search --help` (`agent-mail --text search` appears once, line 9; `search -h` `--text` line starts "Messages whose headers or body contain S"), `draft --help` (five short paragraphs, `Re: `, `drafts_not_found` kept), and `cargo test` (all suites pass).
