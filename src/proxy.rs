//! Reaching the IMAP server through a proxy, as inside the Claude Code
//! sandbox: the proxy setting read from the environment, and hand-written
//! HTTP CONNECT and SOCKS5 (RFC 1928, RFC 1929) client handshakes.
//!
//! A proxy URL is untrusted input and may carry proxy credentials, so
//! neither the URL nor the credentials ever appear in errors, `Debug`
//! output, logs or results. Only the name of the variable is reported.

use std::collections::HashMap;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use zeroize::Zeroizing;

/// The variables a proxy is read from, in order; the first non-empty one
/// wins. `HTTP_PROXY` and `NO_PROXY` are ignored.
pub const PROXY_VARS: [&str; 4] = ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"];

/// The most bytes of a CONNECT reply read before giving up.
const MAX_CONNECT_REPLY: usize = 8 * 1024;

/// The proxy protocol, picked by the URL's scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyKind {
    /// HTTP CONNECT (`http://` or no scheme).
    Connect,
    /// SOCKS5 (`socks5://` or `socks5h://`).
    Socks5,
}

/// A proxy to tunnel IMAP connections through. Its `Debug` output never
/// shows the address or the credentials.
#[derive(Clone)]
pub struct Proxy {
    kind: ProxyKind,
    host: String,
    port: u16,
    auth: Option<(Zeroizing<String>, Zeroizing<String>)>,
    /// The variable the URL was read from, e.g. `HTTPS_PROXY`.
    variable: &'static str,
}

impl std::fmt::Debug for Proxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Proxy({:?}, redacted)", self.kind)
    }
}

impl Proxy {
    /// The protocol.
    pub fn kind(&self) -> ProxyKind {
        self.kind
    }

    /// The variable the proxy URL was read from, e.g. `HTTPS_PROXY`. Never
    /// its value.
    pub fn variable(&self) -> &'static str {
        self.variable
    }
}

/// The proxy setting found in the environment.
#[derive(Clone, Default)]
pub enum ProxySetting {
    /// No proxy variable is set (or all are empty): connect directly.
    #[default]
    Direct,
    /// Tunnel through this proxy.
    Through(Proxy),
    /// The variable named here holds a value that is not a usable proxy
    /// URL. Nothing is connected to.
    Invalid(&'static str),
}

impl std::fmt::Debug for ProxySetting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct => f.write_str("Direct"),
            Self::Through(proxy) => write!(f, "Through({proxy:?})"),
            Self::Invalid(variable) => write!(f, "Invalid({variable})"),
        }
    }
}

impl ProxySetting {
    /// The setting `env` selects: the first non-empty of [`PROXY_VARS`].
    pub fn from_env(env: &HashMap<String, String>) -> Self {
        for variable in PROXY_VARS {
            match env.get(variable).map(String::as_str) {
                None | Some("") => continue,
                Some(value) => {
                    return match parse(value, variable) {
                        Some(proxy) => Self::Through(proxy),
                        None => Self::Invalid(variable),
                    };
                }
            }
        }
        Self::Direct
    }

    /// The variable in effect and whether it holds a usable proxy URL;
    /// `None` when connecting directly. Never the value.
    pub fn summary(&self) -> Option<(&'static str, bool)> {
        match self {
            Self::Direct => None,
            Self::Through(proxy) => Some((proxy.variable, true)),
            Self::Invalid(variable) => Some((variable, false)),
        }
    }
}

/// Why the tunnel was not opened. Never carries a secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelError {
    /// The proxy could not be reached, sent a malformed reply, or the target
    /// host cannot be sent to it.
    Connection,
    /// The proxy answered, and refused the target host because of its
    /// allowlist (CONNECT `403`, SOCKS5 reply `0x02`).
    Refused,
    /// The proxy asked for credentials, or rejected the ones sent (CONNECT
    /// `407`, SOCKS5 method `0xFF` or a failed RFC 1929 login).
    Auth,
}

