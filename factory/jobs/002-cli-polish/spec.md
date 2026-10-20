# CLI polish: no-argument usage hint and mandate-free help description

## Goal
An operator who runs `agent-mail` with no arguments gets a short usage hint pointing at `--help` instead of a silent exit. The one-line description shown by `--help` describes what agent-mail does today without mentioning Mandates, a concept no command exposes yet.

## Context
- Stack: Rust, one binary crate, clap 4 with the `derive` feature (`docs/adr/0001-rust-not-bun.md`).
- Test command: `cargo test`; lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Files touched:
  - `src/lib.rs`: the `Cli` struct and `run`. Today `Cli {}` takes no arguments, `run` does nothing, and the binary exits 0 with no output when called bare.
  - `src/main.rs`: entry point; may need to pass the exit status through if the no-argument path is handled in `run` rather than by clap.
  - `Cargo.toml`: the `description` ("Gives AI agents mandate-scoped access to a mailbox") is what clap's `about` renders in `--help`. Change the description, or set an explicit `about` on the `Cli` struct, so the word disappears from `--help`.
  - `tests/cli.rs`: new integration tests that run `env!("CARGO_BIN_EXE_agent-mail")`, following the existing `--version` test.
- Suggested approach (not mandated): `#[command(arg_required_else_help = true)]` makes clap print help to stderr and exit with code 2 on a bare call. Any approach that meets the criteria below is fine.
- Suggested description, in glossary terms: "Lets AI agents work with email while keeping mail credentials out of their hands".

## Acceptance criteria
1. AC1: Running `agent-mail` with no arguments writes a usage hint to stderr: stderr contains the line prefix `Usage: agent-mail` and the text `--help`.
2. AC2: Running `agent-mail` with no arguments writes nothing to stdout and exits with a non-zero status code (2, clap's usage-error code).
3. AC3: Running `agent-mail --help` exits with code 0, writes nothing to stderr, and its stdout does not contain `mandate` in any letter case (checked case-insensitively).
4. AC4: Running `agent-mail --help` prints a non-empty description on stdout before the `Usage:` line, so the help still says what the tool is for. Checked by: the text before `Usage:` contains at least one non-whitespace character.
5. AC5: `Cargo.toml`'s `[package] description` does not contain `mandate` in any letter case, so package metadata and `--help` agree.
6. AC6: The existing `--version` behaviour is unchanged: `agent-mail --version` prints `agent-mail <CARGO_PKG_VERSION>\n` on stdout, nothing on stderr, and exits 0 (the existing test in `tests/cli.rs` stays green).
7. AC7: `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` all pass.

## Out of scope
- Any subcommands (`mandate create`, `list`, etc.); those come from Tickets 02 onwards, and the help description may mention Mandates again once they exist.
- Rewording other help text, adding colour, or adding a man page or long `--help` text.
- README changes (Ticket 17).

## Notes for reviewers
- Security: no credentials, mailbox access or secrets involved. The only untrusted input is argv, which clap parses; no new parsing code.
- UX: the user is the human operator at a terminal, typing `agent-mail` to see what it does. A bare call should tell them how to get help rather than exit silently with 0. Treating a bare call as a usage error (stderr, exit 2) follows clap and common CLI convention, and lets scripts notice the mistake. AC2 fixes the exit code at 2. If the team would rather a bare call exit 0, change AC2 before the build starts.
- UI: terminal output only. The bare call prints to stderr: the usage line and the `--help` pointer, or clap's full help. `--help` prints to stdout: the new one-line description, then the usage and options.
