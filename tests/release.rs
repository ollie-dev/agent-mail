//! The release files: the installer (AC5, macOS), the workflow (AC3, AC7)
//! and the release notes and README (AC7, AC10), and the static checks and
//! parsing of the Windows `install.ps1` and `uninstall.ps1` (job 027; they
//! run for real in `tests/installers.rs`). All values are fake.

use std::path::Path;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
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
#[cfg(target_os = "macos")]
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

// ---------------------------------------------------------------- job 027

/// The lines of a PowerShell script that are not comments, trimmed.
fn ps_code(path: &str) -> String {
    let script = read(path);
    let code: Vec<&str> = script
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .collect();
    code.join("\n")
}

/// Checks the elevation of `script`: one `Start-Process -Verb RunAs -Wait`
/// of the host itself with the script's own text, behind the
/// Administrators role check, and the refusal ending in `refusal`.
fn assert_elevates_once(script: &str, refusal: &str) {
    let code = ps_code(script);
    assert_eq!(
        code.matches("Start-Process").count(),
        1,
        "{script}: Start-Process"
    );
    assert_eq!(code.matches("RunAs").count(), 1, "{script}: RunAs");
    let start = code.lines().find(|l| l.contains("Start-Process")).unwrap();
    for needle in [
        "-Verb RunAs",
        "-Wait",
        "-PassThru",
        "-EncodedCommand $encoded",
        "-FilePath $hostExe",
    ] {
        assert!(start.contains(needle), "{script}: {start} lacks {needle}");
    }
    assert!(!start.contains("-File "), "{script} relaunches a file");
    // The elevated copy is always the system's Windows PowerShell, which
    // only administrators can replace.
    assert!(code.contains(
        r"$hostExe = [IO.Path]::Combine([Environment]::SystemDirectory, 'WindowsPowerShell\v1.0\powershell.exe')"
    ));
    assert!(
        !code.contains("Get-Process"),
        "{script} relaunches its own host"
    );
    // The elevated copy runs this block's own text, never a file.
    assert!(code.contains("$AgentMailText = $MyInvocation.MyCommand.ScriptBlock.ToString()"));
    assert!(code.contains("$inner = \"& {$AgentMailText} "));
    assert!(code.contains(".IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)"));
    assert!(code.contains("$IsAdministrator = ([Security.Principal.WindowsPrincipal]::new("));
    let check = code
        .find("elseif ($IsAdministrator) {")
        .unwrap_or_else(|| panic!("{script}: no Administrators check"));
    let relaunched = code
        .find("} elseif ($Relaunched) {")
        .unwrap_or_else(|| panic!("{script}: the elevated copy may relaunch"));
    let start_at = code.find("Start-Process").unwrap();
    assert!(
        check < relaunched && relaunched < start_at,
        "{script}: order"
    );
    assert!(
        code.contains("$code = $elevated.ExitCode") && code.contains("exit $code"),
        "{script} does not exit with the elevated copy's code"
    );
    assert_eq!(
        code.matches(&format!("administrator rights are needed; {refusal}\")"))
            .count(),
        2,
        "{script}: refusal text"
    );
    assert!(code.contains("[void](Read-Host 'Press Enter to close this window')"));
    // Nothing comes from the user-writable environment or %TEMP%.
    for forbidden in [
        "$env:ProgramFiles",
        "$env:ProgramData",
        "$env:TEMP",
        "$env:TMP",
        "GetTempPath",
        "AGENT_MAIL_DOWNLOAD_URL",
        "$env:USERPROFILE",
        "$env:SystemRoot",
        "$env:windir",
        "Invoke-Expression",
        "iex ",
    ] {
        assert!(!code.contains(forbidden), "{script} uses {forbidden}");
    }
    assert!(code.contains("Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'agent-mail'"));
    assert!(code.contains("Join-Path ([Environment]::SystemDirectory) 'sc.exe'"));
    assert!(code.contains("[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames"));
    assert!(code.contains("'SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment'"));
}

