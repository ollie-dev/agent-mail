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
fn help_lists_the_mandate_subcommand() {
    // Mandates exist now (`mandate create`), so --help may name them.
    let output = run_agent_mail(&["--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert!(
        stdout
            .lines()
            .any(|l| l.trim_start().starts_with("mandate")),
        "--help does not list `mandate`: {stdout:?}"
    );
    let output = run_agent_mail(&["mandate", "--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0));
    for sub in ["create", "list", "delete"] {
        assert!(
            stdout.lines().any(|l| l.trim_start().starts_with(sub)),
            "mandate --help does not list `{sub}`: {stdout:?}"
        );
    }
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

fn run_list(token: Option<&str>, extra_env: &[(&str, &str)]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_agent-mail"));
    command
        .arg("list")
        .env_remove("AGENT_MAIL_TOKEN")
        .env_remove("AGENT_MAIL_STORE")
        .env_remove(agent_mail::audit::TEST_LOG_VAR);
    if let Some(token) = token {
        command.env("AGENT_MAIL_TOKEN", token);
    }
    for (key, value) in extra_env {
        command.env(key, value);
    }
    command.output().expect("failed to run agent-mail")
}

fn error_code(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let value: serde_json::Value = serde_json::from_str(&stderr)
        .unwrap_or_else(|e| panic!("stderr is not JSON ({e}): {stderr:?}"));
    value["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn help_lists_the_list_subcommand_and_text_flag() {
    let output = run_agent_mail(&["--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("list"), "{stdout:?}");
    assert!(stdout.contains("--text"), "{stdout:?}");

    let output = run_agent_mail(&["list", "--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0));
    assert!(
        !stdout.to_lowercase().contains("mandate"),
        "list --help mentions mandates: {stdout:?}"
    );
}

#[test]
fn ac6_binary_reports_missing_token() {
    for token in [None, Some("")] {
        let output = run_list(token, &[]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "");
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(error_code(&output), "missing_token");
    }
}

#[test]
fn ac6_binary_reports_malformed_and_unknown_tokens_identically() {
    let unknown = agent_mail::seal::AgentToken::mint();
    let malformed = run_list(Some("hello"), &[]);
    let unknown_out = run_list(Some(unknown.expose()), &[]);

    for output in [&malformed, &unknown_out] {
        assert_eq!(String::from_utf8_lossy(&output.stdout), "");
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(error_code(output), "invalid_token");
        assert!(!String::from_utf8_lossy(&output.stderr).contains(unknown.expose()));
    }
    assert_eq!(malformed.stderr, unknown_out.stderr);
}

#[test]
fn binary_appends_an_audit_entry_to_the_redirected_test_log() {
    let log = agent_mail::audit::AuditLog::temporary()
        .path()
        .unwrap()
        .to_path_buf();
    let log_text = log.to_str().unwrap();
    let output = run_list(
        Some("not-a-token"),
        &[(agent_mail::audit::TEST_LOG_VAR, log_text)],
    );
    assert_eq!(output.status.code(), Some(1));
    let text = std::fs::read_to_string(&log).expect("the binary wrote the log");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "{text:?}");
    let entry: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(entry["operation"], "list");
    assert_eq!(entry["outcome"], "denied");
    assert_eq!(entry["reason"], "invalid_token");
    assert!(!text.contains("not-a-token"));
}

#[test]
fn ac11_store_flag_is_an_unknown_argument() {
    let output = run_agent_mail(&["list", "--store", "memory"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert!(
        stderr.contains("unexpected argument '--store'"),
        "{stderr:?}"
    );
}

#[test]
fn ac11_store_env_var_changes_nothing() {
    let token = agent_mail::seal::AgentToken::mint();
    let plain = run_list(Some(token.expose()), &[]);
    let with_var = run_list(Some(token.expose()), &[("AGENT_MAIL_STORE", "memory")]);

    assert_eq!(error_code(&plain), "invalid_token");
    assert_eq!(plain.status.code(), Some(1));
    assert_eq!(with_var.status.code(), Some(1));
    assert_eq!(plain.stdout, with_var.stdout);
    assert_eq!(plain.stderr, with_var.stderr);
}
