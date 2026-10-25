//! The Broker clients (job 026): the CLI, the MCP server and `mandate
//! create` sending their work to the Broker. On every OS they run against
//! `serve_session` through the in-memory fake pipe in `tests/support/broker.rs`
//! and are compared with the in-process run; on Windows the built binary
//! and the real pipe are checked too. All values here are fake.

mod support;

use std::collections::HashMap;
use std::sync::Arc;

use agent_mail::broker::{self, HIGH_INTEGRITY};
use agent_mail::create::{GENERIC, NOTHING_STORED, TLS_IMPLICIT};
use agent_mail::export::MandateFile;
use agent_mail::mandate::{Mandate, Operation};
use agent_mail::seal::{self, AgentToken};
use agent_mail::store::{InMemoryStore, MandateStore};
use agent_mail::terminal::{Answer, Kind, ScriptedTerminal};
use agent_mail::{Context, RunOutput};
use serde_json::{Value, json};
use support::broker::{BrokerSetup, FakeBroker, caller, read_request, write_reply};
use support::{
    ARCHIVE_UIDVALIDITY, BROKER_CREDENTIAL, CountingStore, FakeImap, INBOX_UIDVALIDITY, LOGIN,
    Mode, grant, mandate_with_id, mcp_call, mcp_opening, mcp_parse, mcp_raw, tokens_in,
};

/// The caller SID the fake Broker sees.
const SID: &str = "S-1-5-21-1-2-3-1001";
/// `SECURITY_MANDATORY_MEDIUM_RID`.
const MEDIUM: u32 = 0x2000;
/// The virtual host and port of Mandate M's Account, routed to a fake
/// server, so every run shows the same Account.
const HOST: &str = "localhost";
const PORT: u16 = 993;

/// Mandate M: every operation the comparisons use, `read` on INBOX only.
fn mandate_m() -> Mandate {
    mandate_with_id(
        "cccc3333",
        "compare",
        &[LOGIN],
        PORT,
        vec![
            grant(Operation::List, &["INBOX", "Archive"]),
            grant(Operation::Search, &["INBOX"]),
            grant(Operation::Read, &["INBOX"]),
            grant(Operation::Flag, &["INBOX"]),
            grant(Operation::Move, &["INBOX", "Archive"]),
            grant(Operation::Draft, &[]),
        ],
    )
}

/// A fresh fake server offering MOVE, UIDPLUS and a Drafts mailbox.
fn server() -> FakeImap {
    FakeImap::start_with_list(
        Mode::Implicit,
        &["MOVE", "UIDPLUS"],
        &["Drafts"],
        Some("Drafts"),
    )
}

fn env_with(tokens: Option<&str>) -> HashMap<String, String> {
    let mut env = HashMap::new();
    if let Some(tokens) = tokens {
        env.insert(agent_mail::TOKEN_VAR.to_string(), tokens.to_string());
    }
    env
}

