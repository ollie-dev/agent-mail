//! Minimal in-process HTTP CONNECT and SOCKS5 proxies on real TCP ports on
//! 127.0.0.1. Each records the targets asked for and any credentials
//! received, and can be told to refuse a host. All values here are fake.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

/// What a proxy saw on one accepted connection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seen {
    /// `CONNECT` target for the HTTP proxy, `host:port`; for SOCKS5 the
    /// domain name and port as `host:port`. Empty if none was asked.
    pub target: String,
    /// The `Proxy-Authorization` header value (HTTP), if any.
    pub authorization: Option<String>,
    /// SOCKS5 only: the methods the client offered.
    pub methods: Vec<u8>,
    /// SOCKS5 only: the address type of the CONNECT request.
    pub address_type: Option<u8>,
    /// SOCKS5 only: the RFC 1929 username and password, if sent.
    pub credentials: Option<(String, String)>,
}

#[derive(Default)]
struct Shared {
    seen: Vec<Seen>,
    refused: Vec<String>,
}

/// Which protocol a [`FakeProxy`] speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// HTTP CONNECT.
    Connect,
    /// SOCKS5, accepting method 0x00 or 0x02 and any credentials.
    Socks5,
}

/// An in-process proxy.
pub struct FakeProxy {
    pub port: u16,
    shared: Arc<Mutex<Shared>>,
}

impl FakeProxy {
    /// Starts a proxy speaking `protocol` on an ephemeral port.
    pub fn start(protocol: Protocol) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake proxy");
        let port = listener.local_addr().unwrap().port();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let state = shared.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let state = state.clone();
                thread::spawn(move || match protocol {
                    Protocol::Connect => serve_connect(stream, &state),
                    Protocol::Socks5 => serve_socks5(stream, &state),
                });
            }
        });
        Self { port, shared }
    }

    /// Refuses tunnels to `host` (any port): `403` or SOCKS5 reply `0x02`.
    pub fn refuse(&self, host: &str) {
        self.shared.lock().unwrap().refused.push(host.to_string());
    }

    /// What every accepted connection asked for, in order.
    pub fn seen(&self) -> Vec<Seen> {
        self.shared.lock().unwrap().seen.clone()
    }

    /// The URL of this proxy, e.g. `http://127.0.0.1:1234`.
    pub fn url(&self, scheme: &str) -> String {
        format!("{scheme}://127.0.0.1:{}", self.port)
    }
}

fn record(state: &Mutex<Shared>, seen: &Seen) -> usize {
    let mut state = state.lock().unwrap();
    state.seen.push(seen.clone());
    state.seen.len() - 1
}

fn update(state: &Mutex<Shared>, index: usize, seen: &Seen) {
    state.lock().unwrap().seen[index] = seen.clone();
}

fn is_refused(state: &Mutex<Shared>, host: &str) -> bool {
    state.lock().unwrap().refused.iter().any(|h| h == host)
}

fn serve_connect(mut client: TcpStream, state: &Mutex<Shared>) {
    let mut seen = Seen::default();
    let index = record(state, &seen);
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > 16 * 1024 || client.read_exact(&mut byte).is_err() {
            return;
        }
        head.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&head).into_owned();
    let mut lines = head.split("\r\n");
    let request = lines.next().unwrap_or_default();
    let mut parts = request.split(' ');
    if parts.next() != Some("CONNECT") {
        let _ = client.write_all(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n");
        return;
    }
    seen.target = parts.next().unwrap_or_default().to_string();
    seen.authorization = lines.find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("proxy-authorization")
            .then(|| value.trim().to_string())
    });
    update(state, index, &seen);
    let host = seen
        .target
        .rsplit_once(':')
        .map(|(h, _)| h)
        .unwrap_or_default();
    if is_refused(state, host) {
        let _ = client.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n");
        return;
    }
    let Ok(upstream) = TcpStream::connect(&seen.target) else {
        let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n");
        return;
    };
    if client
        .write_all(b"HTTP/1.1 200 Connection established\r\nVia: fake\r\n\r\n")
        .is_err()
    {
        return;
    }
    pipe(client, upstream);
}

