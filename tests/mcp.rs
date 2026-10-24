//! `agent-mail mcp`: the MCP server over stdio, compared with the CLI.
//! All values are fake.

mod support;

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;

use agent_mail::mandate::{Grant, Operation};
use agent_mail::store::InMemoryStore;
use agent_mail::terminal::ScriptedTerminal;
use agent_mail::{Context, RunOutput};
use serde_json::{Value, json};
use support::*;

/// One side of a comparison: a fresh fake server, a store and an audit log.
struct Side {
    server: FakeImap,
    store: InMemoryStore,
    log: PathBuf,
}

impl Side {
    fn on(server: FakeImap) -> Self {
        Self {
            server,
            store: InMemoryStore::new(),
            log: temp_audit_log(),
        }
    }

    fn new() -> Self {
        Self::on(default_server())
    }

    /// Stores Mandate `id` covering `logins` with `grants`; returns its token.
    fn add(&self, id: &str, logins: &[&str], grants: Vec<Grant>) -> String {
        let mandate = mandate_with_id(id, "test-mandate", logins, self.server.port, grants);
        store_mandate(&self.store, &mandate).expose().to_string()
    }

    /// Mandate W; returns its token.
    fn w(&self) -> String {
        self.add("aaaa1111", &[LOGIN], w_grants())
    }

    fn cli(&self, args: &[&str], tokens: Option<&str>) -> RunOutput {
        run_raw_tokens(args, tokens, &self.store, &self.server, &self.log)
    }

    /// A session (initialize, initialized, then `requests`); returns the
    /// responses after the `initialize` one.
    fn session(&self, requests: &[String], tokens: Option<&str>) -> Vec<Value> {
        let mut lines = mcp_opening();
        lines.extend(requests.iter().cloned());
        let mut out = mcp_lines(&lines, tokens, &self.store, &self.server.ca, &self.log);
        assert_eq!(out[0]["id"], json!(1), "first response is initialize's");
        out.remove(0);
        out
    }

    /// One `tools/call` in a session; returns its JSON-RPC `result`.
    fn tool(&self, name: &str, arguments: Value, tokens: Option<&str>) -> Value {
        let out = self.session(&[mcp_call(2, name, arguments)], tokens);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0]["jsonrpc"], json!("2.0"));
        assert_eq!(out[0]["id"], json!(2));
        assert!(out[0].get("error").is_none(), "a JSON-RPC error: {out:?}");
        out[0]["result"].clone()
    }

    fn audit(&self) -> Vec<String> {
        audit_lines(&self.log)
    }
}

fn default_server() -> FakeImap {
    FakeImap::start_with_list(
        Mode::Implicit,
        &["MOVE", "UIDPLUS"],
        &["Drafts"],
        Some("Drafts"),
    )
}

fn w_grants() -> Vec<Grant> {
    [
        Operation::List,
        Operation::Search,
        Operation::Read,
        Operation::Flag,
        Operation::Move,
        Operation::Draft,
    ]
    .into_iter()
    .map(|op| grant(op, &["*"]))
    .collect()
}

fn inbox_ref() -> String {
    format!("{LOGIN}/INBOX/{INBOX_UIDVALIDITY}/1")
}

fn triage_ref() -> String {
    format!("{LOGIN}/Triage/{TRIAGE_UIDVALIDITY}/1")
}

fn thread_ref() -> String {
    format!("{LOGIN}/Thread/{THREAD_UIDVALIDITY}/1")
}

fn long_ref() -> String {
    format!("{LOGIN}/Read/{READ_UIDVALIDITY}/4")
}

/// `text` with the fake server's port replaced, so two servers compare.
fn without_port(text: &str, port: u16) -> String {
    text.replace(&format!("\"port\":{port}"), "\"port\":PORT")
}

/// An audit line without its leading `time` key, keeping the rest as is.
fn without_time(line: &str) -> String {
    let rest = line
        .strip_prefix("{\"time\":\"")
        .unwrap_or_else(|| panic!("audit line does not start with time: {line}"));
    let end = rest.find("\",").expect("time value ends");
    format!("{{{}", &rest[end + 2..])
}

/// The texts of a tool result's `content` items.
fn texts(result: &Value) -> Vec<String> {
    result["content"]
        .as_array()
        .expect("content is an array")
        .iter()
        .map(|item| {
            assert_eq!(item["type"], json!("text"));
            item["text"].as_str().expect("text item").to_string()
        })
        .collect()
}

/// The lines a CLI stream printed, each without its `\n`.
fn stream_lines(stream: &str) -> Vec<String> {
    assert!(stream.is_empty() || stream.ends_with('\n'));
    stream.lines().map(str::to_string).collect()
}

/// A finished CLI run and tool call on two sides with the same fixtures.
struct Compared {
    cli: Side,
    mcp: Side,
    cli_out: RunOutput,
    result: Value,
}

/// Builds two sides on `server`, applies `setup` (which returns the raw
/// `AGENT_MAIL_TOKEN`) to each, runs the CLI on one and the tool on the
/// other.
fn compare(
    server: fn() -> FakeImap,
    setup: impl Fn(&Side) -> Option<String>,
    cli_args: &[&str],
    tool: &str,
    arguments: Value,
) -> Compared {
    let cli = Side::on(server());
    let mcp = Side::on(server());
    let cli_tokens = setup(&cli);
    let mcp_tokens = setup(&mcp);
    let cli_out = cli.cli(cli_args, cli_tokens.as_deref());
    let result = mcp.tool(tool, arguments, mcp_tokens.as_deref());
    Compared {
        cli,
        mcp,
        cli_out,
        result,
    }
}

impl Compared {
    /// AC2: the tool result is the CLI's stdout line; the warning, if the
    /// CLI printed one, is the second item.
    fn assert_same_success(&self) {
        assert_eq!(self.cli_out.code, 0, "{:?}", self.cli_out);
        assert_eq!(self.result["isError"], json!(false), "{}", self.result);
        let texts = texts(&self.result);
        let stdout = self.cli_out.stdout.strip_suffix('\n').expect("one line");
        assert!(!stdout.contains('\n'));
        assert_eq!(
            without_port(&texts[0], self.mcp.server.port),
            without_port(stdout, self.cli.server.port)
        );
        let parsed: Value = serde_json::from_str(&texts[0]).unwrap();
        assert_eq!(self.result["structuredContent"], parsed);
        let warnings = stream_lines(&self.cli_out.stderr);
        assert_eq!(texts[1..].to_vec(), warnings);
    }

