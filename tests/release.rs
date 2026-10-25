//! The release files: the installer (AC5, macOS), the workflow (AC3, AC7)
//! and the release notes and README (AC7, AC10). All values are fake.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const ONE_LINER: &str =
    "curl -fsSL https://github.com/ollie-dev/agent-mail/releases/latest/download/install.sh | sh";
const KEYCHAIN_NOTE: &str = "On macOS the first Keychain access after an update prompts once, because each release has a new code signature; allow it. A Keychain prompt at any other time is a red flag: deny it.";
const TARGETS: [&str; 2] = ["aarch64-apple-darwin", "x86_64-apple-darwin"];

fn read(path: &str) -> String {
    std::fs::read_to_string(Path::new(ROOT).join(path))
        .unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

/// A fresh, empty temporary directory.
fn temp_dir(label: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "agent-mail-release-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------- AC3

#[test]
fn ac3_workflow_triggers_on_version_tags() {
    let workflow = read(".github/workflows/release.yml");
    assert!(
        workflow.contains("tags: ['v*']") || workflow.contains("tags: [\"v*\"]"),
        "no v* tag trigger"
    );
    let on = workflow.split("\non:").nth(1).expect("no on: block");
    let on = on.split("\njobs:").next().unwrap();
    assert!(on.contains("push:"), "trigger is not push");
}

#[test]
fn ac3_workflow_builds_signs_attests_and_verifies() {
    let workflow = read(".github/workflows/release.yml");
    for target in TARGETS {
        assert!(workflow.contains(target), "{target} missing");
    }
    assert!(workflow.contains("rustup target add x86_64-apple-darwin"));
    assert!(workflow.contains("cargo build --release --locked"));
    assert!(workflow.contains("scripts/sign-macos.sh"));
    assert!(workflow.contains("actions/attest-build-provenance@"));
    assert!(workflow.contains("id-token: write"));
    assert!(workflow.contains("attestations: write"));
    assert!(workflow.contains("contents: write"));
    assert!(workflow.contains("scripts/verify-release.sh"));
    assert!(workflow.contains("needs: publish"));
    assert!(!workflow.contains("dist init"), "no cargo-dist");
    // Third-party actions are pinned to a commit.
    for line in workflow.lines().filter(|l| l.contains("uses:")) {
        let reference = line.split('@').nth(1).expect("action without a version");
        let sha = reference.split_whitespace().next().unwrap();
        assert!(
            sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
            "action not pinned to a commit: {line}"
        );
    }
}

/// The body of one top-level job in the workflow, from its `  name:` line
/// up to the next job.
fn job<'a>(workflow: &'a str, name: &str) -> &'a str {
    let jobs = workflow.split("\njobs:\n").nth(1).expect("no jobs: block");
    let header = format!("\n  {name}:\n");
    let start = format!("\n{jobs}")
        .find(&header)
        .unwrap_or_else(|| panic!("no {name} job"));
    let body = &jobs[start + header.len() - 1..];
    let end = body
        .lines()
        .scan(0, |offset, line| {
            let at = *offset;
            *offset += line.len() + 1;
            Some((at, line))
        })
        .find(|(_, line)| {
            line.starts_with("  ") && !line.starts_with("   ") && line.trim_end().ends_with(':')
        })
        .map(|(at, _)| at)
        .unwrap_or(body.len());
    &body[..end]
}

#[test]
fn ac3_tests_run_in_a_read_only_job_that_publish_depends_on() {
    let workflow = read(".github/workflows/release.yml");
    let test = job(&workflow, "test");
    assert!(
        test.contains("cargo test --locked"),
        "test job runs no tests"
    );
    let permissions = test
        .split("permissions:\n")
        .nth(1)
        .expect("test job has no permissions block");
    let scopes: Vec<&str> = permissions
        .lines()
        .take_while(|l| l.starts_with("      "))
        .map(str::trim)
        .collect();
    assert_eq!(scopes, ["contents: read"], "test job permissions");
    assert!(!test.contains("write"), "test job holds a write scope");

    let publish = job(&workflow, "publish");
    assert!(
        publish.contains("needs: [test, build-windows]"),
        "publish does not need test"
    );
    assert!(
        !publish.contains("cargo test"),
        "publish still runs the tests"
    );
    assert!(publish.contains("id-token: write"));
}

#[test]
fn ac3_workflow_checks_the_tag_against_cargo_toml() {
    let workflow = read(".github/workflows/release.yml");
    let check = workflow.find("Cargo.toml").expect("no version check");
    let build = workflow.find("cargo build").unwrap();
    let test = workflow.find("cargo test").expect("no cargo test");
    assert!(
        check < test && test < build,
        "version check, test, build order"
    );
}

