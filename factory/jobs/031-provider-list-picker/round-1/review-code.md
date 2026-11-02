VERDICT: PASS

## Findings
(none)

## Checked
- Read: full `git diff main...factory/031-provider-list-picker` for `src/` (`provider.rs`: `parse`, `label`, `search_option`, `all`, `sorted`, `by_domain`, `include_str!` plus comment; `terminal.rs`: `SearchOption::matches`, `Terminal::search`, `InquireTerminal::search` scorer, `ScriptedTerminal::search`, `Answer::search`, unit tests; `create.rs`: `ask_provider`, `add_account`, `CUSTOM_SERVER`, `GMAIL_HOST`/`looks_like_gmail` kept), `CLAUDE.md`, and the test diffs (`tests/provider.rs`, `tests/create.rs` function list, `tests/release.rs`, `tests/client.rs`, `tests/proxy.rs`, `tests/credential_manager.rs`, `tests/support/mod.rs`).
- Ran: a Python comparison of `data/providers.json` against the supported entries of `.scratch/provider-list/providers.json`: same ids in order, entries identical. `grep` found no remaining `Account type` outside the negative assertion in `tests/create.rs`, and no `create::GMAIL`/`GENERIC`.
- Ran: `cargo test`: every suite ok, 0 failed (the ignored tests are the opt-in ones). `cargo clippy -- -D warnings`: clean. `cargo fmt --check`: clean.
- Criteria: AC1 is covered by the unit tests in `src/terminal.rs` (hidden-key pick, panic on a non-matching query, cancel). AC2 and AC3 are covered by `tests/provider.rs` (source text, ids, schema, one bad list per rule). AC4 is covered by `ac4_*` in `tests/provider.rs` and by `job031_ac4_*` in `tests/create.rs`. AC5 to AC8 are covered by `job031_ac5/6/7/8_*` in `tests/create.rs` plus the adjusted `export`, `client` and `proxy` suites. AC9 is covered by the `tests/release.rs` check. The old preset flow could not pass these tests, since the `Mail provider:` prompt is new. The list is read only through `include_str!`, with no runtime read.
