//! Shared test support: an in-process fake IMAP server and helpers that drive
//! the CLI adapter in-process. All values here are fake.
#![allow(dead_code)]

pub mod broker;
pub mod proxy;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use agent_mail::mandate::{Account, Credential, Grant, Mandate, Operation, Policy, TlsMode};
use agent_mail::seal::{self, AgentToken};
use agent_mail::store::{InMemoryStore, MandateStore};
use agent_mail::{Context, RunOutput};
use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

/// Fake Account login.
pub const LOGIN: &str = "agent@example.test";
/// Second fake Account login.
pub const OTHER_LOGIN: &str = "other@example.test";
/// Logins the fake server knows.
pub const KNOWN_LOGINS: [&str; 2] = [LOGIN, OTHER_LOGIN];
/// Fake Credential.
pub const CREDENTIAL: &str = "fake-app-password-123";
/// A second fake Credential the server accepts, used by the Broker's
/// `create` tests to check that no reply carries it.
pub const BROKER_CREDENTIAL: &str = "Cred-7f3a-secret";
/// Mailboxes the fake server lists, raw, in this order.
pub const SERVER_MAILBOXES: [&str; 5] =
    ["INBOX", "Archive", "Sent", "Spam", "[Gmail]/Entw&APw-rfe"];

/// How the fake server secures connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// TLS from the first byte.
    Implicit,
    /// Plaintext greeting, advertises and accepts STARTTLS.
    Starttls,
    /// Plaintext only, never advertises STARTTLS, accepts LOGIN in clear.
    PlaintextOnly,
}

/// One fixture message. All values are fake.
#[derive(Debug, Clone)]
pub struct FakeMessage {
    pub uid: u32,
    /// Display name of the From address.
    pub from_name: Option<&'static str>,
    /// `mailbox@host` of the From address.
    pub from: &'static str,
    /// `mailbox@host` of the To address.
    pub to: &'static str,
    /// The raw Subject header (may be RFC 2047).
    pub subject: String,
    pub body: &'static str,
    /// IMAP `date-time`, e.g. `01-Oct-2026 09:00:00 +0000`.
    pub internal_date: &'static str,
    pub flags: Vec<String>,
    /// The full raw MIME source (CRLF line endings). When set, `raw()`
    /// returns it instead of a message built from the fields above.
    pub source: Option<String>,
}

impl FakeMessage {
    /// The From header as it appears in the message.
    fn header_from(&self) -> String {
        match self.from_name {
            Some(name) => format!("{name} <{}>", self.from),
            None => self.from.to_string(),
        }
    }

    /// The whole message, headers and body.
    fn raw(&self) -> String {
        if let Some(source) = &self.source {
            return source.clone();
        }
        format!(
            "From: {}\r\nTo: {}\r\nSubject: {}\r\n\r\n{}\r\n",
            self.header_from(),
            self.to,
            self.subject,
            self.body
        )
    }

    /// The internal date's calendar day, in its own time zone.
    fn day(&self) -> (u32, u32, u32) {
        imap_date(&self.internal_date[..11]).expect("fixture date")
    }
}

/// One fixture mailbox.
#[derive(Debug, Clone)]
pub struct FakeMailbox {
    /// Raw (modified UTF-7) name.
    pub name: &'static str,
    pub uidvalidity: u32,
    pub messages: Vec<FakeMessage>,
}

/// UIDVALIDITY of the fixture `INBOX`.
pub const INBOX_UIDVALIDITY: u32 = 1_700_000_001;
/// UIDVALIDITY of the fixture `Archive`.
pub const ARCHIVE_UIDVALIDITY: u32 = 1_700_000_002;

fn flags(list: &[&str]) -> Vec<String> {
    list.iter().map(|f| (*f).to_string()).collect()
}

/// The fixture mailboxes: `INBOX` (4 messages), `Archive` (25), and empty
/// `Sent`, `Spam` and `[Gmail]/Entwürfe`. `Missing` does not exist.
pub fn fixture_mailboxes() -> Vec<FakeMailbox> {
    let inbox = vec![
        FakeMessage {
            uid: 1,
            from_name: Some("Alice Example"),
            from: "alice@example.com",
            to: LOGIN,
            subject: "Quarterly report".to_string(),
            body: "numbers attached",
            internal_date: "01-Oct-2026 09:00:00 +0000",
            flags: flags(&["\\Seen"]),
            source: None,
        },
        FakeMessage {
            uid: 2,
            from_name: None,
            from: "bob@example.com",
            to: LOGIN,
            subject: "Lunch?".to_string(),
            body: "pizza today",
            internal_date: "05-Oct-2026 09:00:00 +0000",
            flags: Vec::new(),
            source: None,
        },
        FakeMessage {
            uid: 3,
            from_name: None,
            from: "alice@example.com",
            to: "team@example.com",
            subject: "Re: Quarterly report".to_string(),
            body: "pizza on friday",
            internal_date: "10-Oct-2026 09:00:00 +0000",
            flags: flags(&["\\Flagged"]),
            source: None,
        },
        FakeMessage {
            uid: 4,
            from_name: Some("Carol"),
            from: "carol@example.com",
            to: LOGIN,
            subject: "=?UTF-8?Q?Rechnung_f=C3=BCr_Oktober?=".to_string(),
            body: "please pay",
            internal_date: "15-Oct-2026 11:00:00 +0200",
            flags: flags(&["\\Seen", "\\Flagged"]),
            source: None,
        },
    ];
    let archive = (1..=25)
        .map(|uid| FakeMessage {
            uid,
            from_name: None,
            from: "news@example.com",
            to: LOGIN,
            subject: format!("Newsletter {uid}"),
            body: "news of the week",
            internal_date: "02-Sep-2026 08:00:00 +0000",
            flags: Vec::new(),
            source: None,
        })
        .collect();
    let empty = |name, uidvalidity| FakeMailbox {
        name,
        uidvalidity,
        messages: Vec::new(),
    };
    vec![
        FakeMailbox {
            name: "INBOX",
            uidvalidity: INBOX_UIDVALIDITY,
            messages: inbox,
        },
        FakeMailbox {
            name: "Archive",
            uidvalidity: ARCHIVE_UIDVALIDITY,
            messages: archive,
        },
        empty("Sent", 1_700_000_004),
        empty("Spam", 1_700_000_005),
        empty("[Gmail]/Entw&APw-rfe", 1_700_000_003),
        FakeMailbox {
            name: "Read",
            uidvalidity: READ_UIDVALIDITY,
            messages: read_messages(),
        },
        FakeMailbox {
            name: "Triage",
            uidvalidity: TRIAGE_UIDVALIDITY,
            messages: triage_messages(),
        },
        FakeMailbox {
            name: "Filing",
            uidvalidity: FILING_UIDVALIDITY,
            messages: filing_messages(),
        },
        FakeMailbox {
            name: "Thread",
            uidvalidity: THREAD_UIDVALIDITY,
            messages: thread_messages(),
        },
        empty("Drafts", DRAFTS_UIDVALIDITY),
    ]
}

/// UIDVALIDITY of the fixture `Thread`.
pub const THREAD_UIDVALIDITY: u32 = 1_700_000_009;
/// UIDVALIDITY of the fixture `Drafts` (empty, not in `SERVER_MAILBOXES`).
pub const DRAFTS_UIDVALIDITY: u32 = 1_700_000_010;

