//! `agent-mail draft` driven in-process against the fake IMAP server
//! (ticket 11).

mod support;

use std::collections::HashMap;
use std::path::PathBuf;

use agent_mail::RunOutput;
use agent_mail::core::{Command, CoreError, Outcome};
use agent_mail::mandate::{Grant, Mandate, Operation, Policy, TlsMode};
use agent_mail::seal::AgentToken;
use agent_mail::search::MessageRef;
use agent_mail::store::InMemoryStore;
use mail_parser::{HeaderValue, Message, MessageParser};
use serde_json::{Value, json};
use support::{
    CREDENTIAL, FakeImap, LOGIN, Mode, OTHER_LOGIN, account, assert_no_secrets, audit_lines,
    contains_bytes, error_of, grant, raw_header, run_logged, store_mandate, temp_audit_log,
    token_hash,
};

/// Every fixture mailbox, raw names.
const ALL_MAILBOXES: [&str; 10] = [
    "INBOX",
    "Archive",
    "Sent",
    "Spam",
    "[Gmail]/Entw&APw-rfe",
    "Read",
    "Triage",
    "Filing",
    "Thread",
    "Drafts",
];

/// Verbs no `draft` may ever send (AC7).
const FORBIDDEN: [&str; 10] = [
    "SELECT",
    "STORE",
    "UID STORE",
    "COPY",
    "UID COPY",
    "MOVE",
    "UID MOVE",
    "EXPUNGE",
    "UID EXPUNGE",
    "CLOSE",
];

type Snapshot = Vec<(String, Vec<(u32, String, Vec<String>)>)>;

/// A fake server, a store holding Mandate `aaaa1111` with `grants`, its
/// token and a fresh audit log. Dropping it asserts the server never saw a
/// state-changing verb (AC7), across every test in this file.
struct Setup {
    server: FakeImap,
    store: InMemoryStore,
    token: AgentToken,
    log: PathBuf,
}

impl Drop for Setup {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let commands = self.server.commands();
        for verb in FORBIDDEN {
            assert!(
                !commands.iter().any(|c| c == verb),
                "{verb} sent: {commands:?}"
            );
        }
    }
}

impl Setup {
    /// The default fake: `UIDPLUS`, `Drafts` listed and marked `\Drafts`.
    fn default_server() -> FakeImap {
        FakeImap::start_with_list(Mode::Implicit, &["UIDPLUS"], &["Drafts"], Some("Drafts"))
    }

    fn default_grants() -> Vec<Grant> {
        vec![
            grant(Operation::Draft, &[]),
            grant(Operation::Read, &["Thread", "Read"]),
            grant(Operation::List, &["*"]),
        ]
    }

    fn on(server: FakeImap, grants: Vec<Grant>) -> Self {
        let store = InMemoryStore::new();
        let mandate = Mandate {
            name: "work-inbox".to_string(),
            id: "aaaa1111".to_string(),
            created: 1_792_483_200,
            accounts: vec![account(LOGIN, server.port, TlsMode::Implicit)],
            policy: Policy { grants },
        };
        let token = store_mandate(&store, &mandate);
        Self {
            server,
            store,
            token,
            log: temp_audit_log(),
        }
    }

    fn new(grants: Vec<Grant>) -> Self {
        Self::on(Self::default_server(), grants)
    }

    fn granted() -> Self {
        Self::new(Self::default_grants())
    }

    /// Runs `agent-mail <args>` and checks no secret is printed.
    fn run(&self, args: &[&str]) -> RunOutput {
        let out = run_logged(
            args,
            Some(self.token.expose()),
            &self.store,
            &self.server.ca,
            &self.log,
        );
        assert_no_secrets(&out, Some(self.token.expose()));
        out
    }