fn argv<'a>(args: &'a [&'a str]) -> impl Iterator<Item = &'a str> {
    std::iter::once("agent-mail").chain(args.iter().copied())
}

/// `args` in-process, against a store holding M without an owner and a
/// fresh fake server.
fn in_process(args: &[&str], tokens: Option<&str>, token: &AgentToken) -> RunOutput {
    let server = server();
    let store = InMemoryStore::new();
    let record = seal::seal(token, &mandate_m());
    store.put(&record.key, &record.value).unwrap();
    let context = Context::trusting_only(&store, vec![server.ca.clone()]).with_route(
        HOST,
        PORT,
        "localhost",
        server.port,
    );
    agent_mail::run(argv(args), &env_with(tokens), &context)
}

/// A fake Broker whose store holds M owned by [`SID`], with a fresh fake
/// server, and its store.
fn broker_with_m(token: &AgentToken) -> (FakeBroker, Arc<InMemoryStore>, FakeImap) {
    let server = server();
    let store = Arc::new(InMemoryStore::new());
    let record = seal::seal_for(token, &mandate_m(), Some(SID));
    store.put(&record.key, &record.value).unwrap();
    let setup = BrokerSetup::new(store.clone(), caller(SID, MEDIUM)).route(
        &server.ca,
        HOST,
        PORT,
        server.port,
    );
    (FakeBroker::serving(setup), store, server)
}

/// `args` on the Broker route, with `local` as the client's own store.
fn via_broker(
    args: &[&str],
    tokens: Option<&str>,
    token: &AgentToken,
    local: &dyn MandateStore,
) -> RunOutput {
    let (fake, _store, _server) = broker_with_m(token);
    let context = Context::trusting_only(local, Vec::new()).with_broker(&fake);
    agent_mail::run(argv(args), &env_with(tokens), &context)
}

fn r(mailbox: &str, uidvalidity: u32, uid: u32) -> String {
    format!("{LOGIN}/{mailbox}/{uidvalidity}/{uid}")
}

// ---------------------------------------------------------------- AC1

#[test]
fn ac1_broker_route_prints_exactly_what_the_in_process_run_prints() {
    let token = AgentToken::mint();
    let valid = token.expose().to_string();
    let inbox_2 = r("INBOX", INBOX_UIDVALIDITY, 2);
    let denied_ref = r("Archive", ARCHIVE_UIDVALIDITY, 1);
    let local = CountingStore::default();
    let with_garbage = format!("{valid},garbage");
    let cases: Vec<(Vec<&str>, Option<&str>, i32)> = vec![
        (vec!["list"], Some(&valid), 0),
        (vec!["search"], Some(&valid), 0),
        (vec!["read", &inbox_2], Some(&valid), 0),
        (vec!["flag", &inbox_2, "--set", "seen"], Some(&valid), 0),
        (vec!["move", &inbox_2, "--to", "Archive"], Some(&valid), 0),
        (
            vec![
                "draft",
                "--to",
                "a@example.com",
                "--subject",
                "Hi",
                "--body",
                "Hello",
            ],
            Some(&valid),
            0,
        ),
        (vec!["whoami"], Some(&valid), 0),
        (vec!["mandate", "list"], None, 0),
        (vec!["mandate", "delete", "cccc3333"], None, 0),
        // A denied call: no `read` Grant on Archive.
        (vec!["read", &denied_ref], Some(&valid), 1),
        // The invalid-token warning, in the same position.
        (vec!["list"], Some(&with_garbage), 0),
        // No token at all.
        (vec!["list"], None, 1),
    ];
    for (args, tokens, code) in cases {
        for text in [false, true] {
            let mut full: Vec<&str> = Vec::new();
            if text {
                full.push("--text");
            }
            full.extend(&args);
            let expected = in_process(&full, tokens, &token);
            assert_eq!(expected.code, code, "{full:?}: {expected:?}");
            let got = via_broker(&full, tokens, &token, &local);
            assert_eq!(got.stdout, expected.stdout, "{full:?} stdout");
            assert_eq!(got.stderr, expected.stderr, "{full:?} stderr");
            assert_eq!(got.code, expected.code, "{full:?} code");
        }
    }
    // The cases above that must show their distinguishing output.
    let denied = in_process(&["read", &denied_ref], Some(&valid), &token);
    assert!(denied.stderr.contains("\"code\":\"denied\""), "{denied:?}");
    let warned = in_process(&["list"], Some(&with_garbage), &token);
    assert!(warned.stderr.contains("\"warning\""), "{warned:?}");
    let missing = in_process(&["list"], None, &token);
    assert!(missing.stderr.contains("missing_token"), "{missing:?}");
    // The client never touched its own store.
    assert_eq!(local.accesses(), 0);
}

#[test]
fn ac1_each_command_sends_hello_then_one_run_with_the_token_in_it() {
    let token = AgentToken::mint();
    let (fake, _store, _server) = broker_with_m(&token);
    let local = InMemoryStore::new();
    let context = Context::trusting_only(&local, Vec::new()).with_broker(&fake);
    let env = env_with(Some(token.expose()));
    let out = agent_mail::run(argv(&["--text", "list"]), &env, &context);
    assert_eq!(out.code, 0, "{out:?}");
    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0][0],
        json!({"hello": {"version": broker::VERSION}})
    );
    let run = &requests[0][1]["run"];
    assert_eq!(run["command"], json!({"list": {"account": null}}));
    assert_eq!(run["tokens"], json!(token.expose()));
    assert_eq!(run["text"], json!(true));
    assert_eq!(requests[0].len(), 2);
}

// ---------------------------------------------------------------- AC2

/// One `tools/call` with `arguments`, as the MCP server answers it on the
/// in-process route and on the Broker route.
fn mcp_both(tool: &str, arguments: Value) -> (Value, Value) {
    let token = AgentToken::mint();
    let env = env_with(Some(token.expose()));
    let mut lines = mcp_opening();
    lines.push(mcp_call(2, tool, arguments));

    let server = server();
    let store = InMemoryStore::new();
    let record = seal::seal(&token, &mandate_m());
    store.put(&record.key, &record.value).unwrap();
    let context = Context::trusting_only(&store, vec![server.ca.clone()]).with_route(
        HOST,
        PORT,
        "localhost",
        server.port,
    );
    let expected = mcp_parse(&mcp_raw(&lines, &env, &context)).remove(1);

    let (fake, _store, _server) = broker_with_m(&token);
    let local = CountingStore::default();
    let context = Context::trusting_only(&local, Vec::new()).with_broker(&fake);
    let got = mcp_parse(&mcp_raw(&lines, &env, &context)).remove(1);
    assert_eq!(local.accesses(), 0);
    assert_eq!(fake.connections(), 1);
    (expected, got)
}