#[cfg(unix)]
#[test]
fn ac3_scripts_are_executable_and_parse() {
    use std::os::unix::fs::PermissionsExt;
    for script in [
        "scripts/sign-macos.sh",
        "scripts/verify-release.sh",
        "install.sh",
    ] {
        let path = Path::new(ROOT).join(script);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "{script} is not executable");
        let status = Command::new("sh").arg("-n").arg(&path).status().unwrap();
        assert!(status.success(), "{script} does not parse");
    }
    let verify = read("scripts/verify-release.sh");
    for needle in [
        "gh release download",
        "shasum -a 256 -c",
        "--check-only",
        "gh attestation verify",
        "--repo \"$repo\"",
        "--version",
    ] {
        assert!(verify.contains(needle), "verify-release.sh lacks {needle}");
    }
    assert!(verify.contains("repo=ollie-dev/agent-mail"));
    let sign = read("scripts/sign-macos.sh");
    assert!(sign.contains("codesign --sign - --options runtime --force"));
}

// ---------------------------------------------------------------- AC5

#[cfg(target_os = "macos")]
mod installer {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// The release target for this Mac.
    fn host_target() -> &'static str {
        let out = Command::new("uname").arg("-m").output().unwrap();
        match String::from_utf8_lossy(&out.stdout).trim() {
            "arm64" => "aarch64-apple-darwin",
            "x86_64" => "x86_64-apple-darwin",
            other => panic!("unexpected architecture {other}"),
        }
    }

    /// A download directory with both tarballs, each holding a script
    /// `agent-mail` that prints its target, and their `.sha256` files.
    fn downloads() -> PathBuf {
        let dir = temp_dir("downloads");
        for target in TARGETS {
            let stage = temp_dir("stage");
            let binary = stage.join("agent-mail");
            std::fs::write(&binary, format!("#!/bin/sh\necho {target}\n")).unwrap();
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
            let asset = format!("agent-mail-{target}.tar.gz");
            let status = Command::new("tar")
                .arg("-czf")
                .arg(dir.join(&asset))
                .arg("-C")
                .arg(&stage)
                .arg("agent-mail")
                .status()
                .unwrap();
            assert!(status.success());
            let sum = Command::new("shasum")
                .args(["-a", "256", &asset])
                .current_dir(&dir)
                .output()
                .unwrap();
            assert!(sum.status.success());
            std::fs::write(dir.join(format!("{asset}.sha256")), &sum.stdout).unwrap();
        }
        dir
    }

    fn install(home: &Path, downloads: &Path) -> Output {
        Command::new("sh")
            .arg(Path::new(ROOT).join("install.sh"))
            .env("HOME", home)
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .env(
                "AGENT_MAIL_DOWNLOAD_URL",
                format!("file://{}", downloads.display()),
            )
            .output()
            .unwrap()
    }

    fn bin_files(home: &Path) -> Vec<String> {
        match std::fs::read_dir(home.join(".local/bin")) {
            Ok(entries) => entries
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    #[test]
    fn ac5_installs_the_host_binary_with_mode_755() {
        let home = temp_dir("home");
        let output = install(&home, &downloads());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        let installed = home.join(".local/bin/agent-mail");
        let mode = std::fs::metadata(&installed).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        assert_eq!(bin_files(&home), vec!["agent-mail".to_string()]);
        let ran = Command::new(&installed).output().unwrap();
        assert_eq!(
            String::from_utf8_lossy(&ran.stdout).trim(),
            host_target(),
            "installed the wrong target"
        );
        assert!(
            stdout.contains(&installed.display().to_string()),
            "{stdout}"
        );
        // ~/.local/bin is not on the PATH given above.
        assert!(stdout.contains("not on your PATH"), "{stdout}");
    }

    #[test]
    fn ac5_replaces_an_existing_install() {
        let home = temp_dir("home");
        std::fs::create_dir_all(home.join(".local/bin")).unwrap();
        std::fs::write(home.join(".local/bin/agent-mail"), "old").unwrap();
        let output = install(&home, &downloads());
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        let ran = Command::new(home.join(".local/bin/agent-mail"))
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&ran.stdout).trim(), host_target());
        assert_eq!(bin_files(&home), vec!["agent-mail".to_string()]);
    }

    #[test]
    fn ac5_checksum_mismatch_aborts_without_installing() {
        let home = temp_dir("home");
        std::fs::create_dir_all(home.join(".local/bin")).unwrap();
        let installed = home.join(".local/bin/agent-mail");
        std::fs::write(&installed, "old").unwrap();
        let downloads = downloads();
        let asset = format!("agent-mail-{}.tar.gz", host_target());
        std::fs::write(
            downloads.join(format!("{asset}.sha256")),
            format!("{}  {asset}\n", "0".repeat(64)),
        )
        .unwrap();
        let output = install(&home, &downloads);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_ne!(output.status.code(), Some(0));
        assert!(stderr.contains("checksum"), "{stderr}");
        assert_eq!(std::fs::read_to_string(&installed).unwrap(), "old");
        assert_eq!(bin_files(&home), vec!["agent-mail".to_string()]);
    }

    #[test]
    fn ac5_malformed_checksum_file_aborts_without_installing() {
        let home = temp_dir("home");
        let downloads = downloads();
        let asset = format!("agent-mail-{}.tar.gz", host_target());
        // Points the check at another file the attacker controls.
        std::fs::write(
            downloads.join(format!("{asset}.sha256")),
            format!("{}  {asset}.sha256\n", "0".repeat(64)),
        )
        .unwrap();
        let output = install(&home, &downloads);
        assert_ne!(output.status.code(), Some(0));
        assert!(String::from_utf8_lossy(&output.stderr).contains("checksum"));
        assert!(bin_files(&home).is_empty());
    }

    /// Runs the installer with a fake `uname` first on the `PATH`, which
    /// prints `Darwin` for `-s` and `machine` for `-m`.
    fn install_on(home: &Path, downloads: &Path, machine: &str) -> Output {
        let fake = temp_dir("fake-uname");
        let uname = fake.join("uname");
        std::fs::write(
            &uname,
            format!(
                "#!/bin/sh\ncase \"$1\" in\n    -s) echo Darwin ;;\n    -m) echo {machine} ;;\n    *) exit 1 ;;\nesac\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&uname, std::fs::Permissions::from_mode(0o755)).unwrap();
        Command::new("sh")
            .arg(Path::new(ROOT).join("install.sh"))
            .env("HOME", home)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", fake.display()),
            )
            .env(
                "AGENT_MAIL_DOWNLOAD_URL",
                format!("file://{}", downloads.display()),
            )
            .output()
            .unwrap()
    }

    #[test]
    fn job023_ac4_aarch64_is_an_unsupported_architecture() {
        let home = temp_dir("home");
        let output = install_on(&home, &downloads(), "aarch64");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_ne!(output.status.code(), Some(0), "{output:?}");
        assert!(
            stderr.contains("unsupported architecture aarch64"),
            "{stderr}"
        );
        assert!(bin_files(&home).is_empty());
    }

    #[test]
    fn job023_ac5_arm64_and_x86_64_install_their_targets() {
        for (machine, target) in [
            ("arm64", "aarch64-apple-darwin"),
            ("x86_64", "x86_64-apple-darwin"),
        ] {
            let home = temp_dir("home");
            let output = install_on(&home, &downloads(), machine);
            assert_eq!(output.status.code(), Some(0), "{machine}: {output:?}");
            let ran = Command::new(home.join(".local/bin/agent-mail"))
                .output()
                .unwrap();
            assert_eq!(
                String::from_utf8_lossy(&ran.stdout).trim(),
                target,
                "{machine}"
            );
        }
    }

    #[test]
    fn ac5_missing_asset_installs_nothing() {
        let home = temp_dir("home");
        let output = install(&home, &temp_dir("empty"));
        assert_ne!(output.status.code(), Some(0));
        assert!(bin_files(&home).is_empty());
        assert!(!home.join(".local/bin/agent-mail").exists());
    }

    #[test]
    fn ac5_no_sudo_and_temporary_directory_removed() {
        let script = read("install.sh");
        assert!(
            !script
                .lines()
                .filter(|l| !l.trim_start().starts_with('#'))
                .any(|l| l.contains("sudo")),
            "installer uses sudo"
        );
        let tmp = temp_dir("tmpdir");
        let home = temp_dir("home");
        let output = Command::new("sh")
            .arg(Path::new(ROOT).join("install.sh"))
            .env("HOME", &home)
            .env("TMPDIR", &tmp)
            .env(
                "AGENT_MAIL_DOWNLOAD_URL",
                format!("file://{}", downloads().display()),
            )
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(std::fs::read_dir(&tmp).unwrap().count(), 0);
    }
}