    /// Runs `agent-mail <args>`, expects success and one line, returns it.
    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert_eq!(out.code, 0, "{args:?}: stderr {:?}", out.stderr);
        assert_eq!(out.stderr, "");
        assert!(out.stdout.ends_with('\n') && out.stdout.lines().count() == 1);
        out.stdout
    }

    /// Runs `agent-mail draft <args>`, expects success, returns the JSON.
    fn draft(&self, args: &[&str]) -> Value {
        let argv: Vec<&str> = std::iter::once("draft")
            .chain(args.iter().copied())
            .collect();
        let stdout = self.ok(&argv);
        serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout:?}"))
    }

    /// Runs `agent-mail <args>`, expects exit 1 and empty stdout, returns
    /// the parsed error.
    fn fails(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert_eq!(out.code, 1, "{args:?}: stdout {:?}", out.stdout);
        assert_eq!(out.stdout, "", "{args:?}");
        error_of(&out)
    }

    /// Runs `agent-mail <args>` and expects a usage error: exit 2, no
    /// connection, no audit entry.
    fn usage_error(&self, args: &[&str]) {
        let connections = self.server.connections();
        let lines = self.audit().len();
        let out = self.run(args);
        assert_eq!(out.code, 2, "{args:?}: {out:?}");
        assert_eq!(out.stdout, "", "{args:?}");
        assert_eq!(self.server.connections(), connections, "{args:?} connected");
        assert_eq!(self.audit().len(), lines, "{args:?} was audited");
    }

    /// Runs the core directly, as the MCP adapter will.
    fn core(&self, command: &Command) -> Result<Outcome, CoreError> {
        let mut env = HashMap::new();
        env.insert(
            agent_mail::TOKEN_VAR.to_string(),
            self.token.expose().to_string(),
        );
        let context = agent_mail::Context::trusting_only(&self.store, vec![self.server.ca.clone()])
            .with_audit_log(agent_mail::audit::AuditLog::at(&self.log));
        agent_mail::core::execute(&env, command, &context)
    }

    fn snapshot(&self) -> Snapshot {
        ALL_MAILBOXES
            .iter()
            .map(|m| (m.to_string(), self.server.messages_in(m)))
            .collect()
    }

    fn audit(&self) -> Vec<String> {
        audit_lines(&self.log)
    }

    fn audit_bytes(&self) -> Vec<u8> {
        std::fs::read(&self.log).unwrap_or_default()
    }

    /// The raw message of the last `APPEND`.
    fn last_appended(&self) -> String {
        self.server
            .appended()
            .last()
            .expect("a message was appended")
            .2
            .clone()
    }
}

/// `T(n)`: the reference of UID `n` in the fixture `Thread`.
fn t(uid: u32) -> String {
    format!("{LOGIN}/Thread/1700000009/{uid}")
}

/// `R(n)`: the reference of UID `n` in the fixture `Read`.
fn r(uid: u32) -> String {
    format!("{LOGIN}/Read/1700000006/{uid}")
}

/// The audit line with its `time` value replaced by `T`, after checking it
/// is RFC 3339 UTC to the second. Key order is kept.
fn with_t(line: &str) -> String {
    let rest = line
        .strip_prefix("{\"time\":\"")
        .unwrap_or_else(|| panic!("no leading time: {line}"));
    let (time, rest) = rest.split_once('"').unwrap();
    assert_eq!(time.len(), 20, "{time}");
    assert!(time.ends_with('Z') && time.as_bytes()[10] == b'T', "{time}");
    format!("{{\"time\":T{rest}")
}

fn parse(raw: &str) -> Message<'_> {
    MessageParser::default()
        .parse(raw.as_bytes())
        .expect("the appended message parses")
}

/// Every address of a header, in order.
fn addresses(message: &Message<'_>, to: bool) -> Vec<String> {
    let list = if to { message.to() } else { message.from() };
    list.map(|a| {
        a.iter()
            .filter_map(|addr| addr.address.as_deref().map(str::to_string))
            .collect()
    })
    .unwrap_or_default()
}

fn body_of(message: &Message<'_>) -> String {
    let body = message.body_text(0).expect("a text body").to_string();
    agent_mail::read::normalise(&body)
}

/// The header section of `raw`, without the blank line.
fn header_section(raw: &str) -> &str {
    raw.split_once("\r\n\r\n").expect("a header section").0
}

fn assert_has(commands: &[String], verb: &str) {
    assert!(
        commands.iter().any(|c| c == verb),
        "{verb} not sent: {commands:?}"
    );
}

