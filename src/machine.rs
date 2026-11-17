//! The `Machine` port: the setup wizard's view of the operating system.
//!
//! It gives the home, application-data and current folders, the running
//! binary's path, the install location, the folders searched for `claude`,
//! and runs `claude` and `git`. [`PlatformMachine`] is the real one;
//! [`ScriptedMachine`] (test builds only) roots every folder in a fresh
//! temporary folder and plays back scripted replies.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The operating system as the setup wizard sees it.
pub trait Machine {
    /// The human's home folder, from the user database (Unix) or the
    /// `FOLDERID_Profile` known folder (Windows), never the environment.
    fn home(&self) -> Option<PathBuf>;
    /// The per-user application-data folder: `~/Library/Application
    /// Support` on macOS, `FOLDERID_RoamingAppData` on Windows.
    fn app_data(&self) -> Option<PathBuf>;
    /// The process's current folder.
    fn current_dir(&self) -> Option<PathBuf>;
    /// The path of the running binary.
    fn current_exe(&self) -> Option<PathBuf>;
    /// Where the installers put the binary.
    fn install_dir(&self) -> Option<PathBuf>;
    /// The folders searched for `claude`, in order: the absolute `PATH`
    /// entries, then the platform's usual folders.
    fn claude_dirs(&self) -> Vec<PathBuf>;
    /// Runs `program` with `args` in `dir`, never through a shell, with
    /// stdin closed, and captures its output.
    fn run(&self, program: &Path, args: &[&str], dir: &Path) -> RunOutcome;
}

/// What running a program gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    /// The program started and ended.
    Exited(ProgramOutput),
    /// The program could not be started.
    NotStarted {
        /// Why, as the operating system said it.
        reason: String,
    },
}

/// A finished program's exit code and output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramOutput {
    /// The exit code; `None` when a signal ended the program.
    pub code: Option<i32>,
    /// Standard output, decoded lossily as UTF-8.
    pub stdout: String,
    /// Standard error, decoded lossily as UTF-8.
    pub stderr: String,
}

/// The real operating system. A unit struct, so `&PlatformMachine` is
/// `'static`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlatformMachine;

impl Machine for PlatformMachine {
    fn home(&self) -> Option<PathBuf> {
        platform::home()
    }

    fn app_data(&self) -> Option<PathBuf> {
        platform::app_data()
    }

    fn current_dir(&self) -> Option<PathBuf> {
        std::env::current_dir().ok()
    }

    fn current_exe(&self) -> Option<PathBuf> {
        std::env::current_exe().ok()
    }

    fn install_dir(&self) -> Option<PathBuf> {
        platform::install_dir()
    }

    fn claude_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = std::env::var_os("PATH")
            .map(|path| absolute_entries(&path))
            .unwrap_or_default();
        dirs.extend(platform::usual_claude_dirs());
        dirs
    }

    fn run(&self, program: &Path, args: &[&str], dir: &Path) -> RunOutcome {
        let output = Command::new(program)
            .args(args)
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();
        match output {
            Ok(output) => RunOutcome::Exited(ProgramOutput {
                code: output.status.code(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            }),
            Err(error) => RunOutcome::NotStarted {
                reason: error.to_string(),
            },
        }
    }
}

/// The absolute entries of the `PATH` value `path`, in order. Relative and
/// empty entries are dropped, so a `.` cannot point into the current folder.
fn absolute_entries(path: &std::ffi::OsStr) -> Vec<PathBuf> {
    std::env::split_paths(path)
        .filter(|dir| dir.is_absolute())
        .collect()
}

#[cfg(target_os = "macos")]
mod platform {
    use std::path::{Path, PathBuf};

    pub fn home() -> Option<PathBuf> {
        crate::audit::home_from_user_database()
    }

    pub fn app_data() -> Option<PathBuf> {
        home().map(|home| home.join("Library").join("Application Support"))
    }

    pub fn install_dir() -> Option<PathBuf> {
        home().map(|home| home.join(".local").join("bin"))
    }