    /// AC3: the tool result is the CLI's stderr lines, warning first.
    fn assert_same_error(&self) -> Value {
        assert_eq!(self.cli_out.code, 1, "{:?}", self.cli_out);
        assert_eq!(self.cli_out.stdout, "");
        assert_eq!(self.result["isError"], json!(true), "{}", self.result);
        let texts = texts(&self.result);
        let stderr = stream_lines(&self.cli_out.stderr);
        assert_eq!(texts, stderr);
        let last: Value = serde_json::from_str(texts.last().unwrap()).unwrap();
        assert_eq!(self.result["structuredContent"], last);
        assert!(last["error"]["code"].is_string());
        last["error"].clone()
    }

    /// AC6: each side appended exactly `n` lines, equal without `time`.
    fn assert_same_audit(&self, n: usize) {
        let (cli, mcp) = (self.cli.audit(), self.mcp.audit());
        assert_eq!(cli.len(), n, "CLI audit: {cli:?}");
        assert_eq!(mcp.len(), n, "MCP audit: {mcp:?}");
        for (c, m) in cli.iter().zip(&mcp) {
            assert_eq!(without_time(m), without_time(c));
        }
    }
}

fn with_w(side: &Side) -> Option<String> {
    Some(side.w())
}

// ---------------------------------------------------------------- AC1

/// A tool's name, its properties with their JSON types, and its required
/// properties.
type ExpectedTool = (
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static [&'static str],
);

/// A server, the CLI arguments, the tool, its arguments and the error code.
type MailErrorCase<'a> = (fn() -> FakeImap, Vec<&'a str>, &'a str, Value, &'a str);

#[test]
fn ac1_tools_list_returns_the_seven_agent_tools_with_their_schemas() {
    let side = Side::new();
    let out = side.session(
        &[json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}).to_string()],
        None,
    );
    assert_eq!(out.len(), 1);
    let tools = out[0]["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        ["whoami", "list", "search", "read", "flag", "move", "draft"]
    );
    let expected: [ExpectedTool; 7] = [
        ("whoami", &[], &[]),
        ("list", &[("account", "string")], &[]),
        (
            "search",
            &[
                ("account", "string"),
                ("mailbox", "string"),
                ("from", "string"),
                ("to", "string"),
                ("subject", "string"),
                ("text", "string"),
                ("since", "string"),
                ("before", "string"),
                ("gmail", "string"),
                ("unseen", "boolean"),
                ("flagged", "boolean"),
                ("limit", "integer"),
            ],
            &[],
        ),
        ("read", &[("ref", "string"), ("full", "boolean")], &["ref"]),
        (
            "flag",
            &[("ref", "string"), ("set", "string"), ("clear", "string")],
            &["ref"],
        ),
        (
            "move",
            &[("ref", "string"), ("to", "string")],
            &["ref", "to"],
        ),
        (
            "draft",
            &[
                ("to", "array"),
                ("subject", "string"),
                ("body", "string"),
                ("reply_to", "string"),
                ("account", "string"),
            ],
            &["to", "body"],
        ),
    ];
    for (tool, (name, props, required)) in tools.iter().zip(expected) {
        assert!(
            !tool["description"].as_str().unwrap().is_empty(),
            "{name} has no description"
        );
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], json!("object"), "{name}");
        assert_eq!(schema["additionalProperties"], json!(false), "{name}");
        let properties = schema["properties"].as_object().unwrap();
        let mut got: Vec<(&str, &str)> = properties
            .iter()
            .map(|(k, v)| (k.as_str(), v["type"].as_str().unwrap()))
            .collect();
        got.sort();
        let mut want = props.to_vec();
        want.sort();
        assert_eq!(got, want, "{name} properties");
        let mut got_required: Vec<&str> = schema
            .get("required")
            .map(|r| {
                r.as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect()
            })
            .unwrap_or_default();
        got_required.sort();
        let mut want_required = required.to_vec();
        want_required.sort();
        assert_eq!(got_required, want_required, "{name} required");
        for key in properties.keys() {
            assert!(
                !["token", "text_output", "output", "format"].contains(&key.as_str()),
                "{name} has {key}"
            );
        }
    }
    let search = &tools[2]["inputSchema"]["properties"];
    assert_eq!(search["limit"]["minimum"], json!(1));
    assert_eq!(search["limit"]["maximum"], json!(1000));
    assert_eq!(search["limit"]["default"], json!(20));
    assert_eq!(search["mailbox"]["default"], json!("INBOX"));
    assert!(
        search["text"]["description"]
            .as_str()
            .unwrap()
            .contains("TEXT criterion")
    );
    let to = &tools[6]["inputSchema"]["properties"]["to"];
    assert_eq!(to["items"]["type"], json!("string"));
    assert_eq!(to["minItems"], json!(1));
}