/// Parses a proxy URL: `[scheme://][user[:password]@]host[:port][/]`.
/// `None` for anything else, including an unknown scheme.
fn parse(value: &str, variable: &'static str) -> Option<Proxy> {
    let (kind, rest, default_port) = match value.split_once("://") {
        Some((scheme, rest)) => match scheme.to_ascii_lowercase().as_str() {
            "http" => (ProxyKind::Connect, rest, 80),
            "socks5" | "socks5h" => (ProxyKind::Socks5, rest, 1080),
            _ => return None,
        },
        None => (ProxyKind::Connect, value, 80),
    };
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    if rest.contains(['/', '?', '#']) {
        return None;
    }
    let (userinfo, address) = match rest.rsplit_once('@') {
        Some((userinfo, address)) => (Some(userinfo), address),
        None => (None, rest),
    };
    let auth = match userinfo {
        None => None,
        Some(userinfo) => {
            let (user, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
            let user = percent_decode(user)?;
            let password = percent_decode(password)?;
            if user.is_empty() {
                return None;
            }
            // RFC 1929 carries each in at most 255 bytes.
            if kind == ProxyKind::Socks5 && (user.len() > 255 || password.len() > 255) {
                return None;
            }
            Some((user, password))
        }
    };
    let (host, port) = split_host_port(address)?;
    let port = match port {
        None => default_port,
        Some(port) => {
            if port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            match port.parse::<u16>() {
                Ok(0) | Err(_) => return None,
                Ok(port) => port,
            }
        }
    };
    Some(Proxy {
        kind,
        host,
        port,
        auth,
        variable,
    })
}

/// Splits `host[:port]` or `[v6]:port`; the host is checked to be a plain
/// host name or IP address.
fn split_host_port(address: &str) -> Option<(String, Option<&str>)> {
    if let Some(rest) = address.strip_prefix('[') {
        let (inner, after) = rest.split_once(']')?;
        inner.parse::<std::net::Ipv6Addr>().ok()?;
        let port = match after {
            "" => None,
            _ => Some(after.strip_prefix(':')?),
        };
        return Some((inner.to_string(), port));
    }
    let (host, port) = match address.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (address, None),
    };
    if !is_plain_host(host) {
        return None;
    }
    Some((host.to_string(), port))
}

/// Whether `host` is a non-empty host name or IPv4 address: letters,
/// digits, `-`, `.` and `_` only, at most 253 bytes.
fn is_plain_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_'))
}

/// Percent-decodes `text` into UTF-8, `None` on a bad escape.
fn percent_decode(text: &str) -> Option<Zeroizing<String>> {
    let bytes = text.as_bytes();
    let mut out = Zeroizing::new(Vec::with_capacity(bytes.len()));
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let decoded = std::str::from_utf8(&out).ok()?;
    Some(Zeroizing::new(decoded.to_string()))
}

/// Opens a TCP stream to `host:port` through `proxy`. The host is sent to
/// the proxy unresolved. On success the stream is positioned right after
/// the proxy's reply, so the first byte read is the server's.
pub async fn tunnel(proxy: &Proxy, host: &str, port: u16) -> Result<TcpStream, TunnelError> {
    // The host goes into a request line or a length-prefixed field; refuse
    // anything that is not a plain host name or IP address (no CR, LF,
    // spaces or colons outside an IPv6 address).
    let ipv6 = host.parse::<std::net::Ipv6Addr>().is_ok();
    if !ipv6 && !is_plain_host(host) {
        return Err(TunnelError::Connection);
    }
    let mut stream = TcpStream::connect((proxy.host.as_str(), proxy.port))
        .await
        .map_err(|_| TunnelError::Connection)?;
    match proxy.kind {
        ProxyKind::Connect => connect_handshake(&mut stream, proxy, host, ipv6, port).await?,
        ProxyKind::Socks5 => socks5_handshake(&mut stream, proxy, host, port).await?,
    }
    Ok(stream)
}