#[test]
fn ac2_every_tool_answers_the_same_on_the_broker_route() {
    let inbox_2 = r("INBOX", INBOX_UIDVALIDITY, 2);
    let cases = [
        ("whoami", json!({})),
        ("list", json!({})),
        ("search", json!({})),
        ("read", json!({"ref": inbox_2})),
        ("flag", json!({"ref": inbox_2, "set": "seen"})),
        ("move", json!({"ref": inbox_2, "to": "Archive"})),
        (
            "draft",
            json!({"to": ["a@example.com"], "subject": "Hi", "body": "Hello"}),
        ),
        // Denied: no `read` Grant on Archive.
        ("read", json!({"ref": r("Archive", ARCHIVE_UIDVALIDITY, 1)})),
    ];
    for (tool, arguments) in cases {
        let (expected, got) = mcp_both(tool, arguments.clone());
        assert!(expected["result"]["content"].is_array(), "{expected}");
        assert_eq!(
            got["result"]["content"], expected["result"]["content"],
            "{tool} {arguments}"
        );
        assert_eq!(
            got["result"]["structuredContent"], expected["result"]["structuredContent"],
            "{tool} {arguments}"
        );
        assert_eq!(
            got["result"]["isError"], expected["result"]["isError"],
            "{tool} {arguments}"
        );
        assert_eq!(got, expected, "{tool} {arguments}");
    }
    let (denied, _) = mcp_both("read", json!({"ref": r("Archive", ARCHIVE_UIDVALIDITY, 1)}));
    assert_eq!(denied["result"]["isError"], json!(true));
    assert_eq!(
        denied["result"]["structuredContent"]["error"]["code"],
        json!("denied")
    );
}

#[test]
fn ac2_bad_arguments_fail_without_connecting() {
    let fake = FakeBroker::absent();
    let local = InMemoryStore::new();
    let context = Context::trusting_only(&local, Vec::new()).with_broker(&fake);
    let env = env_with(Some(AgentToken::mint().expose()));
    let mut lines = mcp_opening();
    lines.push(mcp_call(2, "read", json!({})));
    lines.push(mcp_call(3, "search", json!({"limit": 0})));
    let out = mcp_parse(&mcp_raw(&lines, &env, &context));
    for response in &out[1..] {
        assert_eq!(
            response["result"]["structuredContent"]["error"]["code"],
            json!("invalid_arguments"),
            "{response}"
        );
    }
    assert_eq!(fake.connections(), 0);
}

#[test]
fn ac2_each_tool_call_opens_its_own_connection_starting_with_hello() {
    let token = AgentToken::mint();
    let (fake, _store, _server) = broker_with_m(&token);
    let local = InMemoryStore::new();
    let context = Context::trusting_only(&local, Vec::new()).with_broker(&fake);
    let env = env_with(Some(token.expose()));

    let mut lines = mcp_opening();
    lines.push(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}).to_string());
    let out = mcp_parse(&mcp_raw(&lines, &env, &context));
    assert_eq!(out.len(), 2);
    assert_eq!(fake.connections(), 0, "initialize and tools/list connect");

    let mut lines = mcp_opening();
    lines.push(mcp_call(2, "whoami", json!({})));
    lines.push(mcp_call(3, "list", json!({})));
    let out = mcp_parse(&mcp_raw(&lines, &env, &context));
    assert_eq!(out[1]["result"]["isError"], json!(false), "{}", out[1]);
    assert_eq!(out[2]["result"]["isError"], json!(false), "{}", out[2]);
    assert_eq!(fake.connections(), 2);
    for requests in fake.requests() {
        assert_eq!(requests[0], json!({"hello": {"version": broker::VERSION}}));
        assert_eq!(requests.len(), 2, "{requests:?}");
    }
}

// ---------------------------------------------------------------- AC3

const MANDATE_NAME: &str = "broker-made";

/// The guided answers for one generic Account on `port` with Credential
/// [`BROKER_CREDENTIAL`] and a `read` Grant on INBOX, up to the "Seal and
/// store" confirmation.
fn create_script(port: u16) -> Vec<Answer> {
    vec![
        Answer::text(MANDATE_NAME),
        Answer::choose(GENERIC),
        Answer::text(HOST),
        Answer::text(&port.to_string()),
        Answer::choose(TLS_IMPLICIT),
        Answer::text(LOGIN),
        Answer::hidden(BROKER_CREDENTIAL),
        Answer::tick(&["INBOX"]),
        Answer::tick(&["read"]),
        Answer::Confirm(false),
        Answer::Confirm(true),
    ]
}

