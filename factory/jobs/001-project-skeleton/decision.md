APPROVE
All five acceptance criteria are covered by tests or lint commands I ran green on 91d53a9, all four round 2 reviews pass, and the diff stays inside the spec's file list with clap as the only dependency.

## Acceptance criteria
- AC1: pass (`tests/cli.rs::version_flag_prints_name_and_crate_version`; it checks stdout against `CARGO_PKG_VERSION`, empty stderr and exit 0. I also ran the binary by hand: it prints exactly `agent-mail 0.1.0\n`)
- AC2: pass (`cargo test` exits 0 and every target reports `test result: ok.`: lib 0, main 0, cli 1, project 2, doc 0)
- AC3: pass (`cargo clippy -- -D warnings`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` all exit 0 in the worktree)
- AC4: pass (`tests/project.rs::claude_md_stack_section_records_layout`; `git diff` confirms only the layout line in CLAUDE.md changed)
- AC5: pass (`tests/project.rs::project_is_a_single_rust_binary_crate`)

## Reviews
- Round 2: code, security, UI and UX all `VERDICT: PASS`, with nits only.
- Round 1: only the security review survived (PASS). human-notes.md (2026-10-20) says the other reviews were lost to a reviewer bug. The round 2 code and UX reviews say they did full reviews to make up for this.

## Scope and human-owned calls
- Files changed: `.gitignore`, `CLAUDE.md` (layout line only), `Cargo.toml`, `Cargo.lock`, `src/main.rs`, `src/lib.rs`, `tests/cli.rs`, `tests/project.rs`. This matches the spec's list exactly.
- The only dependency is `clap` 4 (derive). It does not handle secrets. There is no `build.rs`, `publish = false` is set, and no credentials, fixtures or build artefacts are committed.
- Nothing breaking, and no new way of storing or exposing credentials.

## Follow-ups (non-blocking)
- Security nit, deferred on purpose: `.gitignore` does not yet ignore `.env`, `.env.*` or `*.pem`. The spec allowed only `/target` here. Ticket 12 (Gmail live test) must add these entries before any real credentials go on disk.
- Code nit: the hand-rolled TOML scanner in `tests/project.rs` should be replaced if `Cargo.toml` gains inline tables or multi-line values.
- UX nit: the `about` text uses "mandate", which is jargon until later tickets explain it.
