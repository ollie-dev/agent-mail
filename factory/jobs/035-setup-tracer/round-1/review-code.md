VERDICT: PASS

## Findings
- [follow-up] src/setup.rs:`starts_on_bare_call`: a bare call on a terminal whose store errors (not just empty) silently prints help; acceptable now, but a later ticket could tell the human why the wizard did not start.

## Checked
- Read: full `git diff main...factory/035-setup-tracer` for CLAUDE.md, src/create.rs (`run`, `create_and_store`, `take_back`, `stored_line`, `offer_export`), src/lib.rs (`run`, `setup_command`, `run_cli`, `render_body`), src/main.rs, src/setup.rs (all), tests/draft.rs, and the test list of tests/setup.rs (AC1 to AC10 named tests).
- Ran: `cargo test` in the worktree: all suites ok, 0 failed. `cargo clippy -- -D warnings`: clean. `cargo fmt --check`: clean.
- Criteria: AC1 `ac1_help_lists_setup_on_one_line`; AC2 a-f `ac2a..ac2f`; AC3 `ac3_*` (sandbox, no terminal, Broker route); AC4 `ac4a..d` (mandate create transcript compared, create.rs tests green); AC5 `ac5_*` parses both snippets; AC6 `ac6_*`; AC7 `ac7_*` incl. failure; AC8 `ac8a..f` with token-once assertion; AC9 `ac9_*`; AC10 in-process only. Each would fail without the change (new command, module and flow). Code check: token only reaches `Terminal::reveal`, snippets built with serde_json (path escaped), refusals run before any prompt, `mandate create` reveal text unchanged.