#[test]
fn ac1_bad_arguments_fail_with_invalid_arguments_before_any_connection() {
    let r = inbox_ref();
    let t = thread_ref();
    let cases: Vec<(&str, Value, &str)> = vec![
        (
            "flag",
            json!({"ref": r, "set": "seen", "clear": "seen"}),
            "set",
        ),
        ("flag", json!({"ref": r}), "set"),
        (
            "draft",
            json!({"to": ["carol@example.com"], "subject": "S", "reply_to": t, "body": "B"}),
            "reply_to",
        ),
        (
            "draft",
            json!({"to": ["carol@example.com"], "body": "B"}),
            "subject",
        ),
        (
            "draft",
            json!({"to": ["carol@example.com"], "reply_to": t, "account": LOGIN, "body": "B"}),
            "account",
        ),
        (
            "search",
            json!({"gmail": "has:attachment", "from": "alice"}),
            "gmail",
        ),
        ("search", json!({"limit": 0}), "limit"),
        ("search", json!({"limit": 1001}), "limit"),
        ("search", json!({"limit": "20"}), "limit"),
        ("search", json!({"since": "2026-13-01"}), "since"),
        ("read", json!({"ref": "not-a-ref"}), "ref"),
        ("read", json!({}), "ref"),
        ("move", json!({"ref": r, "to": ""}), "to"),
        ("list", json!({"account": "a\nb"}), "account"),
        (
            "draft",
            json!({"to": ["Bob <bob@example.com>"], "subject": "S", "body": "B"}),
            "to",
        ),
        ("whoami", json!({"verbose": true}), "verbose"),
        ("list", json!({"verbose": true}), "verbose"),
        ("search", json!({"verbose": true}), "verbose"),
        ("read", json!({"ref": r, "verbose": true}), "verbose"),
        (
            "flag",
            json!({"ref": r, "set": "seen", "verbose": true}),
            "verbose",
        ),
        (
            "move",
            json!({"ref": r, "to": "Archive", "verbose": true}),
            "verbose",
        ),
        (
            "draft",
            json!({"to": ["carol@example.com"], "subject": "S", "body": "B", "verbose": true}),
            "verbose",
        ),
    ];
    for (tool, arguments, offending) in cases {
        let side = Side::new();
        let tw = side.w();
        let result = side.tool(tool, arguments.clone(), Some(&tw));
        assert_eq!(result["isError"], json!(true), "{tool} {arguments}");
        let error = &result["structuredContent"]["error"];
        assert_eq!(
            error["code"],
            json!("invalid_arguments"),
            "{tool} {arguments}"
        );
        assert_eq!(
            result["structuredContent"].as_object().unwrap().len(),
            1,
            "{result}"
        );
        assert_eq!(error.as_object().unwrap().len(), 2, "{error}");
        let message = error["message"].as_str().unwrap();
        assert!(
            message.contains(&format!("`{offending}`")),
            "{tool} {arguments}: {message}"
        );
        let text: Value = serde_json::from_str(&texts(&result)[0]).unwrap();
        assert_eq!(text, result["structuredContent"]);
        assert_eq!(side.server.connections(), 0, "{tool} {arguments}");
        assert!(side.audit().is_empty(), "{tool} {arguments}");
    }
}

// ---------------------------------------------------------------- AC2 + AC6

#[test]
fn ac2_whoami_matches_the_cli_and_writes_no_audit_line() {
    let c = compare(default_server, with_w, &["whoami"], "whoami", json!({}));
    c.assert_same_success();
    c.assert_same_audit(0);
}

#[test]
fn ac2_list_matches_the_cli() {
    let c = compare(default_server, with_w, &["list"], "list", json!({}));
    c.assert_same_success();
    c.assert_same_audit(1);
}