/// The `Thread` mailbox's two messages, as full MIME sources, no flags:
/// UID 1 `Re: Budget` with a Message-ID and References folded over two
/// lines, UID 2 `RE: Agenda` with neither.
fn thread_messages() -> Vec<FakeMessage> {
    let message = |uid, from: &'static str, subject: &str, source: &str| FakeMessage {
        uid,
        from_name: None,
        from,
        to: LOGIN,
        subject: subject.to_string(),
        body: "",
        internal_date: "09-Oct-2026 09:00:00 +0000",
        flags: Vec::new(),
        source: Some(source.to_string()),
    };
    let one = "From: alice@example.com\r\n\
               To: agent@example.test\r\n\
               Subject: Re: Budget\r\n\
               Date: Fri, 09 Oct 2026 09:00:00 +0000\r\n\
               Message-ID: <thread-2@example.com>\r\n\
               In-Reply-To: <thread-1@example.com>\r\n\
               References: <thread-0@example.com>\r\n <thread-1@example.com>\r\n\
               MIME-Version: 1.0\r\n\
               Content-Type: text/plain; charset=utf-8\r\n\
               \r\n\
               The budget is attached.\r\n";
    let two = "From: bob@example.com\r\n\
               To: agent@example.test\r\n\
               Subject: RE: Agenda\r\n\
               Date: Fri, 09 Oct 2026 10:00:00 +0000\r\n\
               MIME-Version: 1.0\r\n\
               Content-Type: text/plain; charset=utf-8\r\n\
               \r\n\
               Agenda for Monday.\r\n";
    vec![
        message(1, "alice@example.com", "Re: Budget", one),
        message(2, "bob@example.com", "RE: Agenda", two),
    ]
}

/// UIDVALIDITY of the fixture `Filing`.
pub const FILING_UIDVALIDITY: u32 = 1_700_000_008;

/// The `Filing` mailbox's three simple messages: UID 1 with no flags, UID 2
/// marked `\Deleted` by the human but not yet expunged, UID 3 `\Seen`.
fn filing_messages() -> Vec<FakeMessage> {
    let message = |uid, from, subject: &str, list: &[&str]| FakeMessage {
        uid,
        from_name: None,
        from,
        to: LOGIN,
        subject: subject.to_string(),
        body: "filing body",
        internal_date: "08-Oct-2026 09:00:00 +0000",
        flags: flags(list),
        source: None,
    };
    vec![
        message(1, "alice@example.com", "Filing one", &[]),
        message(2, "bob@example.com", "Filing two", &["\\Deleted"]),
        message(3, "carol@example.com", "Filing three", &["\\Seen"]),
    ]
}

/// UIDVALIDITY of the fixture `Triage`.
pub const TRIAGE_UIDVALIDITY: u32 = 1_700_000_007;

/// The `Triage` mailbox's two simple messages: UID 1 with no flags, UID 2
/// with `\Seen \Flagged \Answered`.
fn triage_messages() -> Vec<FakeMessage> {
    let message = |uid, from, subject: &str, list: &[&str]| FakeMessage {
        uid,
        from_name: None,
        from,
        to: LOGIN,
        subject: subject.to_string(),
        body: "triage body",
        internal_date: "07-Oct-2026 09:00:00 +0000",
        flags: flags(list),
        source: None,
    };
    vec![
        message(1, "alice@example.com", "Triage one", &[]),
        message(
            2,
            "bob@example.com",
            "Triage two",
            &["\\Seen", "\\Flagged", "\\Answered"],
        ),
    ]
}

/// UIDVALIDITY of the fixture `Read`.
pub const READ_UIDVALIDITY: u32 = 1_700_000_006;

/// The HTML of the `Read` fixture's UID 2.
pub const READ_HTML: &str = "<html><head><style>p{color:red}</style></head><body><p>Hello <b>Bob</b>,</p><p>See&nbsp;the <a href=\"https://example.com/x\">report</a> &amp; reply.</p><script>alert(1)</script></body></html>";

/// Content of the `notes.txt` attachment of the `Read` fixture's UID 3.
pub const NOTES_CONTENT: &str = "ATTACHMENT-CONTENT-7f3a";

/// The 1024 bytes of the `report.pdf` attachment: `0x00..=0xFF`, four times.
pub fn report_pdf_bytes() -> Vec<u8> {
    (0..4).flat_map(|_| 0..=255u8).collect()
}

/// The body of the `Read` fixture's UID 4: 19,999 `x`, `ü`, 5,000 `y`.
pub fn long_body() -> String {
    format!("{}ü{}", "x".repeat(19_999), "y".repeat(5_000))
}

/// Quoted-printable encoding of ASCII `text` with CRLF soft line breaks.
fn quoted_printable(text: &str) -> String {
    let mut out = String::new();
    let mut line = 0;
    for b in text.bytes() {
        let piece = if b == b'=' || !(0x20..0x7f).contains(&b) {
            format!("={b:02X}")
        } else {
            char::from(b).to_string()
        };
        if line + piece.len() > 75 {
            out.push_str("=\r\n");
            line = 0;
        }
        line += piece.len();
        out.push_str(&piece);
    }
    out
}

/// Base64 in lines of 76 characters, CRLF-terminated.
fn base64_lines(bytes: &[u8]) -> String {
    use base64::Engine;
    let text = base64::engine::general_purpose::STANDARD.encode(bytes);
    let mut out = String::new();
    for chunk in text.as_bytes().chunks(76) {
        out.push_str(std::str::from_utf8(chunk).unwrap());
        out.push_str("\r\n");
    }
    out
}

/// The `Read` mailbox's five messages, as full MIME sources. None `\Seen`.
fn read_messages() -> Vec<FakeMessage> {
    let message = |uid, from: &'static str, subject: &str, source: String| FakeMessage {
        uid,
        from_name: None,
        from,
        to: LOGIN,
        subject: subject.to_string(),
        body: "",
        internal_date: "06-Oct-2026 10:15:00 +0200",
        flags: Vec::new(),
        source: Some(source),
    };
    let one = "From: Alice Example <alice@example.com>\r\n\
               To: agent@example.test\r\n\
               Cc: Carol <carol@example.com>\r\n\
               Subject: Project update\r\n\
               Date: Tue, 06 Oct 2026 10:15:00 +0200\r\n\
               Message-ID: <update-1@example.com>\r\n\
               MIME-Version: 1.0\r\n\
               Content-Type: text/plain; charset=utf-8\r\n\
               Content-Transfer-Encoding: 7bit\r\n\
               \r\n\
               Hi,\r\n\r\nthe project is on track.\r\n\r\nAlice\r\n"
        .to_string();
    let two = format!(
        "From: news@example.com\r\n\
         To: agent@example.test\r\n\
         Subject: HTML only\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Transfer-Encoding: quoted-printable\r\n\
         \r\n\
         {}\r\n",
        quoted_printable(READ_HTML)
    );
    let three = format!(
        "From: bob@example.com\r\n\
         To: agent@example.test\r\n\
         Subject: Files\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: multipart/mixed; boundary=\"b3\"\r\n\
         \r\n\
         --b3\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         \r\n\
         See the attached files.\r\n\
         --b3\r\n\
         Content-Type: application/pdf\r\n\
         Content-Disposition: attachment; filename=\"report.pdf\"\r\n\
         Content-Transfer-Encoding: base64\r\n\
         \r\n\
         {}\
         --b3\r\n\
         Content-Type: text/plain\r\n\
         Content-Disposition: attachment; filename=\"notes.txt\"\r\n\
         Content-Transfer-Encoding: 7bit\r\n\
         \r\n\
         {NOTES_CONTENT}\r\n\
         --b3--\r\n",
        base64_lines(&report_pdf_bytes())
    );
    let four = format!(
        "From: alice@example.com\r\n\
         To: agent@example.test\r\n\
         Subject: Long\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Content-Transfer-Encoding: 8bit\r\n\
         \r\n\
         {}\r\n",
        long_body()
    );
    let five = "From: alice@example.com\r\n\
                To: agent@example.test\r\n\
                Subject: Both\r\n\
                MIME-Version: 1.0\r\n\
                Content-Type: multipart/alternative; boundary=\"b5\"\r\n\
                \r\n\
                --b5\r\n\
                Content-Type: text/plain; charset=utf-8\r\n\
                \r\n\
                Plain version\r\n\
                --b5\r\n\
                Content-Type: text/html; charset=utf-8\r\n\
                \r\n\
                <p>HTML version</p>\r\n\
                --b5--\r\n"
        .to_string();
    let mut first = message(1, "alice@example.com", "Project update", one);
    first.from_name = Some("Alice Example");
    vec![
        first,
        message(2, "news@example.com", "HTML only", two),
        message(3, "bob@example.com", "Files", three),
        message(4, "alice@example.com", "Long", four),
        message(5, "alice@example.com", "Both", five),
    ]
}

