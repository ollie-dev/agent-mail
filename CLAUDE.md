# Agent Mail

## Stack

- Language: Rust, one binary crate (see `docs/adr/0001-rust-not-bun.md`)
- Test: `cargo test`
- Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`
- Source and test layout: one Cargo package `agent-mail` at the repo root (`Cargo.toml`, committed `Cargo.lock`); `src/main.rs` is the thin binary entry point that parses arguments and hands off; `src/lib.rs` is the library target holding the CLI definition, the core and the adapters, so tests can drive them in-process; modules under `src/` split the core (`core.rs`, including opening the presented tokens, target-Account resolution, Mandate selection and `whoami`), the Mandate model (`mandate.rs`), sealing (`seal.rs`), the Mandate store port with the macOS Keychain store and the `GuardedStore` wrapper that runs the hardened-runtime self-check before every store call (`store.rs`), the hardened-runtime self-check with `csops` and the `hardened_runtime_ok` decision, active only in macOS release builds (`src/selfcheck.rs`), the Terminal port (`terminal.rs`), the `mandate create` guided flow (`create.rs`), the TOML Mandate file written by the export offer and read by `mandate create --from` (`export.rs`), the IMAP connector (`imap.rs`), the proxy setting read from `HTTPS_PROXY`/`ALL_PROXY` with the hand-written HTTP CONNECT and SOCKS5 handshakes (`proxy.rs`), modified UTF-7 decoding and encoding (`utf7.rs`), message search with its criteria, result types and message reference (`search.rs`, including the `--gmail` query sent as `X-GM-RAW`), reading one message with its result types, MIME-to-text conversion and truncation (`read.rs`), setting and clearing the `\Seen` and `\Flagged` flags with the flag name parsing and result type (`flag.rs`), moving one message between mailboxes with its result type and `--text` line (`move_message.rs`), creating drafts with the result type and its `--text` line, recipient and subject validation, building the RFC 5322 message and the reply helpers (`draft.rs`), the append-only audit log with its platform path rule (`audit.rs`) and the MCP server over stdio with its JSON-RPC loop, tool definitions, argument checks and result rendering (`src/mcp.rs`); `tests/` holds integration tests, one file per area (`tests/cli.rs`, `tests/project.rs`, `tests/list.rs`, `tests/sealing.rs`, `tests/create.rs`, `tests/export.rs` (the export offer and `mandate create --from`), `tests/mandates.rs` (`mandate list` and `mandate delete`), `tests/audit.rs` (the audit log), `tests/search.rs` (`search`), `tests/read.rs` (`read`), `tests/flag.rs` (`flag`), `tests/move_message.rs` (`move`), `tests/draft.rs` (`draft`), `tests/proxy.rs` (connecting through `HTTPS_PROXY`/`ALL_PROXY`, `proxy_refused`, `invalid_proxy` and built-in roots in proxy mode, against the in-process CONNECT and SOCKS5 proxies in `tests/support/proxy.rs`), `tests/mcp.rs` (the MCP server: tools, arguments, results and errors compared with the CLI, the protocol and the binary's `mcp` subcommand), `tests/tokens.rs` (several tokens in `AGENT_MAIL_TOKEN`, `--account`, Mandate selection, `ambiguous_mandate`, the invalid-token warning and `whoami`), `tests/selfcheck.rs` (the in-process `GuardedStore` tests, plus opt-in, ignored-by-default release-binary tests run on macOS with `cargo test --test selfcheck -- --ignored`), `tests/release.rs` (the installer against a `file://` download directory on macOS, and checks of the workflow, release notes, README and this file), the opt-in, ignored-by-default `tests/keychain.rs`, run with `cargo test -- --ignored keychain` on macOS, and the opt-in, ignored-by-default live Gmail test `tests/gmail_live.rs`, run with `cargo test --test gmail_live -- --ignored` after exporting `AGENT_MAIL_LIVE_GMAIL_ADDRESS` and `AGENT_MAIL_LIVE_GMAIL_APP_PASSWORD` for a dedicated test account), with shared helpers and the in-process fake IMAP server in `tests/support/`
- Release files: the POSIX `sh` installer `install.sh` at the repo root (also uploaded as a release asset); `scripts/` holds `sign-macos.sh` (ad-hoc signing with the hardened runtime, `--check-only` to verify) and `verify-release.sh <tag>` (checks a published release); `.github/workflows/release.yml` builds, signs, attests and publishes on a `v*` tag push and runs the `verify` job; `.github/release-notes.md` is every release's body. `[profile.release]` pins `debug-assertions = false`, since debug builds skip the self-check
- Test-only code (the in-memory store, the scripted terminal, the trust-only-these-roots TLS hook, the connect override, the audit log redirect and `GuardedStore::with_status` with its fixed code-signing status; test contexts write their audit log to a fresh temporary file, never the user's data directory) sits behind `cfg(any(test, feature = "test-support"))`; the `test-support` feature is never in `default` and is switched on for integration tests by the package's dev-dependency on itself

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
