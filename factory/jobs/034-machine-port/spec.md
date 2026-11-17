# `Machine` port on `Context`

## Goal
This is a prefactor with no visible change. The core gains a `Machine` port, the wizard's view of the operating system. It provides the home, application-data and current folders, the running binary's path, the install location, the folders searched for `claude`, and running `claude` and `git` with arguments. Later setup-wizard tickets (02 to 09) build `agent-mail setup` on this port and test it in-process against a scripted `Machine`. Source: ticket `.scratch/setup-wizard/issues/01-machine-port.md`, feature spec `.scratch/setup-wizard/spec.md` (Implementation Decisions, "New `Machine` port"; Testing Decisions).

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`. Add no new dependencies. `Cargo.toml` and `Cargo.lock` stay unchanged: `windows-sys` already has `Win32_UI_Shell` (`SHGetKnownFolderPath`, `FOLDERID_*`) and `Win32_System_Com`.
- Files this touches:
  - `src/machine.rs` (new). It holds:
    - the `Machine` trait;
    - the run result types;
    - the platform implementation `PlatformMachine`, a unit struct so `&PlatformMachine` is `'static` like `&NoTerminal`;
    - the test implementation `ScriptedMachine`, with `Reply` and `RecordedCommand`, behind `cfg(any(test, feature = "test-support"))`.
  - `src/lib.rs`: `pub mod machine;`.
  - `src/core.rs`:
    - `Context` gains `machine: &'a dyn Machine`.
    - `Context::new` sets `&PlatformMachine`.
    - Add the builder `with_machine` and the accessor `machine()`.
    - `execute_reporting`'s struct literal carries `context.machine`.
  - `src/broker.rs` (`context_for`) and `src/broker/windows.rs` (the session `Context` literal): add the field. `context_for` carries `context.machine`. The Windows session uses `&PlatformMachine`. No Broker request uses it.
  - `src/audit.rs`: share `home_from_user_database` (Unix) and the `known_folder` / `SHGetKnownFolderPath` helper (Windows) with `machine.rs`, so each rule has one implementation. The audit log's behaviour does not change.
  - `tests/machine.rs` (new): the integration tests below.
  - `CLAUDE.md`: the layout line and the test-only line.
- Suggested interface. The tests below use these names. If the builder renames anything, the tests and `CLAUDE.md` must follow.
  ```rust
  pub trait Machine {
      fn home(&self) -> Option<PathBuf>;
      fn app_data(&self) -> Option<PathBuf>;
      fn current_dir(&self) -> Option<PathBuf>;
      fn current_exe(&self) -> Option<PathBuf>;
      fn install_dir(&self) -> Option<PathBuf>;
      fn claude_dirs(&self) -> Vec<PathBuf>;
      fn run(&self, program: &Path, args: &[&str], dir: &Path) -> RunOutcome;
  }
  pub enum RunOutcome { Exited(ProgramOutput), NotStarted { reason: String } }
  pub struct ProgramOutput { pub code: Option<i32>, pub stdout: String, pub stderr: String }
  ```
- Platform values:

  | Method | macOS | Windows |
  |---|---|---|
  | `home` | `getpwuid_r` home (user database) | `FOLDERID_Profile` |
  | `app_data` | `<home>/Library/Application Support` | `FOLDERID_RoamingAppData` (what `%APPDATA%` names; Claude Desktop keeps its config there) |
  | `current_dir` | `std::env::current_dir().ok()` | same |
  | `current_exe` | `std::env::current_exe().ok()` | same |
  | `install_dir` | `<home>/.local/bin` (as `install.sh`) | `FOLDERID_ProgramFiles\agent-mail` (as `install.ps1`) |
  | `claude_dirs` | absolute entries of the process `PATH` in order, then `<home>/.local/bin`, `<home>/.claude/local`, `/opt/homebrew/bin`, `/usr/local/bin` | absolute `PATH` entries, then `<home>\.local\bin`, `<app_data>\npm` |

  Folders that cannot be determined give `None`, or are left out of `claude_dirs`. Other Unix only needs to compile.
- `run` on the platform uses `std::process::Command` directly, never through a shell:
  - working folder `dir`;
  - stdin null;
  - stdout and stderr captured, and decoded with `from_utf8_lossy`.

  A spawn error gives `RunOutcome::NotStarted`.
- Glossary terms: Agent, Agent App, Mandate, Agent Token, Broker. "destination" and "Agent App" carry the feature spec's meaning. Avoid "setup" as a name for a Mandate.

