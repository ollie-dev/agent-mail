VERDICT: PASS

## Findings
- [follow-up] src/setup.rs (write_settings/Settings::to_bytes): a rewritten settings file can end up with mixed indentation (carried over from round 1) → a later job could preserve the original formatting.

## Checked
- Read: round-1/review-ux.md (no [blocking] or [fix-now] items to re-check), spec.md, build.md Round 2 section, and `git show 0ff9bd0` (the only change since round 1: `git_risk` now also walks the canonicalized folder; the doc comment line is long but harmless).
- Ran in the worktree: `cargo test --test setup` showed 61 passed, 0 failed, including the new symlink test. The user-facing flow and messages are unchanged from round 1, so my round-1 walk of the folder prompt, git warnings, add/replace, delete offers, fallbacks, final check and next step still holds.