#[test]
fn ac2_search_matches_the_cli() {
    let c = compare(
        default_server,
        with_w,
        &["search", "--mailbox", "INBOX", "--limit", "2"],
        "search",
        json!({"mailbox": "INBOX", "limit": 2}),
    );
    c.assert_same_success();
    c.assert_same_audit(1);
    assert_eq!(
        c.result["structuredContent"]["messages"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let c = compare(default_server, with_w, &["search"], "search", json!({}));
    c.assert_same_success();
    c.assert_same_audit(1);
    assert_eq!(c.result["structuredContent"]["mailbox"], json!("INBOX"));
}

#[test]
fn ac2_read_matches_the_cli() {
    let r = inbox_ref();
    let c = compare(
        default_server,
        with_w,
        &["read", &r],
        "read",
        json!({"ref": r}),
    );
    c.assert_same_success();
    c.assert_same_audit(1);

    let long = long_ref();
    let c = compare(
        default_server,
        with_w,
        &["read", &long, "--full"],
        "read",
        json!({"ref": long, "full": true}),
    );
    c.assert_same_success();
    c.assert_same_audit(1);
    assert_eq!(c.result["structuredContent"]["truncated"], json!(false));
}

#[test]
fn ac2_flag_matches_the_cli_and_the_server_state() {
    let r = triage_ref();
    let c = compare(
        default_server,
        with_w,
        &["flag", &r, "--set", "seen"],
        "flag",
        json!({"ref": r, "set": "seen"}),
    );
    c.assert_same_success();
    c.assert_same_audit(1);
    assert_eq!(
        c.mcp.server.messages_in("Triage"),
        c.cli.server.messages_in("Triage")
    );
    assert!(
        c.mcp.server.messages_in("Triage")[0]
            .2
            .contains(&"\\Seen".to_string())
    );
}

#[test]
fn ac2_move_matches_the_cli_and_the_server_state() {
    let r = inbox_ref();
    let c = compare(
        default_server,
        with_w,
        &["move", &r, "--to", "Archive"],
        "move",
        json!({"ref": r, "to": "Archive"}),
    );
    c.assert_same_success();
    c.assert_same_audit(1);
    for mailbox in ["INBOX", "Archive"] {
        assert_eq!(
            c.mcp.server.messages_in(mailbox),
            c.cli.server.messages_in(mailbox),
            "{mailbox}"
        );
    }
}

/// The appended drafts' mailbox, flags and headers, without Date and
/// Message-ID, which differ per run.
fn appended_without_date(server: &FakeImap) -> Vec<(String, Vec<String>, Vec<String>)> {
    server
        .appended()
        .into_iter()
        .map(|(mailbox, flags, raw)| {
            let lines = raw
                .split("\r\n")
                .filter(|l| {
                    let lower = l.to_ascii_lowercase();
                    !lower.starts_with("date:") && !lower.starts_with("message-id:")
                })
                .map(str::to_string)
                .collect();
            (mailbox, flags, lines)
        })
        .collect()
}

#[test]
fn ac2_draft_matches_the_cli_and_the_appended_message() {
    let c = compare(
        default_server,
        with_w,
        &[
            "draft",
            "--to",
            "carol@example.com",
            "--subject",
            "S",
            "--body",
            "B",
        ],
        "draft",
        json!({"to": ["carol@example.com"], "subject": "S", "body": "B"}),
    );
    c.assert_same_success();
    c.assert_same_audit(1);
    assert_eq!(c.mcp.server.appended().len(), 1);
    assert_eq!(
        appended_without_date(&c.mcp.server),
        appended_without_date(&c.cli.server)
    );

    let t = thread_ref();
    let c = compare(
        default_server,
        with_w,
        &[
            "draft",
            "--reply-to",
            &t,
            "--to",
            "carol@example.com",
            "--body",
            "B",
        ],
        "draft",
        json!({"reply_to": t, "to": ["carol@example.com"], "body": "B"}),
    );
    c.assert_same_success();
    c.assert_same_audit(1);
    assert_eq!(c.mcp.server.appended().len(), 1);
    assert_eq!(
        appended_without_date(&c.mcp.server),
        appended_without_date(&c.cli.server)
    );
}

#[test]
fn ac2_invalid_token_warning_is_a_second_content_item() {
    let c = compare(
        default_server,
        |s| Some(format!("garbage,{}", s.w())),
        &["list"],
        "list",
        json!({}),
    );
    c.assert_same_success();
    c.assert_same_audit(1);
    let texts = texts(&c.result);
    assert_eq!(texts.len(), 2);
    let warning: Value = serde_json::from_str(&texts[1]).unwrap();
    assert_eq!(warning["warning"]["code"], json!("invalid_token"));
    assert_eq!(warning["warning"]["positions"], json!([1]));
    assert!(
        texts[1]
            .starts_with("{\"warning\":{\"code\":\"invalid_token\",\"positions\":[1],\"message\":")
    );

    let side = Side::new();
    let tw = side.w();
    let result = side.tool("list", json!({}), Some(&tw));
    assert_eq!(result["content"].as_array().unwrap().len(), 1);
}

// ---------------------------------------------------------------- AC3 + AC6

#[test]
fn ac3_missing_and_invalid_tokens_are_tool_errors() {
    let c = compare(default_server, |_| None, &["list"], "list", json!({}));
    assert_eq!(c.assert_same_error()["code"], json!("missing_token"));
    c.assert_same_audit(1);

    let c = compare(
        default_server,
        |s| {
            s.w();
            Some("garbage".to_string())
        },
        &["list"],
        "list",
        json!({}),
    );
    assert_eq!(c.assert_same_error()["code"], json!("invalid_token"));
    c.assert_same_audit(1);
}

#[test]
fn ac3_denied_names_operation_account_and_mailbox() {
    let c = compare(
        default_server,
        |s| {
            Some(s.add(
                "aaaa1111",
                &[LOGIN],
                vec![grant(Operation::List, &["INBOX"])],
            ))
        },
        &["search", "--mailbox", "Archive"],
        "search",
        json!({"mailbox": "Archive"}),
    );
    let error = c.assert_same_error();
    assert_eq!(error["code"], json!("denied"));
    assert_eq!(error["operation"], json!("search"));
    assert_eq!(error["account"], json!(LOGIN));
    assert_eq!(error["mailbox"], json!("Archive"));
    c.assert_same_audit(1);
}

#[test]
fn ac3_account_required_and_ambiguous_mandate() {
    let c = compare(
        default_server,
        |s| {
            Some(s.add(
                "aaaa1111",
                &[LOGIN, OTHER_LOGIN],
                vec![
                    grant(Operation::List, &["*"]),
                    grant_on(Operation::List, OTHER_LOGIN, &["*"]),
                ],
            ))
        },
        &["list"],
        "list",
        json!({}),
    );
    let error = c.assert_same_error();
    assert_eq!(error["code"], json!("account_required"));
    let accounts = error["accounts"].to_string();
    assert!(
        accounts.contains(LOGIN) && accounts.contains(OTHER_LOGIN),
        "{accounts}"
    );
    c.assert_same_audit(1);

    let c = compare(
        default_server,
        |s| {
            let one = s.add("aaaa1111", &[LOGIN], vec![grant(Operation::List, &["*"])]);
            let two = s.add("cccc3333", &[LOGIN], vec![grant(Operation::List, &["*"])]);
            Some(format!("{one},{two}"))
        },
        &["list"],
        "list",
        json!({}),
    );
    assert_eq!(c.assert_same_error()["code"], json!("ambiguous_mandate"));
    c.assert_same_audit(1);
}

#[test]
fn ac3_mail_errors_match_the_cli() {
    let stale = format!("{LOGIN}/INBOX/{}/1", INBOX_UIDVALIDITY + 1);
    let r = inbox_ref();
    let cases: Vec<MailErrorCase> = vec![
        (
            default_server,
            vec!["read", &stale],
            "read",
            json!({"ref": stale}),
            "stale_reference",
        ),
        (
            default_server,
            vec!["flag", &r, "--set", "answered"],
            "flag",
            json!({"ref": r, "set": "answered"}),
            "unsupported_flag",
        ),
        (
            default_server,
            vec!["move", &r, "--to", "INBOX"],
            "move",
            json!({"ref": r, "to": "INBOX"}),
            "same_mailbox",
        ),
        (
            default_server,
            vec!["search", "--gmail", "has:attachment"],
            "search",
            json!({"gmail": "has:attachment"}),
            "unsupported",
        ),
        (
            || FakeImap::start(Mode::Implicit),
            vec![
                "draft",
                "--to",
                "carol@example.com",
                "--subject",
                "S",
                "--body",
                "B",
            ],
            "draft",
            json!({"to": ["carol@example.com"], "subject": "S", "body": "B"}),
            "drafts_not_found",
        ),
    ];
    for (server, args, tool, arguments, code) in cases {
        let c = compare(server, with_w, &args, tool, arguments);
        assert_eq!(c.assert_same_error()["code"], json!(code), "{tool}");
        c.assert_same_audit(1);
    }
}

#[test]
fn ac3_warning_comes_before_the_error() {
    let c = compare(
        default_server,
        |s| Some(format!("garbage,{}", s.w())),
        &["list", "--account", OTHER_LOGIN],
        "list",
        json!({"account": OTHER_LOGIN}),
    );
    let error = c.assert_same_error();
    assert_eq!(error["code"], json!("denied"));
    let texts = texts(&c.result);
    assert_eq!(texts.len(), 2);
    assert!(texts[0].starts_with("{\"warning\":"));
    assert!(texts[1].starts_with("{\"error\":"));
    c.assert_same_audit(1);
}

// ---------------------------------------------------------------- AC4

fn p_grants() -> Vec<Grant> {
    vec![grant_on(Operation::List, OTHER_LOGIN, &["Archive"])]
}

#[test]
fn ac4_tool_arguments_cannot_carry_a_token() {
    for key in ["token", "AGENT_MAIL_TOKEN"] {
        let side = Side::new();
        let tw = side.w();
        let tp = side.add("bbbb2222", &[OTHER_LOGIN], p_grants());
        let result = side.tool("list", json!({ key: tp }), Some(&tw));
        assert_eq!(result["isError"], json!(true));
        assert_eq!(
            result["structuredContent"]["error"]["code"],
            json!("invalid_arguments")
        );
        assert_eq!(side.server.connections(), 0);
        assert!(side.audit().is_empty());
    }
}

#[test]
fn ac4_meta_is_never_a_token_source() {
    let side = Side::new();
    let tw = side.w();
    let tp = side.add("bbbb2222", &[OTHER_LOGIN], p_grants());
    let request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "list",
            "arguments": {},
            "_meta": { "AGENT_MAIL_TOKEN": tp }
        }
    })
    .to_string();
    let out = side.session(&[request], Some(&tw));
    let result = &out[0]["result"];
    assert_eq!(result["isError"], json!(false), "{result}");
    assert_eq!(result["structuredContent"]["account"], json!(LOGIN));
    let audit = side.audit();
    assert_eq!(audit.len(), 1);
    let entry: Value = serde_json::from_str(&audit[0]).unwrap();
    assert_eq!(entry["mandate"], json!("aaaa1111"));
}

