VERDICT: PASS

## Findings
- None.

## Checked
- Read: full `git diff main...factory/033-host-warning` (`src/create.rs`: `add_account` warning, removed Gmail constants and `looks_like_gmail`; `src/provider.rs`: `expected_for_login` and its unit test; `tests/create.rs`: all new job033 tests).
- Ran: `cargo test` in the worktree: all suites ok, 0 failed. `cargo clippy -- -D warnings`: clean. `cargo fmt --check`: clean.
- Criteria: AC1 `job033_ac1_*` (exactly one warning, fields, before Hidden); AC2 `job033_ac2_*`; AC3 a-d `job033_ac3a/b/c/d` plus suffix case `domain_is_matched_exactly_not_as_a_suffix`; AC4 `job033_ac4_*` source scan, and the existing Gmail tests in `tests/create.rs` and `tests/export.rs` still pass unchanged; AC5 `job033_ac5_*` (escaped text, no raw control chars, domain still recognised); AC6 commands above. Each warning test would fail without the change (non-Gmail Providers produced no warning before). The warning text matches the spec. Domain matching uses the exact text after the last `@` via `rsplit_once`, with no suffix match, and unsupported Providers give no warning because `connection()` is `None`.
