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
    assert!(stderr.contains("unknown option --store;"), "{stderr:?}");
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

// ---- Short help text and error messages (job 019) ----

mod support;

use agent_mail::core::{CoreError, RELEASES_URL, Warning};
use agent_mail::mandate::Operation;
use agent_mail::store::InMemoryStore;

/// Every subcommand path whose help pages AC1 checks.
const HELP_PATHS: [&[&str]; 13] = [
    &[],
    &["list"],
    &["search"],
    &["read"],
    &["flag"],
    &["move"],
    &["draft"],
    &["whoami"],
    &["mcp"],
    &["mandate"],
    &["mandate", "create"],
    &["mandate", "list"],
    &["mandate", "delete"],
];

/// Runs `agent-mail <args>` in-process with no token and an empty store.
fn run_in_process(args: &[&str]) -> agent_mail::RunOutput {
    let store = InMemoryStore::new();
    let context = agent_mail::Context::trusting_only(&store, Vec::new());
    let argv = std::iter::once("agent-mail").chain(args.iter().copied());
    agent_mail::run(argv, &std::collections::HashMap::new(), &context)
}

/// The help page of `agent-mail <path> <flag>`, asserting exit 0.
fn help(path: &[&str], flag: &str) -> String {
    let mut args = path.to_vec();
    args.push(flag);
    let out = run_in_process(&args);
    assert_eq!(out.code, 0, "{args:?}: {out:?}");
    out.stdout
}

/// The paragraphs of a rendered help page, per the spec's paragraph rule.
/// Blocks that are lists (first line `Commands:`, `Options:` or
/// `Arguments:`) are measured line by line: the short `-h` page renders
/// its options as one block without blank lines.
fn paragraphs(text: &str) -> Vec<String> {
    let mut blocks: Vec<Vec<&str>> = vec![Vec::new()];
    for line in text.lines() {
        if line.trim().is_empty() {
            blocks.push(Vec::new());
        } else {
            blocks.last_mut().unwrap().push(line);
        }
    }
    let mut out = Vec::new();
    for block in blocks.into_iter().filter(|b| !b.is_empty()) {
        if ["Commands:", "Options:", "Arguments:"].contains(&block[0].trim()) {
            out.extend(block.iter().map(|l| l.trim().to_string()));
        } else {
            let joined: Vec<&str> = block.iter().map(|l| l.trim()).collect();
            out.push(joined.join(" "));
        }
    }
    out
}

#[test]
fn ac1_no_help_paragraph_is_longer_than_300_characters() {
    let mut invocations = 0;
    for path in HELP_PATHS {
        for flag in ["--help", "-h"] {
            invocations += 1;
            for paragraph in paragraphs(&help(path, flag)) {
                assert!(
                    paragraph.chars().count() <= 300,
                    "`agent-mail {} {flag}` has a paragraph of {} characters: {paragraph:?}",
                    path.join(" "),
                    paragraph.chars().count()
                );
            }
        }
    }
    assert_eq!(invocations, 26);
}

#[test]
fn ac2_help_keeps_the_facts_an_agent_depends_on() {
    let cases: [(&[&str], &[&str]); 10] = [
        (&[], &["AGENT_MAIL_TOKEN", "--account", "agent-mail whoami"]),
        (&["search"], &["X-GM-EXT-1", "unsupported"]),
        (&["read"], &["20,000", "stale_reference"]),
        (&["flag"], &["unsupported_flag", "stale_reference"]),
        (&["move"], &["move_unsupported", "stale_reference"]),
        (&["draft"], &["drafts_not_found", "Re: "]),
        (&["whoami"], &["invalid_tokens"]),
        (&["mcp"], &["invalid_arguments", "agent-mail mandate"]),
        (&["mandate", "create"], &["--from"]),
        (&["mandate", "delete"], &["revocation_incomplete"]),
    ];
    for (path, needles) in cases {
        let text = help(path, "--help");
        for needle in needles {
            assert!(
                text.contains(needle),
                "`agent-mail {} --help` lacks {needle:?}: {text}",
                path.join(" ")
            );
        }
    }
}