async fn connect_handshake(
    stream: &mut TcpStream,
    proxy: &Proxy,
    host: &str,
    ipv6: bool,
    port: u16,
) -> Result<(), TunnelError> {
    let authority = if ipv6 {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let mut request = Zeroizing::new(format!(
        "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n"
    ));
    if let Some((user, password)) = &proxy.auth {
        let pair = Zeroizing::new(format!("{}:{}", user.as_str(), password.as_str()));
        let encoded = Zeroizing::new(STANDARD.encode(pair.as_bytes()));
        request.push_str("Proxy-Authorization: Basic ");
        request.push_str(&encoded);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|_| TunnelError::Connection)?;

    // One byte at a time, so nothing past the headers is consumed: in
    // STARTTLS mode the server's greeting may follow right away.
    let mut reply = Vec::with_capacity(256);
    while !reply.ends_with(b"\r\n\r\n") {
        if reply.len() >= MAX_CONNECT_REPLY {
            return Err(TunnelError::Connection);
        }
        let byte = stream
            .read_u8()
            .await
            .map_err(|_| TunnelError::Connection)?;
        reply.push(byte);
    }
    let status_line = reply
        .split(|&b| b == b'\r')
        .next()
        .ok_or(TunnelError::Connection)?;
    status_result(status_code(status_line))
}

/// What a CONNECT reply status means. The reason phrase is never kept.
fn status_result(code: Option<u16>) -> Result<(), TunnelError> {
    match code {
        Some(200..=299) => Ok(()),
        // The host is not on the proxy's allowlist.
        Some(403) => Err(TunnelError::Refused),
        // Missing or wrong proxy credentials.
        Some(407) => Err(TunnelError::Auth),
        _ => Err(TunnelError::Connection),
    }
}

/// The status code of an `HTTP/1.x NNN reason` line.
fn status_code(line: &[u8]) -> Option<u16> {
    let line = std::str::from_utf8(line).ok()?;
    let mut parts = line.splitn(3, ' ');
    let version = parts.next()?;
    if !version.starts_with("HTTP/1.") {
        return None;
    }
    let code = parts.next()?;
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    code.parse().ok()
}

async fn socks5_handshake(
    stream: &mut TcpStream,
    proxy: &Proxy,
    host: &str,
    port: u16,
) -> Result<(), TunnelError> {
    let io = |_| TunnelError::Connection;
    // Offer exactly one method: username/password with credentials, else
    // none.
    let method = if proxy.auth.is_some() { 0x02 } else { 0x00 };
    stream.write_all(&[0x05, 0x01, method]).await.map_err(io)?;
    let mut choice = [0u8; 2];
    stream.read_exact(&mut choice).await.map_err(io)?;
    if choice[0] != 0x05 {
        return Err(TunnelError::Connection);
    }
    // No acceptable method: the proxy wants credentials we did not offer.
    if choice[1] == 0xFF {
        return Err(TunnelError::Auth);
    }
    if choice[1] != method {
        return Err(TunnelError::Connection);
    }

    if let Some((user, password)) = &proxy.auth {
        let mut request = Zeroizing::new(Vec::with_capacity(3 + user.len() + password.len()));
        request.push(0x01);
        request.push(u8::try_from(user.len()).map_err(|_| TunnelError::Connection)?);
        request.extend_from_slice(user.as_bytes());
        request.push(u8::try_from(password.len()).map_err(|_| TunnelError::Connection)?);
        request.extend_from_slice(password.as_bytes());
        stream.write_all(&request).await.map_err(io)?;
        let mut status = [0u8; 2];
        stream.read_exact(&mut status).await.map_err(io)?;
        if status[0] != 0x01 {
            return Err(TunnelError::Connection);
        }
        if status[1] != 0x00 {
            return Err(TunnelError::Auth);
        }
    }

    // CONNECT with address type DOMAINNAME: the proxy resolves the host.
    let name = host.as_bytes();
    let length = u8::try_from(name.len()).map_err(|_| TunnelError::Connection)?;
    let mut request = Vec::with_capacity(7 + name.len());
    request.extend_from_slice(&[0x05, 0x01, 0x00, 0x03, length]);
    request.extend_from_slice(name);
    request.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&request).await.map_err(io)?;

    let mut head = [0u8; 4];
    stream.read_exact(&mut head).await.map_err(io)?;
    if head[0] != 0x05 || head[2] != 0x00 {
        return Err(TunnelError::Connection);
    }
    match head[1] {
        0x00 => {}
        // Connection not allowed by ruleset.
        0x02 => return Err(TunnelError::Refused),
        _ => return Err(TunnelError::Connection),
    }
    // The bound address, then its port; read exactly, so nothing past the
    // reply is consumed.
    let address_length = match head[3] {
        0x01 => 4,
        0x04 => 16,
        0x03 => usize::from(stream.read_u8().await.map_err(io)?),
        _ => return Err(TunnelError::Connection),
    };
    let mut bound = vec![0u8; address_length + 2];
    stream.read_exact(&mut bound).await.map_err(io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    fn through(pairs: &[(&str, &str)]) -> Proxy {
        match ProxySetting::from_env(&env(pairs)) {
            ProxySetting::Through(proxy) => proxy,
            other => panic!("expected a proxy, got {other:?}"),
        }
    }

    #[test]
    fn first_non_empty_variable_wins() {
        let proxy = through(&[
            ("HTTPS_PROXY", ""),
            ("https_proxy", "http://127.0.0.1:3128"),
            ("ALL_PROXY", "socks5://127.0.0.1:1080"),
        ]);
        assert_eq!(proxy.kind, ProxyKind::Connect);
        assert_eq!((proxy.host.as_str(), proxy.port), ("127.0.0.1", 3128));
        let proxy = through(&[("all_proxy", "socks5h://proxy.test")]);
        assert_eq!(proxy.kind, ProxyKind::Socks5);
        assert_eq!((proxy.host.as_str(), proxy.port), ("proxy.test", 1080));
    }

    #[test]
    fn http_proxy_and_empty_values_are_ignored() {
        assert!(matches!(
            ProxySetting::from_env(&env(&[
                ("HTTP_PROXY", "http://127.0.0.1:1"),
                ("HTTPS_PROXY", ""),
                ("ALL_PROXY", ""),
                ("NO_PROXY", "*"),
            ])),
            ProxySetting::Direct
        ));
    }

    #[test]
    fn no_scheme_is_connect_with_port_80() {
        let proxy = through(&[("HTTPS_PROXY", "proxy.test/")]);
        assert_eq!(proxy.kind, ProxyKind::Connect);
        assert_eq!(proxy.port, 80);
        let proxy = through(&[("HTTPS_PROXY", "http://[::1]:8080")]);
        assert_eq!((proxy.host.as_str(), proxy.port), ("::1", 8080));
    }

    #[test]
    fn credentials_are_percent_decoded() {
        let proxy = through(&[("HTTPS_PROXY", "http://u%3Ax:pr%40xy@127.0.0.1:9")]);
        let (user, password) = proxy.auth.as_ref().unwrap();
        assert_eq!((user.as_str(), password.as_str()), ("u:x", "pr@xy"));
        assert!(!format!("{proxy:?}").contains("pr@xy"));
        assert!(!format!("{proxy:?}").contains("127.0.0.1"));
    }

    #[test]
    fn bad_urls_are_invalid_naming_the_variable() {
        for value in [
            "https://127.0.0.1:1",
            "ftp://127.0.0.1:1",
            "socks4://127.0.0.1:1",
            "http://",
            "http://:8080",
            "http://127.0.0.1:",
            "http://127.0.0.1:0",
            "http://127.0.0.1:65536",
            "http://127.0.0.1:80a",
            "http://127.0.0.1:80/path",
            "http://user:%zz@127.0.0.1:1",
            "http://:pw@127.0.0.1:1",
            "http://bad host:1",
            "http://[nothex]:1",
        ] {
            match ProxySetting::from_env(&env(&[("ALL_PROXY", value)])) {
                ProxySetting::Invalid(variable) => assert_eq!(variable, "ALL_PROXY", "{value}"),
                other => panic!("{value} gave {other:?}"),
            }
        }
    }

    #[test]
    fn status_codes_are_parsed_strictly() {
        assert_eq!(
            status_code(b"HTTP/1.1 200 Connection established"),
            Some(200)
        );
        assert_eq!(status_code(b"HTTP/1.0 403"), Some(403));
        assert_eq!(status_code(b"HTTP/2 200 OK"), None);
        assert_eq!(status_code(b"HTTP/1.1 20 OK"), None);
        assert_eq!(status_code(b"garbage"), None);
    }

    #[test]
    fn connect_statuses_map_to_tunnel_errors() {
        assert_eq!(status_result(Some(200)), Ok(()));
        assert_eq!(status_result(Some(403)), Err(TunnelError::Refused));
        assert_eq!(status_result(Some(407)), Err(TunnelError::Auth));
        for code in [400, 404, 429, 502] {
            assert_eq!(
                status_result(Some(code)),
                Err(TunnelError::Connection),
                "{code}"
            );
        }
        assert_eq!(status_result(None), Err(TunnelError::Connection));
    }

    #[test]
    fn summary_names_the_variable_only() {
        assert_eq!(ProxySetting::Direct.summary(), None);
        let setting = ProxySetting::from_env(&env(&[("https_proxy", "http://u:p@127.0.0.1:9")]));
        assert_eq!(setting.summary(), Some(("https_proxy", true)));
        let setting = ProxySetting::from_env(&env(&[("all_proxy", "ftp://127.0.0.1:1")]));
        assert_eq!(setting.summary(), Some(("all_proxy", false)));
    }

    #[test]
    fn hosts_with_line_breaks_are_never_sent() {
        let proxy = through(&[("HTTPS_PROXY", "http://127.0.0.1:1")]);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        for host in ["evil\r\nX: y", "a b", "host:1", ""] {
            assert_eq!(
                runtime.block_on(tunnel(&proxy, host, 993)).unwrap_err(),
                TunnelError::Connection
            );
        }
    }
}