#[test]
fn ac4_without_a_token_every_tool_fails_with_missing_token() {
    let r = inbox_ref();
    let side = Side::new();
    let tw = side.w();
    let calls = [
        ("whoami", json!({})),
        ("list", json!({})),
        ("list", json!({ "token": tw })),
        ("search", json!({})),
        ("read", json!({ "ref": r })),
        ("flag", json!({ "ref": r, "set": "seen" })),
        ("move", json!({ "ref": r, "to": "Archive" })),
        (
            "draft",
            json!({ "to": ["carol@example.com"], "subject": "S", "body": "B" }),
        ),
        ("draft", json!({ "AGENT_MAIL_TOKEN": tw })),
    ];
    for (tool, arguments) in calls {
        let result = side.tool(tool, arguments.clone(), None);
        assert_eq!(result["isError"], json!(true), "{tool} {arguments}");
        assert_eq!(
            result["structuredContent"]["error"]["code"],
            json!("missing_token"),
            "{tool} {arguments}"
        );
    }
    assert_eq!(side.server.connections(), 0);
}

#[test]
fn ac4_the_token_is_captured_once_for_the_session() {
    let side = Side::new();
    let tw = side.w();
    let out = side.session(
        &[
            mcp_call(2, "list", json!({})),
            mcp_call(3, "list", json!({})),
        ],
        Some(&tw),
    );
    assert_eq!(out.len(), 2);
    for response in &out {
        assert_eq!(response["result"]["isError"], json!(false));
        assert_eq!(
            response["result"]["structuredContent"]["account"],
            json!(LOGIN)
        );
    }
    assert_eq!(out[0]["result"], out[1]["result"]);
    assert_eq!(side.audit().len(), 2);
}

#[test]
fn ac4_no_secret_leaves_in_a_whole_session() {
    let side = Side::new();
    let tw = side.w();
    let tp = side.add("bbbb2222", &[OTHER_LOGIN], p_grants());
    let r = inbox_ref();
    let requests = vec![
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}).to_string(),
        mcp_call(3, "whoami", json!({})),
        mcp_call(4, "list", json!({})),
        mcp_call(5, "search", json!({})),
        mcp_call(6, "read", json!({"ref": r})),
        mcp_call(7, "flag", json!({"ref": triage_ref(), "set": "flagged"})),
        mcp_call(
            8,
            "draft",
            json!({"to": ["carol@example.com"], "subject": "S", "body": "B"}),
        ),
        mcp_call(9, "move", json!({"ref": r, "to": "Archive"})),
        // A denial and an error.
        mcp_call(
            10,
            "search",
            json!({"account": OTHER_LOGIN, "mailbox": "Archive"}),
        ),
        mcp_call(11, "read", json!({"ref": format!("{LOGIN}/INBOX/1/1")})),
        mcp_call(12, "list", json!({"token": tp})),
    ];
    let mut lines = mcp_opening();
    lines.extend(requests);
    let tokens = format!("{tw},{tp}");
    let mut env = HashMap::new();
    env.insert(agent_mail::TOKEN_VAR.to_string(), tokens);
    let context = Context::trusting_only(&side.store, vec![side.server.ca.clone()])
        .with_audit_log(agent_mail::audit::AuditLog::at(&side.log));
    let raw = mcp_raw(&lines, &env, &context);
    let parsed = mcp_parse(&raw);
    assert_eq!(parsed.len(), 12);
    // The denial (P does not grant search) and the error came back.
    let results: Vec<&Value> = parsed.iter().map(|p| &p["result"]).collect();
    assert_eq!(
        results[9]["structuredContent"]["error"]["code"],
        json!("denied")
    );
    assert_eq!(results[10]["isError"], json!(true));
    for secret in [
        tw.clone(),
        tp.clone(),
        CREDENTIAL.to_string(),
        token_hash(&tw),
        token_hash(&tp),
    ] {
        assert!(!raw.contains(&secret), "output holds a secret");
    }
    assert!(tokens_in(&raw).is_empty());
}

// ---------------------------------------------------------------- AC5