const TEXT_HELP: &str = "Print human-readable text instead of JSON; may also follow the subcommand, except for search, whose --text is a search criterion";

#[test]
fn ac3_global_text_flag_help_line() {
    for flag in ["--help", "-h"] {
        let text = help(&[], flag);
        assert!(
            !text.contains("put it before the subcommand"),
            "{flag}: {text}"
        );
        let line = text
            .lines()
            .find(|l| l.trim_start().starts_with("--text"))
            .unwrap_or_else(|| panic!("{flag}: no --text line: {text}"));
        let description = line.trim_start().trim_start_matches("--text").trim();
        assert_eq!(description, TEXT_HELP, "{flag}");
    }
}

#[test]
fn ac3_text_before_or_after_whoami_prints_the_same() {
    let store = InMemoryStore::new();
    let token = support::store_mandate(
        &store,
        &support::mandate(993, agent_mail::mandate::TlsMode::Implicit, Vec::new()),
    );
    let run = |args: &[&str]| {
        let mut env = std::collections::HashMap::new();
        env.insert(
            agent_mail::TOKEN_VAR.to_string(),
            token.expose().to_string(),
        );
        let context = agent_mail::Context::trusting_only(&store, Vec::new());
        let argv = std::iter::once("agent-mail").chain(args.iter().copied());
        agent_mail::run(argv, &env, &context)
    };
    let before = run(&["--text", "whoami"]);
    let after = run(&["whoami", "--text"]);
    assert_eq!(before.code, 0, "{before:?}");
    assert!(!before.stdout.is_empty());
    assert_eq!(before.stdout, after.stdout);
    assert_eq!(after.code, 0);
}

#[test]
fn ac4_search_help_names_human_readable_output_once() {
    let text = help(&["search"], "--help");
    assert_eq!(
        text.matches("agent-mail --text search").count(),
        1,
        "{text}"
    );
    let short = help(&["search"], "-h");
    let line = short
        .lines()
        .find(|l| l.trim_start().starts_with("--text"))
        .unwrap_or_else(|| panic!("no --text line: {short}"));
    let description = line.trim_start().trim_start_matches("--text <S>").trim();
    assert!(
        description.starts_with("Messages whose headers or body contain S"),
        "{line:?}"
    );
}