/// Checks that `script`, when elevated, loads only `modules` from the
/// running PowerShell's own `$PSHOME\Modules` and autoloads nothing, so a
/// module planted on the user-writable `PSModulePath` never runs as
/// administrator; and that only .NET and built-in commands run first.
fn assert_loads_only_shipped_modules(script: &str, modules: &str) {
    let text = read(script);
    let block = text
        .split("\n& {\n")
        .nth(1)
        .unwrap_or_else(|| panic!("{script}: no script block"));
    let hardened = block
        .find("\n        $PSModuleAutoLoadingPreference = 'None'\n    }\n")
        .map(|at| at + 1)
        .unwrap_or_else(|| panic!("{script}: autoloading stays on"));
    let before = &block[..hardened];
    assert_eq!(
        before,
        format!(
            "    param({params})
    $ErrorActionPreference = 'Stop'
    Set-StrictMode -Version 2.0

    # Elevated, only the modules this PowerShell ships with are loaded, and
    # nothing is autoloaded: the user's own PSModulePath entries (such as
    # Documents\\WindowsPowerShell\\Modules) are writable by every process of
    # the user, so a module planted there must never run as administrator.
    # Only .NET and built-in commands run before this.
    $IsAdministrator = ([Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    $SavedModulePath = $env:PSModulePath
    if ($IsAdministrator) {{
        $env:PSModulePath = [IO.Path]::Combine($PSHOME, 'Modules')
        foreach ($module in @({modules})) {{
            Import-Module -Name ([IO.Path]::Combine($PSHOME, 'Modules', $module, \"$module.psd1\"))
        }}
",
            params = if script == "install.ps1" {
                "[string]$DownloadUrl, [switch]$Relaunched"
            } else {
                "[switch]$KeepData, [switch]$Force, [switch]$Relaunched"
            }
        ),
        "{script}: the start of the block"
    );
    let code = ps_code(script);
    // The elevated copy's bootstrap uses .NET only, before the block runs.
    let boot = code.lines().find(|l| l.starts_with("$boot = ")).unwrap();
    assert!(!boot.contains("New-Object"), "{script}: {boot}");
    assert!(boot.contains("[IO.Compression.GZipStream]::new("));
    assert!(boot.contains("[IO.StreamReader]::new("));
    assert!(!code.contains("Get-FileHash"), "{script} uses Get-FileHash");
    // The operator's shell gets its PSModulePath back after `iex`.
    code.find("$env:PSModulePath = $SavedModulePath\nif ($AgentMailExits) { exit $code }")
        .unwrap_or_else(|| panic!("{script}: PSModulePath not restored"));
}

/// Neither script compiles C# at run time: Windows PowerShell's `Add-Type`
/// writes its source and output to the user-writable `%TEMP%` and loads
/// the result into the elevated process. The Path change is announced by
/// .NET's own `WM_SETTINGCHANGE` broadcast instead.
#[test]
fn job027_elevated_scripts_compile_no_code() {
    for script in ["install.ps1", "uninstall.ps1"] {
        let code = ps_code(script);
        for forbidden in [
            "-MemberDefinition",
            "-TypeDefinition",
            "Add-Type -Path",
            "Add-Type -LiteralPath",
            "DllImport",
            "[AgentMail.Env]",
            "SendMessageTimeout",
            "CompileAssemblyFromSource",
            "CSharpCodeProvider",
        ] {
            assert!(!code.contains(forbidden), "{script} uses {forbidden}");
        }
        for line in code.lines().filter(|l| l.contains("Add-Type")) {
            assert!(
                line.starts_with("Add-Type -AssemblyName System.IO.Compression"),
                "{script}: {line}"
            );
        }
        assert_eq!(
            code.lines()
                .filter(|l| l.contains("SetEnvironmentVariable"))
                .collect::<Vec<_>>(),
            [
                "[Environment]::SetEnvironmentVariable('AGENT_MAIL_INSTALLER_UNUSED', $null, 'Machine')"
            ],
            "{script}: the broadcast"
        );
    }
}

#[test]
fn job027_elevated_scripts_load_only_the_modules_powershell_ships_with() {
    assert_loads_only_shipped_modules(
        "install.ps1",
        "'Microsoft.PowerShell.Management', 'Microsoft.PowerShell.Utility'",
    );
    assert_loads_only_shipped_modules(
        "uninstall.ps1",
        "'Microsoft.PowerShell.Management', 'Microsoft.PowerShell.Utility', 'CimCmdlets'",
    );
}

#[test]
fn job027_ac1_install_elevates_once_behind_the_administrators_check() {
    assert_elevates_once("install.ps1", "nothing was installed");
    let code = ps_code("install.ps1");
    assert!(code.contains("param([string]$DownloadUrl, [switch]$Relaunched)"));
    // The URL reaches the elevated copy inside its command line.
    assert!(code.contains(
        "-DownloadUrl ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('$url'))) -Relaunched"
    ));
}

#[test]
fn job027_ac1_install_verifies_before_extracting_and_stages_in_the_install_folder() {
    let code = ps_code("install.ps1");
    let mismatch = code.find("checksum mismatch").unwrap();
    let extract = code.find("ExtractToFile").unwrap();
    assert!(mismatch < extract, "extracts before the checksum check");
    assert!(code.find("is malformed; nothing was installed").unwrap() < extract);
    for message in [
        "checksum mismatch for $Asset; nothing was installed",
        "checksum file for $Asset is malformed; nothing was installed",
        "could not download $baseUrl/$Asset; nothing was installed",
        "$Asset must hold exactly agent-mail.exe; nothing was installed",
        "agent-mail for Windows is built for x64 (AMD64) only",
        "$arch -ne 'AMD64'",
        "the new binary did not start; the previous version was restored",
        "Installed agent-mail $version",
        "Open a new terminal",
    ] {
        assert!(code.contains(message), "install.ps1 lacks {message}");
    }
    assert!(code.contains("$Prefix = 'agent-mail installer'"));
    // Downloads and extraction happen in a fresh folder inside the
    // admin-only install folder.
    assert!(
        code.contains(
            "$staging = Join-Path $folder ('.staging-' + [Guid]::NewGuid().ToString('N'))"
        )
    );
    assert!(code.contains("$binary = Get-Release $staging"));
    assert!(code.contains("$zip = Join-Path $Staging $Asset"));
}

#[test]
fn job027_ac1_install_registers_the_broker_with_a_quoted_path_and_an_explicit_access_list() {
    let code = ps_code("install.ps1");
    assert!(code.contains(r#"$binPath = 'binPath= "\"' + $Exe + '\" broker"'"#));
    assert!(code.contains(r#"obj= `"NT SERVICE\$ServiceName`" start= delayed-auto type= own"#));
    assert!(code.contains("$ServiceSddl = 'D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLOCRRC;;;IU)(A;;CCLCSWLOCRRC;;;SU)'"));
    assert!(code.contains("Invoke-Sc \"sdset $ServiceName $ServiceSddl\""));
    assert!(code.contains(
        "/inheritance:r /grant:r *S-1-5-18:(OI)(CI)F *S-1-5-32-544:(OI)(CI)F *S-1-5-32-545:(OI)(CI)RX"
    ));
    // The new binary is checked with --version before anything is stopped
    // or replaced.
    let version = code.find("$version = Get-BinaryVersion $binary").unwrap();
    let stop = code
        .find("Stop-Broker\n}\nif (Test-Path -LiteralPath $exe)")
        .unwrap();
    assert!(version < stop);
}

#[test]
fn job027_ac3_uninstall_asks_before_elevating_and_purges_through_the_broker() {
    assert_elevates_once("uninstall.ps1", "nothing was removed");
    let code = ps_code("uninstall.ps1");
    assert!(code.contains("param([switch]$KeepData, [switch]$Force, [switch]$Relaunched)"));
    for message in [
        "Read-Host 'Delete all stored Mandates and the audit log? Agents lose access and this cannot be undone. [y/N]'",
        "Keeping the stored Mandates and the audit log.",
        "cannot ask for confirmation; rerun with -KeepData or -Force",
        "could not delete the stored Mandates; nothing else was removed",
        "agent-mail was removed.",
        "agent-mail is not installed.",
        "Invoke-Sc \"start $ServiceName purge\"",
        "Invoke-Sc \"delete $ServiceName\"",
        "$Prefix = 'uninstall.ps1'",
        "@('y', 'yes') -contains ([string]$answer).Trim().ToLowerInvariant()",
        "$status.ExitCode -eq 0 -and $status.ServiceSpecificExitCode -eq 0",
    ] {
        assert!(code.contains(message), "uninstall.ps1 lacks {message}");
    }
    // -KeepData wins over -Force, the confirmation comes before the
    // elevation, and the elevated copy is told the answer.
    let keep = code.find("if ($KeepData) {").unwrap();
    let force = code.find("} elseif ($Force) {").unwrap();
    let ask = code.find("Read-Host 'Delete all").unwrap();
    let elevate = code.find("Start-Process").unwrap();
    assert!(keep < force && force < ask && ask < elevate);
    assert!(code.contains("$choice = if ($deleteData) { '-Force' } else { '-KeepData' }"));
    assert!(code.contains("$inner = \"& {$AgentMailText} $choice -Relaunched\""));
    // The purge runs before the service is deleted.
    assert!(code.find("(Invoke-Purge)").unwrap() < code.find("Invoke-Sc \"delete").unwrap());
}

#[test]
fn job027_ac4_scripts_are_ascii_with_lf_line_endings() {
    // Windows PowerShell 5.1 reads a file without a BOM as ANSI.
    for script in ["install.ps1", "uninstall.ps1"] {
        let text = read(script);
        assert!(text.is_ascii(), "{script} is not ASCII");
        assert!(!text.contains('\r'), "{script} has CR line endings");
    }
}

#[test]
fn job027_ac4_publish_uploads_both_installers() {
    let workflow = read(".github/workflows/release.yml");
    let publish = job(&workflow, "publish");
    assert!(publish.contains("cp install.ps1 dist/install.ps1"));
    assert!(publish.contains("cp uninstall.ps1 dist/uninstall.ps1"));
    let create = publish
        .split("gh release create")
        .nth(1)
        .expect("no gh release create");
    let files: Vec<&str> = create
        .lines()
        .map(|l| l.trim().trim_end_matches('\\').trim())
        .collect();
    for asset in ["dist/install.ps1", "dist/uninstall.ps1"] {
        assert!(files.contains(&asset), "gh release create lacks {asset}");
    }
}

#[test]
fn job027_ac5_verify_release_checks_the_installer_assets() {
    let verify = read("scripts/verify-release.sh");
    for needle in [
        "uninstall.ps1",
        "install.ps1",
        "missing asset",
        "git show",
        r#"for name in install.ps1 uninstall.ps1; do"#,
        r#"gh release download "$tag" --repo "$repo" --dir "$dir" --pattern "$name""#,
        r#"[ -f "$dir/$name" ] || { echo "missing asset $name" >&2; exit 1; }"#,
        r#"git -C "$root" show "$tag:$name" > "$dir/$name.expected""#,
        r#"cmp -s "$dir/$name" "$dir/$name.expected""#,
        r#"echo "$name differs from $tag" >&2"#,
    ] {
        assert!(verify.contains(needle), "verify-release.sh lacks {needle}");
    }
    let header: Vec<&str> = verify.lines().take_while(|l| l.starts_with('#')).collect();
    let header = header.join("\n");
    for needle in [
        "install.ps1",
        "uninstall.ps1",
        "missing asset",
        "git show",
        "differs from",
    ] {
        assert!(header.contains(needle), "header comment lacks {needle}");
    }
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
fn job027_ac6_windows_ci_runs_the_installer_tests() {
    const STEP: &str = "      - name: Installer tests\n        if: runner.os == 'Windows'\n        run: cargo test --locked --test installers -- --ignored --test-threads=1\n";
    let ci = read(".github/workflows/ci.yml");
    assert!(ci.contains(STEP), "ci.yml lacks the installer step");
    let release = read(".github/workflows/release.yml");
    let test = job(&release, "test");
    assert!(
        test.contains(STEP),
        "release.yml test job lacks the installer step"
    );
    assert_eq!(scopes_of(test), ["contents: read"]);
}

#[cfg(windows)]
mod powershell {
    use super::*;

    /// Windows PowerShell 5.1 always, PowerShell 7 too where installed.
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

    fn run(shell: &str, command: &str) -> Output {
        Command::new(shell)
            .args(["-NoProfile", "-NonInteractive", "-Command", command])
            .output()
            .unwrap()
    }

    #[test]
    fn job027_ac4_both_scripts_parse_under_each_shell() {
        for shell in shells() {
            for script in ["install.ps1", "uninstall.ps1"] {
                let path = Path::new(ROOT).join(script);
                let out = run(
                    shell,
                    &format!(
                        "$errors = $null; [void][System.Management.Automation.Language.Parser]::ParseFile('{}', [ref]$null, [ref]$errors); $errors.Count",
                        path.display()
                    ),
                );
                assert!(out.status.success(), "{shell} {script}: {out:?}");
                assert_eq!(
                    String::from_utf8_lossy(&out.stdout).trim(),
                    "0",
                    "{shell}: {script} has parse errors"
                );
            }
        }
    }

    /// The elevated relaunch's command line, built as the scripts build it
    /// but from their whole text and a long URL, stays well under the
    /// 32,767-character limit.
    #[test]
    fn job027_ac1_the_relaunch_command_line_fits() {
        for script in ["install.ps1", "uninstall.ps1"] {
            let path = Path::new(ROOT).join(script);
            let out = run(
                "powershell.exe",
                &format!(
                    "$text = [IO.File]::ReadAllText('{}'); \
                     $inner = '& {{' + $text + '}} -DownloadUrl ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String(' + ('A' * 400) + '))) -Relaunched'; \
                     $buffer = New-Object IO.MemoryStream; \
                     $gzip = New-Object IO.Compression.GZipStream($buffer, [IO.Compression.CompressionMode]::Compress); \
                     $bytes = [Text.Encoding]::UTF8.GetBytes($inner); $gzip.Write($bytes, 0, $bytes.Length); $gzip.Close(); \
                     $payload = [Convert]::ToBase64String($buffer.ToArray()); \
                     $boot = 'GZipStream FromBase64String ScriptBlock StreamReader Decompress ReadToEnd ' * 4 + $payload; \
                     ('-NoProfile -EncodedCommand ' + [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($boot))).Length",
                    path.display()
                ),
            );
            assert!(out.status.success(), "{script}: {out:?}");
            let length: usize = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
            // Room for the host's own path in front.
            assert!(length < 32_767 - 1_024, "{script}: {length} characters");
        }
    }

    /// The body of `if ($IsAdministrator) { ... }`, the elevated module
    /// hardening, from `script`.
    fn hardening(script: &str) -> String {
        let text = read(script);
        const OPEN: &str = "    if ($IsAdministrator) {\n";
        let start = text.find(OPEN).unwrap() + OPEN.len();
        let end = start + text[start..].find("\n    }\n").unwrap();
        text[start..end].to_string()
    }

    /// Run elevated, the scripts' hardening keeps a module planted first on
    /// `PSModulePath` (as in the user-writable Documents folder) from
    /// supplying any command, under each shell. Without it, the planted
    /// `Get-Service` would win.
    #[test]
    fn job027_a_module_planted_on_psmodulepath_never_loads_after_the_hardening() {
        let planted =
            std::env::temp_dir().join(format!("agent-mail-release-planted-{}", std::process::id()));
        let module = planted.join("Planted");
        std::fs::create_dir_all(&module).unwrap();
        std::fs::write(
            module.join("Planted.psm1"),
            "function Get-Service { 'PLANTED' }\nfunction Get-CimInstance { 'PLANTED' }\nfunction Write-Host { 'PLANTED' }\nExport-ModuleMember -Function *\n",
        )
        .unwrap();
        let path = std::env::var("PSModulePath").unwrap_or_default();
        let planted_path = format!("{};{path}", planted.display());
        // Runs `command` under `shell` with the planted module first on
        // PSModulePath, passed encoded so no quoting is lost.
        let run_planted = |shell: &str, command: &str| {
            use base64::Engine;
            let utf16: Vec<u8> = command.encode_utf16().flat_map(u16::to_le_bytes).collect();
            Command::new(shell)
                .args(["-NoProfile", "-NonInteractive", "-EncodedCommand"])
                .arg(base64::engine::general_purpose::STANDARD.encode(utf16))
                .env("PSModulePath", &planted_path)
                .output()
                .unwrap()
        };
        for shell in shells() {
            for (script, probe, expected) in [
                (
                    "install.ps1",
                    "(Get-Service -Name Winmgmt).Name; Write-Host 'host'",
                    &["Winmgmt", "host"][..],
                ),
                (
                    "uninstall.ps1",
                    "(Get-Service -Name Winmgmt).Name; (Get-CimInstance Win32_Service -Filter \"Name='Winmgmt'\").Name; Write-Host 'host'",
                    &["Winmgmt", "Winmgmt", "host"][..],
                ),
            ] {
                let control = run_planted(shell, probe);
                assert!(
                    String::from_utf8_lossy(&control.stdout).contains("PLANTED"),
                    "{shell}: the planted module does not load even without the hardening: {control:?}"
                );
                let out = run_planted(
                    shell,
                    &format!(
                        "$ErrorActionPreference = 'Stop'; & {{\n{}\n{probe}\n}}",
                        hardening(script)
                    ),
                );
                let stdout = String::from_utf8_lossy(&out.stdout);
                assert!(out.status.success(), "{shell} {script}: {out:?}");
                assert_eq!(
                    stdout.trim().lines().map(str::trim).collect::<Vec<_>>(),
                    expected,
                    "{shell} {script}"
                );
            }
        }
        let _ = std::fs::remove_dir_all(&planted);
    }
}