/// A fake Broker with an empty store, seeing an elevated caller, trusting
/// `server`.
fn create_broker(server: &FakeImap) -> (FakeBroker, Arc<InMemoryStore>) {
    let store = Arc::new(InMemoryStore::new());
    let setup = BrokerSetup::new(store.clone(), caller(SID, HIGH_INTEGRITY)).route(
        &server.ca,
        HOST,
        server.port,
        server.port,
    );
    (FakeBroker::serving(setup), store)
}

/// Runs `mandate create <extra>` on the Broker route through `terminal`,
/// with the elevation status `elevated` and `env`.
fn create_via(
    fake: &FakeBroker,
    server: Option<&FakeImap>,
    terminal: &ScriptedTerminal,
    local: &dyn MandateStore,
    elevated: bool,
    extra: &[&str],
    env: &HashMap<String, String>,
) -> RunOutput {
    let roots = server.map(|s| vec![s.ca.clone()]).unwrap_or_default();
    let context = Context::trusting_only(local, roots)
        .with_terminal(terminal)
        .with_broker(fake)
        .with_elevation(elevated);
    let mut args = vec!["mandate", "create"];
    args.extend(extra);
    agent_mail::run(argv(&args), env, &context)
}

/// Every frame but `create`, as text, and the raw bytes of all frames.
fn non_create_frames(fake: &FakeBroker) -> Vec<String> {
    fake.requests()
        .into_iter()
        .flatten()
        .filter(|frame| frame.get("create").is_none())
        .map(|frame| frame.to_string())
        .collect()
}

fn temp_path(name: &str) -> std::path::PathBuf {
    let log = support::temp_audit_log();
    let dir = log.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn ac3_not_elevated_refuses_before_any_prompt_connection_or_file_read() {
    let missing = temp_path("missing.toml");
    let missing = missing.to_str().unwrap();
    for extra in [&[][..], &["--from", missing][..]] {
        let fake = FakeBroker::absent();
        let terminal = ScriptedTerminal::new(Vec::new());
        let local = CountingStore::default();
        let out = create_via(
            &fake,
            None,
            &terminal,
            &local,
            false,
            extra,
            &HashMap::new(),
        );
        assert_eq!(out.code, 1, "{out:?}");
        assert_eq!(out.stdout, "");
        let error: Value = serde_json::from_str(&out.stderr).unwrap();
        assert_eq!(error["error"]["code"], "elevation_required", "{out:?}");
        let message = error["error"]["message"].as_str().unwrap();
        assert!(message.contains("agent-mail mandate create"), "{message}");
        assert!(
            message.contains("elevated (Run as administrator) terminal"),
            "{message}"
        );
        assert!(message.chars().count() <= 300, "{message}");
        assert!(terminal.transcript().is_empty());
        assert_eq!(fake.connections(), 0);
        assert_eq!(local.accesses(), 0);
    }
}

#[test]
fn ac3_the_terminal_and_sandbox_refusals_still_come_first() {
    let fake = FakeBroker::absent();
    let local = InMemoryStore::new();
    let terminal = ScriptedTerminal::non_interactive();
    let out = create_via(&fake, None, &terminal, &local, false, &[], &HashMap::new());
    assert_eq!(out.code, 1);
    assert!(
        out.stderr.contains("must be run by a human in a terminal"),
        "{out:?}"
    );

    let terminal = ScriptedTerminal::new(Vec::new());
    let env = HashMap::from([("CLAUDECODE".to_string(), "1".to_string())]);
    let out = create_via(&fake, None, &terminal, &local, false, &[], &env);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("\"sandbox_detected\""), "{out:?}");
    assert!(terminal.transcript().is_empty());
    assert_eq!(fake.connections(), 0);
}