/// One sample of every `CoreError` variant, built with the AC5 values. The
/// exhaustive `match` makes a new variant without a sample fail to compile.
fn error_samples() -> Vec<CoreError> {
    let account = || "agent@example.com".to_string();
    let mailbox = || "INBOX".to_string();
    let ops = [
        Operation::Read,
        Operation::Flag,
        Operation::Move,
        Operation::Draft,
    ];
    let seeds = vec![
        CoreError::MissingToken,
        CoreError::InvalidToken,
        CoreError::Denied {
            operation: Operation::Read,
            account: account(),
            mailbox: None,
        },
        CoreError::Uncovered {
            operation: Operation::Read,
            account: account(),
            mailbox: None,
        },
        CoreError::SameMailbox {
            account: account(),
            mailbox: mailbox(),
        },
        CoreError::MoveUnsupported {
            account: account(),
            mailbox: mailbox(),
        },
        CoreError::Unsupported {
            account: account(),
            mailbox: mailbox(),
        },
        CoreError::StaleReference {
            operation: Operation::Read,
            account: account(),
            mailbox: mailbox(),
        },
        CoreError::MessageNotFound {
            operation: Operation::Read,
            account: account(),
            mailbox: mailbox(),
        },
        CoreError::InvalidReference {
            operation: Operation::Read,
        },
        CoreError::DraftsNotFound { account: account() },
        CoreError::InvalidDraft { field: "to" },
        CoreError::UnsupportedFlag,
        CoreError::MailboxNotFound {
            account: account(),
            mailbox: mailbox(),
        },
        CoreError::TlsRequired { account: account() },
        CoreError::TlsCertificate { account: account() },
        CoreError::Connection { account: account() },
        CoreError::ProxyRefused {
            account: account(),
            host: "imap.example.com".to_string(),
            port: 993,
        },
        CoreError::ProxyAuthFailed {
            account: account(),
            host: "imap.example.com".to_string(),
            port: 993,
            variable: "HTTPS_PROXY",
        },
        CoreError::InvalidProxy {
            account: account(),
            variable: "HTTPS_PROXY",
        },
        CoreError::LoginFailed { account: account() },
        CoreError::Protocol { account: account() },
        CoreError::StoreUnavailable,
        CoreError::HardenedRuntimeMissing,
        CoreError::AccountRequired {
            accounts: vec![account(), "other@example.com".to_string()],
        },
        CoreError::AmbiguousMandate {
            account: account(),
            mandates: vec!["aaaa1111".to_string(), "bbbb2222".to_string()],
        },
        CoreError::InteractiveTerminalRequired,
        CoreError::SandboxDetected,
        CoreError::Aborted,
        CoreError::InvalidMandateFile {
            problem: "line 3: unknown operation \"send\"".to_string(),
        },
        CoreError::NotFound {
            id: "aaaa1111".to_string(),
        },
        CoreError::RevocationIncomplete {
            id: "aaaa1111".to_string(),
            deleted: 1,
            unreadable: 2,
        },
        CoreError::AuditUnavailable,
        CoreError::DraftRefused {
            account: account(),
            mailbox: mailbox(),
        },
    ];
    let mut samples = Vec::new();
    for seed in seeds {
        match seed {
            CoreError::MissingToken
            | CoreError::InvalidToken
            | CoreError::SameMailbox { .. }
            | CoreError::MoveUnsupported { .. }
            | CoreError::Unsupported { .. }
            | CoreError::DraftsNotFound { .. }
            | CoreError::DraftRefused { .. }
            | CoreError::UnsupportedFlag
            | CoreError::MailboxNotFound { .. }
            | CoreError::TlsRequired { .. }
            | CoreError::TlsCertificate { .. }
            | CoreError::Connection { .. }
            | CoreError::ProxyRefused { .. }
            | CoreError::ProxyAuthFailed { .. }
            | CoreError::InvalidProxy { .. }
            | CoreError::LoginFailed { .. }
            | CoreError::Protocol { .. }
            | CoreError::StoreUnavailable
            | CoreError::HardenedRuntimeMissing
            | CoreError::AccountRequired { .. }
            | CoreError::AmbiguousMandate { .. }
            | CoreError::InteractiveTerminalRequired
            | CoreError::SandboxDetected
            | CoreError::Aborted
            | CoreError::InvalidMandateFile { .. }
            | CoreError::NotFound { .. }
            | CoreError::RevocationIncomplete { .. }
            | CoreError::AuditUnavailable => samples.push(seed),
            CoreError::Denied { .. } => {
                for operation in ops {
                    samples.push(CoreError::Denied {
                        operation,
                        account: account(),
                        mailbox: None,
                    });
                    samples.push(CoreError::Denied {
                        operation,
                        account: account(),
                        mailbox: Some(mailbox()),
                    });
                }
            }
            CoreError::Uncovered { .. } => {
                for operation in ops {
                    samples.push(CoreError::Uncovered {
                        operation,
                        account: account(),
                        mailbox: Some(mailbox()),
                    });
                }
            }
            CoreError::StaleReference { .. } => {
                for operation in ops {
                    samples.push(CoreError::StaleReference {
                        operation,
                        account: account(),
                        mailbox: mailbox(),
                    });
                }
            }
            CoreError::MessageNotFound { .. } => {
                for operation in ops {
                    samples.push(CoreError::MessageNotFound {
                        operation,
                        account: account(),
                        mailbox: mailbox(),
                    });
                }
            }
            CoreError::InvalidReference { .. } => {
                for operation in ops {
                    samples.push(CoreError::InvalidReference { operation });
                }
            }
            CoreError::InvalidDraft { .. } => {
                for field in ["to", "subject", "body"] {
                    samples.push(CoreError::InvalidDraft { field });
                }
            }
        }
    }
    samples
}