#[test]
fn ac5_no_tool_manages_mandates() {
    let side = Side::new();
    let tw = side.w();
    let before = side.store.entries();
    let terminal = ScriptedTerminal::new(Vec::new());
    let names = [
        "mandate",
        "mandate_create",
        "mandate_list",
        "mandate_delete",
        "create",
        "delete",
    ];
    let mut lines = mcp_opening();
    lines.extend(
        names
            .iter()
            .enumerate()
            .map(|(i, name)| mcp_call(i as u64 + 2, name, json!({}))),
    );
    let mut env = HashMap::new();
    env.insert(agent_mail::TOKEN_VAR.to_string(), tw);
    let context = Context::trusting_only(&side.store, vec![side.server.ca.clone()])
        .with_audit_log(agent_mail::audit::AuditLog::at(&side.log))
        .with_terminal(&terminal);
    let out = mcp_parse(&mcp_raw(&lines, &env, &context));
    assert_eq!(out.len(), names.len() + 1);
    for (response, name) in out[1..].iter().zip(names) {
        assert!(response.get("result").is_none(), "{response}");
        let error = &response["error"];
        assert_eq!(error["code"], json!(-32602), "{name}");
        let message = error["message"].as_str().unwrap();
        assert!(message.contains(&format!("`{name}`")), "{message}");
        assert!(message.contains("tools/list"), "{message}");
        assert!(message.contains("`agent-mail mandate`"), "{message}");
        assert!(message.contains("terminal"), "{message}");
    }
    assert_eq!(side.store.entries(), before);
    assert!(side.audit().is_empty());
    assert!(terminal.transcript().is_empty());
    assert_eq!(side.server.connections(), 0);
}

#[test]
fn ac5_mcp_help_says_mandates_are_not_managed_over_mcp() {
    let env = HashMap::new();
    let store = InMemoryStore::new();
    let context = Context::trusting_only(&store, Vec::new());
    let out = agent_mail::run(["agent-mail", "mcp", "--help"], &env, &context);
    assert_eq!(out.code, 0, "{out:?}");
    let help = out.stdout.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        help.contains("Run as an MCP server over stdio for a host app"),
        "{help}"
    );
    assert!(
        help.contains("Mandate management is not offered over MCP"),
        "{help}"
    );
}

// ---------------------------------------------------------------- AC6

#[test]
fn ac6_protocol_messages_and_rejected_calls_write_no_audit_line() {
    let side = Side::new();
    let tw = side.w();
    let out = side.session(
        &[
            json!({"jsonrpc":"2.0","id":2,"method":"ping"}).to_string(),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}).to_string(),
            mcp_call(4, "whoami", json!({})),
            mcp_call(5, "list", json!({"verbose": true})),
            mcp_call(6, "mandate_create", json!({})),
        ],
        Some(&tw),
    );
    assert_eq!(out.len(), 5);
    assert!(side.audit().is_empty(), "{:?}", side.audit());
}

#[test]
fn ac6_audit_unavailable_is_a_tool_error_as_on_the_cli() {
    let dir = std::env::temp_dir().join(format!("agent-mail-mcp-audit-dir-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let side = Side::new();
    let tw = side.w();
    let mut env = HashMap::new();
    env.insert(agent_mail::TOKEN_VAR.to_string(), tw);
    let context = Context::trusting_only(&side.store, vec![side.server.ca.clone()])
        .with_audit_log(agent_mail::audit::AuditLog::at(&dir));
    let cli = agent_mail::run(["agent-mail", "list"], &env, &context);
    let mut lines = mcp_opening();
    lines.push(mcp_call(2, "list", json!({})));
    let out = mcp_parse(&mcp_raw(&lines, &env, &context));
    let result = &out[1]["result"];
    assert_eq!(result["isError"], json!(true));
    assert_eq!(
        result["structuredContent"]["error"]["code"],
        json!("audit_unavailable")
    );
    assert_eq!(cli.stdout, "");
    assert_eq!(texts(result), stream_lines(&cli.stderr));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------- AC7

fn protocol(lines: &[&str]) -> (String, Vec<Value>) {
    let store = InMemoryStore::new();
    let env = HashMap::new();
    let context = Context::trusting_only(&store, Vec::new());
    let lines: Vec<String> = lines.iter().map(|l| (*l).to_string()).collect();
    let raw = mcp_raw(&lines, &env, &context);
    let parsed = mcp_parse(&raw);
    (raw, parsed)
}

#[test]
fn ac7_initialize_negotiates_the_protocol_version() {
    for (requested, answered) in [
        ("2025-06-18", "2025-06-18"),
        ("2025-03-26", "2025-03-26"),
        ("2024-11-05", "2025-06-18"),
        ("bogus", "2025-06-18"),
    ] {
        let line = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": requested, "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}
        })
        .to_string();
        let (_, out) = protocol(&[&line]);
        let mut response = out[0].clone();
        let instructions = response["result"]
            .as_object_mut()
            .unwrap()
            .remove("instructions")
            .expect("instructions");
        assert!(instructions.as_str().unwrap().contains("whoami"));
        assert_eq!(
            response,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "protocolVersion": answered,
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "agent-mail", "version": env!("CARGO_PKG_VERSION")}
                }
            })
        );
    }
}

#[test]
fn ac7_notifications_ping_and_unknown_methods() {
    let (raw, out) = protocol(&[
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":"abc","method":"resources/list"}"#,
        r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":"s-9","method":"tools/call","params":{"arguments":{}}}"#,
    ]);
    assert_eq!(out.len(), 4, "{raw}");
    assert_eq!(out[0], json!({"jsonrpc":"2.0","id":7,"result":{}}));
    assert_eq!(out[1]["id"], json!("abc"));
    assert_eq!(out[1]["error"]["code"], json!(-32601));
    assert_eq!(out[2]["id"], json!(8));
    assert_eq!(out[2]["error"]["code"], json!(-32602));
    assert_eq!(out[3]["id"], json!("s-9"));
    assert_eq!(out[3]["error"]["code"], json!(-32602));
}

#[test]
fn ac7_bad_json_is_answered_and_the_server_keeps_going() {
    let (raw, out) = protocol(&[
        "this is not json",
        r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
        "{\"jsonrpc\":",
    ]);
    assert_eq!(out[0]["error"]["code"], json!(-32700));
    assert_eq!(out[0]["id"], Value::Null);
    assert!(out[0].as_object().unwrap().contains_key("id"));
    assert_eq!(out[1], json!({"jsonrpc":"2.0","id":2,"result":{}}));
    assert_eq!(out[2]["error"]["code"], json!(-32700));
    assert_eq!(out.len(), 3);
    assert_eq!(raw.lines().count(), 3);
}