#[test]
fn ac3_elevated_create_stores_an_owned_mandate_and_reveals_its_token() {
    let server = FakeImap::start(Mode::Implicit);
    let (fake, store) = create_broker(&server);
    let export = temp_path("exported.toml");
    let mut script = create_script(server.port);
    script.push(Answer::Confirm(true));
    script.push(Answer::text(export.to_str().unwrap()));
    let terminal = ScriptedTerminal::new(script);
    let local = CountingStore::default();
    let out = create_via(
        &fake,
        Some(&server),
        &terminal,
        &local,
        true,
        &[],
        &HashMap::new(),
    );
    assert_eq!(
        out,
        RunOutput {
            stdout: String::new(),
            stderr: String::new(),
            code: 0
        }
    );
    assert_eq!(terminal.remaining(), 0);
    assert_eq!(local.accesses(), 0);

    // Exactly one `create`, matching the answers.
    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].len(), 2, "{:?}", requests[0]);
    let create = &requests[0][1]["create"];
    assert_eq!(create["name"], MANDATE_NAME);
    let accounts = create["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0]["login"], LOGIN);
    assert_eq!(accounts[0]["host"], HOST);
    assert_eq!(accounts[0]["port"], server.port);
    assert_eq!(accounts[0]["credential"], BROKER_CREDENTIAL);
    assert_eq!(
        create["grants"],
        json!([{"operation": "read", "account": LOGIN, "mailboxes": ["INBOX"]}])
    );

    // The Broker stored one Mandate owned by the caller, opened by the
    // token the terminal revealed.
    let entries = store.entries();
    assert_eq!(entries.len(), 1);
    let reveals: Vec<String> = terminal
        .transcript()
        .into_iter()
        .filter(|e| e.kind == Kind::Reveal)
        .map(|e| e.text)
        .collect();
    assert_eq!(reveals.len(), 1);
    let tokens = tokens_in(&reveals[0]);
    assert_eq!(tokens.len(), 1);
    let token = AgentToken::parse(&tokens[0]).unwrap();
    let (mandate, owner) = seal::open_with_owner(&token, &entries[0].1).unwrap();
    assert_eq!(owner.as_deref(), Some(SID));
    assert_eq!(mandate.name, MANDATE_NAME);
    assert!(reveals[0].contains(&format!("(ID {})", mandate.id)));

    // The export: Accounts and Grants, no Credential, no token.
    let exported = std::fs::read_to_string(&export).unwrap();
    assert!(exported.contains(LOGIN), "{exported}");
    assert!(!exported.contains(BROKER_CREDENTIAL));
    assert!(!exported.contains(token.expose()));

    // The secrets reach no output and no other request.
    for stream in [&out.stdout, &out.stderr] {
        assert!(!stream.contains(BROKER_CREDENTIAL));
        assert!(!stream.contains(token.expose()));
    }
    for frame in non_create_frames(&fake) {
        assert!(!frame.contains(BROKER_CREDENTIAL), "{frame}");
        assert!(!frame.contains(token.expose()), "{frame}");
    }
    assert!(!terminal.transcript_text().contains(BROKER_CREDENTIAL));
}

#[test]
fn ac3_from_file_prefills_the_flow_exactly_as_in_process() {
    let server = FakeImap::start(Mode::Implicit);
    let file = temp_path("from.toml");
    let saved = mandate_with_id(
        "dddd4444",
        MANDATE_NAME,
        &[LOGIN],
        server.port,
        vec![grant(Operation::Read, &["INBOX"])],
    );
    std::fs::write(&file, MandateFile::of(&saved).to_toml()).unwrap();
    let path = file.to_str().unwrap();
    // Keep every pre-filled value; the Credential is still typed.
    let script = || {
        vec![
            Answer::Accept,
            Answer::Confirm(true),
            Answer::Accept,
            Answer::Accept,
            Answer::Accept,
            Answer::Accept,
            Answer::hidden(BROKER_CREDENTIAL),
            Answer::Accept,
            Answer::Accept,
            Answer::Confirm(false),
            Answer::Confirm(true),
            Answer::Confirm(false),
        ]
    };

    let in_terminal = ScriptedTerminal::new(script());
    let in_store = InMemoryStore::new();
    let context =
        Context::trusting_only(&in_store, vec![server.ca.clone()]).with_terminal(&in_terminal);
    let expected = agent_mail::run(
        argv(&["mandate", "create", "--from", path]),
        &HashMap::new(),
        &context,
    );
    assert_eq!(expected.code, 0, "{expected:?}");

    let (fake, store) = create_broker(&server);
    let terminal = ScriptedTerminal::new(script());
    let local = CountingStore::default();
    let out = create_via(
        &fake,
        Some(&server),
        &terminal,
        &local,
        true,
        &["--from", path],
        &HashMap::new(),
    );
    assert_eq!(out, expected);
    let without_reveal = |t: &ScriptedTerminal| -> Vec<_> {
        t.transcript()
            .into_iter()
            .filter(|e| e.kind != Kind::Reveal)
            .collect()
    };
    assert_eq!(without_reveal(&terminal), without_reveal(&in_terminal));
    assert!(
        terminal.transcript().iter().any(|e| e.kind == Kind::Hidden),
        "the Credential was typed"
    );
    let create = &fake.requests()[0][1]["create"];
    assert_eq!(create["accounts"][0]["credential"], BROKER_CREDENTIAL);
    assert_eq!(
        create["grants"],
        json!([{"operation": "read", "account": LOGIN, "mailboxes": ["INBOX"]}])
    );
    assert_eq!(store.entries().len(), 1);
    assert_eq!(local.accesses(), 0);
}