fn assert_lacks(commands: &[String], verb: &str) {
    assert!(
        !commands.iter().any(|c| c == verb),
        "{verb} sent: {commands:?}"
    );
}

const AC1_JSON: &str = "{\"ref\":\"agent@example.test/Drafts/1700000010/1\",\"account\":\"agent@example.test\",\"mailbox\":\"Drafts\",\"to\":[\"carol@example.com\",\"dave@example.com\"],\"subject\":\"Status\",\"original\":null}\n";

const AC1_ARGS: [&str; 9] = [
    "draft",
    "--to",
    "carol@example.com",
    "--to",
    "dave@example.com",
    "--subject",
    "Status",
    "--body",
    "Line one\nLine two",
];

#[test]
fn ac1_draft_is_appended_with_the_draft_flag_to_the_drafts_mailbox() {
    let s = Setup::granted();
    assert_eq!(s.ok(&AC1_ARGS), AC1_JSON);

    let appended = s.server.appended();
    assert_eq!(appended.len(), 1);
    assert_eq!(appended[0].0, "Drafts");
    assert_eq!(appended[0].1, vec!["\\Draft".to_string()]);
    assert_eq!(
        s.server.messages_in("Drafts"),
        vec![(1, "Status".to_string(), vec!["\\Draft".to_string()])]
    );

    let raw = &appended[0].2;
    let message = parse(raw);
    assert_eq!(addresses(&message, false), ["agent@example.test"]);
    assert_eq!(
        addresses(&message, true),
        ["carol@example.com", "dave@example.com"]
    );
    assert_eq!(message.subject(), Some("Status"));
    assert!(message.date().is_some_and(|d| d.is_valid()), "{raw}");
    let id = message.message_id().expect("a Message-ID");
    let (hex, domain) = id.split_once('@').unwrap();
    assert_eq!(domain, "example.test");
    assert!(
        hex.len() == 32
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "{id}"
    );
    assert!(raw_header(raw, "In-Reply-To").is_none());
    assert!(raw_header(raw, "References").is_none());
    assert_eq!(body_of(&message), "Line one\nLine two");

    let commands = s.server.commands();
    assert_has(&commands, "LIST");
    assert_has(&commands, "APPEND");
    for verb in FORBIDDEN {
        assert_lacks(&commands, verb);
    }

    // A second identical call: the next UID and a new Message-ID.
    let second = s.draft(&AC1_ARGS[1..]);
    assert_eq!(
        second["ref"],
        json!("agent@example.test/Drafts/1700000010/2")
    );
    let second_raw = s.last_appended();
    let second_id = parse(&second_raw).message_id().unwrap().to_string();
    assert_ne!(second_id, id);

    // --text, after the subcommand or global.
    let mut text_args = AC1_ARGS.to_vec();
    text_args.push("--text");
    let t = Setup::granted();
    assert_eq!(
        t.ok(&text_args),
        "draft created: agent@example.test/Drafts/1700000010/1\n"
    );
    let g = Setup::granted();
    let mut global = vec!["--text"];
    global.extend_from_slice(&AC1_ARGS);
    assert_eq!(
        g.ok(&global),
        "draft created: agent@example.test/Drafts/1700000010/1\n"
    );
}

#[test]
fn ac2_drafts_mailbox_is_found_by_special_use_not_by_name() {
    let server = FakeImap::start_with_list(
        Mode::Implicit,
        &["UIDPLUS"],
        &["Drafts"],
        Some("[Gmail]/Entw&APw-rfe"),
    );
    let mut grants = Setup::default_grants();
    grants.push(grant(Operation::Read, &["[Gmail]/Entwürfe"]));
    let s = Setup::on(server, grants);
    let result = s.draft(&[
        "--to",
        "carol@example.com",
        "--subject",
        "Status",
        "--body",
        "Hi",
    ]);
    assert_eq!(result["mailbox"], json!("[Gmail]/Entwürfe"));
    // The reference in the form `search` prints, so `read` takes it.
    let expected_ref = MessageRef {
        account: LOGIN.to_string(),
        mailbox: "[Gmail]/Entwürfe".to_string(),
        uidvalidity: 1_700_000_003,
        uid: 1,
    }
    .format();
    assert_eq!(
        expected_ref,
        "agent@example.test/%5BGmail%5D%2FEntw%C3%BCrfe/1700000003/1"
    );
    assert_eq!(result["ref"], json!(expected_ref));

    let appended = s.server.appended();
    assert_eq!(appended.len(), 1);
    assert_eq!(appended[0].0, "[Gmail]/Entw&APw-rfe");
    assert_eq!(appended[0].1, vec!["\\Draft".to_string()]);
    assert!(s.server.messages_in("Drafts").is_empty());

    let read: Value = serde_json::from_str(&s.ok(&["read", &expected_ref])).unwrap();
    assert_eq!(read["subject"], json!("Status"));
}

