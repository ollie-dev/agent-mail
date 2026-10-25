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