#[test]
fn ac3_a_create_error_from_the_broker_is_printed_and_nothing_is_stored() {
    let server = FakeImap::start(Mode::Implicit);
    // The Broker's own live login fails.
    let fake = FakeBroker::scripted(|end| {
        let _hello = read_request(end);
        write_reply(end, &json!({"ok": {"version": broker::VERSION}}));
        let _create = read_request(end);
        write_reply(
            end,
            &json!({"error": {"code": "login_failed", "account": LOGIN,
                "message": "the server refused the login; ask the human to check the credential"}}),
        );
    });
    let terminal = ScriptedTerminal::new(create_script(server.port));
    let local = CountingStore::default();
    let out = create_via(
        &fake,
        Some(&server),
        &terminal,
        &local,
        true,
        &[],
        &HashMap::new(),
    );
    assert_eq!(out.code, 1, "{out:?}");
    assert_eq!(out.stdout, "");
    let error: Value = serde_json::from_str(&out.stderr).unwrap();
    assert_eq!(error["error"]["code"], "login_failed");
    let messages: Vec<String> = terminal
        .transcript()
        .into_iter()
        .filter(|e| e.kind == Kind::Message)
        .map(|e| e.text)
        .collect();
    assert_eq!(messages.last().map(String::as_str), Some(NOTHING_STORED));
    assert!(!terminal.transcript().iter().any(|e| e.kind == Kind::Reveal));
    assert!(!out.stderr.contains(BROKER_CREDENTIAL));
    assert_eq!(local.accesses(), 0);
}

#[test]
fn ac3_a_failed_reveal_deletes_the_new_mandate_through_the_broker() {
    let server = FakeImap::start(Mode::Implicit);
    let (fake, store) = create_broker(&server);
    let terminal = ScriptedTerminal::failing_reveal(create_script(server.port));
    let local = CountingStore::default();
    let out = create_via(
        &fake,
        Some(&server),
        &terminal,
        &local,
        true,
        &[],
        &HashMap::new(),
    );
    assert_eq!(out.code, 1, "{out:?}");
    let error: Value = serde_json::from_str(&out.stderr).unwrap();
    assert_eq!(error["error"]["code"], "aborted");
    assert!(
        terminal
            .transcript_text()
            .contains("could not be shown on the terminal. Nothing was stored."),
        "{}",
        terminal.transcript_text()
    );
    assert!(store.entries().is_empty(), "the Broker's store is empty");
    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let frames = &requests[0];
    assert_eq!(frames.len(), 3, "{frames:?}");
    let id = frames[2]["run"]["command"]["mandate_delete"]["id"]
        .as_str()
        .expect("a mandate_delete run");
    assert_eq!(id.len(), 8);
    for frame in non_create_frames(&fake) {
        assert!(!frame.contains(BROKER_CREDENTIAL), "{frame}");
    }
    assert!(tokens_in(&out.stderr).is_empty());
    assert_eq!(local.accesses(), 0);
}

// ---------------------------------------------------------------- AC4

fn assert_broker_unavailable(out: &RunOutput) {
    assert_eq!(out.code, 1, "{out:?}");
    assert_eq!(out.stdout, "");
    let error: Value = serde_json::from_str(&out.stderr).unwrap();
    assert_eq!(error["error"]["code"], "broker_unavailable", "{out:?}");
}

fn run_with(fake: &FakeBroker, args: &[&str], tokens: Option<&str>) -> RunOutput {
    let local = InMemoryStore::new();
    let context = Context::trusting_only(&local, Vec::new()).with_broker(fake);
    agent_mail::run(argv(args), &env_with(tokens), &context)
}

#[test]
fn ac4_no_pipe_gives_broker_unavailable_naming_the_fix() {
    let fake = FakeBroker::absent();
    let token = AgentToken::mint();
    let out = run_with(&fake, &["list"], Some(token.expose()));
    assert_broker_unavailable(&out);
    let error: Value = serde_json::from_str(&out.stderr).unwrap();
    let message = error["error"]["message"].as_str().unwrap();
    assert!(message.contains("sc.exe start agent-mail"), "{message}");
    assert!(message.contains("install.ps1"), "{message}");
    assert!(message.chars().count() <= 300, "{message}");
    assert!(!out.stderr.contains(token.expose()));

    let text = run_with(&fake, &["--text", "list"], Some(token.expose()));
    assert_eq!(text.code, 1);
    assert_eq!(text.stdout, "");
    assert_eq!(
        text.stderr,
        format!("error: broker_unavailable: {message}\n")
    );
}