    pub fn usual_claude_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(home) = home() {
            dirs.push(home.join(".local").join("bin"));
            dirs.push(home.join(".claude").join("local"));
        }
        dirs.push(Path::new("/opt/homebrew/bin").to_path_buf());
        dirs.push(Path::new("/usr/local/bin").to_path_buf());
        dirs
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    use std::path::{Path, PathBuf};

    pub fn home() -> Option<PathBuf> {
        crate::audit::home_from_user_database()
    }

    pub fn app_data() -> Option<PathBuf> {
        home().map(|home| home.join(".config"))
    }

    pub fn install_dir() -> Option<PathBuf> {
        home().map(|home| home.join(".local").join("bin"))
    }

    pub fn usual_claude_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(home) = home() {
            dirs.push(home.join(".local").join("bin"));
            dirs.push(home.join(".claude").join("local"));
        }
        dirs.push(Path::new("/usr/local/bin").to_path_buf());
        dirs
    }
}

#[cfg(windows)]
mod platform {
    use std::path::PathBuf;

    use windows_sys::Win32::UI::Shell::{
        FOLDERID_Profile, FOLDERID_ProgramFiles, FOLDERID_RoamingAppData,
    };

    use crate::audit::known_folder_path;

    pub fn home() -> Option<PathBuf> {
        known_folder_path(&FOLDERID_Profile)
    }

    pub fn app_data() -> Option<PathBuf> {
        known_folder_path(&FOLDERID_RoamingAppData)
    }

    pub fn install_dir() -> Option<PathBuf> {
        known_folder_path(&FOLDERID_ProgramFiles).map(|dir| dir.join("agent-mail"))
    }

    pub fn usual_claude_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(home) = home() {
            dirs.push(home.join(".local").join("bin"));
        }
        if let Some(app_data) = app_data() {
            dirs.push(app_data.join("npm"));
        }
        dirs
    }
}

#[cfg(not(any(unix, windows)))]
mod platform {
    use std::path::PathBuf;

    pub fn home() -> Option<PathBuf> {
        None
    }

    pub fn app_data() -> Option<PathBuf> {
        None
    }

    pub fn install_dir() -> Option<PathBuf> {
        None
    }

    pub fn usual_claude_dirs() -> Vec<PathBuf> {
        Vec::new()
    }
}

#[cfg(any(test, feature = "test-support"))]
pub use scripted::{RecordedCommand, Reply, ScriptedMachine};

#[cfg(any(test, feature = "test-support"))]
mod scripted {
    use std::collections::VecDeque;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{Machine, ProgramOutput, RunOutcome};

    /// Marker that a release build must not contain.
    pub const MARKER: &str = "agent-mail-scripted-machine";

    /// Keeps the marker bytes in every binary this module is compiled into.
    #[used]
    static MARKER_BYTES: [u8; MARKER.len()] = *b"agent-mail-scripted-machine";

    /// The programs a [`ScriptedMachine`] answers for, by file stem.
    const PROGRAMS: [&str; 2] = ["claude", "git"];

    /// A scripted answer to the next `run`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Reply(RunOutcome);

    impl Reply {
        /// The program ran and exited with `code`, printing `stdout` and
        /// `stderr`.
        pub fn exit(code: i32, stdout: &str, stderr: &str) -> Self {
            Self(RunOutcome::Exited(ProgramOutput {
                code: Some(code),
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            }))
        }

        /// The program could not be started.
        pub fn not_started() -> Self {
            Self(RunOutcome::NotStarted {
                reason: "scripted: program not started".to_string(),
            })
        }
    }