/// What the fake server holds and has seen, shared by its connections.
#[derive(Debug, Default)]
struct State {
    commands: Vec<String>,
    mailboxes: Vec<FakeMailbox>,
    searches: Vec<String>,
    fetch_uid_counts: Vec<usize>,
    fetches: Vec<String>,
    stores: Vec<String>,
    /// Extra capability atoms advertised, e.g. `MOVE` and `UIDPLUS`.
    capabilities: Vec<String>,
    /// Raw names `LIST` returns after `SERVER_MAILBOXES`.
    extra_listed: Vec<String>,
    /// The raw name `LIST` marks `\Drafts`, if any.
    drafts: Option<String>,
    /// Every accepted `APPEND`: raw mailbox, flags, raw message.
    appended: Vec<(String, Vec<String>, String)>,
    /// Scripted `X-GM-RAW` results: raw mailbox, exact query, UIDs.
    gmail: Vec<(String, String, Vec<u32>)>,
    /// A scripted refusal of the next `APPEND`: whether it comes before
    /// the literal (in place of `+ ready`), and the reply after the tag.
    refuse_append: Option<(bool, String)>,
}

impl State {
    fn advertises(&self, capability: &str) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.eq_ignore_ascii_case(capability))
    }
}

/// An in-process fake IMAP server on a real TCP port.
pub struct FakeImap {
    pub port: u16,
    pub ca: CertificateDer<'static>,
    state: Arc<Mutex<State>>,
    connections: Arc<AtomicUsize>,
}

impl FakeImap {
    /// Starts a server in `mode` on an ephemeral localhost port. It
    /// advertises neither `MOVE` nor `UIDPLUS`.
    pub fn start(mode: Mode) -> Self {
        Self::start_with_capabilities(mode, &[])
    }

    /// Like [`FakeImap::start`], also advertising `capabilities` (e.g.
    /// `MOVE`, `UIDPLUS`), before and after login alike.
    pub fn start_with_capabilities(mode: Mode, capabilities: &[&str]) -> Self {
        Self::start_with_list(mode, capabilities, &[], None)
    }

    /// Like [`FakeImap::start_with_capabilities`]; `LIST` also returns the
    /// raw names `extra_listed` after `SERVER_MAILBOXES`, and marks the raw
    /// name `drafts` with `(\HasNoChildren \Drafts)`, as Gmail does.
    pub fn start_with_list(
        mode: Mode,
        capabilities: &[&str],
        extra_listed: &[&str],
        drafts: Option<&str>,
    ) -> Self {
        let (ca, tls) = tls_config();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake server");
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(State {
            mailboxes: fixture_mailboxes(),
            capabilities: capabilities.iter().map(|c| (*c).to_string()).collect(),
            extra_listed: extra_listed.iter().map(|c| (*c).to_string()).collect(),
            drafts: drafts.map(str::to_string),
            ..State::default()
        }));
        let connections = Arc::new(AtomicUsize::new(0));
        let (c, n) = (state.clone(), connections.clone());
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                n.fetch_add(1, Ordering::SeqCst);
                let (c, tls) = (c.clone(), tls.clone());
                thread::spawn(move || serve(stream, mode, tls, c));
            }
        });
        Self {
            port,
            ca,
            state,
            connections,
        }
    }

    /// Command verbs received, upper-cased, in order (`UID SEARCH` and
    /// `UID FETCH` with their sub-verb). Arguments are dropped so the
    /// Credential never lands in test output.
    pub fn commands(&self) -> Vec<String> {
        self.state.lock().unwrap().commands.clone()
    }

    /// The arguments of every `UID SEARCH`, in order.
    pub fn searches(&self) -> Vec<String> {
        self.state.lock().unwrap().searches.clone()
    }

    /// The arguments of every `UID FETCH`, in order, e.g. `1 (UID FLAGS)`.
    pub fn fetches(&self) -> Vec<String> {
        self.state.lock().unwrap().fetches.clone()
    }

    /// The arguments of every `STORE` and `UID STORE`, in order, e.g.
    /// `1 +FLAGS (\Seen)`.
    pub fn stores(&self) -> Vec<String> {
        self.state.lock().unwrap().stores.clone()
    }

    /// For each `UID FETCH`, how many UIDs its set covered.
    pub fn fetch_uid_counts(&self) -> Vec<usize> {
        self.state.lock().unwrap().fetch_uid_counts.clone()
    }

    /// The current flags of every message, as `(mailbox, uid, flags)`.
    pub fn all_flags(&self) -> Vec<(String, u32, Vec<String>)> {
        let state = self.state.lock().unwrap();
        state
            .mailboxes
            .iter()
            .flat_map(|m| {
                m.messages
                    .iter()
                    .map(|msg| (m.name.to_string(), msg.uid, msg.flags.clone()))
            })
            .collect()
    }

    /// UID, subject and flags of every message in mailbox `name` (raw
    /// name), in UID order. Empty if there is no such mailbox.
    pub fn messages_in(&self, name: &str) -> Vec<(u32, String, Vec<String>)> {
        let state = self.state.lock().unwrap();
        let mut messages: Vec<(u32, String, Vec<String>)> = state
            .mailboxes
            .iter()
            .filter(|m| m.name == name)
            .flat_map(|m| {
                m.messages
                    .iter()
                    .map(|msg| (msg.uid, msg.subject.clone(), msg.flags.clone()))
            })
            .collect();
        messages.sort_by_key(|(uid, _, _)| *uid);
        messages
    }

    /// Every accepted `APPEND`, in order: the raw mailbox name, the flags
    /// and the raw message.
    pub fn appended(&self) -> Vec<(String, Vec<String>, String)> {
        self.state.lock().unwrap().appended.clone()
    }

    /// Scripts the UIDs an `X-GM-RAW` search for exactly `query` returns in
    /// `INBOX`. Unscripted queries match nothing. Only answered when the
    /// server advertises `X-GM-EXT-1`; otherwise `X-GM-RAW` gets `BAD`.
    pub fn script_gmail_search(&self, query: &str, uids: &[u32]) {
        self.script_gmail_search_in("INBOX", query, uids);
    }

    /// Like [`FakeImap::script_gmail_search`], in mailbox `mailbox` (raw
    /// name).
    pub fn script_gmail_search_in(&self, mailbox: &str, query: &str, uids: &[u32]) {
        self.state.lock().unwrap().gmail.push((
            mailbox.to_string(),
            query.to_string(),
            uids.to_vec(),
        ));
    }

    /// Answers the next `APPEND` with `<tag> <reply>` and stores nothing:
    /// in place of `+ ready` when `before_literal`, else after reading the
    /// literal.
    pub fn refuse_append(&self, before_literal: bool, reply: &str) {
        self.state.lock().unwrap().refuse_append = Some((before_literal, reply.to_string()));
    }

    /// Number of TCP connections accepted.
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }
}

fn tls_config() -> (CertificateDer<'static>, Arc<ServerConfig>) {
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca_key = KeyPair::generate().unwrap();
    let ca_cert = ca_params.self_signed(&ca_key).unwrap();
    let issuer = Issuer::new(ca_params, ca_key);

    let leaf_key = KeyPair::generate().unwrap();
    let leaf = CertificateParams::new(vec!["localhost".to_string(), "imap.gmail.com".to_string()])
        .unwrap()
        .signed_by(&leaf_key, &issuer)
        .unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![leaf.der().clone()],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der())),
        )
        .unwrap();
    (ca_cert.der().clone(), Arc::new(config))
}

