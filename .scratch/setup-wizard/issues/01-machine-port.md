# 01: `Machine` port on `Context`

**What to build:** a prefactor with no visible change. The core gains a port for everything `agent-mail setup` will touch outside agent-mail: the home, application-data and current folders, the running binary's path and the install location, the folders searched for `claude`, and running `claude` and `git` with arguments. The platform implementation is the default; a test implementation, rooted in a temporary folder, answers `claude` and `git` from a script and records every command it was asked to run. See `../spec.md`, Implementation Decisions ("New `Machine` port") and Testing Decisions.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] `Context` carries a `Machine`; `Context::new` uses the platform implementation, and a builder method swaps it like `with_terminal`
- [ ] The platform implementation takes the home folder from the user database, not from `$HOME` (the audit log's rule), and the Windows application-data folder from the known-folder API
- [ ] Running a program returns its exit status and output; a program that cannot be started is a distinct result, not a panic
- [ ] The test implementation sits behind `cfg(any(test, feature = "test-support"))`, roots every folder in a fresh temporary folder, plays back scripted replies for `claude` and `git`, and exposes the recorded commands (program, arguments, working folder)
- [ ] An in-process test exercises the test implementation's folders and recording; no behaviour of any existing command changes and every existing test passes unchanged
- [ ] CLAUDE.md's layout description names the port
- [ ] `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass

## Comments
