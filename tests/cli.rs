//! Tests that drive the built `agent-mail` binary from the outside.

use std::process::Command;

#[test]
fn version_flag_prints_name_and_crate_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_agent-mail"))
        .arg("--version")
        .output()
        .expect("failed to run agent-mail");

    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("agent-mail {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert_eq!(output.status.code(), Some(0));
}

fn run_agent_mail(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_agent-mail"))
        .args(args)
        .output()
        .expect("failed to run agent-mail")
}

#[test]
fn bare_call_prints_usage_hint_on_stderr() {
    let output = run_agent_mail(&[]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr
            .lines()
            .any(|line| line.starts_with("Usage: agent-mail")),
        "stderr has no `Usage: agent-mail` line: {stderr:?}"
    );
    assert!(
        stderr.contains("--help"),
        "stderr does not mention --help: {stderr:?}"
    );
}

#[test]
fn bare_call_is_a_usage_error_with_empty_stdout() {
    let output = run_agent_mail(&[]);

    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn help_does_not_mention_mandates() {
    let output = run_agent_mail(&["--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert!(
        !stdout.to_lowercase().contains("mandate"),
        "--help mentions mandates: {stdout:?}"
    );
}

#[test]
fn help_describes_the_tool_before_usage() {
    let output = run_agent_mail(&["--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let usage_at = stdout
        .find("Usage:")
        .unwrap_or_else(|| panic!("--help has no `Usage:` line: {stdout:?}"));

    assert!(
        !stdout[..usage_at].trim().is_empty(),
        "--help has no description before `Usage:`: {stdout:?}"
    );
}