fn serve(mut stream: TcpStream, mode: Mode, tls: Arc<ServerConfig>, commands: Arc<Mutex<State>>) {
    match mode {
        Mode::Implicit => {
            let conn = ServerConnection::new(tls).unwrap();
            let mut tls_stream = StreamOwned::new(conn, stream);
            let _ = session(&mut tls_stream, mode, true, &commands, true);
        }
        Mode::Starttls | Mode::PlaintextOnly => {
            if session(&mut stream, mode, false, &commands, true) == Some(Upgrade) {
                let conn = ServerConnection::new(tls).unwrap();
                let mut tls_stream = StreamOwned::new(conn, stream);
                let _ = session(&mut tls_stream, mode, true, &commands, false);
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Upgrade;

fn send(stream: &mut impl Write, text: &str) -> Option<()> {
    stream.write_all(text.as_bytes()).ok()?;
    stream.flush().ok()
}

fn read_line(stream: &mut impl Read) -> Option<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(1) => {
                if byte[0] == b'\n' {
                    break;
                }
                line.push(byte[0]);
            }
            _ => return None,
        }
    }
    Some(String::from_utf8_lossy(&line).trim_end().to_string())
}

fn session(
    stream: &mut (impl Read + Write),
    mode: Mode,
    secure: bool,
    state: &Mutex<State>,
    greet: bool,
) -> Option<Upgrade> {
    if greet {
        send(stream, "* OK fake IMAP ready\r\n")?;
    }
    // The opened mailbox's index, and whether it was opened read-only.
    let mut opened: Option<(usize, bool)> = None;
    loop {
        let line = read_line(stream)?;
        let mut parts = line.splitn(3, ' ');
        let tag = parts.next().unwrap_or("").to_string();
        let mut verb = parts.next().unwrap_or("").to_ascii_uppercase();
        let mut args = parts.next().unwrap_or("").to_string();
        if verb == "UID" {
            let (sub, rest) = args.split_once(' ').unwrap_or((&args, ""));
            verb = format!("UID {}", sub.to_ascii_uppercase());
            args = rest.to_string();
        }
        let args = args.as_str();
        state.lock().unwrap().commands.push(verb.clone());
        match verb.as_str() {
            "CAPABILITY" => {
                let mut caps = if !secure && mode == Mode::Starttls {
                    "IMAP4rev1 STARTTLS LOGINDISABLED".to_string()
                } else {
                    "IMAP4rev1 AUTH=PLAIN".to_string()
                };
                for extra in &state.lock().unwrap().capabilities {
                    caps.push(' ');
                    caps.push_str(extra);
                }
                send(
                    stream,
                    &format!("* CAPABILITY {caps}\r\n{tag} OK CAPABILITY done\r\n"),
                )?;
            }
            "STARTTLS" if !secure && mode == Mode::Starttls => {
                send(stream, &format!("{tag} OK begin TLS now\r\n"))?;
                return Some(Upgrade);
            }
            "LOGIN" if login_accepted(args) => {
                send(stream, &format!("{tag} OK logged in\r\n"))?;
            }
            "LOGIN" | "AUTHENTICATE" => {
                send(stream, &format!("{tag} NO login refused\r\n"))?;
            }
            "LIST" => {
                let mut reply = String::new();
                let state = state.lock().unwrap();
                let names = SERVER_MAILBOXES
                    .iter()
                    .map(|n| n.to_string())
                    .chain(state.extra_listed.iter().cloned());
                for name in names {
                    let attributes = if state.drafts.as_deref() == Some(name.as_str()) {
                        "\\HasNoChildren \\Drafts"
                    } else {
                        "\\HasNoChildren"
                    };
                    reply.push_str(&format!("* LIST ({attributes}) \"/\" \"{name}\"\r\n"));
                }
                drop(state);
                reply.push_str(&format!("{tag} OK LIST done\r\n"));
                send(stream, &reply)?;
            }
            "EXAMINE" | "SELECT" => {
                let name = imap_strings(args).into_iter().next().unwrap_or_default();
                let state = state.lock().unwrap();
                match state.mailboxes.iter().position(|m| m.name == name) {
                    Some(index) => {
                        let mailbox = &state.mailboxes[index];
                        let read_only = verb == "EXAMINE";
                        opened = Some((index, read_only));
                        let access = if read_only { "READ-ONLY" } else { "READ-WRITE" };
                        let next = mailbox.messages.last().map_or(1, |m| m.uid + 1);
                        send(
                            stream,
                            &format!(
                                "* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)\r\n\
                                 * {} EXISTS\r\n* 0 RECENT\r\n\
                                 * OK [UIDVALIDITY {}] UIDs valid\r\n\
                                 * OK [UIDNEXT {next}] predicted next UID\r\n\
                                 {tag} OK [{access}] {verb} done\r\n",
                                mailbox.messages.len(),
                                mailbox.uidvalidity
                            ),
                        )?;
                    }
                    None => {
                        opened = None;
                        send(stream, &format!("{tag} NO no such mailbox\r\n"))?;
                    }
                }
            }
            "UID SEARCH" => {
                let mut state = state.lock().unwrap();
                state.searches.push(args.to_string());
                let reply = match opened {
                    None => format!("{tag} BAD no mailbox open\r\n"),
                    Some((index, _)) => match search_uids(
                        &state.mailboxes[index],
                        args,
                        state.advertises("X-GM-EXT-1").then_some(&state.gmail),
                    ) {
                        Some(uids) => {
                            let uids: Vec<String> = uids.iter().map(u32::to_string).collect();
                            let sep = if uids.is_empty() { "" } else { " " };
                            format!(
                                "* SEARCH{sep}{}\r\n{tag} OK UID SEARCH done\r\n",
                                uids.join(" ")
                            )
                        }
                        None => format!("{tag} BAD bad search criteria\r\n"),
                    },
                };
                drop(state);
                send(stream, &reply)?;
            }
            "UID FETCH" => {
                let mut state = state.lock().unwrap();
                state.fetches.push(args.to_string());
                let reply = match opened {
                    None => format!("{tag} BAD no mailbox open\r\n"),
                    Some((index, _)) => {
                        let (set, items) = args.split_once(' ').unwrap_or((args, ""));
                        let mailbox = &mut state.mailboxes[index];
                        let mut reply = String::new();
                        let mut count = 0;
                        for (seq, message) in mailbox.messages.iter_mut().enumerate() {
                            if in_uid_set(set, message.uid) {
                                count += 1;
                                reply.push_str(&fetch_response(seq + 1, message, items));
                            }
                        }
                        state.fetch_uid_counts.push(count);
                        reply.push_str(&format!("{tag} OK UID FETCH done\r\n"));
                        reply
                    }
                };
                drop(state);
                send(stream, &reply)?;
            }
            "STORE" | "UID STORE" => {
                let mut state = state.lock().unwrap();
                state.stores.push(args.to_string());
                let reply = match opened {
                    None => format!("{tag} BAD no mailbox open\r\n"),
                    Some((_, true)) => format!("{tag} NO mailbox is read-only\r\n"),
                    Some((index, false)) => {
                        let by_uid = verb == "UID STORE";
                        match store(&mut state.mailboxes[index], args, by_uid) {
                            Some(mut reply) => {
                                reply.push_str(&format!("{tag} OK {verb} done\r\n"));
                                reply
                            }
                            None => format!("{tag} BAD bad store arguments\r\n"),
                        }
                    }
                };
                drop(state);
                send(stream, &reply)?;
            }
            "COPY" | "UID COPY" | "MOVE" | "UID MOVE" => {
                let mut state = state.lock().unwrap();
                let moving = verb.ends_with("MOVE");
                let reply = if moving && !state.advertises("MOVE") {
                    format!("{tag} BAD MOVE not advertised\r\n")
                } else {
                    match opened {
                        None => format!("{tag} BAD no mailbox open\r\n"),
                        Some((_, true)) if moving => {
                            format!("{tag} NO mailbox is read-only\r\n")
                        }
                        Some((index, _)) => {
                            let by_uid = verb.starts_with("UID");
                            copy_or_move(&mut state, index, args, by_uid, moving, &tag)
                        }
                    }
                };
                drop(state);
                send(stream, &reply)?;
            }
            "EXPUNGE" | "UID EXPUNGE" => {
                let mut state = state.lock().unwrap();
                let by_uid = verb == "UID EXPUNGE";
                let reply = if by_uid && !state.advertises("UIDPLUS") {
                    format!("{tag} BAD UID EXPUNGE needs UIDPLUS\r\n")
                } else {
                    match opened {
                        None => format!("{tag} BAD no mailbox open\r\n"),
                        Some((_, true)) => format!("{tag} NO mailbox is read-only\r\n"),
                        Some((index, false)) => {
                            let set = by_uid.then_some(args.trim());
                            let mut reply = expunge(&mut state.mailboxes[index], set);
                            reply.push_str(&format!("{tag} OK {verb} done\r\n"));
                            reply
                        }
                    }
                };
                drop(state);
                send(stream, &reply)?;
            }
            "APPEND" => {
                let reply = append(stream, state, args, &tag)?;
                send(stream, &reply)?;
            }
            "LOGOUT" => {
                send(stream, &format!("* BYE bye\r\n{tag} OK LOGOUT done\r\n"))?;
                return None;
            }
            _ => {
                send(stream, &format!("{tag} BAD unknown command\r\n"))?;
            }
        }
    }
}

/// Handles `APPEND <mailbox> (<flags>) {n}` with a synchronising literal:
/// answers `+ ready`, reads `n` bytes and the rest of the line, and stores
/// the message in the named mailbox with UID = last UID + 1 and the given
/// flags. Returns the tagged reply: `OK [APPENDUID <uidvalidity> <uid>]`
/// with `UIDPLUS`, plain `OK` without, `NO [TRYCREATE]` for a missing
/// mailbox.
fn append(
    stream: &mut (impl Read + Write),
    state: &Mutex<State>,
    args: &str,
    tag: &str,
) -> Option<String> {
    let name = imap_strings(args).into_iter().next().unwrap_or_default();
    let flags: Vec<String> = match (args.find('('), args.find(')')) {
        (Some(open), Some(close)) if open < close => args[open + 1..close]
            .split_whitespace()
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    };
    let size: usize = args
        .rsplit_once('{')
        .and_then(|(_, rest)| rest.strip_suffix('}'))
        .and_then(|n| n.parse().ok())?;
    let refusal = state.lock().unwrap().refuse_append.take();
    if let Some((true, reply)) = &refusal {
        return Some(format!("{tag} {reply}\r\n"));
    }
    send(stream, "+ ready\r\n")?;
    let mut bytes = vec![0u8; size];
    stream.read_exact(&mut bytes).ok()?;
    let _rest = read_line(stream)?;
    if let Some((false, reply)) = &refusal {
        return Some(format!("{tag} {reply}\r\n"));
    }
    let raw = String::from_utf8_lossy(&bytes).into_owned();
    let mut state = state.lock().unwrap();
    let Some(index) = state.mailboxes.iter().position(|m| m.name == name) else {
        return Some(format!("{tag} NO [TRYCREATE] no such mailbox\r\n"));
    };
    let subject = raw_header(&raw, "Subject").unwrap_or_default();
    let mailbox = &mut state.mailboxes[index];
    let uid = mailbox.messages.last().map_or(1, |m| m.uid + 1);
    let uidvalidity = mailbox.uidvalidity;
    mailbox.messages.push(FakeMessage {
        uid,
        from_name: None,
        from: "",
        to: "",
        subject,
        body: "",
        internal_date: "22-Oct-2026 09:00:00 +0000",
        flags: flags.clone(),
        source: Some(raw.clone()),
    });
    let code = if state.advertises("UIDPLUS") {
        format!("[APPENDUID {uidvalidity} {uid}] ")
    } else {
        String::new()
    };
    state.appended.push((name, flags, raw));
    Some(format!("{tag} OK {code}APPEND done\r\n"))
}

/// The value of the first header `name` in `raw`, unfolded (CRLF before
/// whitespace removed), surrounding whitespace trimmed.
pub fn raw_header(raw: &str, name: &str) -> Option<String> {
    let head = raw.split("\r\n\r\n").next().unwrap_or(raw);
    let unfolded = head.replace("\r\n ", " ").replace("\r\n\t", "\t");
    unfolded.split("\r\n").find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name)
            .then(|| value.trim().to_string())
    })
}