## Acceptance criteria
1. AC1: `Context` carries a `Machine`. `Context::new` uses the platform implementation, and a builder method swaps it like `with_terminal`.
   - Test: `Context::new(&store).machine().home()` equals `PlatformMachine.home()`. `Context::new(&store).machine().install_dir()` equals `PlatformMachine.install_dir()`.
   - Test: with `let m = ScriptedMachine::new(vec![])`, `Context::new(&store).with_machine(&m).machine().home()` equals `m.home()`, which starts with `m.root()`.
   - Test: `Context::new(&store).with_machine(&m).with_terminal(&t)` still answers `machine().home()` with `m.home()`, and so does `with_terminal(&t).with_machine(&m)`. The builder order does not matter.
   - Test: `Context::trusting_only(&store, roots)` uses the platform implementation (its `machine().home()` equals `PlatformMachine.home()`).
2. AC2: The platform implementation takes the home folder from the user database, not from `$HOME` (the audit log's rule), and the Windows application-data folder from the known-folder API.
   - Test (probe, both OSes): a test in `tests/machine.rs` runs its own test binary (`std::env::current_exe()`) again, with `--exact <probe test name> --nocapture`. The child gets these environment overrides:
     - `HOME`, `USERPROFILE` and `APPDATA` set to `<temp>/not-home`;
     - `PATH` set to `std::env::join_paths(["relative-dir", "<temp>/abs-dir"])`;
     - `AGENT_MAIL_MACHINE_PROBE=1`.

     The probe test does nothing unless `AGENT_MAIL_MACHINE_PROBE` is set. When it is set, it prints `home=`, `app_data=` and `claude_dirs=` lines with `PlatformMachine`'s values in `{:?}` form. The parent asserts:
     - the child's `home` and `app_data` lines equal the parent's own `{:?}` of `PlatformMachine.home()` and `.app_data()`, so the environment overrides changed nothing;
     - neither line contains `not-home`;
     - the child's `claude_dirs` starts with `<temp>/abs-dir`, and `relative-dir` is not in it.
   - Test (macOS only): `audit::platform_path()` equals `PlatformMachine.app_data().unwrap().join("agent-mail").join("audit.log")`, and `PlatformMachine.install_dir()` equals `home.join(".local").join("bin")`.
   - Test (Windows only):
     - `app_data()` is absolute and, when the test process has `APPDATA` set, equals it;
     - `install_dir()` ends with `agent-mail` and, when `ProgramFiles` is set, equals `<ProgramFiles>\agent-mail`.
   - Test: `PlatformMachine.claude_dirs()` ends with the platform's usual folders, in the order given in Context. On macOS that is `[<home>/.local/bin, <home>/.claude/local, /opt/homebrew/bin, /usr/local/bin]`. On Windows it is `[<home>\.local\bin, <app_data>\npm]`.
   - Test: `PlatformMachine.current_dir()` equals `std::env::current_dir().ok()`, and `current_exe()` equals `std::env::current_exe().ok()`.
3. AC3: Running a program returns its exit status and output. A program that cannot be started is a distinct result, not a panic.
   - Test (Unix): `PlatformMachine.run(Path::new("/bin/sh"), &["-c", "printf out; printf err >&2; exit 3"], tmp)` returns `RunOutcome::Exited` with `code == Some(3)`, `stdout == "out"` and `stderr == "err"`.
   - Test (Windows): `run(Path::new("cmd.exe"), &["/C", "echo out& echo err 1>&2& exit 3"], tmp)` returns `Exited` with `code == Some(3)`. The trimmed stdout is `"out"` and the trimmed stderr is `"err"`.
   - Test (working folder): `tmp` holds `marker.txt` with content `here`. Unix runs `/bin/sh -c "cat marker.txt"` and Windows runs `cmd.exe /C type marker.txt`, both in `tmp`. Each returns `Exited` with `code == Some(0)` and trimmed stdout `"here"`.
   - Test: `run(&tmp.join("no-such-program"), &["--version"], tmp)` returns `RunOutcome::NotStarted { .. }` and does not panic.
4. AC4: The test implementation sits behind `cfg(any(test, feature = "test-support"))`. It roots every folder in a fresh temporary folder, plays back scripted replies for `claude` and `git`, and exposes the recorded commands (program, arguments, working folder).
   - Test: `ScriptedMachine::new(vec![])` gives a `root()` under `std::env::temp_dir()`. A second `ScriptedMachine::new` gets a different `root()`.
   - Test: these all start with `root()`, and none of them equals another:
     - `home()`;
     - `app_data()`;
     - `current_dir()`;
     - `current_exe()`;
     - `install_dir()`;
     - every entry of `claude_dirs()` (there is at least one).

     `home()`, `app_data()` and `current_dir()` exist as folders right after `new`.
   - Test: `m = ScriptedMachine::new(vec![Reply::exit(0, "claude 2.0.0", ""), Reply::exit(1, "", "fatal: no"), Reply::not_started()])` answers three calls in order:
     - `run(Path::new("claude"), &["--version"], d1)` returns `Exited { code: Some(0), stdout: "claude 2.0.0", stderr: "" }`;
     - `run(Path::new("git"), &["check-ignore", "-q", ".claude/settings.local.json"], d2)` returns `Exited` with `code == Some(1)` and `stderr == "fatal: no"`;
     - `run(&m.claude_dirs()[0].join("claude"), &["plugin", "list"], d1)` returns `NotStarted`.

     Afterwards `m.commands()` equals the three `RecordedCommand { program, args, dir }` values, in call order. `m.remaining()` is 0.
   - Test (`#[should_panic]`): running `git` when no reply is left panics, and the message names the program, as `ScriptedTerminal` does when its script runs out.
   - Test (`#[should_panic]`): running a program whose file stem is neither `claude` nor `git` (for example `npm`) panics. The stems `claude.exe`, `claude.cmd` and `git.exe` are accepted.
   - Test: after `drop(m)`, its `root()` folder no longer exists.
   - Check: `ScriptedMachine`, `Reply` and `RecordedCommand` are inside a `cfg(any(test, feature = "test-support"))` item. `cargo build --release` succeeds without the feature. A grep of `src/machine.rs` shows the cfg gate on the test module.
5. AC5: An in-process test exercises the test implementation's folders and recording. No existing command changes its behaviour, and every existing test passes unchanged.
   - Test: build `Context::new(&store).with_machine(&m)` with the in-memory store. Call `context.machine().run(Path::new("git"), &["rev-parse", "--show-toplevel"], &context.machine().current_dir().unwrap())`. It returns the scripted reply, and `m.commands()` records exactly that program, those arguments and that folder.
   - Check: `git diff main --stat -- tests/` lists only the new `tests/machine.rs`. No existing file under `tests/` changes. `src/main.rs` needs no change.
6. AC6: CLAUDE.md's layout description names the port.
   - Test:
     - the existing `tests/project.rs` layout test passes, which means the layout line names `` `machine.rs` `` and `` `tests/machine.rs` ``;
     - a new assertion in `tests/machine.rs` checks that `CLAUDE.md` contains `` `Machine` `` and `` `ScriptedMachine` ``;
     - the "Test-only code" line names `ScriptedMachine`.
7. AC7: `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass. CI runs them on macOS and Windows. The `machine` field must be read in non-test builds (through the public `machine()` accessor), so no `dead_code` warning appears.

## Out of scope
- The `setup` subcommand, the wizard module, and any terminal text (ticket 02 onward).
- The `claude` lookup order, "the first that runs `claude --version`", and npm's global bin folder on macOS: ticket 04 adds the npm folder, which may itself need running `npm`. This job provides only the folder list and `run`.
- Copying the running binary to `install_dir()` (ticket 07).
- Launching the Windows elevated child, and installing or upgrading with the Broker through the `Machine` (tickets 08 and 09). The feature spec puts these on the port later.
- Timeouts for child programs, and stdin forwarding.
- Locating Claude Desktop's config, including the Microsoft Store redirect (ticket 05).
- Any change to `mandate create`, the Mandate model, the Broker protocol, the MCP server or the audit log's path rule.

## Notes for reviewers
- Security:
  - No credentials or Agent Tokens pass through this port in this slice.
  - `run` must never go through a shell, and arguments are passed as separate items. Later tickets will pass human-typed folder paths as working folders. Ticket 04 must re-check this for `claude.cmd` on Windows, where Rust escapes the arguments of batch files itself.
  - `claude_dirs` drops relative and empty `PATH` entries, so a `.` in `PATH` cannot make the wizard run a `claude` planted in the current project folder. AC2's probe covers this.
  - The home and application-data folders come from the user database and the known-folder API, never from `HOME`, `USERPROFILE` or `APPDATA`. The environment cannot redirect where later tickets write tokens.
  - Child processes inherit the environment, including any `AGENT_MAIL_TOKEN` of the human's shell. Ticket 04 may want `env_remove(TOKEN_VAR)` before running `claude` or `git`. This job does not require it, because the inheritance cannot be tested without changing the test process's environment.
  - `ScriptedMachine` must not compile into release builds (AC4).
  - The Broker's per-session `Context` gets a `Machine` but never calls it. If reviewers prefer, an inert implementation there is fine, as long as no Broker test changes.
  - Windows "user database" home: Windows has no `getpwuid`. The ticket's "user database" rule is read as `FOLDERID_Profile` from the known-folder API, so `USERPROFILE` does not redirect it.
- UX: there is no operator or Agent-facing change. Builders of tickets 02 to 09 are the users of the port. They build a `ScriptedMachine` with replies and pass it with `with_machine`. Contexts from `Context::trusting_only` keep the platform `Machine`, so any future test that reaches a `run` call must swap in a `ScriptedMachine`, or it would run the real `claude` or `git`. If `Context` has to be `Sync` somewhere, `ScriptedMachine` can use a `Mutex` instead of a `RefCell`.
- UI: no UI surface.
- Ticket criteria are carried over unchanged and in order as AC1 to AC7. None contradicts an ADR.
  - ADR-0004: the Broker stays the only place Mandates are stored on Windows.
  - ADR-0005: `install_dir` matches the installers' locations.