#[test]
fn ac4_an_early_close_or_unreadable_reply_gives_broker_unavailable() {
    let closes_after_hello = FakeBroker::scripted(|end| {
        let _ = read_request(end);
    });
    let closes_after_request = FakeBroker::scripted(|end| {
        let _ = read_request(end);
        write_reply(end, &json!({"ok": {"version": broker::VERSION}}));
        let _ = read_request(end);
    });
    let garbage = FakeBroker::scripted(|end| {
        let _ = read_request(end);
        write_reply(end, &json!({"ok": {"version": broker::VERSION}}));
        let _ = read_request(end);
        let _ = broker::write_frame(end, b"not json");
    });
    let oversized = FakeBroker::scripted(|end| {
        use std::io::Write;
        let _ = read_request(end);
        let _ = end.write_all(&((broker::MAX_FRAME + 1) as u32).to_be_bytes());
    });
    let neither = FakeBroker::scripted(|end| {
        let _ = read_request(end);
        write_reply(end, &json!({"ok": {"version": broker::VERSION}}));
        let _ = read_request(end);
        write_reply(end, &json!({}));
    });
    let secret_proxy = "http://user:proxy-secret-9@proxy.example:8080";
    for fake in [
        closes_after_hello,
        closes_after_request,
        garbage,
        oversized,
        neither,
    ] {
        let token = AgentToken::mint();
        let local = InMemoryStore::new();
        let context = Context::trusting_only(&local, Vec::new()).with_broker(&fake);
        let mut env = env_with(Some(token.expose()));
        env.insert("HTTPS_PROXY".to_string(), secret_proxy.to_string());
        let out = agent_mail::run(argv(&["list"]), &env, &context);
        assert_broker_unavailable(&out);
        assert!(!out.stderr.contains(token.expose()));
        assert!(!out.stderr.contains("proxy-secret-9"));
    }
}

#[test]
fn ac4_a_version_mismatch_names_install_ps1_and_sends_nothing_more() {
    let error_reply = FakeBroker::scripted(|end| {
        let _ = read_request(end);
        write_reply(
            end,
            &json!({"error": {"code": "broker_version_mismatch", "message": "old wording"}}),
        );
    });
    let other_version = FakeBroker::scripted(|end| {
        let _ = read_request(end);
        write_reply(end, &json!({"ok": {"version": "0.0.0-other"}}));
        while read_request(end).is_some() {}
    });
    for fake in [&error_reply, &other_version] {
        let out = run_with(fake, &["list"], Some(AgentToken::mint().expose()));
        assert_eq!(out.code, 1, "{out:?}");
        assert_eq!(out.stdout, "");
        let error: Value = serde_json::from_str(&out.stderr).unwrap();
        assert_eq!(error["error"]["code"], "broker_version_mismatch");
        let message = error["error"]["message"].as_str().unwrap();
        assert!(message.contains("install.ps1"), "{message}");
        assert!(message.chars().count() <= 300, "{message}");
        let requests = fake.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].len(), 1, "only hello: {:?}", requests[0]);
    }
    let out = run_with(&other_version, &["list"], None);
    assert!(out.stderr.contains("0.0.0-other"), "{out:?}");
}