/// Copies (or moves) the messages of the open mailbox `index` that `args`
/// (`<set> <mailbox>`) names to the target mailbox. Each copy keeps its
/// flags and gets UID = the target's last UID + 1. With `UIDPLUS`, reports
/// `COPYUID`: untagged before the `EXPUNGE` responses for a move, in the
/// tagged `OK` for a copy. A missing target answers `NO [TRYCREATE]`.
fn copy_or_move(
    state: &mut State,
    index: usize,
    args: &str,
    by_uid: bool,
    moving: bool,
    tag: &str,
) -> String {
    let Some((set, rest)) = args.split_once(' ') else {
        return format!("{tag} BAD bad arguments\r\n");
    };
    let Some(name) = imap_strings(rest).into_iter().next() else {
        return format!("{tag} BAD bad arguments\r\n");
    };
    let Some(target) = state.mailboxes.iter().position(|m| m.name == name) else {
        return format!("{tag} NO [TRYCREATE] no such mailbox\r\n");
    };
    let chosen: Vec<(usize, FakeMessage)> = state.mailboxes[index]
        .messages
        .iter()
        .enumerate()
        .filter(|(seq, m)| {
            let id = if by_uid { m.uid } else { *seq as u32 + 1 };
            in_uid_set(set, id)
        })
        .map(|(seq, m)| (seq, m.clone()))
        .collect();
    let mut old = Vec::new();
    let mut new = Vec::new();
    for (_, message) in &chosen {
        let destination = &mut state.mailboxes[target];
        let uid = destination.messages.last().map_or(1, |m| m.uid + 1);
        old.push(message.uid.to_string());
        new.push(uid.to_string());
        destination.messages.push(FakeMessage {
            uid,
            ..message.clone()
        });
    }
    let copyuid = (state.advertises("UIDPLUS") && !chosen.is_empty()).then(|| {
        format!(
            "[COPYUID {} {} {}] ",
            state.mailboxes[target].uidvalidity,
            old.join(","),
            new.join(",")
        )
    });
    let verb = if moving { "MOVE" } else { "COPY" };
    if !moving {
        return format!("{tag} OK {}{verb} done\r\n", copyuid.unwrap_or_default());
    }
    let mut reply = String::new();
    if let Some(code) = copyuid {
        reply.push_str(&format!("* OK {code}moved\r\n"));
    }
    // Highest sequence number first, so each stays valid as it is sent.
    for (seq, _) in chosen.iter().rev() {
        state.mailboxes[index].messages.remove(*seq);
        reply.push_str(&format!("* {} EXPUNGE\r\n", seq + 1));
    }
    reply.push_str(&format!("{tag} OK {verb} done\r\n"));
    reply
}

/// Removes the `\Deleted` messages of `mailbox`, all of them, or with
/// `uid_set` only those in it. Returns one `EXPUNGE` response per message.
fn expunge(mailbox: &mut FakeMailbox, uid_set: Option<&str>) -> String {
    let mut reply = String::new();
    let mut seq = 0;
    mailbox.messages.retain(|m| {
        seq += 1;
        let deleted = m.flags.iter().any(|f| f.eq_ignore_ascii_case("\\Deleted"));
        let named = uid_set.is_none_or(|set| in_uid_set(set, m.uid));
        if deleted && named {
            // Later messages move down by one with each removal.
            reply.push_str(&format!("* {seq} EXPUNGE\r\n"));
            seq -= 1;
            false
        } else {
            true
        }
    });
    reply
}