#[test]
fn ac5_every_error_message_is_at_most_300_characters() {
    let samples = error_samples();
    for error in &samples {
        let message = error.body().message;
        assert!(
            message.chars().count() <= 300,
            "{} has a message of {} characters: {message:?}",
            error.code(),
            message.chars().count()
        );
    }
    // The RevocationIncomplete sample with nothing deleted, too.
    let none_deleted = CoreError::RevocationIncomplete {
        id: "aaaa1111".to_string(),
        deleted: 0,
        unreadable: 2,
    };
    assert!(none_deleted.body().message.chars().count() <= 300);
}

#[test]
fn ac6_proxy_refused_and_unsupported_keep_their_facts() {
    let proxy = CoreError::ProxyRefused {
        account: "agent@example.com".to_string(),
        host: "imap.example.com".to_string(),
        port: 993,
    }
    .body()
    .message;
    assert!(proxy.chars().count() <= 300, "{proxy}");
    for needle in [
        "imap.example.com:993",
        "agent@example.com",
        "allowed domains",
    ] {
        assert!(proxy.contains(needle), "{needle:?} missing: {proxy}");
    }
    let gmail = CoreError::Unsupported {
        account: "agent@example.com".to_string(),
        mailbox: "INBOX".to_string(),
    }
    .body()
    .message;
    assert!(gmail.chars().count() <= 300, "{gmail}");
    assert!(gmail.contains("--gmail"), "{gmail}");
}

#[test]
fn ac7_hardened_runtime_missing_says_once_the_keychain_was_not_accessed() {
    assert_eq!(
        CoreError::HardenedRuntimeMissing.body().message,
        format!(
            "this agent-mail binary is not signed with the hardened runtime, so it refused to access the Keychain; ask the human to reinstall agent-mail from {RELEASES_URL}"
        )
    );
}

#[test]
fn ac8_invalid_token_warning_is_at_most_300_characters() {
    for positions in [&[2][..], &[1, 3, 4][..]] {
        let warning = Warning::invalid_tokens(positions).expect("a warning");
        assert!(
            warning.message.chars().count() <= 300,
            "{positions:?}: {}",
            warning.message
        );
    }
}

// ---- Agent-readable usage errors and the in-process `mcp` refusal (job 020) ----

use support::{
    CountingStore, FakeImap, Mode, audit_lines, grant, mandate, run_logged, store_mandate,
    temp_audit_log,
};

/// A well-formed message reference.
const VALID_REF: &str = "agent@example.test/INBOX/1700000001/2";

/// Asserts `out` is a usage error (exit 2, empty stdout, stderr one JSON
/// line `{"error":{"code":"invalid_arguments","message":...}}` with no
/// other key) and returns its message.
fn usage_message(args: &[&str], out: &agent_mail::RunOutput) -> String {
    assert_eq!(out.code, 2, "{args:?}: {out:?}");
    assert_eq!(out.stdout, "", "{args:?}");
    assert!(out.stderr.ends_with('\n'), "{args:?}: {:?}", out.stderr);
    assert_eq!(out.stderr.lines().count(), 1, "{args:?}: {:?}", out.stderr);
    let value: serde_json::Value = serde_json::from_str(&out.stderr)
        .unwrap_or_else(|e| panic!("{args:?}: stderr is not JSON ({e}): {:?}", out.stderr));
    let outer: Vec<&String> = value.as_object().unwrap().keys().collect();
    assert_eq!(outer, ["error"], "{args:?}");
    let error = value["error"].as_object().unwrap();
    let mut keys: Vec<&String> = error.keys().collect();
    keys.sort();
    assert_eq!(keys, ["code", "message"], "{args:?}");
    assert_eq!(error["code"], "invalid_arguments", "{args:?}");
    let message = error["message"].as_str().expect("a string message");
    assert!(
        !message.contains('\n') && !message.contains('\r'),
        "{message:?}"
    );
    message.to_string()
}

