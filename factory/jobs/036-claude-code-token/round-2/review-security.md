VERDICT: PASS

## Findings
- Round 1 [blocking] src/setup.rs `git_risk` lexical-only `.git` walk: resolved. The walk now runs on both the folder as typed and `std::fs::canonicalize(folder)`. A canonicalize failure returns `GitRisk::GitFailed`, so it counts as a risk rather than "outside git". Git still runs with the typed folder as its working directory, never as an argument. The new Unix test `cc_ac2_folder_reached_through_a_symlink_into_a_repository_is_checked` covers a symlink into a repository subfolder: it gets the "not ignored by git" fallback, check-ignore is recorded, nothing is written under `repo/sub`, and the token is shown once.
- [follow-up] src/setup.rs `write_settings`: the stale temporary file after a crash is still open from round 1. It is carried in the ledger and not repeated here as new.
- [follow-up] src/setup.rs `git_risk`: `GIT_DIR`/`core.worktree` detection is still open from round 1. It is carried in the ledger.

## Checked
- Read: `git log main..HEAD` (5165385, 0ff9bd0), `git diff --stat main...factory/036-claude-code-token` (CLAUDE.md, src/setup.rs, tests/setup.rs, with Cargo.toml/Cargo.lock unchanged), `git show 0ff9bd0` in full (`git_risk` in src/setup.rs and the new symlink test in tests/setup.rs), and `git_risk` as it stands now at src/setup.rs:504-532. Read build.md Round 2 after the diff. Its claims match the code.
- Ran: `cargo test --test setup`: 61 passed, 0 failed, including the new symlink test. `cargo clippy -- -D warnings`: clean. `cargo fmt --check`: clean.