#[test]
fn ac3_no_drafts_mailbox_fails_with_a_structured_error_and_creates_nothing() {
    let server = FakeImap::start_with_list(Mode::Implicit, &["UIDPLUS"], &["Drafts"], None);
    let s = Setup::on(server, Setup::default_grants());
    let before = s.snapshot();
    let error = s.fails(&[
        "draft",
        "--to",
        "carol@example.com",
        "--subject",
        "Status",
        "--body",
        "Hi",
    ]);
    let mut keys: Vec<&String> = error.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["account", "code", "message", "operation"]);
    assert_eq!(error["code"], json!("drafts_not_found"));
    assert_eq!(error["operation"], json!("draft"));
    assert_eq!(error["account"], json!(LOGIN));
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("no mailbox"), "{message}");
    assert!(
        message.contains("marked as the Drafts mailbox"),
        "{message}"
    );
    assert!(message.contains("no draft was created"), "{message}");
    assert!(
        message.contains("ask the human to create a Drafts folder, or mark one"),
        "{message}"
    );

    let commands = s.server.commands();
    assert_has(&commands, "LIST");
    assert_lacks(&commands, "APPEND");
    assert!(s.server.appended().is_empty());
    assert_eq!(s.snapshot(), before);

    let lines = s.audit();
    assert_eq!(lines.len(), 1);
    assert_eq!(
        with_t(&lines[0]),
        "{\"time\":T,\"mandate\":\"aaaa1111\",\"operation\":\"draft\",\"account\":\"agent@example.test\",\"recipients\":[\"carol@example.com\"],\"outcome\":\"allowed\",\"reason\":\"drafts_not_found\"}"
    );
}

#[test]
fn ac4_reply_draft_threads_with_the_original_and_prefixes_re_once() {
    let s = Setup::granted();
    let thread_before = s.server.messages_in("Thread");
    let read_before = s.server.messages_in("Read");

    let result = s.draft(&[
        "--to",
        "alice@example.com",
        "--reply-to",
        &t(1),
        "--body",
        "Thanks",
    ]);
    assert_eq!(result["subject"], json!("Re: Budget"));
    assert_eq!(
        result["original"],
        json!("agent@example.test/Thread/1700000009/1")
    );
    let raw = s.last_appended();
    assert_eq!(
        raw_header(&raw, "In-Reply-To").as_deref(),
        Some("<thread-2@example.com>")
    );
    assert_eq!(
        raw_header(&raw, "References").as_deref(),
        Some("<thread-0@example.com> <thread-1@example.com> <thread-2@example.com>")
    );
    assert_eq!(parse(&raw).subject(), Some("Re: Budget"));

    let result = s.draft(&[
        "--to",
        "alice@example.com",
        "--reply-to",
        &r(1),
        "--body",
        "Thanks",
    ]);
    assert_eq!(result["subject"], json!("Re: Project update"));
    let raw = s.last_appended();
    assert_eq!(parse(&raw).subject(), Some("Re: Project update"));
    assert_eq!(
        raw_header(&raw, "In-Reply-To").as_deref(),
        Some("<update-1@example.com>")
    );
    assert_eq!(
        raw_header(&raw, "References").as_deref(),
        Some("<update-1@example.com>")
    );

    let result = s.draft(&[
        "--to",
        "bob@example.com",
        "--reply-to",
        &t(2),
        "--body",
        "Ok",
    ]);
    assert_eq!(result["subject"], json!("RE: Agenda"));
    let raw = s.last_appended();
    assert_eq!(parse(&raw).subject(), Some("RE: Agenda"));
    assert!(raw_header(&raw, "In-Reply-To").is_none());
    assert!(raw_header(&raw, "References").is_none());
    assert!(matches!(parse(&raw).references(), HeaderValue::Empty));

    // The originals were never marked \Seen.
    assert_eq!(s.server.messages_in("Thread"), thread_before);
    assert_eq!(s.server.messages_in("Read"), read_before);
    let fetches = s.server.fetches();
    assert_eq!(fetches.len(), 3, "{fetches:?}");
    for fetch in &fetches {
        assert!(fetch.to_ascii_uppercase().contains("BODY.PEEK"), "{fetch}");
    }
    let commands = s.server.commands();
    assert_has(&commands, "EXAMINE");
    assert_lacks(&commands, "SELECT");

    // --subject conflicts with --reply-to.
    s.usage_error(&[
        "draft",
        "--to",
        "a@example.com",
        "--reply-to",
        &t(1),
        "--subject",
        "X",
        "--body",
        "Y",
    ]);
}