#[test]
fn job020_ac5_an_invalid_draft_recipient_is_a_json_usage_error() {
    let args = [
        "draft",
        "--to",
        "not-an-address-zq7",
        "--subject",
        "S",
        "--body",
        "B",
    ];
    let message = usage_message(&args, &run_in_process(&args));
    assert!(message.contains("--to"), "{message}");
    assert!(
        message.contains("must be a bare address local@domain"),
        "{message}"
    );
    assert!(
        message.ends_with("run `agent-mail draft --help` for usage"),
        "{message}"
    );
    assert!(!message.contains("not-an-address-zq7"), "{message}");
}

#[test]
fn job020_ac6_every_command_reports_usage_errors_as_json() {
    let cases: [(&[&str], Option<&str>, &str); 11] = [
        (
            &["list", "--account", ""],
            Some("--account"),
            "agent-mail list --help",
        ),
        (
            &["search", "--limit", "0"],
            Some("--limit"),
            "agent-mail search --help",
        ),
        (
            &["read", "not-a-ref"],
            Some("REF"),
            "agent-mail read --help",
        ),
        (
            &["flag", VALID_REF],
            Some("--set"),
            "agent-mail flag --help",
        ),
        (&["move", VALID_REF], Some("--to"), "agent-mail move --help"),
        (
            &[
                "draft",
                "--to",
                "a@b.example",
                "--subject",
                "S",
                "--body",
                "B",
                "--reply-to",
                VALID_REF,
            ],
            Some("--subject"),
            "agent-mail draft --help",
        ),
        (
            &["whoami", "--full"],
            Some("--full"),
            "agent-mail whoami --help",
        ),
        (
            &["mandate", "delete"],
            Some("<ID>"),
            "agent-mail mandate delete --help",
        ),
        (
            &["mandate", "create", "--bogus"],
            Some("--bogus"),
            "agent-mail mandate create --help",
        ),
        (
            &["mcp", "--bogus"],
            Some("--bogus"),
            "agent-mail mcp --help",
        ),
        (&["lsit"], None, "agent-mail --help"),
    ];
    for (args, names, pointer) in cases {
        let message = usage_message(args, &run_in_process(args));
        if let Some(names) = names {
            assert!(message.contains(names), "{args:?}: {message}");
        }
        assert!(
            message.ends_with(&format!("run `{pointer}` for usage")),
            "{args:?}: {message}"
        );
        assert!(message.chars().count() <= 300, "{args:?}: {message}");
        // The help pointer is joined once, never after another `;`.
        assert_eq!(message.matches(';').count(), 1, "{args:?}: {message}");
    }

    // A suggested command is named inside the problem, not as a second
    // clause before the pointer.
    for (args, suggested, pointer) in [
        (&["lsit"][..], "list", "agent-mail --help"),
        (&["mandate", "delet"], "delete", "agent-mail mandate --help"),
    ] {
        let message = usage_message(args, &run_in_process(args));
        assert_eq!(
            message,
            format!("unknown command (did you mean `{suggested}`?); run `{pointer}` for usage")
        );
        assert!(!message.contains("?;"), "{message}");
    }

    // The built binary prints the same as the in-process run.
    let args = [
        "draft",
        "--to",
        "not-an-address-zq7",
        "--subject",
        "S",
        "--body",
        "B",
    ];
    let binary = run_agent_mail(&args);
    let in_process = run_in_process(&args);
    assert_eq!(binary.status.code(), Some(in_process.code));
    assert_eq!(String::from_utf8_lossy(&binary.stdout), in_process.stdout);
    assert_eq!(String::from_utf8_lossy(&binary.stderr), in_process.stderr);
}

