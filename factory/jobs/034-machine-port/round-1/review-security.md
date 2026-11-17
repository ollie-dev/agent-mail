VERDICT: PASS

## Findings
- [follow-up] tests/project.rs:181: the new release marker `agent-mail-scripted-machine` (src/machine.rs:244) is not in `TEST_ONLY_MARKERS`, so `ac11_release_binary_has_no_test_only_code` does not prove `ScriptedMachine` stays out of release builds. AC5 forbids changing existing tests, and I checked it by hand (see Checked) → a later job adds `("agent-mail-scripted-machine", "src/machine.rs")` to the list.
- [follow-up] src/machine.rs:92: `PlatformMachine::run` passes the whole environment to the child, including any `AGENT_MAIL_TOKEN` from the human's shell. Nothing calls `run` yet → ticket 04 should `env_remove` the token variable before running `claude` or `git`, and test it with a scripted check or a child probe.
- [follow-up] src/machine.rs:92: on Windows, ticket 04 may run `claude.cmd`. Rust escapes batch-file arguments itself, and the fix for CVE-2024-24576 needs Rust 1.77.2 or later. The crate does not pin `rust-version` → ticket 04 should pass only fixed arguments to `.cmd` or `.bat` targets, never human-typed text, or set a `rust-version` floor.
- [follow-up] src/broker/windows.rs:993: the Broker session's `Context` gets `&PlatformMachine`. No request uses it today. If a later request handler did call `context.machine().run(...)`, the program would run as `NT SERVICE\agent-mail` → when tickets 08 and 09 touch the Broker, give the session an inert `Machine` that returns `NotStarted` and `None`, unless a Broker request really needs it.

## Checked
- Read: the full `git diff main...factory/034-machine-port` (commit 98d2df6): all of `src/machine.rs`, the changes in `src/core.rs` (field, `Context::new`, `with_machine`, `machine()`, the `execute_reporting` literal), `src/broker.rs` `context_for`, `src/broker/windows.rs` `serve_client`, `src/audit.rs` (`known_folder_path`, `program_data`, `home_from_user_database` made `pub(crate)`), `src/lib.rs`, `CLAUDE.md`, and all of `tests/machine.rs`. Then `build.md`, whose claims I checked against the code. Also read the `TEST_ONLY_MARKERS` release check in `tests/project.rs:175-245`.
- Security points checked in the code:
  - `run` calls `Command::new(program).args(args)` directly: no shell, each argument a separate item, stdin null, output captured, and a spawn error becomes `NotStarted`.
  - `claude_dirs` keeps only `is_absolute()` `PATH` entries, so `.`, empty and relative entries are dropped. On Windows that also drops `\x` and `C:x`.
  - Home comes from `getpwuid_r` and `FOLDERID_Profile`. App data comes from home or `FOLDERID_RoamingAppData`, and the install folder from `FOLDERID_ProgramFiles`. None of them reads `HOME`, `USERPROFILE` or `APPDATA`. `known_folder` still frees the buffer and keeps only absolute paths. The audit log's `local_app_data` and `program_data` rules are unchanged.
  - No credentials, tokens or secrets appear in the new code or fixtures. A grep for TOKEN, password and secret in `src/machine.rs` and `tests/machine.rs` found nothing.
  - No caller of `machine()` or `run` exists outside `src/machine.rs` (checked with grep).
  - `Cargo.toml` and `Cargo.lock` are unchanged, so there are no new dependencies.
- Ran:
  - `cargo build --release`, then `grep -a -c` on `target/release/agent-mail`: `agent-mail-scripted-machine` 0, `agent-mail-scripted-terminal` 0, "script ran out" 0.
  - Positive control: `cargo build --release --features test-support` (separate target folder) contains `agent-mail-scripted-machine` once, so the gate works.
  - `cargo test --test machine`: 22 passed, including the `HOME`/`USERPROFILE`/`APPDATA`/`PATH` probe on macOS.
  - `cargo test --lib machine`: 3 passed.
  - `cargo test --test audit --test broker`: 19 and 21 passed.
  - `cargo clippy --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.
- Not run: the Windows paths. Only macOS is available here, and the builder did not compile them either. CI on Windows will be the first run of the AC2 probe, which checks that `USERPROFILE` and `APPDATA` do not redirect `FOLDERID_Profile` or `FOLDERID_RoamingAppData`.