#[test]
fn ac5_reply_needs_read_on_the_original_mailbox() {
    let s = Setup::new(vec![
        grant(Operation::Draft, &[]),
        grant(Operation::Read, &["Read"]),
    ]);
    let error = s.fails(&[
        "draft",
        "--to",
        "alice@example.com",
        "--reply-to",
        &t(1),
        "--body",
        "Thanks",
    ]);
    let mut keys: Vec<&String> = error.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["account", "code", "mailbox", "message", "operation"]);
    assert_eq!(error["code"], json!("denied"));
    assert_eq!(error["operation"], json!("read"));
    assert_eq!(error["account"], json!(LOGIN));
    assert_eq!(error["mailbox"], json!("Thread"));
    assert_eq!(s.server.connections(), 0);
    assert!(s.server.appended().is_empty());
    let lines = s.audit();
    assert_eq!(lines.len(), 1);
    assert_eq!(
        with_t(&lines[0]),
        "{\"time\":T,\"mandate\":\"aaaa1111\",\"operation\":\"draft\",\"account\":\"agent@example.test\",\"messages\":[\"agent@example.test/Thread/1700000009/1\"],\"recipients\":[\"alice@example.com\"],\"outcome\":\"denied\",\"reason\":\"denied\"}"
    );

    // A reference naming an Account the Mandate does not cover.
    let o = Setup::granted();
    let other = format!("{OTHER_LOGIN}/Thread/1700000009/1");
    let error = o.fails(&[
        "draft",
        "--to",
        "alice@example.com",
        "--reply-to",
        &other,
        "--body",
        "Thanks",
    ]);
    assert_eq!(error["code"], json!("denied"));
    assert_eq!(error["operation"], json!("read"));
    assert_eq!(error["account"], json!(OTHER_LOGIN));
    assert_eq!(error["mailbox"], json!("Thread"));
    assert_eq!(o.server.connections(), 0);
    assert!(o.server.appended().is_empty());
}

#[test]
fn ac6_without_a_draft_grant_the_call_is_denied_and_audited() {
    let s = Setup::new(vec![
        grant(Operation::Read, &["*"]),
        grant(Operation::List, &["*"]),
    ]);
    let error = s.fails(&[
        "draft",
        "--to",
        "carol@example.com",
        "--subject",
        "Status",
        "--body",
        "Hi",
    ]);
    let mut keys: Vec<&String> = error.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["account", "code", "message", "operation"]);
    assert_eq!(error["code"], json!("denied"));
    assert_eq!(error["operation"], json!("draft"));
    assert_eq!(error["account"], json!(LOGIN));
    assert_eq!(s.server.connections(), 0);
    assert!(s.server.appended().is_empty());
    let lines = s.audit();
    assert_eq!(lines.len(), 1);
    assert_eq!(
        with_t(&lines[0]),
        "{\"time\":T,\"mandate\":\"aaaa1111\",\"operation\":\"draft\",\"account\":\"agent@example.test\",\"recipients\":[\"carol@example.com\"],\"outcome\":\"denied\",\"reason\":\"denied\"}"
    );

    // Reply mode with `read` on Thread but no `draft`.
    let reply = Setup::new(vec![grant(Operation::Read, &["Thread"])]);
    let error = reply.fails(&[
        "draft",
        "--to",
        "alice@example.com",
        "--reply-to",
        &t(1),
        "--body",
        "Thanks",
    ]);
    assert_eq!(error["code"], json!("denied"));
    assert_eq!(error["operation"], json!("draft"));
    assert!(error.get("mailbox").is_none());
    assert_eq!(reply.server.connections(), 0);
}

