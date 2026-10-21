# Agent Mail

## Stack

- Language: Rust, one binary crate (see `docs/adr/0001-rust-not-bun.md`)
- Test: `cargo test`
- Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`
- Source and test layout: one Cargo package `agent-mail` at the repo root (`Cargo.toml`, committed `Cargo.lock`); `src/main.rs` is the thin binary entry point that parses arguments and hands off; `src/lib.rs` is the library target holding the CLI definition, the core and the adapters, so tests can drive them in-process; modules under `src/` split the core (`core.rs`), the Mandate model (`mandate.rs`), sealing (`seal.rs`), the Mandate store port with the macOS Keychain store (`store.rs`), the Terminal port (`terminal.rs`), the `mandate create` guided flow (`create.rs`), the TOML Mandate file written by the export offer and read by `mandate create --from` (`export.rs`), the IMAP connector (`imap.rs`), modified UTF-7 (`utf7.rs`) and the append-only audit log with its platform path rule (`audit.rs`); `tests/` holds integration tests, one file per area (`tests/cli.rs`, `tests/project.rs`, `tests/list.rs`, `tests/sealing.rs`, `tests/create.rs`, `tests/export.rs` (the export offer and `mandate create --from`), `tests/mandates.rs` (`mandate list` and `mandate delete`), `tests/audit.rs` (the audit log), and the opt-in, ignored-by-default `tests/keychain.rs`, run with `cargo test -- --ignored keychain` on macOS), with shared helpers and the in-process fake IMAP server in `tests/support/`
- Test-only code (the in-memory store, the scripted terminal, the trust-only-these-roots TLS hook, the connect override and the audit log redirect; test contexts write their audit log to a fresh temporary file, never the user's data directory) sits behind `cfg(any(test, feature = "test-support"))`; the `test-support` feature is never in `default` and is switched on for integration tests by the package's dev-dependency on itself

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
