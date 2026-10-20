# Project skeleton: `agent-mail --version`

## Goal
The repo holds a Rust project that builds the single `agent-mail` binary. A developer or the mail owner can run `agent-mail --version` and see the binary name and version. The stack and the source and test layout are recorded in `CLAUDE.md`, so every later job (starting with ticket 02, the tracer bullet) builds on the same base. There is no mail functionality yet.

## Context
- **Ticket:** `.scratch/agent-mail-v1/issues/01-project-skeleton.md` (feature spec `.scratch/agent-mail-v1/spec.md`, ADR-0001 `docs/adr/0001-rust-not-bun.md`).
- **Stack:** Rust (ADR-0001), stable toolchain (rustc/cargo 1.95.0 is installed locally). One Cargo package named `agent-mail` at the repo root, version `0.1.0`, producing one binary target `agent-mail`. CLI parsing with `clap` (derive feature): the feature spec already names clap, and its built-in `--version` handling gives the required output. Do not add any other runtime dependency in this job. Tests use only the standard library (`std::process::Command` with `env!("CARGO_BIN_EXE_agent-mail")`), so `assert_cmd` and similar crates are optional and best left out.
- **Test command:** `cargo test`. **Lint:** `cargo clippy -- -D warnings` (also run with `--all-targets` so test code is linted) and `cargo fmt --check`.
- **Layout to create and record:**
  - `Cargo.toml`, `Cargo.lock` (committed, because this is a binary)
  - `src/main.rs`: thin entry point that parses arguments and hands off
  - `src/lib.rs`: library target of the same package. It holds the CLI definition now, and the core and adapters later, so tests can drive them in-process as the feature spec's Testing Decisions require.
  - `tests/`: integration tests, one file per area (this job: `tests/cli.rs`, plus `tests/project.rs` for the project checks)
- **Files touched:** `Cargo.toml`, `Cargo.lock`, `src/main.rs`, `src/lib.rs`, `tests/cli.rs`, `tests/project.rs`, `.gitignore` (add `/target`; keep `.worktrees/`), `CLAUDE.md` (the layout line under `## Stack` only).

## Acceptance criteria
1. AC1: `agent-mail --version` prints the binary name and the crate version and exits 0. Concretely, running the built binary with the single argument `--version` writes exactly `agent-mail 0.1.0\n` to stdout, where `0.1.0` is the `package.version` in `Cargo.toml` (the test compares against `env!("CARGO_PKG_VERSION")`, not a hard-coded string). Stderr is empty and the exit status is 0.
2. AC2: `cargo test` runs green and contains at least one test that exercises the `--version` behaviour. Concretely, `tests/cli.rs` holds a test (e.g. `version_flag_prints_name_and_crate_version`) that spawns `env!("CARGO_BIN_EXE_agent-mail")` with `--version` and asserts the AC1 output. `cargo test` ends with `test result: ok.` for every test target and exits 0.
3. AC3: `cargo clippy -- -D warnings` and `cargo fmt --check` pass. Concretely, both commands (and `cargo clippy --all-targets -- -D warnings`) exit 0 with no warnings or diffs on the branch head. The builder checks this by running the commands and records them in build.md's AC table instead of a `cargo test` test (see Notes for reviewers).
4. AC4: The `## Stack` section of CLAUDE.md keeps its language, test and lint lines, and its layout line is replaced with the actual source and test layout. Concretely, a test in `tests/project.rs` reads `CLAUDE.md` from `env!("CARGO_MANIFEST_DIR")` and asserts that the `## Stack` section:
   - still contains the lines `- Language: Rust, one binary crate (see \`docs/adr/0001-rust-not-bun.md\`)`, ``- Test: `cargo test` ``, and ``- Lint: `cargo clippy -- -D warnings` and `cargo fmt --check` ``, unchanged;
   - no longer contains the text `set by ticket 01`;
   - has a `- Source and test layout:` line that names `src/main.rs`, `src/lib.rs` and `tests/`.
   The rest of `CLAUDE.md` is unchanged.
5. AC5: The project is a single binary crate; no other language runtime is required to build or run it. Concretely, a test in `tests/project.rs` reads the repo-root `Cargo.toml` and asserts that it has a `[package]` with `name = "agent-mail"`, no `[workspace]` section, and no `[[bin]]` other than `agent-mail`. It also asserts that none of `package.json`, `bun.lockb`, `bun.lock`, `tsconfig.json`, `pyproject.toml`, `requirements.txt` or `go.mod` exist at the repo root, and that `Cargo.toml` has no `build =` key (no build script that could call out to another runtime).

## Out of scope
- Any subcommand (`list`, `mandate`, `whoami`, and the rest), the core, the Mandate store port, the Terminal port, IMAP, TLS, MCP and the audit log. These start with ticket 02 and later tickets.
- Dependencies beyond `clap`: tokio, async-imap, rustls, keyring and the others listed in the feature spec are added by the tickets that need them.
- What the binary does with no arguments or with `--help`. Whatever clap does by default is fine, and no test pins it down.
- CI workflows, release builds, signing, the hardened-runtime self-check and installers (ticket 16), and Windows (ticket 18).
- `rust-toolchain.toml`, a pinned MSRV, and an edition decision beyond picking one current stable edition (2021 or 2024) in `Cargo.toml`.
- README changes.

## Notes for reviewers
- Security: No Credentials, Mailbox access or untrusted input in this job. Check that the commit adds no secrets and no fixture files, and that `/target` is git-ignored so build artefacts are never committed. Supply chain: the only runtime dependency allowed is `clap`, and there is no build script (`build.rs`). Keeping `src/lib.rs` in the same package does not add a crate a release could be pointed at. Ticket 02 still has to keep the test-only in-memory Mandate store behind `#[cfg(test)]` or an equivalent test-only gate.
- UX: Two users. The developer (and the factory builder agent) runs `cargo test` and the lint commands. The human mail owner, or an Agent, runs `agent-mail --version` to confirm which build is installed. The output is a single predictable line in the conventional `<name> <version>` form, so scripts and the future installer (ticket 16) can parse it.
- UI: Terminal output only: one line on stdout for `--version`. No pages or prompts.
- Ticket fidelity: AC1 to AC5 are the ticket's five criteria in order, with only concrete inputs and outputs added. On "single binary crate": this spec has the package carry a library target (`src/lib.rs`) next to its one binary target. That is still one Cargo package with one binary and no workspace, so it matches ADR-0001 and the CLAUDE.md wording, and it is what lets the feature spec's in-process core and adapter tests reach the code. Reviewers who read "one binary crate" strictly as "no lib target" should raise it here rather than block the build.
- Testability: AC3 is checked by running the lint commands rather than by a test inside `cargo test`. Calling cargo from inside `cargo test` would contend for the target-directory lock and slow the suite. The builder records the exact commands and their exit status in build.md as the "test" for AC3. AC4's test pins three CLAUDE.md lines verbatim, so a later job that deliberately rewords them must update `tests/project.rs` too. That is intended, because the ticket requires those lines to be kept.
