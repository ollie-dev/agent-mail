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
        publish.contains("needs: test"),
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