#[test]
fn ac7_no_command_edits_or_deletes_a_draft() {
    let s = Setup::granted();
    let help = s.run(&["--help"]);
    assert_eq!(help.code, 0);
    let commands_section = help
        .stdout
        .split("Commands:")
        .nth(1)
        .expect("a Commands section")
        .split("\n\n")
        .next()
        .unwrap();
    let names: Vec<&str> = commands_section
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert_eq!(
        names,
        [
            "list", "search", "read", "flag", "move", "draft", "mandate", "help"
        ]
    );

    let draft_help = s.run(&["draft", "--help"]);
    assert_eq!(draft_help.code, 0);
    let options: Vec<String> = draft_help
        .stdout
        .lines()
        .map(str::trim_start)
        .filter(|l| l.starts_with('-'))
        .map(|l| l.split("  ").next().unwrap().to_string())
        .collect();
    assert_eq!(
        options,
        [
            "--to <ADDR>",
            "--subject <TEXT>",
            "--body <TEXT>",
            "--reply-to <REF>",
            "--text",
            "-h, --help"
        ]
    );
    for option in &options {
        for word in ["edit", "delete", "replace", "update"] {
            assert!(!option.contains(word), "{option}");
        }
    }
    assert!(draft_help.stdout.contains("cannot be changed or removed"));

    s.usage_error(&["draft", "delete"]);
    s.usage_error(&["draft", "edit", "agent@example.test/Drafts/1700000010/1"]);
    s.usage_error(&[
        "draft",
        "--to",
        "a@example.com",
        "--subject",
        "X",
        "--body",
        "Y",
        "--replace",
        "agent@example.test/Drafts/1700000010/1",
    ]);
    assert_eq!(s.server.connections(), 0);
    assert!(s.audit().is_empty());
    // Every Setup asserts on drop that no STORE, EXPUNGE, COPY, MOVE or
    // SELECT reached the fake.
}

/// Asserts the log holds none of the forbidden values.
fn assert_log_clean(s: &Setup) {
    let bytes = s.audit_bytes();
    let token = s.token.expose().to_string();
    let hash = token_hash(&token);
    for needle in [
        "Line one",
        "Thanks",
        "Status",
        "Budget",
        "thread-2@example.com",
        CREDENTIAL,
        token.as_str(),
        hash.as_str(),
    ] {
        assert!(
            !contains_bytes(&bytes, needle.as_bytes()),
            "the audit log contains {needle:?}"
        );
    }
}

