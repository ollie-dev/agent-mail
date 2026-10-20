## Round 1
**Changed:**
- `src/lib.rs`: `Cli` gets `arg_required_else_help = true`, so a bare `agent-mail` prints clap's help (description, `Usage: agent-mail`, options incl. `-h, --help`) to stderr and exits 2. `main.rs` unchanged.
- `Cargo.toml`: `description` is now "Lets AI agents work with email while keeping mail credentials out of their hands"; clap's `about` renders it in `--help`.
- `tests/cli.rs`: four new tests that run the built binary (bare call, `--help`). `tests/project.rs`: one test on `CARGO_PKG_DESCRIPTION`.
- Stack is already recorded in `CLAUDE.md`; no change.

**Commits:** 76f7038
**Tests:** `cargo test` → every target `test result: ok.` (cli: 5 passed; project: 3 passed; lib/main/doc: 0). Also `cargo clippy -- -D warnings` exit 0, `cargo clippy --all-targets -- -D warnings` exit 0, `cargo fmt --check` exit 0.

| AC | Test | Status |
|----|------|--------|
| AC1 | `tests/cli.rs::bare_call_prints_usage_hint_on_stderr` | pass |
| AC2 | `tests/cli.rs::bare_call_is_a_usage_error_with_empty_stdout` | pass |
| AC3 | `tests/cli.rs::help_does_not_mention_mandates` | pass |
| AC4 | `tests/cli.rs::help_describes_the_tool_before_usage` | pass |
| AC5 | `tests/project.rs::package_description_does_not_mention_mandates` | pass |
| AC6 | `tests/cli.rs::version_flag_prints_name_and_crate_version` (unchanged) | pass |
| AC7 | `cargo test`, `cargo clippy -- -D warnings`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` (all exit 0 on 76f7038) | pass |

**Review findings:** none (first round).