fn serve_socks5(mut client: TcpStream, state: &Mutex<Shared>) {
    let mut seen = Seen::default();
    let index = record(state, &seen);
    let mut head = [0u8; 2];
    if client.read_exact(&mut head).is_err() || head[0] != 5 {
        return;
    }
    let mut methods = vec![0u8; usize::from(head[1])];
    if client.read_exact(&mut methods).is_err() {
        return;
    }
    seen.methods = methods.clone();
    update(state, index, &seen);
    let method = if methods.contains(&0x02) {
        0x02
    } else if methods.contains(&0x00) {
        0x00
    } else {
        let _ = client.write_all(&[5, 0xFF]);
        return;
    };
    if client.write_all(&[5, method]).is_err() {
        return;
    }
    if method == 0x02 {
        let Some(credentials) = read_rfc1929(&mut client) else {
            return;
        };
        seen.credentials = Some(credentials);
        update(state, index, &seen);
        if client.write_all(&[1, 0]).is_err() {
            return;
        }
    }
    let mut request = [0u8; 4];
    if client.read_exact(&mut request).is_err() || request[..3] != [5, 1, 0] {
        return;
    }
    seen.address_type = Some(request[3]);
    let host = match request[3] {
        3 => {
            let mut len = [0u8; 1];
            if client.read_exact(&mut len).is_err() {
                return;
            }
            let mut name = vec![0u8; usize::from(len[0])];
            if client.read_exact(&mut name).is_err() {
                return;
            }
            String::from_utf8_lossy(&name).into_owned()
        }
        1 => {
            let mut ip = [0u8; 4];
            if client.read_exact(&mut ip).is_err() {
                return;
            }
            std::net::Ipv4Addr::from(ip).to_string()
        }
        _ => return,
    };
    let mut port = [0u8; 2];
    if client.read_exact(&mut port).is_err() {
        return;
    }
    let port = u16::from_be_bytes(port);
    seen.target = format!("{host}:{port}");
    update(state, index, &seen);
    if is_refused(state, &host) {
        let _ = client.write_all(&[5, 0x02, 0, 1, 0, 0, 0, 0, 0, 0]);
        return;
    }
    let Ok(upstream) = TcpStream::connect((host.as_str(), port)) else {
        let _ = client.write_all(&[5, 0x05, 0, 1, 0, 0, 0, 0, 0, 0]);
        return;
    };
    // Bound address as a domain name, to exercise the variable-length reply.
    let mut reply = vec![5, 0, 0, 3, 9];
    reply.extend_from_slice(b"fake.test");
    reply.extend_from_slice(&[0, 0]);
    if client.write_all(&reply).is_err() {
        return;
    }
    pipe(client, upstream);
}

fn read_rfc1929(client: &mut TcpStream) -> Option<(String, String)> {
    let mut version = [0u8; 2];
    client.read_exact(&mut version).ok()?;
    if version[0] != 1 {
        return None;
    }
    let mut user = vec![0u8; usize::from(version[1])];
    client.read_exact(&mut user).ok()?;
    let mut len = [0u8; 1];
    client.read_exact(&mut len).ok()?;
    let mut password = vec![0u8; usize::from(len[0])];
    client.read_exact(&mut password).ok()?;
    Some((
        String::from_utf8(user).ok()?,
        String::from_utf8(password).ok()?,
    ))
}

/// Copies bytes both ways until either side closes.
fn pipe(client: TcpStream, upstream: TcpStream) {
    let (Ok(mut client_read), Ok(mut upstream_read)) = (client.try_clone(), upstream.try_clone())
    else {
        return;
    };
    let mut client_write = client;
    let mut upstream_write = upstream;
    let up = thread::spawn(move || {
        let _ = std::io::copy(&mut client_read, &mut upstream_write);
        let _ = upstream_write.shutdown(Shutdown::Write);
    });
    let _ = std::io::copy(&mut upstream_read, &mut client_write);
    let _ = client_write.shutdown(Shutdown::Write);
    let _ = up.join();
}