#[test]
fn ac8_audit_entry_records_recipients_and_references_only() {
    let s = Setup::granted();
    s.ok(&AC1_ARGS);
    let lines = s.audit();
    assert_eq!(lines.len(), 1);
    assert_eq!(
        with_t(&lines[0]),
        "{\"time\":T,\"mandate\":\"aaaa1111\",\"operation\":\"draft\",\"account\":\"agent@example.test\",\"mailbox\":\"Drafts\",\"messages\":[\"agent@example.test/Drafts/1700000010/1\"],\"recipients\":[\"carol@example.com\",\"dave@example.com\"],\"outcome\":\"allowed\"}"
    );
    assert_log_clean(&s);

    let reply = Setup::granted();
    reply.draft(&[
        "--to",
        "alice@example.com",
        "--reply-to",
        &t(1),
        "--body",
        "Thanks",
    ]);
    let lines = reply.audit();
    assert_eq!(lines.len(), 1);
    assert_eq!(
        with_t(&lines[0]),
        "{\"time\":T,\"mandate\":\"aaaa1111\",\"operation\":\"draft\",\"account\":\"agent@example.test\",\"mailbox\":\"Drafts\",\"messages\":[\"agent@example.test/Thread/1700000009/1\",\"agent@example.test/Drafts/1700000010/1\"],\"recipients\":[\"alice@example.com\"],\"outcome\":\"allowed\"}"
    );
    assert_log_clean(&reply);

    // Without UIDPLUS: no new reference.
    let plain = Setup::on(
        FakeImap::start_with_list(Mode::Implicit, &[], &["Drafts"], Some("Drafts")),
        Setup::default_grants(),
    );
    let result = plain.draft(&AC1_ARGS[1..]);
    assert_eq!(result["ref"], Value::Null);
    let lines = plain.audit();
    assert_eq!(
        with_t(&lines[0]),
        "{\"time\":T,\"mandate\":\"aaaa1111\",\"operation\":\"draft\",\"account\":\"agent@example.test\",\"mailbox\":\"Drafts\",\"recipients\":[\"carol@example.com\",\"dave@example.com\"],\"outcome\":\"allowed\"}"
    );
    let mut text_args = AC1_ARGS.to_vec();
    text_args.push("--text");
    assert_eq!(
        plain.ok(&text_args),
        "draft created in Drafts (new reference not reported)\n"
    );
    assert_log_clean(&plain);

    // Other operations never carry recipients.
    let others = Setup::new(vec![
        grant(Operation::List, &["*"]),
        grant(Operation::Search, &["*"]),
        grant(Operation::Read, &["*"]),
    ]);
    others.ok(&["list"]);
    others.ok(&["search", "--mailbox", "Thread"]);
    others.ok(&["read", &t(1)]);
    others.fails(&["flag", &t(1), "--set", "seen"]);
    others.fails(&["move", &t(1), "--to", "Archive"]);
    let lines = others.audit();
    assert_eq!(lines.len(), 5);
    for line in lines {
        assert!(!line.contains("recipients"), "{line}");
    }
}

#[test]
fn header_injection_values_are_refused_by_the_cli_and_the_core() {
    let s = Setup::granted();
    for to in [
        "Carol <carol@example.com>",
        "a@b.c\r\nBcc: x@y.z",
        "a@b@c",
        "",
    ] {
        s.usage_error(&["draft", "--to", to, "--subject", "Hi", "--body", "x"]);
    }
    for subject in ["Hi\r\nBcc: x@y.z", ""] {
        s.usage_error(&[
            "draft",
            "--to",
            "a@example.com",
            "--subject",
            subject,
            "--body",
            "x",
        ]);
    }
    s.usage_error(&[
        "draft",
        "--to",
        "a@example.com",
        "--subject",
        "Hi",
        "--body",
        "a\0b",
    ]);
    s.usage_error(&[
        "draft",
        "--to",
        "a@example.com",
        "--reply-to",
        "not-a-ref",
        "--body",
        "x",
    ]);
    s.usage_error(&["draft", "--to", "a@example.com", "--body", "x"]);

    let cases: Vec<(Command, &str, &str)> = vec![
        (
            Command::Draft {
                to: vec!["Carol <carol@example.com>".to_string()],
                subject: Some("Hi".to_string()),
                body: "x".to_string(),
                reply_to: None,
            },
            "to",
            "Carol",
        ),
        (
            Command::Draft {
                to: vec!["a@b.c\r\nBcc: x@y.z".to_string()],
                subject: Some("Hi".to_string()),
                body: "x".to_string(),
                reply_to: None,
            },
            "to",
            "Bcc",
        ),
        (
            Command::Draft {
                to: vec!["a@example.com".to_string()],
                subject: Some("Hi\r\nBcc: x@y.z".to_string()),
                body: "x".to_string(),
                reply_to: None,
            },
            "subject",
            "Bcc",
        ),
        (
            Command::Draft {
                to: vec!["a@example.com".to_string()],
                subject: None,
                body: "x".to_string(),
                reply_to: None,
            },
            "subject",
            "\"subject\":\"",
        ),
        (
            Command::Draft {
                to: vec!["a@example.com".to_string()],
                subject: Some("Hi".to_string()),
                body: "secret\0body".to_string(),
                reply_to: None,
            },
            "body",
            "secret",
        ),
    ];
    for (command, field, needle) in cases {
        let before = s.audit().len();
        let error = s.core(&command).expect_err("refused");
        assert_eq!(error, CoreError::InvalidDraft { field });
        let body = serde_json::to_string(&error.body()).unwrap();
        assert!(body.contains("\"code\":\"invalid_draft\""), "{body}");
        assert!(body.contains("\"operation\":\"draft\""), "{body}");
        assert!(
            body.contains(&format!("`{field}`")) || body.contains(field),
            "{body}"
        );
        assert!(!body.contains(needle), "{body}");
        let lines = s.audit();
        assert_eq!(lines.len(), before + 1);
        assert!(!lines[before].contains(needle), "{}", lines[before]);
        assert!(lines[before].contains("\"reason\":\"invalid_draft\""));
    }
    assert_eq!(s.server.connections(), 0);
}