/// Applies `STORE` arguments (`<set> [+|-]FLAGS[.SILENT] (<flags>)`) to
/// `mailbox`; `set` holds UIDs if `by_uid`, else sequence numbers. Returns
/// the untagged responses: unless `.SILENT`, one `FETCH` with `UID` and
/// `FLAGS` per message whose flags changed. `None` if malformed.
fn store(mailbox: &mut FakeMailbox, args: &str, by_uid: bool) -> Option<String> {
    let mut parts = args.splitn(3, ' ');
    let set = parts.next()?;
    let item = parts.next()?.to_ascii_uppercase();
    let list = parts.next()?.trim();
    let list = list.strip_prefix('(')?.strip_suffix(')')?;
    let given: Vec<String> = list.split_whitespace().map(str::to_string).collect();
    let (item, silent) = match item.strip_suffix(".SILENT") {
        Some(item) => (item.to_string(), true),
        None => (item, false),
    };
    let mut reply = String::new();
    for (seq, message) in mailbox.messages.iter_mut().enumerate() {
        let id = if by_uid { message.uid } else { seq as u32 + 1 };
        if !in_uid_set(set, id) {
            continue;
        }
        let before = message.flags.clone();
        let has = |flags: &[String], f: &str| flags.iter().any(|x| x.eq_ignore_ascii_case(f));
        match item.as_str() {
            "+FLAGS" => {
                for flag in &given {
                    if !has(&message.flags, flag) {
                        message.flags.push(flag.clone());
                    }
                }
            }
            "-FLAGS" => message.flags.retain(|f| !has(&given, f)),
            "FLAGS" => message.flags = given.clone(),
            _ => return None,
        }
        if !silent && message.flags != before {
            reply.push_str(&format!(
                "* {} FETCH (UID {} FLAGS ({}))\r\n",
                seq + 1,
                message.uid,
                message.flags.join(" ")
            ));
        }
    }
    Some(reply)
}

/// Parses an IMAP `date` (`05-Oct-2026` or `5-Oct-2026`) as
/// `(year, month, day)`.
fn imap_date(text: &str) -> Option<(u32, u32, u32)> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let mut parts = text.trim().split('-');
    let day = parts.next()?.parse().ok()?;
    let month = parts.next()?.to_ascii_lowercase();
    let month = MONTHS.iter().position(|m| *m == month)? as u32 + 1;
    let year = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((year, month, day))
}

/// Evaluates `UID SEARCH` criteria (RFC 3501 subset: `CHARSET`, `ALL`,
/// `FROM`, `TO`, `SUBJECT`, `TEXT`, `SINCE`, `BEFORE`, `UNSEEN`, `FLAGGED`,
/// combined with AND) against `mailbox`, and, when `gmail` holds the
/// scripted results (the server advertises `X-GM-EXT-1`), `X-GM-RAW`.
/// `None` for anything else.
fn search_uids(
    mailbox: &FakeMailbox,
    args: &str,
    gmail: Option<&Vec<(String, String, Vec<u32>)>>,
) -> Option<Vec<u32>> {
    let tokens = imap_strings(args);
    let mut tokens = tokens.iter();
    type Test = Box<dyn Fn(&FakeMessage) -> bool>;
    let mut tests: Vec<Test> = Vec::new();
    let contains =
        |haystack: &str, needle: &str| haystack.to_lowercase().contains(&needle.to_lowercase());
    while let Some(key) = tokens.next() {
        match key.to_ascii_uppercase().as_str() {
            "CHARSET" => {
                let charset = tokens.next()?;
                if !charset.eq_ignore_ascii_case("UTF-8")
                    && !charset.eq_ignore_ascii_case("US-ASCII")
                {
                    return None;
                }
            }
            "ALL" => {}
            "FROM" => {
                let s = tokens.next()?.clone();
                tests.push(Box::new(move |m| contains(&m.header_from(), &s)));
            }
            "TO" => {
                let s = tokens.next()?.clone();
                tests.push(Box::new(move |m| contains(m.to, &s)));
            }
            "SUBJECT" => {
                let s = tokens.next()?.clone();
                tests.push(Box::new(move |m| contains(&m.subject, &s)));
            }
            "TEXT" => {
                let s = tokens.next()?.clone();
                tests.push(Box::new(move |m| contains(&m.raw(), &s)));
            }
            "SINCE" => {
                let day = imap_date(tokens.next()?)?;
                tests.push(Box::new(move |m| m.day() >= day));
            }
            "BEFORE" => {
                let day = imap_date(tokens.next()?)?;
                tests.push(Box::new(move |m| m.day() < day));
            }
            "UNSEEN" => tests.push(Box::new(|m| !m.flags.iter().any(|f| f == "\\Seen"))),
            "FLAGGED" => tests.push(Box::new(|m| m.flags.iter().any(|f| f == "\\Flagged"))),
            "X-GM-RAW" => {
                let query = tokens.next()?;
                let uids: Vec<u32> = gmail?
                    .iter()
                    .filter(|(name, q, _)| name == mailbox.name && q == query)
                    .flat_map(|(_, _, uids)| uids.iter().copied())
                    .collect();
                tests.push(Box::new(move |m| uids.contains(&m.uid)));
            }
            _ => return None,
        }
    }
    Some(
        mailbox
            .messages
            .iter()
            .filter(|m| tests.iter().all(|t| t(m)))
            .map(|m| m.uid)
            .collect(),
    )
}

/// Whether `uid` is in the IMAP sequence set `set` (`n`, `a:b`, `*`).
fn in_uid_set(set: &str, uid: u32) -> bool {
    set.split(',').any(|part| {
        let bound = |s: &str| {
            if s == "*" {
                Some(u32::MAX)
            } else {
                s.parse().ok()
            }
        };
        match part.split_once(':') {
            Some((a, b)) => match (bound(a), bound(b)) {
                (Some(a), Some(b)) => (a.min(b)..=a.max(b)).contains(&uid),
                _ => false,
            },
            None => bound(part) == Some(uid),
        }
    })
}

/// An IMAP quoted string.
fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// An IMAP literal.
fn literal(text: &str) -> String {
    format!("{{{}}}\r\n{text}", text.len())
}

/// An envelope address list with one address.
fn address(name: Option<&str>, addr: &str) -> String {
    let (mailbox, host) = addr.split_once('@').unwrap_or((addr, ""));
    let name = name.map_or_else(|| "NIL".to_string(), quote);
    format!("(({name} NIL {} {}))", quote(mailbox), quote(host))
}