#[test]
fn job020_ac7_usage_errors_never_echo_the_callers_values() {
    let args = ["list", "--bogus=hunter2zq7"];
    let message = usage_message(&args, &run_in_process(&args));
    assert!(message.contains("--bogus"), "{message}");
    assert!(!message.contains("hunter2zq7"), "{message}");

    let args = ["read", VALID_REF, "extra-word-zq7"];
    let message = usage_message(&args, &run_in_process(&args));
    assert!(!message.contains("extra-word-zq7"), "{message}");
    assert!(message.contains("unexpected argument"), "{message}");

    let args = ["search", "--subject", "a\nb-zq7"];
    let message = usage_message(&args, &run_in_process(&args));
    assert!(message.contains("--subject"), "{message}");
    assert!(!message.contains("zq7"), "{message}");

    // `--text` does not change the shape: parsing failed, so the output
    // mode is unknown.
    let args = ["--text", "list", "--bogus=hunter2zq7"];
    let message = usage_message(&args, &run_in_process(&args));
    assert!(!message.contains("hunter2zq7"), "{message}");
}

#[test]
fn job020_ac8_help_and_version_keep_clap_plain_text() {
    for args in [
        &["--help"][..],
        &["-h"],
        &["--version"],
        &["draft", "--help"],
        &["help", "draft"],
    ] {
        let out = run_in_process(args);
        assert_eq!(out.code, 0, "{args:?}: {out:?}");
        assert_eq!(out.stderr, "", "{args:?}");
        assert!(!out.stdout.is_empty(), "{args:?}");
        assert!(
            serde_json::from_str::<serde_json::Value>(&out.stdout).is_err(),
            "{args:?}: {:?}",
            out.stdout
        );
    }
    for args in [&[][..], &["mandate"]] {
        let out = run_in_process(args);
        assert_eq!(out.code, 2, "{args:?}: {out:?}");
        assert_eq!(out.stdout, "", "{args:?}");
        assert!(
            out.stderr.contains("Usage: agent-mail"),
            "{args:?}: {out:?}"
        );
        assert!(
            serde_json::from_str::<serde_json::Value>(&out.stderr).is_err(),
            "{args:?}: {:?}",
            out.stderr
        );
    }
}

#[test]
fn job020_ac9_a_usage_error_reads_no_store_connects_nowhere_and_audits_nothing() {
    let server = FakeImap::start(Mode::Implicit);
    let store = CountingStore::default();
    let token = store_mandate(
        &store.inner,
        &mandate(
            server.port,
            agent_mail::mandate::TlsMode::Implicit,
            vec![grant(Operation::Draft, &[])],
        ),
    );
    let log = temp_audit_log();
    let args = [
        "draft",
        "--to",
        "not-an-address",
        "--subject",
        "S",
        "--body",
        "B",
    ];
    let out = run_logged(&args, Some(token.expose()), &store, &server.ca, &log);
    usage_message(&args, &out);
    assert_eq!(store.accesses(), 0);
    assert_eq!(server.connections(), 0);
    assert!(audit_lines(&log).is_empty());
}

#[test]
fn job020_ac10_in_process_mcp_is_refused_as_json_or_text() {
    let message =
        "`agent-mail mcp` runs as a server on stdin and stdout; start the binary from an MCP host";
    let out = run_in_process(&["mcp"]);
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert_eq!(
        out.stderr,
        format!("{{\"error\":{{\"code\":\"invalid_arguments\",\"message\":\"{message}\"}}}}\n")
    );
    let out = run_in_process(&["--text", "mcp"]);
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr, format!("error: invalid_arguments: {message}\n"));
}
