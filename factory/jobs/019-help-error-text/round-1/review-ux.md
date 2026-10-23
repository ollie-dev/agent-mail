VERDICT: PASS

## Findings
- [follow-up] src/lib.rs:46: the root `-h` prints the full `after_help` (three paragraphs), same as `--help`, so the short form is not shorter at the root. Behaviour was already like this and it meets the 300-character rule → a later job could move the account/Mandate explanation into `--help` only.

## Checked
- Read: spec.md, the full diff of factory/019-help-error-text against main (src/lib.rs, src/core.rs, tests, review-ui.md bullet, selfcheck test change).
- Ran: `agent-mail --help` and `-h` (new --text line, split after_help, whoami/token/--account kept); `search --help` (`agent-mail --text search` appears once) and `search -h` (`--text <S>  Messages whose headers or body contain S`); `draft --help` (drafts_not_found, `Re: `, --to/--subject/--reply-to option lines carry the dropped details); `mandate create -h` (--from present, first line stands alone). Checked the rewritten messages in src/core.rs: next steps (run whoami, retry, ask the human, search again) are kept; the invalid_token warning is short (~130 chars).
- Ran `cargo test` (all pass; an earlier failure in tests/cli.rs was a build race and passed on rerun), `cargo clippy -- -D warnings` (clean), `cargo fmt --check` (clean).