// ---------------------------------------------------------------- job 024 AC4

#[cfg(windows)]
mod windows_installer {
    use super::*;

    const WINDOWS_ASSET: &str = "agent-mail-x86_64-pc-windows-msvc.zip";

    /// The fake binary every test zip holds.
    const FAKE_EXE: &[u8] = b"MZ fake agent-mail.exe for installer tests";

    /// The PowerShell hosts to run the installer under: Windows PowerShell
    /// 5.1 always, PowerShell 7 too where it is installed.
    fn shells() -> Vec<&'static str> {
        let mut shells = vec!["powershell.exe"];
        let pwsh = Command::new("pwsh.exe")
            .args(["-NoProfile", "-Command", "exit 0"])
            .output();
        if pwsh.is_ok_and(|o| o.status.success()) {
            shells.push("pwsh.exe");
        }
        shells
    }

    fn powershell(command: &str) -> Output {
        let out = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", command])
            .output()
            .unwrap();
        assert!(out.status.success(), "{command}: {out:?}");
        out
    }

    /// The `.sha256` line for `digest`.
    fn sum_line(digest: &str) -> String {
        format!("{digest}  {WINDOWS_ASSET}\n")
    }

    /// A download directory holding a zip of `files` (name, bytes), built
    /// with `Compress-Archive`, and its correct `.sha256`.
    fn downloads_with(files: &[(&str, &[u8])]) -> PathBuf {
        let dir = temp_dir("win-downloads");
        let stage = temp_dir("win-stage");
        for (name, bytes) in files {
            std::fs::write(stage.join(name), bytes).unwrap();
        }
        let zip = dir.join(WINDOWS_ASSET);
        powershell(&format!(
            "Compress-Archive -Path '{}\\*' -DestinationPath '{}'",
            stage.display(),
            zip.display()
        ));
        let out = powershell(&format!(
            "(Get-FileHash -Algorithm SHA256 -LiteralPath '{}').Hash.ToLowerInvariant()",
            zip.display()
        ));
        let digest = String::from_utf8(out.stdout).unwrap().trim().to_string();
        assert_eq!(digest.len(), 64, "{digest}");
        std::fs::write(
            dir.join(format!("{WINDOWS_ASSET}.sha256")),
            sum_line(&digest),
        )
        .unwrap();
        dir
    }

    fn downloads() -> PathBuf {
        downloads_with(&[("agent-mail.exe", FAKE_EXE)])
    }

    /// A finished installer run: its output and the temporary directory it
    /// was given.
    struct Run {
        output: Output,
        tmp: PathBuf,
    }

    impl Run {
        fn stdout(&self) -> String {
            String::from_utf8_lossy(&self.output.stdout).into_owned()
        }

        fn stderr(&self) -> String {
            String::from_utf8_lossy(&self.output.stderr).into_owned()
        }
    }

    fn install_with(shell: &str, home: &Path, downloads: &Path, arch: &str) -> Run {
        let tmp = temp_dir("win-tmp");
        let url = format!(
            "file:///{}",
            downloads.display().to_string().replace('\\', "/")
        );
        let output = Command::new(shell)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(Path::new(ROOT).join("install.ps1"))
            .env("USERPROFILE", home)
            .env("TMP", &tmp)
            .env("TEMP", &tmp)
            .env("PROCESSOR_ARCHITECTURE", arch)
            .env("AGENT_MAIL_DOWNLOAD_URL", url)
            .output()
            .unwrap();
        Run { output, tmp }
    }

    fn install(shell: &str, home: &Path, downloads: &Path) -> Run {
        install_with(shell, home, downloads, "AMD64")
    }

    fn bin_dir(home: &Path) -> PathBuf {
        home.join(".local").join("bin")
    }

    fn bin_files(home: &Path) -> Vec<String> {
        match std::fs::read_dir(bin_dir(home)) {
            Ok(entries) => entries
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    fn assert_refused(run: &Run, home: &Path) {
        assert_ne!(run.output.status.code(), Some(0), "{:?}", run.output);
        assert!(
            bin_files(home).is_empty(),
            "installed: {:?}",
            bin_files(home)
        );
    }

    #[test]
    fn job024_ac4a_matching_checksum_installs_agent_mail_exe() {
        for shell in shells() {
            let home = temp_dir("win-home");
            let run = install(shell, &home, &downloads());
            assert_eq!(
                run.output.status.code(),
                Some(0),
                "{shell}: {:?}",
                run.output
            );
            let installed = bin_dir(&home).join("agent-mail.exe");
            assert_eq!(std::fs::read(&installed).unwrap(), FAKE_EXE, "{shell}");
            assert_eq!(bin_files(&home), ["agent-mail.exe"], "{shell}");
            assert!(
                run.stdout().contains(&installed.display().to_string()),
                "{shell}: {}",
                run.stdout()
            );
            assert!(
                run.stdout().contains("not on your PATH"),
                "{}",
                run.stdout()
            );
            assert_eq!(
                std::fs::read_dir(&run.tmp).unwrap().count(),
                0,
                "{shell}: temporary directory left"
            );
        }
    }

    #[test]
    fn job024_ac4b_existing_install_is_replaced() {
        for shell in shells() {
            let home = temp_dir("win-home");
            std::fs::create_dir_all(bin_dir(&home)).unwrap();
            std::fs::write(bin_dir(&home).join("agent-mail.exe"), "old").unwrap();
            let run = install(shell, &home, &downloads());
            assert_eq!(
                run.output.status.code(),
                Some(0),
                "{shell}: {:?}",
                run.output
            );
            assert_eq!(
                std::fs::read(bin_dir(&home).join("agent-mail.exe")).unwrap(),
                FAKE_EXE
            );
            assert_eq!(bin_files(&home), ["agent-mail.exe"], "{shell}");
        }
    }

    #[test]
    fn job024_ac4c_checksum_mismatch_installs_nothing() {
        for shell in shells() {
            let home = temp_dir("win-home");
            let downloads = downloads();
            std::fs::write(
                downloads.join(format!("{WINDOWS_ASSET}.sha256")),
                sum_line(&"0".repeat(64)),
            )
            .unwrap();
            let run = install(shell, &home, &downloads);
            assert_refused(&run, &home);
            assert!(
                run.stderr().contains(
                    "checksum mismatch for agent-mail-x86_64-pc-windows-msvc.zip; nothing was installed"
                ),
                "{shell}: {}",
                run.stderr()
            );
            assert_eq!(std::fs::read_dir(&run.tmp).unwrap().count(), 0);
        }
    }

    #[test]
    fn job024_ac4d_malformed_checksum_file_installs_nothing() {
        for shell in shells() {
            let digest = "a".repeat(64);
            for malformed in [
                format!("{digest}  {WINDOWS_ASSET}.sha256\n"),
                format!("{digest} {WINDOWS_ASSET}\n"),
                format!("{}  {WINDOWS_ASSET}\n", "A".repeat(64)),
                format!("{digest}  {WINDOWS_ASSET}\r\n"),
                format!("{digest}  {WINDOWS_ASSET}\n{digest}  {WINDOWS_ASSET}\n"),
                String::new(),
            ] {
                let home = temp_dir("win-home");
                let downloads = downloads();
                std::fs::write(
                    downloads.join(format!("{WINDOWS_ASSET}.sha256")),
                    &malformed,
                )
                .unwrap();
                let run = install(shell, &home, &downloads);
                assert_refused(&run, &home);
                assert!(
                    run.stderr().contains("checksum file")
                        && run.stderr().contains("malformed")
                        && run.stderr().contains("nothing was installed"),
                    "{shell} {malformed:?}: {}",
                    run.stderr()
                );
            }
        }
    }

    #[test]
    fn job024_ac4e_missing_zip_installs_nothing() {
        for shell in shells() {
            let home = temp_dir("win-home");
            let downloads = downloads();
            std::fs::remove_file(downloads.join(WINDOWS_ASSET)).unwrap();
            let run = install(shell, &home, &downloads);
            assert_refused(&run, &home);
            assert!(
                run.stderr().contains("could not download"),
                "{}",
                run.stderr()
            );
        }
    }

    #[test]
    fn job024_ac4f_zip_without_only_agent_mail_exe_installs_nothing() {
        for shell in shells() {
            for files in [
                &[("other.exe", FAKE_EXE)][..],
                &[("agent-mail.exe", FAKE_EXE), ("extra.dll", b"x")][..],
            ] {
                let home = temp_dir("win-home");
                let run = install(shell, &home, &downloads_with(files));
                assert_refused(&run, &home);
                assert!(
                    run.stderr().contains("must hold exactly agent-mail.exe"),
                    "{shell}: {}",
                    run.stderr()
                );
            }
        }
    }

    #[test]
    fn job024_ac4g_non_x64_is_refused_with_a_message_naming_x64() {
        let home = temp_dir("win-home");
        let run = install_with("powershell.exe", &home, &downloads(), "ARM64");
        assert_refused(&run, &home);
        assert!(run.stderr().contains("x64"), "{}", run.stderr());
    }
}

#[test]
fn job024_ac4g_installer_never_elevates_runs_the_binary_or_changes_path() {
    let script = read("install.ps1");
    let code: Vec<&str> = script
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .collect();
    let code = code.join("\n");
    for forbidden in [
        "Start-Process",
        "RunAs",
        "Invoke-Expression",
        "iex",
        "SetEnvironmentVariable",
        "$env:PATH =",
        "$env:Path =",
        "$env:PATH +=",
        "$env:Path +=",
        "setx",
        "& $dest",
        "& $binary",
        "& $staged",
        "Invoke-Item",
    ] {
        assert!(
            !code.contains(forbidden),
            "install.ps1 contains {forbidden}"
        );
    }
    assert!(code.contains("$arch -ne 'AMD64'"), "no AMD64 check");
    assert!(code.contains("x64"), "the refusal does not name x64");
    assert!(code.contains("not on your PATH"), "no PATH hint");
    // Nothing is extracted before the checksum passes.
    let mismatch = code.find("checksum mismatch").unwrap();
    let extract = code.find("ExtractToFile").unwrap();
    assert!(mismatch < extract, "extracts before the checksum check");
}

// ---------------------------------------------------------------- AC7

#[test]
fn ac7_release_notes_hold_the_one_liner_and_the_keychain_note() {
    let notes = read(".github/release-notes.md");
    assert!(notes.contains("first Keychain access after an update prompts once"));
    assert!(notes.contains(KEYCHAIN_NOTE));
    assert!(notes.contains(ONE_LINER));
    let workflow = read(".github/workflows/release.yml");
    assert!(workflow.contains("gh release create"));
    assert!(workflow.contains("--notes-file .github/release-notes.md"));
}

fn readme_section(heading: &str) -> String {
    let readme = read("README.md");
    let start = readme
        .find(&format!("\n{heading}\n"))
        .unwrap_or_else(|| panic!("README has no {heading}"));
    let level = heading.split(' ').next().unwrap();
    let rest = &readme[start + heading.len() + 2..];
    let end = rest
        .match_indices('\n')
        .map(|(i, _)| i)
        .find(|&i| {
            let line = &rest[i + 1..];
            line.starts_with('#')
                && line
                    .split(' ')
                    .next()
                    .is_some_and(|h| h.len() <= level.len())
        })
        .unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn ac7_readme_install_section_has_the_keychain_note() {
    assert!(readme_section("## Install").contains(KEYCHAIN_NOTE));
}

// ---------------------------------------------------------------- AC10

#[test]
fn ac10_readme_install_section_and_releasing_paragraph() {
    let install = readme_section("## Install");
    for needle in [
        ONE_LINER,
        "~/.local/bin",
        "checksum",
        "gh attestation verify",
        "--repo ollie-dev/agent-mail",
    ] {
        assert!(install.contains(needle), "Install section lacks {needle}");
    }
    let development = readme_section("## Development");
    let releasing = development
        .split("Releasing")
        .nth(1)
        .expect("no Releasing paragraph under Development");
    for needle in ["Cargo.toml", "vX.Y.Z", "verify"] {
        assert!(releasing.contains(needle), "Releasing lacks {needle}");
    }
}

#[test]
fn ac10_claude_md_lists_the_new_files() {
    let claude = read("CLAUDE.md");
    for needle in [
        "src/selfcheck.rs",
        "tests/selfcheck.rs",
        "tests/release.rs",
        "install.sh",
        "scripts/",
        ".github/workflows/release.yml",
    ] {
        assert!(claude.contains(needle), "CLAUDE.md lacks {needle}");
    }
}

#[test]
fn release_profile_pins_debug_assertions_off() {
    let manifest = read("Cargo.toml");
    let release = manifest
        .split("[profile.release]")
        .nth(1)
        .expect("no [profile.release]");
    let release = release.split("\n[").next().unwrap();
    assert!(release.contains("debug-assertions = false"));
}

// ---------------------------------------------------------------- job 023

#[test]
fn job023_ac4_installer_no_longer_accepts_aarch64() {
    let script = read("install.sh");
    assert!(!script.contains("aarch64)"), "install.sh accepts aarch64");
    assert!(!script.contains("| aarch64"), "install.sh accepts aarch64");
    assert!(script.contains("arm64) target=aarch64-apple-darwin"));
    assert!(script.contains("x86_64) target=x86_64-apple-darwin"));
}

/// The text of `mod release` in `tests/selfcheck.rs`, up to the closing
/// brace at the start of a line.
fn selfcheck_release_module() -> String {
    let source = read("tests/selfcheck.rs");
    let start = source
        .find("\nmod release {\n")
        .expect("tests/selfcheck.rs has no mod release");
    let body = &source[start + 1..];
    let end = body.find("\n}\n").map_or(body.len(), |e| e + 3);
    body[..end].to_string()
}

#[test]
fn job023_ac6_release_binary_tests_run_no_audited_command() {
    const AUDITED: [&str; 6] = ["list", "search", "read", "flag", "move", "draft"];
    let module = selfcheck_release_module();
    // Every argument array, whether passed to `run(` directly or through
    // the loop, starts with a non-audited command.
    for (i, _) in module.match_indices("[\"") {
        let first = module[i + 2..].split('"').next().unwrap();
        assert!(
            !AUDITED.contains(&first),
            "mod release runs audited command {first:?}"
        );
    }
    for (i, _) in module.match_indices("run(") {
        let call = &module[i..];
        let call = &call[..call.find('\n').unwrap_or(call.len())];
        for command in AUDITED {
            assert!(
                !call.contains(&format!("&[\"{command}\"")),
                "run call with {command}: {call}"
            );
        }
    }
    let squashed: String = module.split_whitespace().collect();
    assert!(
        squashed.contains(
            r#"forargsin[&["whoami"][..],&["mandate","list"],&["mandate","delete","deadbeef"],]{"#
        ),
        "the refusal loop is not whoami, mandate list, mandate delete"
    );
    assert!(!module.contains("denied line"), "stale audit comment");
    assert!(
        !module.contains("TEST_LOG_VAR"),
        "release test redirects the log"
    );
}

#[test]
fn job023_ac7_readme_says_release_binary_tests_write_nothing_to_the_audit_log() {
    let readme = read("README.md");
    assert!(!readme.contains("adds one `denied` line to your audit log"));
    let paragraph = readme
        .split("\n\n")
        .find(|p| p.contains("**Release-binary tests.**"))
        .expect("no Release-binary tests paragraph");
    assert!(paragraph.contains("audit log"), "{paragraph}");
    assert!(
        paragraph.contains("nothing") || paragraph.contains(" no "),
        "{paragraph}"
    );
}

// ---------------------------------------------------------------- job 024

const WINDOWS_ONE_LINER: &str =
    "irm https://github.com/ollie-dev/agent-mail/releases/latest/download/install.ps1 | iex";

/// The trimmed scopes of the first `permissions:` block in `job`.
fn scopes_of(job: &str) -> Vec<String> {
    let block = job
        .split("permissions:\n")
        .nth(1)
        .expect("job has no permissions block");
    let indent = block.len() - block.trim_start().len();
    block
        .lines()
        .take_while(|l| l.len() > indent && l[..indent].trim().is_empty())
        .map(|l| l.trim().to_string())
        .collect()
}

#[test]
fn job024_ac3a_build_windows_job_packages_and_checksums_the_exe() {
    let workflow = read(".github/workflows/release.yml");
    let build = job(&workflow, "build-windows");
    assert!(build.contains("runs-on: windows-latest"));
    assert!(build.contains("needs: test"));
    assert_eq!(scopes_of(build), ["contents: read"]);
    assert!(
        !build.contains("write"),
        "build-windows holds a write scope"
    );
    assert!(build.contains("cargo build --release --locked --target x86_64-pc-windows-msvc"));
    assert!(build.contains(
        "Compress-Archive -Path stage/agent-mail.exe -DestinationPath dist/agent-mail-x86_64-pc-windows-msvc.zip"
    ));
    assert!(build.contains(r#"printf '%s  %s\n' "$sum" "$asset" > "$asset.sha256""#));
    assert!(build.contains("asset=agent-mail-x86_64-pc-windows-msvc.zip"));
    assert!(build.contains("actions/upload-artifact@"));
    assert!(build.contains("dist/agent-mail-x86_64-pc-windows-msvc.zip.sha256"));
}

#[test]
fn job024_ac3b_publish_attests_and_releases_the_windows_zip_and_installer() {
    let workflow = read(".github/workflows/release.yml");
    let publish = job(&workflow, "publish");
    assert!(publish.contains("needs: [test, build-windows]"));
    assert!(publish.contains("actions/download-artifact@"));
    assert!(publish.contains("path: dist/"));
    assert!(publish.contains("cp install.ps1 dist/install.ps1"));
    let subjects = publish
        .split("subject-path: |\n")
        .nth(1)
        .expect("no subject-path");
    let subjects: Vec<&str> = subjects
        .lines()
        .take_while(|l| l.starts_with("            "))
        .map(str::trim)
        .collect();
    assert_eq!(
        subjects,
        [
            "dist/agent-mail-aarch64-apple-darwin.tar.gz",
            "dist/agent-mail-x86_64-apple-darwin.tar.gz",
            "dist/agent-mail-x86_64-pc-windows-msvc.zip",
        ]
    );
    let create = publish
        .split("gh release create")
        .nth(1)
        .expect("no gh release create");
    for asset in [
        "dist/agent-mail-x86_64-pc-windows-msvc.zip \\",
        "dist/agent-mail-x86_64-pc-windows-msvc.zip.sha256 \\",
        "dist/install.sh \\",
        "dist/install.ps1",
    ] {
        assert!(create.contains(asset), "gh release create lacks {asset}");
    }
    // Only publish holds write scopes.
    for name in ["test", "build-windows", "verify"] {
        assert!(!job(&workflow, name).contains("write"), "{name} can write");
    }
    // Every action is pinned to a full commit SHA with a version comment.
    for line in workflow.lines().filter(|l| l.contains("uses:")) {
        let reference = line.split('@').nth(1).expect("action without a version");
        let mut words = reference.split_whitespace();
        let sha = words.next().unwrap();
        assert!(
            sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
            "{line}"
        );
        assert_eq!(words.next(), Some("#"), "no version comment: {line}");
        assert!(words.next().is_some_and(|v| v.starts_with('v')), "{line}");
    }
}

#[test]
fn job024_ac3c_verify_release_checks_the_windows_zip() {
    let verify = read("scripts/verify-release.sh");
    let marker = r#"asset="agent-mail-x86_64-pc-windows-msvc.zip""#;
    let windows = verify
        .split(marker)
        .nth(1)
        .expect("verify-release.sh does not name the Windows zip");
    for needle in [
        r#"gh release download "$tag" --repo "$repo" --dir "$dir""#,
        r#"--pattern "$asset" --pattern "$asset.sha256""#,
        r#"grep -Eqx "[0-9a-f]{64}  $asset" "$dir/$asset.sha256""#,
        r#"(cd "$dir" && shasum -a 256 -c "$asset.sha256")"#,
        r#"unzip -Z1 "$dir/$asset""#,
        r#"[ "$members" = "agent-mail.exe" ]"#,
        r#"gh attestation verify "$dir/$asset" --repo "$repo""#,
    ] {
        assert!(windows.contains(needle), "Windows check lacks {needle}");
    }
    assert!(
        !windows.contains("sign-macos.sh"),
        "signature check on Windows"
    );
    assert!(!windows.contains("--version"), "runs the Windows binary");
    #[cfg(unix)]
    {
        let status = Command::new("sh")
            .arg("-n")
            .arg(Path::new(ROOT).join("scripts/verify-release.sh"))
            .status()
            .unwrap();
        assert!(status.success(), "verify-release.sh does not parse");
    }
}

#[test]
fn job024_ac3d_test_job_runs_on_macos_and_windows() {
    let workflow = read(".github/workflows/release.yml");
    let test = job(&workflow, "test");
    assert!(test.contains("os: [macos-latest, windows-latest]"));
    assert!(test.contains("runs-on: ${{ matrix.os }}"));
    assert_eq!(scopes_of(test), ["contents: read"]);
    // The version check is sh, so it must not run under PowerShell.
    let check = test
        .split("Tag matches the Cargo.toml version")
        .nth(1)
        .unwrap();
    assert!(check.trim_start().starts_with("shell: bash"));
}

#[test]
fn job024_ac2c_readme_says_the_hardened_runtime_check_is_macos_only() {
    let install = readme_section("## Install");
    assert!(
        install.contains(
            "The hardened-runtime check and `hardened_runtime_missing` apply to macOS only"
        )
    );
}

#[test]
fn job024_ac5a_readme_windows_paragraph_says_a_hijacked_agent_can_read_credentials() {
    let section = readme_section("## What each OS protects against");
    let windows = section
        .split("\n\n")
        .find(|p| p.trim_start().starts_with("**Windows.**"))
        .expect("no Windows paragraph");
    for needle in [
        "Credential Manager",
        "hijacked Agent",
        "same user",
        "honest Agents",
        "extract",
    ] {
        assert!(windows.contains(needle), "Windows paragraph lacks {needle}");
    }
    assert!(!windows.contains("no Windows release"));
    assert!(!read("README.md").contains("no Windows release"));
}

#[test]
fn job024_ac5b_readme_and_release_notes_give_the_windows_install() {
    let install = readme_section("## Install");
    for needle in [
        WINDOWS_ONE_LINER,
        r"%USERPROFILE%\.local\bin\agent-mail.exe",
        "SHA-256 checksum",
        "gh attestation verify agent-mail-x86_64-pc-windows-msvc.zip --repo ollie-dev/agent-mail",
    ] {
        assert!(install.contains(needle), "Install section lacks {needle}");
    }
    let audit = readme_section("## Audit log");
    assert!(audit.contains(r"Windows: `%LOCALAPPDATA%\agent-mail\audit.log`"));
    let notes = read(".github/release-notes.md");
    assert!(notes.contains(WINDOWS_ONE_LINER));
    assert!(notes.contains(ONE_LINER));
}

#[test]
fn job024_ac6a_ci_workflow_tests_on_macos_and_windows() {
    let ci = read(".github/workflows/ci.yml");
    let on = ci.split("\non:\n").nth(1).expect("no on: block");
    let on = on.split("\npermissions:").next().unwrap();
    assert!(on.contains("push:\n    branches: [main]"), "{on}");
    assert!(on.contains("pull_request:"), "{on}");
    let top = ci.split("\njobs:\n").next().unwrap();
    assert!(top.contains("\npermissions:\n  contents: read\n"));
    assert!(!ci.contains("write"), "ci holds a write scope");
    assert!(ci.contains("os: [macos-latest, windows-latest]"));
    assert!(ci.contains("runs-on: ${{ matrix.os }}"));
    assert!(ci.contains("run: cargo test --locked"));
    let uses: Vec<&str> = ci.lines().filter(|l| l.contains("uses:")).collect();
    assert!(!uses.is_empty());
    for line in uses {
        let sha = line
            .split('@')
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        assert!(
            sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
            "{line}"
        );
        assert!(line.contains(" # v"), "no version comment: {line}");
    }
}

#[test]
fn job024_ac6b_gitattributes_checks_out_lf_everywhere() {
    let attributes = read(".gitattributes");
    assert!(attributes.lines().any(|l| l.trim() == "* text=auto eol=lf"));
    // Text files are checked out with LF line endings, also on Windows.
    for file in ["tests/release.rs", "README.md", "src/terminal.rs"] {
        assert!(!read(file).contains('\r'), "{file} has CR line endings");
    }
}

#[test]
fn job024_claude_md_lists_the_windows_files() {
    let claude = read("CLAUDE.md");
    for needle in [
        "install.ps1",
        ".github/workflows/ci.yml",
        "tests/credential_manager.rs",
        "tests/credential_layout.rs",
        "CredentialManagerStore",
        ".gitattributes",
        "x86_64-pc-windows-msvc",
    ] {
        assert!(claude.contains(needle), "CLAUDE.md lacks {needle}");
    }
}
