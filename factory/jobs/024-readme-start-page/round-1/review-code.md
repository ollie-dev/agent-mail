VERDICT: PASS

## Findings
- [follow-up] README.md:11: the platform badge links to `#what-each-os-protects-against`; the spec leaves the target open, so this is fine, but a later job could revisit it if a better target appears.

## Checked
- Read: full diff `git diff main...factory/024-readme-start-page` (README.md top part; tests/plugin.rs: `readme_header`, `readme_h2s`, `readme_top`, `github_anchor`, `assert_in_order`, and the job024_ac1..ac9 tests). Everything from `## Install` on is unchanged in the diff. The "prints its Agent Token" claim in Quick start matches README line 92.
- Ran: `cargo test` in the worktree: all suites ok, 0 failed (plugin and release suites included). `cargo clippy -- -D warnings`: clean. `cargo fmt --check`: clean.
- Criteria: AC1 to AC9 each have a job024_acN test that asserts on the new top part, so each would fail on the old README. AC8's paragraph check skips HTML blocks and fences and splits list items as the spec says. AC9's existing placeholder scan passes, and there is a new top-part token check. The header is a centred div with icon, h1, tagline and three linked badges. Quick start has 2 fenced blocks and no token value. The Contents anchors match the headings in order.
