VERDICT: PASS

## Findings
- [follow-up] src/setup.rs (write_settings/Settings::to_bytes): an existing settings file is rewritten with 2-space pretty printing, but untouched values keep their original raw text, so a hand-formatted file can end up with mixed indentation → a later job could preserve the original formatting, or say so in the docs ticket (10).

## Checked
- Read: spec.md, the full diff main...factory/036-claude-code-token (src/setup.rs, tests/setup.rs by name, CLAUDE.md change), no human-notes.md and no earlier review.
- Ran in the worktree: `cargo test --test setup` (60 passed), `cargo test --test release` (passed, so the CLAUDE.md check holds), `cargo clippy -- -D warnings` (clean), `cargo fmt --check` (clean).
- Walked the flows in code: the folder prompt (default is the current folder, relative paths resolved, "No such folder: <path>" and asked again, cancel goes to the cancel path); git-risk warnings that name the reason (not ignored, tracked, git could not be run) followed by the reveal or SHOWN_ABOVE; Add/Replace choice with Add as default and the old token never echoed; delete offers defaulting to No, with name, ID and the warning; invalid JSON, wrong shape and unwritable file messages that name the file and not its content; the final check reading the file back; the per-folder "Restart Claude Code in <folder>..." next step. All match the spec. Messages are one short line each.