#[test]
fn non_ascii_subjects_and_long_bodies_are_encoded() {
    let s = Setup::granted();
    s.draft(&[
        "--to",
        "a@example.com",
        "--subject",
        "Grüße",
        "--body",
        "für dich",
    ]);
    let raw = s.last_appended();
    assert!(header_section(&raw).is_ascii(), "{raw}");
    let message = parse(&raw);
    assert_eq!(message.subject(), Some("Grüße"));
    assert_eq!(body_of(&message), "für dich");

    let long = format!("{}{}", "x".repeat(1000), "ü".repeat(1000));
    s.draft(&[
        "--to",
        "a@example.com",
        "--subject",
        "Long",
        "--body",
        &long,
    ]);
    let raw = s.last_appended();
    assert!(raw.split("\r\n").all(|l| l.len() <= 998));
    assert!(raw.is_ascii());
    assert_eq!(body_of(&parse(&raw)), long);
}

#[test]
fn stale_or_missing_reply_references_create_nothing() {
    let s = Setup::granted();
    let error = s.fails(&[
        "draft",
        "--to",
        "alice@example.com",
        "--reply-to",
        "agent@example.test/Thread/1700000099/1",
        "--body",
        "Thanks",
    ]);
    assert_eq!(error["code"], json!("stale_reference"));
    assert_eq!(error["operation"], json!("draft"));
    assert_eq!(error["mailbox"], json!("Thread"));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("no draft was created")
    );
    let error = s.fails(&[
        "draft",
        "--to",
        "alice@example.com",
        "--reply-to",
        &t(99),
        "--body",
        "Thanks",
    ]);
    assert_eq!(error["code"], json!("message_not_found"));
    assert_eq!(error["operation"], json!("draft"));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("no draft was created")
    );
    assert_lacks(&s.server.commands(), "APPEND");
    let lines = s.audit();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("\"outcome\":\"allowed\",\"reason\":\"stale_reference\""));
    assert!(lines[1].contains("\"outcome\":\"allowed\",\"reason\":\"message_not_found\""));
}

#[test]
fn help_says_subject_and_reply_to_cannot_be_combined() {
    let s = Setup::granted();
    for flag in ["-h", "--help"] {
        let help = s.run(&["draft", flag]);
        assert_eq!(help.code, 0);
        let subject = help
            .stdout
            .lines()
            .map(str::trim_start)
            .skip_while(|l| !l.starts_with("--subject"))
            .take_while(|l| !l.starts_with("--body"))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            subject.contains("required without --reply-to and not allowed with it"),
            "draft {flag}: {subject:?}"
        );
    }
    let long = s.run(&["draft", "--help"]);
    assert!(long.stdout.contains("but not both"));
    s.usage_error(&[
        "draft",
        "--to",
        "a@example.com",
        "--subject",
        "X",
        "--body",
        "Y",
        "--reply-to",
        &t(1),
    ]);
}
