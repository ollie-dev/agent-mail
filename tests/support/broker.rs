//! An in-memory duplex standing in for the Broker's pipe: the server end
//! runs `agent_mail::broker::serve_session` with a `Caller` the test sets,
//! the client end is driven by a script on another thread. All values here
//! are fake.

use std::io::{self, Read, Write};
use std::sync::mpsc::{Receiver, Sender, channel};

use agent_mail::Context;
use agent_mail::broker::{self, Caller};
use serde_json::{Value, json};

/// One end of the duplex: reads the chunks the other end wrote, in order,
/// and sees the end of the stream once the other end is dropped.
pub struct End {
    rx: Receiver<Vec<u8>>,
    tx: Sender<Vec<u8>>,
    pending: Vec<u8>,
    at: usize,
}

impl Read for End {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.at == self.pending.len() {
            match self.rx.recv() {
                Ok(chunk) => {
                    self.pending = chunk;
                    self.at = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let n = buf.len().min(self.pending.len() - self.at);
        buf[..n].copy_from_slice(&self.pending[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

impl Write for End {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.tx
            .send(buf.to_vec())
            .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Two connected ends.
pub fn duplex() -> (End, End) {
    let (a_tx, b_rx) = channel();
    let (b_tx, a_rx) = channel();
    let end = |rx, tx| End {
        rx,
        tx,
        pending: Vec::new(),
        at: 0,
    };
    (end(a_rx, a_tx), end(b_rx, b_tx))
}

/// The client end of a session, with every reply frame it received.
pub struct Client {
    end: End,
    /// Every reply frame body received, in order.
    pub received: Vec<Vec<u8>>,
}

impl Client {
    /// Sends `request` as one frame.
    pub fn send(&mut self, request: &Value) {
        self.send_body(&serde_json::to_vec(request).unwrap());
    }

    /// Sends `body` as one frame.
    pub fn send_body(&mut self, body: &[u8]) {
        let _ = broker::write_frame(&mut self.end, body);
    }

    /// Sends raw bytes, framing nothing.
    pub fn send_bytes(&mut self, bytes: &[u8]) {
        let _ = self.end.write_all(bytes);
    }

    /// The next reply frame's body as raw text, or `None` once the server
    /// closed the connection.
    pub fn recv_text(&mut self) -> Option<String> {
        match broker::read_frame(&mut self.end) {
            Ok(Some(body)) => {
                self.received.push(body.to_vec());
                Some(String::from_utf8(body.to_vec()).expect("replies are UTF-8"))
            }
            Ok(None) => None,
            Err(_) => panic!("the reply frame is malformed"),
        }
    }

    /// The next reply frame as JSON, or `None` once the server closed.
    pub fn recv(&mut self) -> Option<Value> {
        self.recv_text()
            .map(|text| serde_json::from_str(&text).expect("replies are JSON"))
    }

    /// Sends `request` and returns the reply.
    pub fn call(&mut self, request: &Value) -> Value {
        self.send(request);
        self.recv().expect("the server closed without replying")
    }

    /// Sends the matching `hello` and checks the `ok` reply.
    pub fn hello(&mut self) {
        let reply = self.call(&hello(broker::VERSION));
        assert_eq!(reply, json!({"ok": {"version": broker::VERSION}}));
    }

    /// Whether the server has closed the connection: no further frame.
    pub fn closed(&mut self) -> bool {
        self.recv_text().is_none()
    }
}

/// The `hello` request for `version`.
pub fn hello(version: &str) -> Value {
    json!({"hello": {"version": version}})
}

/// A `run` request for `command` with the raw `tokens`, no proxy.
pub fn run(command: Value, tokens: Option<&str>) -> Value {
    json!({"run": {"command": command, "tokens": tokens, "proxy": {}}})
}

/// A caller with SID `sid` at integrity `integrity`.
pub fn caller(sid: &str, integrity: u32) -> Caller {
    Caller {
        sid: sid.to_string(),
        integrity,
    }
}

/// Runs one Broker session for `caller` against `context`: the server on
/// this thread, `script` on another with the client end. Returns what the
/// script returned, after the server has finished.
pub fn session<R: Send>(
    caller: Caller,
    context: &Context<'_>,
    script: impl FnOnce(&mut Client) -> R + Send,
) -> R {
    let (server_end, client_end) = duplex();
    std::thread::scope(|scope| {
        let script = scope.spawn(move || {
            let mut client = Client {
                end: client_end,
                received: Vec::new(),
            };
            script(&mut client)
        });
        {
            let mut server_end = server_end;
            let _ = broker::serve_session(&mut server_end, |_| Ok(caller), context);
        }
        match script.join() {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    })
}

// ------------------------------------------------ Broker clients (job 026)

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use agent_mail::audit::AuditLog;
use agent_mail::client::{Connector, Transport};
use agent_mail::core::CoreError;
use agent_mail::store::InMemoryStore;
use rustls::pki_types::CertificateDer;

/// What a fake Broker serves each connection with: its store, the caller
/// it sees, the fake IMAP servers it trusts and routes to, and its log.
#[derive(Clone)]
pub struct BrokerSetup {
    /// The Broker's store.
    pub store: Arc<InMemoryStore>,
    /// The caller every connection comes from.
    pub caller: Caller,
    /// CAs to trust.
    pub roots: Vec<CertificateDer<'static>>,
    /// `(host, port, fake server port on localhost)` routes.
    pub routes: Vec<(String, u16, u16)>,
    /// The Broker's audit log.
    pub audit: AuditLog,
}

impl BrokerSetup {
    /// A Broker with `store`, seeing `caller`, logging to a fresh file.
    pub fn new(store: Arc<InMemoryStore>, caller: Caller) -> Self {
        Self {
            store,
            caller,
            roots: Vec::new(),
            routes: Vec::new(),
            audit: AuditLog::temporary(),
        }
    }

    /// Also trusting `ca` and connecting `host:port` to `to_port`.
    pub fn route(
        mut self,
        ca: &CertificateDer<'static>,
        host: &str,
        port: u16,
        to_port: u16,
    ) -> Self {
        self.roots.push(ca.clone());
        self.routes.push((host.to_string(), port, to_port));
        self
    }

    /// Serves one connection on `end` with `serve_session`.
    fn serve(&self, mut end: End) {
        let mut context = Context::trusting_only(&*self.store, self.roots.clone())
            .with_audit_log(self.audit.clone());
        for (host, port, to_port) in &self.routes {
            context = context.with_route(host, *port, "localhost", *to_port);
        }
        let caller = self.caller.clone();
        let _ = broker::serve_session(&mut end, |_| Ok(caller), &context);
    }
}

/// The client end of one fake connection, keeping a copy of every byte the
/// client wrote.
struct Recording {
    end: End,
    sent: Arc<Mutex<Vec<u8>>>,
}

impl Read for Recording {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.end.read(buf)
    }
}

impl Write for Recording {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.sent.lock().unwrap().extend_from_slice(buf);
        self.end.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

type Server = Arc<dyn Fn(End) + Send + Sync>;

/// A connector to a fake Broker: each connection gets its own in-memory
/// pipe whose server end runs on a new thread. Counts the connections and
/// records what the client sent on each.
pub struct FakeBroker {
    server: Option<Server>,
    connections: AtomicUsize,
    sent: Mutex<Vec<Arc<Mutex<Vec<u8>>>>>,
}

impl FakeBroker {
    fn with(server: Option<Server>) -> Self {
        Self {
            server,
            connections: AtomicUsize::new(0),
            sent: Mutex::new(Vec::new()),
        }
    }

    /// A Broker running `serve_session` as `setup` describes.
    pub fn serving(setup: BrokerSetup) -> Self {
        Self::with(Some(Arc::new(move |end| setup.serve(end))))
    }

    /// A Broker whose server end runs `script` instead.
    pub fn scripted(script: impl Fn(&mut End) + Send + Sync + 'static) -> Self {
        Self::with(Some(Arc::new(move |mut end| script(&mut end))))
    }

    /// No Broker: every connection fails as a missing pipe does.
    pub fn absent() -> Self {
        Self::with(None)
    }

    /// Connections opened, or attempted for [`FakeBroker::absent`].
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    /// Every frame the client sent, per connection, parsed as JSON.
    pub fn requests(&self) -> Vec<Vec<Value>> {
        self.sent_bytes()
            .iter()
            .map(|bytes| {
                let mut rest = &bytes[..];
                let mut frames = Vec::new();
                while let Ok(Some(body)) = broker::read_frame(&mut rest) {
                    frames.push(serde_json::from_slice(&body).expect("requests are JSON"));
                }
                frames
            })
            .collect()
    }

    /// Every byte the client sent, per connection.
    pub fn sent_bytes(&self) -> Vec<Vec<u8>> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.lock().unwrap().clone())
            .collect()
    }
}

impl Connector for FakeBroker {
    fn connect(&self) -> Result<Box<dyn Transport + '_>, CoreError> {
        self.connections.fetch_add(1, Ordering::SeqCst);
        let Some(server) = self.server.clone() else {
            return Err(CoreError::BrokerUnavailable);
        };
        let (server_end, client_end) = duplex();
        std::thread::spawn(move || server(server_end));
        let sent = Arc::new(Mutex::new(Vec::new()));
        self.sent.lock().unwrap().push(sent.clone());
        Ok(Box::new(Recording {
            end: client_end,
            sent,
        }))
    }
}

/// Reads one frame on a scripted server end, or `None` at the end.
pub fn read_request(end: &mut End) -> Option<Value> {
    match broker::read_frame(end) {
        Ok(Some(body)) => serde_json::from_slice(&body).ok(),
        _ => None,
    }
}

/// Writes `reply` as one frame on a scripted server end.
pub fn write_reply(end: &mut End, reply: &Value) {
    let _ = broker::write_frame(end, &serde_json::to_vec(reply).unwrap());
}

/// A Windows test pipe served by `serve_on` on its own thread until
/// dropped; its server is this test process, running as this user.
#[cfg(windows)]
pub struct TestPipe {
    /// The pipe's name, for `AGENT_MAIL_TEST_PIPE`.
    pub name: String,
    stop: Arc<agent_mail::broker::windows::StopSignal>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(windows)]
impl TestPipe {
    /// A fresh pipe name nobody serves.
    pub fn unused_name() -> String {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        format!(
            r"\\.\pipe\agent-mail-client-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        )
    }

    /// Serves `store`, logging to `audit` and connecting through `network`,
    /// and returns once the pipe accepts clients.
    pub fn start(
        store: Arc<InMemoryStore>,
        audit: AuditLog,
        network: agent_mail::imap::Network,
    ) -> Self {
        use agent_mail::broker::windows as win;
        let name = Self::unused_name();
        let stop = Arc::new(win::StopSignal::new().unwrap());
        let thread = {
            let (name, stop) = (name.clone(), stop.clone());
            std::thread::spawn(move || {
                let _ = win::serve_on(&name, &*store, &network, &audit, &stop);
            })
        };
        for _ in 0..200 {
            if win::connect(&name).is_ok() {
                return Self {
                    name,
                    stop,
                    thread: Some(thread),
                };
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!("the test pipe did not start");
    }
}

#[cfg(windows)]
impl Drop for TestPipe {
    fn drop(&mut self) {
        self.stop.signal();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