#[test]
fn ac7_unknown_names_are_echoed_escaped() {
    let long = "x".repeat(10_000);
    let request =
        json!({"jsonrpc":"2.0","id":4,"method":format!("evil\u{1b}[2J\r{long}")}).to_string();
    let call =
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"evil\u{7}\u{1b}"}})
            .to_string();
    let (raw, out) = protocol(&[&request, &call]);
    assert_eq!(out[0]["error"]["code"], json!(-32601));
    assert_eq!(out[1]["error"]["code"], json!(-32602));
    for response in &out {
        let message = response["error"]["message"].as_str().unwrap();
        assert!(!message.chars().any(char::is_control), "{message:?}");
        assert!(message.len() < 500);
    }
    assert!(raw.len() < 2_000);
}

#[test]
fn ac7_eof_ends_the_session_cleanly() {
    let (raw, out) = protocol(&[]);
    assert_eq!(raw, "");
    assert!(out.is_empty());
}

#[test]
fn ac7_the_binary_serves_mcp_on_stdio() {
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_agent-mail"))
        .arg("mcp")
        .env_remove("AGENT_MAIL_TOKEN")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn agent-mail mcp");
    {
        let mut stdin = child.stdin.take().unwrap();
        let mut input = mcp_opening().join("\n");
        input.push('\n');
        input.push_str(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}).to_string());
        input.push('\n');
        stdin.write_all(input.as_bytes()).unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let out = mcp_parse(&stdout);
    assert_eq!(out.len(), 2, "{stdout}");
    assert_eq!(out[0]["result"]["serverInfo"]["name"], json!("agent-mail"));
    let names: Vec<&str> = out[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["whoami", "list", "search", "read", "flag", "move", "draft"]
    );
}

// ---------------------------------------------------------------- AC8

fn repo_file(name: &str) -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name))
        .unwrap_or_else(|e| panic!("cannot read {name}: {e}"))
}

#[test]
fn ac8_readme_help_and_claude_md_describe_the_mcp_server() {
    let readme = repo_file("README.md");
    assert!(
        readme.contains("\n## MCP server\n"),
        "README lacks the section"
    );
    for needle in [
        "agent-mail mcp",
        "AGENT_MAIL_TOKEN",
        "invalid_arguments",
        "claude_desktop_config.json",
        "\"command\": \"agent-mail\"",
        "\"args\": [\"mcp\"]",
        ".mcp.json",
        "claude mcp add",
    ] {
        assert!(readme.contains(needle), "README lacks {needle:?}");
    }
    for tool in ["whoami", "list", "search", "read", "flag", "move", "draft"] {
        assert!(readme.contains(&format!("`{tool}`")), "README lacks {tool}");
    }

    let env = HashMap::new();
    let store = InMemoryStore::new();
    let context = Context::trusting_only(&store, Vec::new());
    let help = agent_mail::run(["agent-mail", "--help"], &env, &context);
    assert_eq!(help.code, 0);
    assert!(
        help.stdout
            .lines()
            .any(|l| l.trim_start().starts_with("mcp ")),
        "{}",
        help.stdout
    );

    let claude = repo_file("CLAUDE.md");
    assert!(claude.contains("src/mcp.rs"));
    assert!(claude.contains("tests/mcp.rs"));
}

// ---------------------------------------------------------------- job 020

/// A server whose `\Drafts` mailbox `Ghost` does not exist, so `APPEND`
/// gets `NO [TRYCREATE]`.
fn ghost_server() -> FakeImap {
    FakeImap::start_with_list(Mode::Implicit, &["UIDPLUS"], &["Ghost"], Some("Ghost"))
}

#[test]
fn job020_ac4_draft_refused_matches_the_cli() {
    let c = compare(
        ghost_server,
        with_w,
        &[
            "draft",
            "--to",
            "carol@example.com",
            "--subject",
            "Status",
            "--body",
            "Hi",
        ],
        "draft",
        json!({"to": ["carol@example.com"], "subject": "Status", "body": "Hi"}),
    );
    let error = c.assert_same_error();
    assert_eq!(error["code"], json!("draft_refused"));
    assert_eq!(error["mailbox"], json!("Ghost"));
    c.assert_same_audit(1);
    assert!(c.cli.server.appended().is_empty());
    assert!(c.mcp.server.appended().is_empty());
}

const NO_TOKEN: &str = "no agent token presented; set AGENT_MAIL_TOKEN";

/// The `missing_token` error a tool call without a token returns.
fn missing_token_message(side: &Side, tool: &str, arguments: Value) -> (Value, String) {
    let result = side.tool(tool, arguments.clone(), None);
    assert_eq!(result["isError"], json!(true), "{tool} {arguments}");
    let error = result["structuredContent"]["error"].clone();
    let mut keys: Vec<&String> = error.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["code", "message"], "{tool} {arguments}");
    assert_eq!(error["code"], json!("missing_token"), "{tool} {arguments}");
    let message = error["message"].as_str().unwrap().to_string();
    (result, message)
}

#[test]
fn job020_ac11_missing_token_also_reports_argument_mistakes() {
    let side = Side::new();
    let (_, message) = missing_token_message(&side, "read", json!({}));
    assert!(message.starts_with(NO_TOKEN), "{message}");

    // The message the same call gets as `invalid_arguments` with a token.
    let with_token = Side::new();
    let tw = with_token.w();
    let result = with_token.tool("read", json!({}), Some(&tw));
    assert_eq!(
        result["structuredContent"]["error"]["code"],
        json!("invalid_arguments")
    );
    let invalid = result["structuredContent"]["error"]["message"]
        .as_str()
        .unwrap();
    assert_eq!(invalid, "missing argument `ref`: it is required");
    assert!(message.contains(invalid), "{message}");
    assert!(message.chars().count() <= 300, "{message}");

    assert!(side.audit().is_empty());
    assert_eq!(side.server.connections(), 0);
}

#[test]
fn job020_ac12_missing_token_alone_keeps_its_message() {
    let side = Side::new();
    let (_, message) = missing_token_message(&side, "read", json!({ "ref": inbox_ref() }));
    assert_eq!(message, NO_TOKEN);
    assert_eq!(side.server.connections(), 0);
}

