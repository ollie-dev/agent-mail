# 01: Project skeleton

**What to build:** A Rust project for the single `agent-mail` binary exists, builds, and reports its version. The stack is recorded so every later job builds on it (ADR-0001). No mail functionality yet.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] `agent-mail --version` prints the binary name and the crate version and exits 0
- [ ] `cargo test` runs green and contains at least one test that exercises the `--version` behaviour
- [ ] `cargo clippy -- -D warnings` and `cargo fmt --check` pass
- [ ] The `## Stack` section of CLAUDE.md keeps its language, test and lint lines, and its layout line is replaced with the actual source and test layout
- [ ] The project is a single binary crate; no other language runtime is required to build or run it
