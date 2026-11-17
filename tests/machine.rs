//! The `Machine` port: the platform implementation's folders and `run`, the
//! scripted test implementation, and `Context` carrying one.

use std::path::{Path, PathBuf};
use std::process::Command;

use agent_mail::Context;
use agent_mail::machine::{
    Machine, PlatformMachine, ProgramOutput, RecordedCommand, Reply, RunOutcome, ScriptedMachine,
};
use agent_mail::store::InMemoryStore;
use agent_mail::terminal::ScriptedTerminal;

/// A fresh folder under the system temporary folder, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "agent-mail-machine-test-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn exited(outcome: RunOutcome) -> ProgramOutput {
    match outcome {
        RunOutcome::Exited(output) => output,
        other => panic!("expected Exited, got {other:?}"),
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// AC1: `Context` carries a `Machine`.

#[test]
fn ac1_context_new_uses_the_platform_machine() {
    let store = InMemoryStore::new();
    let context = Context::new(&store);
    assert_eq!(context.machine().home(), PlatformMachine.home());
    assert_eq!(
        context.machine().install_dir(),
        PlatformMachine.install_dir()
    );
}

#[test]
fn ac1_with_machine_swaps_the_machine() {
    let store = InMemoryStore::new();
    let m = ScriptedMachine::new(vec![]);
    let context = Context::new(&store).with_machine(&m);
    assert_eq!(context.machine().home(), m.home());
    assert!(m.home().unwrap().starts_with(m.root()));
}

#[test]
fn ac1_builder_order_does_not_matter() {
    let store = InMemoryStore::new();
    let m = ScriptedMachine::new(vec![]);
    let t = ScriptedTerminal::new(vec![]);
    let machine_first = Context::new(&store).with_machine(&m).with_terminal(&t);
    assert_eq!(machine_first.machine().home(), m.home());
    let terminal_first = Context::new(&store).with_terminal(&t).with_machine(&m);
    assert_eq!(terminal_first.machine().home(), m.home());
}

#[test]
fn ac1_trusting_only_uses_the_platform_machine() {
    let store = InMemoryStore::new();
    let context = Context::trusting_only(&store, Vec::new());
    assert_eq!(context.machine().home(), PlatformMachine.home());
}

// AC2: the platform implementation's folders.

const PROBE_VAR: &str = "AGENT_MAIL_MACHINE_PROBE";
const PROBE_TEST: &str = "ac2_probe_prints_platform_folders";

/// Does nothing unless run as the child of
/// `ac2_environment_does_not_redirect_home_or_app_data`.
#[test]
fn ac2_probe_prints_platform_folders() {
    if std::env::var_os(PROBE_VAR).is_none() {
        return;
    }
    println!("home={:?}", PlatformMachine.home());
    println!("app_data={:?}", PlatformMachine.app_data());
    println!("claude_dirs={:?}", PlatformMachine.claude_dirs());
}

#[test]
fn ac2_environment_does_not_redirect_home_or_app_data() {
    let temp = TempDir::new("probe");
    let not_home = temp.path().join("not-home");
    let abs_dir = temp.path().join("abs-dir");
    let path = std::env::join_paths([PathBuf::from("relative-dir"), abs_dir.clone()]).unwrap();

    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", PROBE_TEST, "--nocapture"])
        .env("HOME", &not_home)
        .env("USERPROFILE", &not_home)
        .env("APPDATA", &not_home)
        .env("PATH", &path)
        .env(PROBE_VAR, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "probe failed: {stdout}");
    let line = |key: &str| {
        let prefix = format!("{key}=");
        stdout
            .lines()
            .find_map(|line| line.find(&prefix).map(|at| &line[at + prefix.len()..]))
            .unwrap_or_else(|| panic!("probe printed no {key} line: {stdout}"))
            .to_string()
    };

    let home = line("home");
    let app_data = line("app_data");
    assert_eq!(home, format!("{:?}", PlatformMachine.home()));
    assert_eq!(app_data, format!("{:?}", PlatformMachine.app_data()));
    assert!(!home.contains("not-home"), "{home}");
    assert!(!app_data.contains("not-home"), "{app_data}");

    let claude_dirs = line("claude_dirs");
    let first = format!("[{:?}", abs_dir);
    assert!(
        claude_dirs.starts_with(&first),
        "{claude_dirs} does not start with {first}"
    );
    assert!(!claude_dirs.contains("relative-dir"), "{claude_dirs}");
}

#[cfg(target_os = "macos")]
#[test]
fn ac2_macos_app_data_matches_the_audit_log_and_install_dir_matches_install_sh() {
    let app_data = PlatformMachine.app_data().unwrap();
    assert_eq!(
        agent_mail::audit::platform_path(),
        Some(app_data.join("agent-mail").join("audit.log"))
    );
    let home = PlatformMachine.home().unwrap();
    assert_eq!(
        PlatformMachine.install_dir(),
        Some(home.join(".local").join("bin"))
    );
}

#[cfg(windows)]
#[test]
fn ac2_windows_app_data_and_install_dir_come_from_known_folders() {
    let app_data = PlatformMachine.app_data().unwrap();
    assert!(app_data.is_absolute(), "{app_data:?}");
    if let Some(env) = std::env::var_os("APPDATA") {
        assert_eq!(app_data, PathBuf::from(env));
    }
    let install = PlatformMachine.install_dir().unwrap();
    assert!(install.ends_with("agent-mail"), "{install:?}");
    if let Some(env) = std::env::var_os("ProgramFiles") {
        assert_eq!(install, PathBuf::from(env).join("agent-mail"));
    }
}

#[cfg(target_os = "macos")]
#[test]
fn ac2_claude_dirs_end_with_the_usual_folders() {
    let home = PlatformMachine.home().unwrap();
    let usual = [
        home.join(".local").join("bin"),
        home.join(".claude").join("local"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    assert!(
        PlatformMachine.claude_dirs().ends_with(&usual),
        "{:?}",
        PlatformMachine.claude_dirs()
    );
}

#[cfg(windows)]
#[test]
fn ac2_claude_dirs_end_with_the_usual_folders() {
    let home = PlatformMachine.home().unwrap();
    let app_data = PlatformMachine.app_data().unwrap();
    let usual = [home.join(".local").join("bin"), app_data.join("npm")];
    assert!(
        PlatformMachine.claude_dirs().ends_with(&usual),
        "{:?}",
        PlatformMachine.claude_dirs()
    );
}

#[test]
fn ac2_current_dir_and_current_exe_are_the_process_values() {
    assert_eq!(PlatformMachine.current_dir(), std::env::current_dir().ok());
    assert_eq!(PlatformMachine.current_exe(), std::env::current_exe().ok());
}

// AC3: running a program.

#[cfg(unix)]
#[test]
fn ac3_run_returns_exit_code_and_output() {
    let tmp = TempDir::new("run");
    let output = exited(PlatformMachine.run(
        Path::new("/bin/sh"),
        &["-c", "printf out; printf err >&2; exit 3"],
        tmp.path(),
    ));
    assert_eq!(output.code, Some(3));
    assert_eq!(output.stdout, "out");
    assert_eq!(output.stderr, "err");
}

#[cfg(windows)]
#[test]
fn ac3_run_returns_exit_code_and_output() {
    let tmp = TempDir::new("run");
    let output = exited(PlatformMachine.run(
        Path::new("cmd.exe"),
        &["/C", "echo out& echo err 1>&2& exit 3"],
        tmp.path(),
    ));
    assert_eq!(output.code, Some(3));
    assert_eq!(output.stdout.trim(), "out");
    assert_eq!(output.stderr.trim(), "err");
}

#[test]
fn ac3_run_uses_the_working_folder() {
    let tmp = TempDir::new("cwd");
    std::fs::write(tmp.path().join("marker.txt"), "here").unwrap();
    #[cfg(unix)]
    let outcome = PlatformMachine.run(Path::new("/bin/sh"), &["-c", "cat marker.txt"], tmp.path());
    #[cfg(windows)]
    let outcome = PlatformMachine.run(Path::new("cmd.exe"), &["/C", "type marker.txt"], tmp.path());
    let output = exited(outcome);
    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout.trim(), "here");
}

#[test]
fn ac3_a_missing_program_is_not_started() {
    let tmp = TempDir::new("missing");
    let outcome = PlatformMachine.run(
        &tmp.path().join("no-such-program"),
        &["--version"],
        tmp.path(),
    );
    assert!(
        matches!(outcome, RunOutcome::NotStarted { .. }),
        "{outcome:?}"
    );
}

// AC4: the scripted test implementation.

#[test]
fn ac4_each_scripted_machine_gets_a_fresh_root_under_temp() {
    let first = ScriptedMachine::new(vec![]);
    let second = ScriptedMachine::new(vec![]);
    assert!(first.root().starts_with(std::env::temp_dir()));
    assert!(second.root().starts_with(std::env::temp_dir()));
    assert_ne!(first.root(), second.root());
}

#[test]
fn ac4_every_folder_is_distinct_and_under_root() {
    let m = ScriptedMachine::new(vec![]);
    let claude_dirs = m.claude_dirs();
    assert!(!claude_dirs.is_empty());
    let mut all = vec![
        m.home().unwrap(),
        m.app_data().unwrap(),
        m.current_dir().unwrap(),
        m.current_exe().unwrap(),
        m.install_dir().unwrap(),
    ];
    all.extend(claude_dirs);
    for (i, path) in all.iter().enumerate() {
        assert!(path.starts_with(m.root()), "{path:?} is not under the root");
        for other in &all[i + 1..] {
            assert_ne!(path, other);
        }
    }
    for dir in [m.home(), m.app_data(), m.current_dir()] {
        let dir = dir.unwrap();
        assert!(dir.is_dir(), "{dir:?} does not exist");
    }
}

#[test]
fn ac4_replies_play_back_in_order_and_commands_are_recorded() {
    let m = ScriptedMachine::new(vec![
        Reply::exit(0, "claude 2.0.0", ""),
        Reply::exit(1, "", "fatal: no"),
        Reply::not_started(),
    ]);
    let d1 = m.current_dir().unwrap();
    let d2 = m.home().unwrap();
    let claude = m.claude_dirs()[0].join("claude");

    assert_eq!(
        m.run(Path::new("claude"), &["--version"], &d1),
        RunOutcome::Exited(ProgramOutput {
            code: Some(0),
            stdout: "claude 2.0.0".to_string(),
            stderr: String::new(),
        })
    );
    let git = exited(m.run(
        Path::new("git"),
        &["check-ignore", "-q", ".claude/settings.local.json"],
        &d2,
    ));
    assert_eq!(git.code, Some(1));
    assert_eq!(git.stderr, "fatal: no");
    assert!(matches!(
        m.run(&claude, &["plugin", "list"], &d1),
        RunOutcome::NotStarted { .. }
    ));

    let recorded = |program: &Path, args: &[&str], dir: &Path| RecordedCommand {
        program: program.to_path_buf(),
        args: args.iter().map(|a| a.to_string()).collect(),
        dir: dir.to_path_buf(),
    };
    assert_eq!(
        m.commands(),
        vec![
            recorded(Path::new("claude"), &["--version"], &d1),
            recorded(
                Path::new("git"),
                &["check-ignore", "-q", ".claude/settings.local.json"],
                &d2
            ),
            recorded(&claude, &["plugin", "list"], &d1),
        ]
    );
    assert_eq!(m.remaining(), 0);
}

#[test]
#[should_panic(expected = "git")]
fn ac4_running_out_of_replies_panics_naming_the_program() {
    let m = ScriptedMachine::new(vec![]);
    let dir = m.current_dir().unwrap();
    m.run(Path::new("git"), &["status"], &dir);
}

#[test]
#[should_panic(expected = "npm")]
fn ac4_a_program_other_than_claude_or_git_panics() {
    let m = ScriptedMachine::new(vec![Reply::exit(0, "", "")]);
    let dir = m.current_dir().unwrap();
    m.run(Path::new("npm"), &["--version"], &dir);
}

#[test]
fn ac4_platform_suffixes_of_claude_and_git_are_accepted() {
    let m = ScriptedMachine::new(vec![
        Reply::exit(0, "a", ""),
        Reply::exit(0, "b", ""),
        Reply::exit(0, "c", ""),
    ]);
    let dir = m.current_dir().unwrap();
    for (program, stdout) in [("claude.exe", "a"), ("claude.cmd", "b"), ("git.exe", "c")] {
        let output = exited(m.run(Path::new(program), &[], &dir));
        assert_eq!(output.stdout, stdout);
    }
    assert_eq!(m.remaining(), 0);
}

#[test]
fn ac4_dropping_the_machine_removes_its_root() {
    let m = ScriptedMachine::new(vec![]);
    let root = m.root().to_path_buf();
    assert!(root.is_dir());
    drop(m);
    assert!(!root.exists(), "{root:?} still exists");
}

#[test]
fn ac4_scripted_machine_is_test_only() {
    let source = std::fs::read_to_string(repo_root().join("src").join("machine.rs")).unwrap();
    let gate = "#[cfg(any(test, feature = \"test-support\"))]\nmod scripted {";
    let module = source
        .find(gate)
        .expect("the scripted module is not behind the test-support cfg");
    assert!(source.contains(
        "#[cfg(any(test, feature = \"test-support\"))]\npub use scripted::{RecordedCommand, Reply, ScriptedMachine};"
    ));
    for item in [
        "pub struct ScriptedMachine",
        "pub struct Reply",
        "pub struct RecordedCommand",
    ] {
        let at = source.find(item).unwrap_or_else(|| panic!("no {item}"));
        assert!(at > module, "{item} is outside the gated module");
        assert_eq!(source.matches(item).count(), 1, "{item}");
    }
}

// AC5: the Context runs commands through the scripted Machine.

#[test]
fn ac5_context_runs_through_the_scripted_machine() {
    let store = InMemoryStore::new();
    let m = ScriptedMachine::new(vec![Reply::exit(0, "/repo\n", "")]);
    let context = Context::new(&store).with_machine(&m);
    let dir = context.machine().current_dir().unwrap();
    let output = exited(context.machine().run(
        Path::new("git"),
        &["rev-parse", "--show-toplevel"],
        &dir,
    ));
    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout, "/repo\n");
    assert_eq!(
        m.commands(),
        vec![RecordedCommand {
            program: PathBuf::from("git"),
            args: vec!["rev-parse".to_string(), "--show-toplevel".to_string()],
            dir,
        }]
    );
}

// AC6: CLAUDE.md names the port.

#[test]
fn ac6_claude_md_names_the_machine_port() {
    let claude = std::fs::read_to_string(repo_root().join("CLAUDE.md")).unwrap();
    assert!(claude.contains("`Machine`"));
    assert!(claude.contains("`ScriptedMachine`"));
    let test_only = claude
        .lines()
        .find(|line| line.starts_with("- Test-only code"))
        .expect("CLAUDE.md has no Test-only code line");
    assert!(test_only.contains("ScriptedMachine"), "{test_only}");
}