#[test]
fn job020_ac13_token_like_arguments_are_named_never_echoed() {
    let side = Side::new();
    let token = side.w();
    for (tool, key) in [("list", "token"), ("draft", "AGENT_MAIL_TOKEN")] {
        let (result, message) = missing_token_message(&side, tool, json!({ key: token }));
        assert!(message.starts_with(NO_TOKEN), "{message}");
        assert!(message.contains(&format!("`{key}`")), "{message}");
        for text in texts(&result) {
            assert!(!text.contains(&token), "{tool}: the token is in the text");
        }
        assert!(
            !result["structuredContent"].to_string().contains(&token),
            "{tool}: the token is in structuredContent"
        );
    }
    assert!(side.audit().is_empty());
    assert_eq!(side.server.connections(), 0);
}

/// One `tools/call` with `extra` variables in the server's environment;
/// returns its JSON-RPC `result`.
fn tool_with(
    side: &Side,
    name: &str,
    arguments: Value,
    tokens: &str,
    extra: &[(&str, &str)],
) -> Value {
    let mut lines = mcp_opening();
    lines.push(mcp_call(2, name, arguments));
    let out = mcp_lines_with(
        &lines,
        Some(tokens),
        extra,
        &side.store,
        &side.server.ca,
        &side.log,
    );
    assert_eq!(out.len(), 2, "{out:?}");
    assert_eq!(out[1]["id"], json!(2));
    out[1]["result"].clone()
}

#[test]
fn ac8_proxy_auth_failed_matches_the_cli() {
    let side = Side::new();
    let tw = side.w();
    let proxy = proxy::FakeProxy::start(proxy::Protocol::Connect);
    proxy.require_credentials("proxyuser", "proxy-secret");
    let url = proxy.url("http");
    let extra = [("HTTPS_PROXY", url.as_str())];

    let cli = run_logged_with(
        &["list"],
        Some(&tw),
        &extra,
        &side.store,
        &side.server.ca,
        &side.log,
    );
    assert_eq!(cli.code, 1, "{cli:?}");
    let cli_error: Value = serde_json::from_str(cli.stderr.trim_end()).unwrap();
    assert_eq!(cli_error["error"]["code"], "proxy_auth_failed");

    let result = tool_with(&side, "list", json!({}), &tw, &extra);
    assert_eq!(result["isError"], json!(true), "{result}");
    let texts = texts(&result);
    let tool_error: Value = serde_json::from_str(texts.last().unwrap()).unwrap();
    assert_eq!(tool_error, cli_error);
    assert_eq!(result["structuredContent"], cli_error);
    assert_eq!(side.server.connections(), 0);
}

#[test]
fn ac13_whoami_tool_reports_the_proxy_like_the_cli() {
    let side = Side::new();
    let tw = side.w();
    let usable = "http://proxyuser:proxy-secret@127.0.0.1:9";
    let invalid = "ftp://proxyuser:proxy-secret@127.0.0.1:1";
    for (var, value, expected) in [
        (
            "HTTPS_PROXY",
            usable,
            json!({"variable": "HTTPS_PROXY", "usable": true}),
        ),
        (
            "ALL_PROXY",
            invalid,
            json!({"variable": "ALL_PROXY", "usable": false}),
        ),
    ] {
        let extra = [(var, value)];
        let cli = run_logged_with(
            &["whoami"],
            Some(&tw),
            &extra,
            &side.store,
            &side.server.ca,
            &side.log,
        );
        assert_eq!(cli.code, 0, "{cli:?}");
        let result = tool_with(&side, "whoami", json!({}), &tw, &extra);
        assert_eq!(result["isError"], json!(false), "{result}");
        let texts = texts(&result);
        assert_eq!(texts[0], cli.stdout.trim_end());
        let parsed: Value = serde_json::from_str(&texts[0]).unwrap();
        assert_eq!(parsed["proxy"], expected);
        assert_eq!(result["structuredContent"], parsed);
        for secret in ["127.0.0.1", "proxyuser", "proxy-secret", "ftp://"] {
            assert!(!texts[0].contains(secret), "{secret}");
        }
    }
    assert_eq!(side.server.connections(), 0);
}

// ------------------------------------------------- job 022: token cap, `to`

#[test]
fn job022_ac4_too_many_tokens_matches_the_cli() {
    let nine = |side: &Side| {
        Some(
            (1..=9)
                .map(|i| {
                    side.add(
                        &format!("cccc000{i}"),
                        &[LOGIN],
                        vec![grant(Operation::List, &["INBOX"])],
                    )
                })
                .collect::<Vec<_>>()
                .join(","),
        )
    };
    let c = compare(default_server, nine, &["list"], "list", json!({}));
    let error = c.assert_same_error();
    assert_eq!(error["code"], json!("too_many_tokens"));
    assert_eq!(
        c.result["structuredContent"]["error"]["code"],
        json!("too_many_tokens")
    );
    c.assert_same_audit(1);
    assert_eq!(c.cli.server.connections(), 0);
    assert_eq!(c.mcp.server.connections(), 0);
}

#[test]
fn job022_ac5_draft_rejects_an_invalid_recipient_without_audit_or_echo() {
    let bad = "not-an-address-zq7";
    let side = Side::new();
    let tw = side.add("aaaa1111", &[LOGIN], vec![grant(Operation::Draft, &["*"])]);
    let result = side.tool(
        "draft",
        json!({"to": ["carol@example.com", bad], "subject": "S", "body": "B"}),
        Some(&tw),
    );
    assert_eq!(result["isError"], json!(true), "{result}");
    let error = &result["structuredContent"]["error"];
    assert_eq!(error["code"], json!("invalid_arguments"));
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("`to`"), "{message}");
    assert!(!result.to_string().contains(bad), "{result}");
    assert!(side.audit().is_empty(), "{:?}", side.audit());
    assert_eq!(side.server.connections(), 0);

    let side = Side::new();
    let tw = side.add("aaaa1111", &[LOGIN], vec![grant(Operation::Draft, &["*"])]);
    let out = side.cli(
        &[
            "draft",
            "--to",
            "carol@example.com",
            "--to",
            bad,
            "--subject",
            "S",
            "--body",
            "B",
        ],
        Some(&tw),
    );
    assert_eq!(out.code, 2, "{out:?}");
    assert_eq!(error_of(&out)["code"], json!("invalid_arguments"));
    assert!(!out.stderr.contains(bad), "{out:?}");
    assert!(side.audit().is_empty(), "{:?}", side.audit());
    assert_eq!(side.server.connections(), 0);
}