    /// One `run` call, as the [`ScriptedMachine`] received it.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RecordedCommand {
        /// The program as given.
        pub program: PathBuf,
        /// The arguments, in order.
        pub args: Vec<String>,
        /// The working folder.
        pub dir: PathBuf,
    }

    /// A test machine rooted in a fresh temporary folder, removed on drop.
    /// It answers `run` for `claude` and `git` only, with the scripted
    /// replies in order, and records every call.
    #[derive(Debug)]
    pub struct ScriptedMachine {
        root: PathBuf,
        replies: Mutex<VecDeque<Reply>>,
        commands: Mutex<Vec<RecordedCommand>>,
    }

    impl ScriptedMachine {
        /// A machine in a new temporary folder that will give `replies` in
        /// order. Creates the home, application-data and current folders.
        pub fn new(replies: Vec<Reply>) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let root = std::env::temp_dir().join(format!(
                "agent-mail-machine-{}-{nanos}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            let machine = Self {
                root,
                replies: Mutex::new(replies.into()),
                commands: Mutex::new(Vec::new()),
            };
            for dir in [machine.home_dir(), machine.app_data_dir(), machine.cwd()] {
                std::fs::create_dir_all(&dir)
                    .unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
            }
            machine
        }

        /// The temporary folder every folder of this machine is under.
        pub fn root(&self) -> &Path {
            &self.root
        }

        /// Every `run` call so far, in call order.
        pub fn commands(&self) -> Vec<RecordedCommand> {
            self.commands.lock().unwrap().clone()
        }

        /// How many scripted replies are left.
        pub fn remaining(&self) -> usize {
            self.replies.lock().unwrap().len()
        }

        fn home_dir(&self) -> PathBuf {
            self.root.join("home")
        }

        fn app_data_dir(&self) -> PathBuf {
            self.root.join("app-data")
        }

        fn cwd(&self) -> PathBuf {
            self.root.join("project")
        }
    }

    impl Machine for ScriptedMachine {
        fn home(&self) -> Option<PathBuf> {
            Some(self.home_dir())
        }

        fn app_data(&self) -> Option<PathBuf> {
            Some(self.app_data_dir())
        }

        fn current_dir(&self) -> Option<PathBuf> {
            Some(self.cwd())
        }

        fn current_exe(&self) -> Option<PathBuf> {
            Some(
                self.root
                    .join("downloads")
                    .join(format!("agent-mail{}", std::env::consts::EXE_SUFFIX)),
            )
        }

        fn install_dir(&self) -> Option<PathBuf> {
            Some(self.root.join("install"))
        }

        fn claude_dirs(&self) -> Vec<PathBuf> {
            vec![
                self.root.join("path-bin"),
                self.home_dir().join(".local").join("bin"),
            ]
        }

        fn run(&self, program: &Path, args: &[&str], dir: &Path) -> RunOutcome {
            let stem = program.file_stem().and_then(|s| s.to_str());
            if !stem.is_some_and(|stem| PROGRAMS.contains(&stem)) {
                panic!("ScriptedMachine runs only claude and git, not {program:?}");
            }
            self.commands.lock().unwrap().push(RecordedCommand {
                program: program.to_path_buf(),
                args: args.iter().map(|arg| arg.to_string()).collect(),
                dir: dir.to_path_buf(),
            });
            let reply = self.replies.lock().unwrap().pop_front();
            match reply {
                Some(Reply(outcome)) => outcome,
                None => panic!("script ran out running {program:?} {args:?}"),
            }
        }
    }

    impl Drop for ScriptedMachine {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn marker_matches_its_bytes() {
            assert_eq!(&MARKER_BYTES, MARKER.as_bytes());
        }

        #[test]
        fn accepts_platform_suffixes_of_claude_and_git() {
            let machine = ScriptedMachine::new(vec![
                Reply::exit(0, "", ""),
                Reply::exit(0, "", ""),
                Reply::exit(0, "", ""),
            ]);
            let dir = machine.current_dir().unwrap();
            for program in ["claude.exe", "claude.cmd", "git.exe"] {
                assert!(matches!(
                    machine.run(Path::new(program), &[], &dir),
                    RunOutcome::Exited(_)
                ));
            }
            assert_eq!(machine.remaining(), 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_entries_keep_only_absolute_folders_in_order() {
        let first = std::env::temp_dir().join("first");
        let second = std::env::temp_dir().join("second");
        let path = std::env::join_paths([
            first.clone(),
            PathBuf::from("."),
            PathBuf::from("rel"),
            second.clone(),
        ])
        .unwrap();
        assert_eq!(absolute_entries(&path), vec![first, second]);
    }
}
