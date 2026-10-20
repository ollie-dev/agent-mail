APPROVE
All seven acceptance criteria have passing tests, all four round-1 reviews pass, and the diff is limited to the description string, one clap attribute, and tests.

## Acceptance criteria
- AC1: pass (tests/cli.rs::bare_call_prints_usage_hint_on_stderr)
- AC2: pass (tests/cli.rs::bare_call_is_a_usage_error_with_empty_stdout)
- AC3: pass (tests/cli.rs::help_does_not_mention_mandates)
- AC4: pass (tests/cli.rs::help_describes_the_tool_before_usage)
- AC5: pass (tests/project.rs::package_description_does_not_mention_mandates)
- AC6: pass (tests/cli.rs::version_flag_prints_name_and_crate_version, unchanged)
- AC7: pass (`cargo test`: cli 5/5, project 3/3; `cargo clippy -- -D warnings` clean; `cargo fmt --check` clean, run on 76f7038 in the worktree)

## For the human
Nothing needs a human decision. There are no new dependencies, Cargo.lock is unchanged, no credentials are involved, and no existing behaviour breaks. The spec allowed the bare call to exit with code 2.
UI and UX reviewers each left a nit. A bare call prints clap's full help to stderr, not a short "try '--help'" line, so `--help` shows up only as the `-h, --help` option row. That meets AC1 as written. If you want the shorter hint, it can go in a later polish ticket.
