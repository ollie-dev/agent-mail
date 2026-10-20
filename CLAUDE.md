# Agent Mail

## Stack

- Language: Rust, one binary crate (see `docs/adr/0001-rust-not-bun.md`)
- Test: `cargo test`
- Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`
- Source and test layout: one Cargo package `agent-mail` at the repo root (`Cargo.toml`, committed `Cargo.lock`); `src/main.rs` is the thin binary entry point that parses arguments and hands off; `src/lib.rs` is the library target holding the CLI definition, the core and the adapters, so tests can drive them in-process; `tests/` holds integration tests, one file per area (`tests/cli.rs`, `tests/project.rs`)

## Conventions

- Credentials never appear in logs, CLI output, agent-visible responses, test fixtures, or commits.

## Software factory

Features are built by `/factory` (`.claude/skills/factory/SKILL.md`). State and job records live in `factory/`; each job is built on branch `factory/<job-id>` in the worktree `.worktrees/<job-id>/`. Dashboard: `cd factory && python3 -m http.server`, then open http://localhost:8000/dashboard.html.

## Agent skills

### Issue tracker

Issues and specs are local Markdown files under `.scratch/<feature>/`. See `docs/agents/issue-tracker.md`.

### Triage labels

The default labels, recorded as a `Status:` line in each issue file. See `docs/agents/triage-labels.md`.

### Domain docs

Single context: `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.
