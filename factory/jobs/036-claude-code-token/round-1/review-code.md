VERDICT: PASS

## Findings
- [follow-up] src/setup.rs (claude_code): `if replace || old.is_empty()` repeats the choice logic that `replace` already encodes; the value-building could be a small helper. Style only, no behaviour issue.

## Checked
- Read: full `git diff main...factory/036-claude-code-token` (CLAUDE.md, src/setup.rs, tests/setup.rs list of tests); src/setup.rs: `claude_code`, `project_folder`, `git_risk`, `Settings::load/tokens/set_token`, `Members`, `write_settings`, `offer_old_deletes`, `final_check`/`check`, `hook`. Cargo.toml: `raw_value` only, no `preserve_order` (key order kept via `Members`/`RawValue`, avoiding crate-wide change).
- Ran: `cargo test` all suites ok (tests/setup.rs 60 passed, 0 failed); `cargo clippy -- -D warnings` clean; `cargo fmt --check` clean.
- Criteria: AC1 cc_ac1a-d; AC2 cc_ac2a-e (command order and git file case); AC3 cc_ac3a/b; AC4 cc_ac4; AC5 cc_ac5a-d; AC6 cc_ac6a,b,c,e; AC7 cc_ac7a-e; AC8 cc_ac8a-c; AC9 cc_ac9a/b (test hook); AC10 cc_ac10; AC11 suite green with updated option-list test (ac5/ac6 existing). Each exercises the new code paths, which did not exist before, so they would fail without the change. Code matches spec: git risk checked before writing, tokens never echoed, write via temp file plus rename with 0600, delete offered only after a successful Replace.
