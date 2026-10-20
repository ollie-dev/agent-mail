//! Checks on the project layout and its recorded conventions.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Returns the body of the Markdown section that starts with `heading`,
/// up to the next heading of the same or a higher level.
fn markdown_section<'a>(text: &'a str, heading: &str) -> &'a str {
    let start = text
        .find(&format!("\n{heading}\n"))
        .unwrap_or_else(|| panic!("section {heading:?} not found"))
        + heading.len()
        + 2;
    let rest = &text[start..];
    let end = rest
        .lines()
        .scan(0, |offset, line| {
            let here = *offset;
            *offset += line.len() + 1;
            Some((here, line))
        })
        .find(|(_, line)| line.starts_with("# ") || line.starts_with("## "))
        .map_or(rest.len(), |(here, _)| here);
    &rest[..end]
}

#[test]
fn claude_md_stack_section_records_layout() {
    let claude = read(&repo_root().join("CLAUDE.md"));
    let stack = markdown_section(&claude, "## Stack");
    let lines: Vec<&str> = stack.lines().collect();

    for kept in [
        "- Language: Rust, one binary crate (see `docs/adr/0001-rust-not-bun.md`)",
        "- Test: `cargo test`",
        "- Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`",
    ] {
        assert!(lines.contains(&kept), "## Stack lost the line {kept:?}");
    }

    assert!(
        !stack.contains("set by ticket 01"),
        "## Stack still has the placeholder layout line"
    );

    let layout = lines
        .iter()
        .find(|line| line.starts_with("- Source and test layout:"))
        .expect("## Stack has no `- Source and test layout:` line");
    for part in ["src/main.rs", "src/lib.rs", "tests/"] {
        assert!(layout.contains(part), "layout line does not name {part}");
    }
}

/// Returns the `[table]` headers and `key = value` lines of a TOML file,
/// paired with the table they belong to. Enough for the flat Cargo.toml here.
fn toml_entries(text: &str) -> Vec<(String, String)> {
    let mut table = String::new();
    let mut entries = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            table = line.to_string();
            entries.push((table.clone(), String::new()));
        } else {
            entries.push((table.clone(), line.to_string()));
        }
    }
    entries
}

#[test]
fn project_is_a_single_rust_binary_crate() {
    let root = repo_root();
    let manifest = read(&root.join("Cargo.toml"));
    let entries = toml_entries(&manifest);

    let key_is = |line: &str, key: &str| line.split_once('=').is_some_and(|(k, _)| k.trim() == key);

    assert!(
        entries
            .iter()
            .any(|(t, l)| t == "[package]" && key_is(l, "name") && l.ends_with("\"agent-mail\"")),
        "Cargo.toml has no [package] named agent-mail"
    );
    assert!(
        !entries.iter().any(|(t, _)| t.starts_with("[workspace")),
        "Cargo.toml must not declare a workspace"
    );
    for (_, line) in entries
        .iter()
        .filter(|(t, l)| t == "[[bin]]" && key_is(l, "name"))
    {
        assert!(
            line.ends_with("\"agent-mail\""),
            "unexpected binary target: {line}"
        );
    }
    assert!(
        !entries.iter().any(|(_, l)| key_is(l, "build")),
        "Cargo.toml must not set a build script"
    );
    assert!(!root.join("build.rs").exists(), "build.rs must not exist");

    for foreign in [
        "package.json",
        "bun.lockb",
        "bun.lock",
        "tsconfig.json",
        "pyproject.toml",
        "requirements.txt",
        "go.mod",
    ] {
        assert!(
            !root.join(foreign).exists(),
            "{foreign} must not exist at the repo root"
        );
    }
}

#[test]
fn package_description_does_not_mention_mandates() {
    let description = env!("CARGO_PKG_DESCRIPTION");
    assert!(!description.is_empty(), "package description is empty");
    assert!(
        !description.to_lowercase().contains("mandate"),
        "package description mentions mandates: {description:?}"
    );
}

#[test]
fn ac11_test_support_feature_is_not_a_default() {
    let manifest = read(&repo_root().join("Cargo.toml"));
    let entries = toml_entries(&manifest);

    assert!(
        entries
            .iter()
            .any(|(t, l)| t == "[features]" && l.starts_with("test-support")),
        "Cargo.toml has no test-support feature"
    );
    let default = entries
        .iter()
        .find(|(t, l)| t == "[features]" && l.split('=').next().unwrap().trim() == "default");
    if let Some((_, line)) = default {
        assert!(
            !line.contains("test-support"),
            "default features include test-support: {line}"
        );
    }
}

const STORE_MARKER: &str = "agent-mail-in-memory-store";

#[test]
fn ac11_release_binary_has_no_in_memory_store() {
    let root = repo_root();
    // The marker must exist in the source, or this check proves nothing.
    let store_source = read(&root.join("src/store.rs"));
    assert!(store_source.contains(STORE_MARKER));

    // A separate target directory, so this does not wait on the test build's lock.
    let target = root.join("target").join("release-check");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = std::process::Command::new(cargo)
        .current_dir(&root)
        .args(["build", "--release", "--locked", "--bin", "agent-mail"])
        .arg("--target-dir")
        .arg(&target)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .status()
        .expect("failed to run cargo build --release");
    assert!(status.success(), "cargo build --release failed");

    let binary = target
        .join("release")
        .join(format!("agent-mail{}", std::env::consts::EXE_SUFFIX));
    let bytes =
        fs::read(&binary).unwrap_or_else(|e| panic!("cannot read {}: {e}", binary.display()));
    assert!(
        !bytes
            .windows(STORE_MARKER.len())
            .any(|w| w == STORE_MARKER.as_bytes()),
        "release binary contains the in-memory store"
    );
}
