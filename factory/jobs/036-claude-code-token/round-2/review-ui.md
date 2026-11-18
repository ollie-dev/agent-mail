VERDICT: PASS

## Findings
- [follow-up] src/setup.rs: the wizard messages ending "so the token was not written there." repeat the same tail in several variants; a later job could share one helper.

## Checked
- Read: spec.md, round-1/review-ui.md (no blocking or fix-now items), build.md Round 2 section, `git log`, and the rendered-text constants and messages in the `src/setup.rs` diff.
- Ran: nothing interactive. The round-2 change (canonicalized `.git` walk in `git_risk`, plus a symlink test) adds no new rendered text; the existing warning lines are unchanged, one line each, under about 200 characters, and read the same with colour off.