/// Splits fetch items at top-level spaces, keeping `BODY[...]` sections
/// and parenthesised lists whole.
fn fetch_items(items: &str) -> Vec<String> {
    let items = items.trim();
    let items = items
        .strip_prefix('(')
        .and_then(|i| i.strip_suffix(')'))
        .unwrap_or(items);
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    for c in items.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            ' ' if depth == 0 => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// One `FETCH` response for `message`. A non-peek body fetch sets `\Seen`,
/// as RFC 3501 says a server must; the fake does so even after `EXAMINE`,
/// so a client that forgets `.PEEK` shows up in tests.
fn fetch_response(seq: usize, message: &mut FakeMessage, items: &str) -> String {
    let mut parts = vec![format!("UID {}", message.uid)];
    let mut seen = false;
    for item in fetch_items(items) {
        let upper = item.to_ascii_uppercase();
        if upper == "UID" {
            continue;
        } else if upper == "FLAGS" {
            // Filled in below, after any implicit \Seen.
        } else if upper == "INTERNALDATE" {
            parts.push(format!("INTERNALDATE {}", quote(message.internal_date)));
        } else if upper == "ENVELOPE" {
            parts.push(format!(
                "ENVELOPE (NIL {} {from} {from} {from} {} NIL NIL NIL NIL)",
                quote(&message.subject),
                address(None, message.to),
                from = address(message.from_name, message.from),
            ));
        } else if let Some(section) = upper.strip_prefix("BODY.PEEK") {
            parts.push(format!("BODY{section} {}", literal(&message.raw())));
        } else if upper.starts_with("BODY[") || upper == "RFC822" {
            seen = true;
            parts.push(format!("{item} {}", literal(&message.raw())));
        } else if upper == "RFC822.TEXT" {
            seen = true;
            parts.push(format!("RFC822.TEXT {}", literal(message.body)));
        } else if upper == "RFC822.HEADER" {
            parts.push(format!("RFC822.HEADER {}", literal(&message.raw())));
        }
    }
    if seen && !message.flags.iter().any(|f| f == "\\Seen") {
        message.flags.push("\\Seen".to_string());
    }
    if fetch_items(items)
        .iter()
        .any(|i| i.eq_ignore_ascii_case("FLAGS"))
        || seen
    {
        parts.insert(1, format!("FLAGS ({})", message.flags.join(" ")));
    }
    format!("* {seq} FETCH ({})\r\n", parts.join(" "))
}

/// Whether `LOGIN` arguments name a known login with a fake Credential.
fn login_accepted(args: &str) -> bool {
    let fields = imap_strings(args);
    matches!(fields.as_slice(), [login, credential]
        if KNOWN_LOGINS.contains(&login.as_str())
            && (credential == CREDENTIAL || credential == BROKER_CREDENTIAL))
}

/// Splits IMAP atoms and quoted strings (with `\` escapes).
fn imap_strings(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = args.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c == ' ' {
            chars.next();
        } else if c == '"' {
            chars.next();
            let mut field = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => field.extend(chars.next()),
                    '"' => break,
                    c => field.push(c),
                }
            }
            out.push(field);
        } else {
            let mut field = String::new();
            while let Some(&c) = chars.peek() {
                if c == ' ' {
                    break;
                }
                field.push(c);
                chars.next();
            }
            out.push(field);
        }
    }
    out
}

/// The fake Account `login` on `localhost:port`.
pub fn account(login: &str, port: u16, tls: TlsMode) -> Account {
    Account {
        login: login.to_string(),
        host: "localhost".to_string(),
        port,
        tls,
        credential: Credential::new(CREDENTIAL),
    }
}

/// A Mandate covering `accounts` with the given Grants.
pub fn mandate_for(accounts: Vec<Account>, grants: Vec<Grant>) -> Mandate {
    Mandate {
        name: "test-mandate".to_string(),
        id: "0123abcd".to_string(),
        created: 1_700_000_000,
        accounts,
        policy: Policy { grants },
    }
}

/// A Mandate for the fake Account on `port` with the given Grants.
pub fn mandate(port: u16, tls: TlsMode, grants: Vec<Grant>) -> Mandate {
    mandate_for(vec![account(LOGIN, port, tls)], grants)
}

/// A Grant of `operation` on `mailboxes` of the fake Account.
pub fn grant(operation: Operation, mailboxes: &[&str]) -> Grant {
    grant_on(operation, LOGIN, mailboxes)
}

/// A Grant of `operation` on `mailboxes` of the Account `login`.
pub fn grant_on(operation: Operation, login: &str, mailboxes: &[&str]) -> Grant {
    Grant {
        operation,
        account: login.to_string(),
        mailboxes: mailboxes.iter().map(|m| (*m).into()).collect(),
    }
}

/// Seals `mandate` under a fresh token and stores it in `store`.
pub fn store_mandate(store: &InMemoryStore, mandate: &Mandate) -> AgentToken {
    let token = AgentToken::mint();
    let record = seal::seal(&token, mandate);
    store.put(&record.key, &record.value).unwrap();
    token
}

/// Runs `agent-mail <args>` in-process with `token` in `AGENT_MAIL_TOKEN`,
/// trusting only the fake server's CA.
pub fn run(
    args: &[&str],
    token: Option<&str>,
    store: &InMemoryStore,
    server: &FakeImap,
) -> RunOutput {
    run_trusting(args, token, store, &server.ca)
}

/// Like [`run`], trusting only the CA `ca`.
pub fn run_trusting(
    args: &[&str],
    token: Option<&str>,
    store: &InMemoryStore,
    ca: &CertificateDer<'static>,
) -> RunOutput {
    let mut env = HashMap::new();
    if let Some(token) = token {
        env.insert(agent_mail::TOKEN_VAR.to_string(), token.to_string());
    }
    let argv = std::iter::once("agent-mail").chain(args.iter().copied());
    let context = Context::trusting_only(store, vec![ca.clone()]);
    agent_mail::run(argv, &env, &context)
}

/// Asserts neither the token nor the Credential appears in the output (AC10).
pub fn assert_no_secrets(output: &RunOutput, token: Option<&str>) {
    for stream in [&output.stdout, &output.stderr] {
        assert!(
            !stream.contains(CREDENTIAL),
            "output contains the Credential"
        );
        if let Some(token) = token.filter(|t| !t.is_empty()) {
            assert!(!stream.contains(token), "output contains the agent token");
        }
    }
}

/// Parses stderr as the structured error object and returns `error`.
pub fn error_of(output: &RunOutput) -> serde_json::Value {
    let value: serde_json::Value = serde_json::from_str(&output.stderr)
        .unwrap_or_else(|e| panic!("stderr is not JSON ({e}): {:?}", output.stderr));
    value["error"].clone()
}

/// Runs `agent-mail <args>` in-process with `env`, talking through
/// `terminal`, trusting only `roots` and connecting `routes`
/// (`host`, `port`, fake server port on localhost).
pub fn run_with_terminal(
    args: &[&str],
    env: &HashMap<String, String>,
    store: &dyn MandateStore,
    terminal: &dyn agent_mail::terminal::Terminal,
    roots: Vec<CertificateDer<'static>>,
    routes: &[(&str, u16, u16)],
) -> RunOutput {
    let argv = std::iter::once("agent-mail").chain(args.iter().copied());
    let mut context = Context::trusting_only(store, roots).with_terminal(terminal);
    for (host, port, to_port) in routes {
        context = context.with_route(host, *port, "localhost", *to_port);
    }
    agent_mail::run(argv, env, &context)
}

/// Every Agent Token-shaped string (`amt_` plus 43 base64url characters)
/// in `text`.
pub fn tokens_in(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(seal::TOKEN_PREFIX) {
        let body: String = rest[at + seal::TOKEN_PREFIX.len()..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if body.len() == 43 {
            found.push(format!("{}{body}", seal::TOKEN_PREFIX));
        }
        rest = &rest[at + seal::TOKEN_PREFIX.len()..];
    }
    found
}

/// Whether `needle` occurs in `haystack`.
pub fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// A store that counts every access, wrapping an [`InMemoryStore`].
#[derive(Default)]
pub struct CountingStore {
    pub inner: InMemoryStore,
    accesses: AtomicUsize,
}

impl CountingStore {
    /// Number of `get`, `put` and `delete` calls so far.
    pub fn accesses(&self) -> usize {
        self.accesses.load(Ordering::SeqCst)
    }
}

impl MandateStore for CountingStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, agent_mail::store::StoreError> {
        self.accesses.fetch_add(1, Ordering::SeqCst);
        self.inner.get(key)
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), agent_mail::store::StoreError> {
        self.accesses.fetch_add(1, Ordering::SeqCst);
        self.inner.put(key, value)
    }

    fn delete(&self, key: &[u8]) -> Result<bool, agent_mail::store::StoreError> {
        self.accesses.fetch_add(1, Ordering::SeqCst);
        self.inner.delete(key)
    }

    fn entries(&self) -> Result<Vec<(Vec<u8>, Vec<u8>)>, agent_mail::store::StoreError> {
        self.accesses.fetch_add(1, Ordering::SeqCst);
        MandateStore::entries(&self.inner)
    }
}