#[test]
fn ac4_the_same_codes_come_back_as_mcp_tool_errors() {
    let absent = FakeBroker::absent();
    let mismatch = FakeBroker::scripted(|end| {
        let _ = read_request(end);
        write_reply(end, &json!({"ok": {"version": "0.0.0-other"}}));
    });
    for (fake, code) in [
        (&absent, "broker_unavailable"),
        (&mismatch, "broker_version_mismatch"),
    ] {
        let local = InMemoryStore::new();
        let context = Context::trusting_only(&local, Vec::new()).with_broker(fake);
        let env = env_with(Some(AgentToken::mint().expose()));
        let mut lines = mcp_opening();
        lines.push(mcp_call(2, "list", json!({})));
        let out = mcp_parse(&mcp_raw(&lines, &env, &context));
        let result = &out[1]["result"];
        assert_eq!(result["isError"], json!(true), "{result}");
        assert_eq!(result["structuredContent"]["error"]["code"], json!(code));
        let text = result["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed, result["structuredContent"]);
    }
}

#[test]
fn ac4_the_broker_is_expected_to_run_as_its_service_account() {
    // `sc.exe showsid agent-mail` prints this SID.
    assert_eq!(
        broker::service_sid(broker::SERVICE_NAME),
        "S-1-5-80-1857854207-1894916148-3857740000-210916766-453871425"
    );
}

// ------------------------------------------------- Windows: pipe and binary

#[cfg(windows)]
mod windows {
    use std::process::Command;

    use agent_mail::audit::AuditLog;
    use agent_mail::broker::windows::{
        PipeConnector, connect, current_user_sid, pipe_owner, pipe_server_pid, process_user_sid,
        record_one_client, served_by,
    };
    use agent_mail::imap::Network;
    use support::broker::TestPipe;

    use super::*;

    /// A test pipe serving M owned by this user, with `server` behind
    /// [`HOST`]:[`PORT`].
    fn pipe_with_m(token: &AgentToken, server: &FakeImap) -> TestPipe {
        let store = Arc::new(InMemoryStore::new());
        let record = seal::seal_for(token, &mandate_m(), Some(&current_user_sid().unwrap()));
        store.put(&record.key, &record.value).unwrap();
        let network = Network::trusting_only(vec![server.ca.clone()]).with_route(
            HOST,
            PORT,
            "localhost",
            server.port,
        );
        TestPipe::start(store, AuditLog::temporary(), network)
    }

    fn binary(args: &[&str], token: &AgentToken, pipe: &str) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_agent-mail"))
            .args(args)
            .env(agent_mail::TOKEN_VAR, token.expose())
            .env(agent_mail::client::TEST_PIPE_VAR, pipe)
            .env_remove(agent_mail::audit::TEST_LOG_VAR)
            .output()
            .expect("failed to run agent-mail")
    }

    #[test]
    fn ac1_the_binary_forwards_list_to_the_pipe() {
        let token = AgentToken::mint();
        let server = server();
        let pipe = pipe_with_m(&token, &server);
        let expected = in_process(&["list"], Some(token.expose()), &token);
        assert_eq!(expected.code, 0, "{expected:?}");
        let out = binary(&["list"], &token, &pipe.name);
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected.stdout);
        assert_eq!(out.status.code(), Some(0));

        let out = binary(&["list"], &token, &TestPipe::unused_name());
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "");
        let error: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(error["error"]["code"], "broker_unavailable");
    }

    #[test]
    fn ac4_a_pipe_served_by_another_account_gets_nothing() {
        let name = TestPipe::unused_name();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let server = {
            let name = name.clone();
            std::thread::spawn(move || {
                record_one_client(&name, || ready_tx.send(()).unwrap()).unwrap()
            })
        };
        ready_rx.recv().unwrap();
        let expecting_service =
            PipeConnector::new(&name, &broker::service_sid(broker::SERVICE_NAME));
        assert!(matches!(
            expecting_service.open(),
            Err(agent_mail::core::CoreError::BrokerUnavailable)
        ));
        assert_eq!(server.join().unwrap(), Vec::<u8>::new(), "a frame was sent");

        // Through a context too: the command fails before sending anything.
        let name = TestPipe::unused_name();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let server = {
            let name = name.clone();
            std::thread::spawn(move || {
                record_one_client(&name, || ready_tx.send(()).unwrap()).unwrap()
            })
        };
        ready_rx.recv().unwrap();
        let connector = PipeConnector::new(&name, &broker::service_sid(broker::SERVICE_NAME));
        let local = InMemoryStore::new();
        let context = Context::trusting_only(&local, Vec::new()).with_broker(&connector);
        let token = AgentToken::mint();
        let out = agent_mail::run(argv(&["list"]), &env_with(Some(token.expose())), &context);
        assert_broker_unavailable(&out);
        assert_eq!(server.join().unwrap(), Vec::<u8>::new(), "a frame was sent");
    }

    #[test]
    fn ac4_a_client_expecting_this_user_connects_and_runs() {
        let token = AgentToken::mint();
        let server = server();
        let pipe = pipe_with_m(&token, &server);
        let connector = PipeConnector::new(&pipe.name, &current_user_sid().unwrap());
        let local = CountingStore::default();
        let context = Context::trusting_only(&local, Vec::new()).with_broker(&connector);
        let out = agent_mail::run(argv(&["whoami"]), &env_with(Some(token.expose())), &context);
        let expected = in_process(&["whoami"], Some(token.expose()), &token);
        assert_eq!(out, expected);
        assert_eq!(local.accesses(), 0);
    }

    #[test]
    fn ac4_the_pipe_owner_and_server_process_name_the_serving_account() {
        let token = AgentToken::mint();
        let server = server();
        let pipe = pipe_with_m(&token, &server);
        let me = current_user_sid().unwrap();
        // The pipe may be busy for a moment after `start`'s own probe.
        let client = (0..200)
            .find_map(|_| {
                connect(&pipe.name)
                    .inspect_err(|_| std::thread::sleep(std::time::Duration::from_millis(25)))
                    .ok()
            })
            .expect("the pipe accepts a client");
        assert_eq!(pipe_owner(&client).unwrap(), me);
        let pid = pipe_server_pid(&client).unwrap();
        assert_eq!(pid, std::process::id());
        assert_eq!(process_user_sid(pid).unwrap(), me);
        assert!(served_by(&client, &me));
        assert!(!served_by(
            &client,
            &broker::service_sid(broker::SERVICE_NAME)
        ));
    }
}