/// Mandate A of the list and delete fixtures: `work-inbox`, `aaaa1111`,
/// created 2026-10-20T08:00:00Z, the fake Account on `port`.
pub fn mandate_a(port: u16) -> Mandate {
    Mandate {
        name: "work-inbox".to_string(),
        id: "aaaa1111".to_string(),
        created: 1_792_483_200,
        accounts: vec![account(LOGIN, port, TlsMode::Implicit)],
        policy: Policy {
            grants: vec![
                grant(Operation::List, &["INBOX", "Archive"]),
                grant(Operation::Search, &["INBOX"]),
            ],
        },
    }
}

/// Mandate B of the list and delete fixtures: `other-archive`, `bbbb2222`,
/// created 2026-10-21T09:30:15Z, the second fake Account on `port`.
pub fn mandate_b(port: u16) -> Mandate {
    Mandate {
        name: "other-archive".to_string(),
        id: "bbbb2222".to_string(),
        created: 1_792_575_015,
        accounts: vec![account(OTHER_LOGIN, port, TlsMode::Implicit)],
        policy: Policy {
            grants: vec![grant_on(Operation::List, OTHER_LOGIN, &["Archive"])],
        },
    }
}

/// The token hash as the record format defines it: base64url without
/// padding of SHA-256 over the token's 32 secret bytes.
pub fn token_hash(token: &str) -> String {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let body = token
        .strip_prefix(seal::TOKEN_PREFIX)
        .expect("token prefix");
    let secret = URL_SAFE_NO_PAD.decode(body).expect("token body");
    URL_SAFE_NO_PAD.encode(ring::digest::digest(&ring::digest::SHA256, &secret))
}

/// A fresh audit log path in a new temporary directory, never the user's
/// data directory. Nothing exists there yet.
pub fn temp_audit_log() -> std::path::PathBuf {
    agent_mail::audit::AuditLog::temporary()
        .path()
        .expect("temporary logs have a path")
        .to_path_buf()
}

/// Like [`run_trusting`], appending audit entries to `log`.
pub fn run_logged(
    args: &[&str],
    token: Option<&str>,
    store: &dyn MandateStore,
    ca: &CertificateDer<'static>,
    log: &std::path::Path,
) -> RunOutput {
    let mut env = HashMap::new();
    if let Some(token) = token {
        env.insert(agent_mail::TOKEN_VAR.to_string(), token.to_string());
    }
    let argv = std::iter::once("agent-mail").chain(args.iter().copied());
    let context = Context::trusting_only(store, vec![ca.clone()])
        .with_audit_log(agent_mail::audit::AuditLog::at(log));
    agent_mail::run(argv, &env, &context)
}

/// The audit log's lines, without their `\n`; empty if there is no log.
/// Panics if the file does not end in `\n`.
pub fn audit_lines(log: &std::path::Path) -> Vec<String> {
    let text = match std::fs::read_to_string(log) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => panic!("cannot read the audit log: {e}"),
    };
    assert!(
        text.is_empty() || text.ends_with('\n'),
        "audit log does not end in a newline"
    );
    text.lines().map(str::to_string).collect()
}

/// A Mandate with short ID `id` and name `name`, covering `logins` on the
/// fake server at `port` (implicit TLS) with the given Grants.
pub fn mandate_with_id(
    id: &str,
    name: &str,
    logins: &[&str],
    port: u16,
    grants: Vec<Grant>,
) -> Mandate {
    Mandate {
        name: name.to_string(),
        id: id.to_string(),
        created: 1_792_483_200,
        accounts: logins
            .iter()
            .map(|login| account(login, port, TlsMode::Implicit))
            .collect(),
        policy: Policy { grants },
    }
}

/// Runs `agent-mail <args>` in-process with the raw string `tokens` in
/// `AGENT_MAIL_TOKEN` (or the variable unset for `None`), trusting only
/// `server`'s CA and appending audit entries to `log`.
pub fn run_raw_tokens(
    args: &[&str],
    tokens: Option<&str>,
    store: &dyn MandateStore,
    server: &FakeImap,
    log: &std::path::Path,
) -> RunOutput {
    run_logged(args, tokens, store, &server.ca, log)
}

/// Feeds the JSON-RPC `lines` (one per request, without `\n`) to
/// `agent_mail::mcp::serve` with `env` and `context`, and returns everything
/// it wrote. Panics if `serve` fails.
pub fn mcp_raw(lines: &[String], env: &HashMap<String, String>, context: &Context<'_>) -> String {
    let mut input = String::new();
    for line in lines {
        input.push_str(line);
        input.push('\n');
    }
    let mut output = Vec::new();
    agent_mail::mcp::serve(input.as_bytes(), &mut output, env, context).expect("serve");
    String::from_utf8(output).expect("serve writes UTF-8")
}

/// Every output line of [`mcp_raw`], parsed; panics unless each line is
/// one JSON object followed by `\n`.
pub fn mcp_parse(raw: &str) -> Vec<serde_json::Value> {
    assert!(
        raw.is_empty() || raw.ends_with('\n'),
        "output does not end in a newline: {raw:?}"
    );
    raw.lines()
        .map(|line| {
            let value: serde_json::Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("output line is not JSON ({e}): {line:?}"));
            assert!(value.is_object(), "output line is not an object: {line}");
            value
        })
        .collect()
}

/// Runs `lines` through `agent_mail::mcp::serve` with the raw string
/// `tokens` in `AGENT_MAIL_TOKEN` (unset for `None`), the store `store`,
/// trusting only `ca` and appending audit entries to `log`, and returns
/// the parsed response lines.
pub fn mcp_lines(
    lines: &[String],
    tokens: Option<&str>,
    store: &dyn MandateStore,
    ca: &CertificateDer<'static>,
    log: &std::path::Path,
) -> Vec<serde_json::Value> {
    let mut env = HashMap::new();
    if let Some(tokens) = tokens {
        env.insert(agent_mail::TOKEN_VAR.to_string(), tokens.to_string());
    }
    let context = Context::trusting_only(store, vec![ca.clone()])
        .with_audit_log(agent_mail::audit::AuditLog::at(log));
    mcp_parse(&mcp_raw(lines, &env, &context))
}

/// [`mcp_lines`] with `extra` variables (such as proxy variables) added
/// to the server's environment.
pub fn mcp_lines_with(
    lines: &[String],
    tokens: Option<&str>,
    extra: &[(&str, &str)],
    store: &dyn MandateStore,
    ca: &CertificateDer<'static>,
    log: &std::path::Path,
) -> Vec<serde_json::Value> {
    let mut env: HashMap<String, String> = extra
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    if let Some(tokens) = tokens {
        env.insert(agent_mail::TOKEN_VAR.to_string(), tokens.to_string());
    }
    let context = Context::trusting_only(store, vec![ca.clone()])
        .with_audit_log(agent_mail::audit::AuditLog::at(log));
    mcp_parse(&mcp_raw(lines, &env, &context))
}

/// [`run_logged`] with `extra` variables (such as proxy variables) added to
/// the environment.
pub fn run_logged_with(
    args: &[&str],
    token: Option<&str>,
    extra: &[(&str, &str)],
    store: &dyn MandateStore,
    ca: &CertificateDer<'static>,
    log: &std::path::Path,
) -> RunOutput {
    let mut env: HashMap<String, String> = extra
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    if let Some(token) = token {
        env.insert(agent_mail::TOKEN_VAR.to_string(), token.to_string());
    }
    let argv = std::iter::once("agent-mail").chain(args.iter().copied());
    let context = Context::trusting_only(store, vec![ca.clone()])
        .with_audit_log(agent_mail::audit::AuditLog::at(log));
    agent_mail::run(argv, &env, &context)
}

/// The `initialize` request (id 1) and the `notifications/initialized`
/// notification that open a session.
pub fn mcp_opening() -> Vec<String> {
    vec![
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "test-host", "version": "0" }
            }
        })
        .to_string(),
        serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string(),
    ]
}

/// A `tools/call` request line for `name` with `arguments`.
pub fn mcp_call(id: u64, name: &str, arguments: serde_json::Value) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments }
    })
    .to_string()
}
