//! The Broker client (ADR-0004): on Windows, every CLI command and every
//! MCP tool call is sent to the Broker over its pipe instead of running
//! in-process. The client parses arguments, does terminal I/O and renders
//! the reply exactly as the in-process run would.
//!
//! Compiled on every OS and run over any `Read + Write` transport, so
//! tests can drive it against `serve_session` through an in-memory pipe.
//! Each command opens its own connection, which starts with `hello`.
//!
//! Every reply is untrusted: it is parsed into the protocol types, checked,
//! and its text escaped on `--text`. A reply that cannot be read, or a
//! connection that ends early, gives `broker_unavailable`.

use std::collections::HashMap;
use std::io::{Read, Write};

use serde::Deserialize;
use serde_json::Value;
use serde_json::value::RawValue;
use zeroize::{Zeroize, Zeroizing};

use crate::broker::{self, ProxyVars, Reply, Request, RunRequest, Secret, VERSION};
use crate::core::{Command, CoreError, TOKEN_VAR};
use crate::terminal::escape_controls;

/// Environment variable naming the pipe a test-support build of the
/// binary on Windows sends to, served by the test's own user. A release
/// build has no such variable.
#[cfg(any(test, feature = "test-support"))]
pub const TEST_PIPE_VAR: &str = "AGENT_MAIL_TEST_PIPE";

/// A connection to the Broker: any byte stream in both directions.
pub trait Transport: Read + Write {}

impl<T: Read + Write + ?Sized> Transport for T {}

/// Opens connections to the Broker.
pub trait Connector {
    /// A new connection to the Broker, already checked to be the Broker's.
    /// Fails with [`CoreError::BrokerUnavailable`] when there is none.
    fn connect(&self) -> Result<Box<dyn Transport + '_>, CoreError>;
}

/// The connector the binary uses: on Windows the Broker's pipe, checked to
/// be served by `NT SERVICE\agent-mail` (in a test-support build, the pipe
/// named by [`TEST_PIPE_VAR`] instead, served by this user); `None`
/// elsewhere, where every command runs in-process.
pub fn platform_connector() -> Option<Box<dyn Connector>> {
    #[cfg(windows)]
    {
        use crate::broker::windows::PipeConnector;
        #[cfg(any(test, feature = "test-support"))]
        if let Some(name) = std::env::var_os(TEST_PIPE_VAR)
            && let (Some(name), Ok(sid)) =
                (name.to_str(), crate::broker::windows::current_user_sid())
        {
            return Some(Box::new(PipeConnector::new(name, &sid)));
        }
        Some(Box::new(PipeConnector::broker()))
    }
    #[cfg(not(windows))]
    None
}

/// The Broker route of a [`crate::Context`]: how to reach the Broker, and
/// whether this client runs elevated.
#[derive(Clone, Copy)]
pub struct Route<'a> {
    connector: &'a dyn Connector,
    /// A fixed elevation status (test builds), or `None` for the process
    /// token's.
    elevation: Option<bool>,
}

impl<'a> Route<'a> {
    /// The route through `connector`, checking the process token's
    /// elevation.
    pub(crate) fn new(connector: &'a dyn Connector) -> Self {
        Self {
            connector,
            elevation: None,
        }
    }

    /// The same route with a fixed elevation status. Test builds only.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn with_elevation(self, elevated: bool) -> Self {
        Self {
            elevation: Some(elevated),
            ..self
        }
    }

    /// Whether this client runs elevated: on Windows, the process token's
    /// `TokenElevation`. A convenience check only; the Broker checks the
    /// caller's integrity itself.
    pub(crate) fn elevated(&self) -> bool {
        match self.elevation {
            Some(elevated) => elevated,
            #[cfg(windows)]
            None => broker::windows::process_elevated(),
            #[cfg(not(windows))]
            None => true,
        }
    }
}

/// Why a Broker call failed: an error found here, or the error the Broker
/// replied with, as its JSON body.
#[derive(Debug)]
pub(crate) enum Failure {
    /// Found by the client, such as `broker_unavailable`.
    Local(CoreError),
    /// The Broker's error body: an object with string `code` and `message`.
    Remote(Value),
}

impl From<CoreError> for Failure {
    fn from(error: CoreError) -> Self {
        Self::Local(error)
    }
}

impl Failure {
    /// The error as the CLI prints it on stderr.
    pub(crate) fn render(&self, text: bool) -> String {
        match self {
            Self::Local(error) => crate::render_error(error, text),
            Self::Remote(body) if text => format!(
                "error: {}: {}\n",
                escape_controls(body["code"].as_str().unwrap_or_default()),
                escape_controls(body["message"].as_str().unwrap_or_default())
            ),
            Self::Remote(body) => crate::json_line(&serde_json::json!({ "error": body })),
        }
    }
}

/// A checked reply to a request: the result or the error, and the
/// invalid-token warning, if any.
pub(crate) struct Answer {
    /// The result, as the JSON the CLI prints (a JSON string on `--text`).
    pub(crate) result: Result<Box<RawValue>, Value>,
    /// The warning object, as the CLI prints it under `warning`.
    pub(crate) warning: Option<Box<RawValue>>,
}

/// The parts of a warning the `--text` line shows.
#[derive(Deserialize)]
struct WarningText {
    code: String,
    message: String,
}

/// One connection to the Broker, past the version handshake.
pub(crate) struct Session<'c> {
    transport: Box<dyn Transport + 'c>,
}

impl<'c> Session<'c> {
    /// Connects through `route` and exchanges `hello`. A Broker of another
    /// version gives `broker_version_mismatch`, and nothing more is sent.
    pub(crate) fn open(route: &Route<'c>) -> Result<Self, CoreError> {
        let mut session = Self {
            transport: route.connector.connect()?,
        };
        let hello = Request::Hello {
            version: VERSION.to_string(),
        };
        let mut reply = session.exchange(&hello)?;
        if reply.error.as_ref().and_then(|e| e["code"].as_str()) == Some("broker_version_mismatch")
        {
            return Err(CoreError::BrokerVersionMismatch { broker: None });
        }
        match reply.ok.take() {
            Some(ok) if ok.version == VERSION => Ok(session),
            Some(ok) => Err(CoreError::BrokerVersionMismatch {
                broker: shown_version(&ok.version),
            }),
            None => Err(CoreError::BrokerUnavailable),
        }
    }

    /// Sends `request` and reads one reply frame.
    fn exchange(&mut self, request: &Request) -> Result<Reply, CoreError> {
        // Room for the largest frame up front, so a `create` request holding
        // Credentials is never reallocated and left behind unwiped.
        let mut body = Zeroizing::new(Vec::with_capacity(broker::MAX_FRAME));
        serde_json::to_writer(&mut *body, request).map_err(|_| CoreError::BrokerUnavailable)?;
        broker::write_frame(&mut self.transport, &body)
            .map_err(|_| CoreError::BrokerUnavailable)?;
        drop(body);
        let frame = match broker::read_frame(&mut self.transport) {
            Ok(Some(frame)) => frame,
            Ok(None) | Err(_) => return Err(CoreError::BrokerUnavailable),
        };
        serde_json::from_slice::<Reply>(&frame).map_err(|_| CoreError::BrokerUnavailable)
    }

    /// Sends `request` and returns the checked answer.
    pub(crate) fn call(&mut self, request: &Request) -> Result<Answer, CoreError> {
        let mut reply = self.exchange(request)?;
        let warning = reply.warning.take();
        if let Some(warning) = &warning
            && serde_json::from_str::<WarningText>(warning.get()).is_err()
        {
            return Err(CoreError::BrokerUnavailable);
        }
        let result = match (reply.result.take(), reply.error.take()) {
            (Some(result), None) => Ok(result),
            (None, Some(error)) if is_error_body(&error) => Err(error),
            _ => return Err(CoreError::BrokerUnavailable),
        };
        Ok(Answer { result, warning })
    }
}

/// Wipes a reply's raw JSON, such as a `create` result holding the new
/// Agent Token, from memory.
pub(crate) fn wipe(raw: Box<RawValue>) {
    let text: Box<str> = raw.into();
    text.into_string().zeroize();
}

/// The Broker's version for a message, if it is short and plain.
fn shown_version(version: &str) -> Option<String> {
    let plain = !version.is_empty()
        && version.len() <= 32
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'));
    plain.then(|| version.to_string())
}

/// Whether `error` is an object with a string `code` and `message`.
fn is_error_body(error: &Value) -> bool {
    error["code"].is_string() && error["message"].is_string()
}

/// The `run` request for `command` with the client's token and proxy
/// variables from `env`.
fn run_request(command: Command, env: &HashMap<String, String>, text: bool) -> Request {
    let get = |name: &str| env.get(name).map(|value| Secret::new(value.as_str()));
    Request::Run(Box::new(RunRequest {
        command,
        tokens: get(TOKEN_VAR),
        proxy: ProxyVars {
            https_upper: get("HTTPS_PROXY"),
            https_lower: get("https_proxy"),
            all_upper: get("ALL_PROXY"),
            all_lower: get("all_proxy"),
        },
        text,
    }))
}

/// Sends `command` to the Broker over a new connection.
pub(crate) fn call(
    route: &Route<'_>,
    command: Command,
    env: &HashMap<String, String>,
    text: bool,
) -> Result<Answer, CoreError> {
    let mut session = Session::open(route)?;
    session.call(&run_request(command, env, text))
}

/// Runs `command` on the Broker and renders it as [`crate::run_cli`] does
/// in-process: the warning first on stderr, then the result on stdout or
/// the error on stderr.
pub(crate) fn run_command(
    route: &Route<'_>,
    command: Command,
    env: &HashMap<String, String>,
    text: bool,
) -> crate::RunOutput {
    let answer = match call(route, command, env, text) {
        Ok(answer) => answer,
        Err(error) => return failed(String::new(), &Failure::Local(error), text),
    };
    let mut stderr = answer
        .warning
        .as_deref()
        .map(|w| render_warning(w, text))
        .unwrap_or_default();
    match answer.result {
        Ok(result) if text => match serde_json::from_str::<String>(result.get()) {
            Ok(stdout) => crate::RunOutput {
                stdout,
                stderr,
                code: 0,
            },
            Err(_) => failed(String::new(), &CoreError::BrokerUnavailable.into(), text),
        },
        Ok(result) => crate::RunOutput {
            stdout: format!("{}\n", result.get()),
            stderr,
            code: 0,
        },
        Err(body) => {
            stderr.push_str(&Failure::Remote(body).render(text));
            crate::RunOutput {
                stdout: String::new(),
                stderr,
                code: 1,
            }
        }
    }
}

/// The output of a failed run: `stderr` so far, then `failure`; exit 1.
pub(crate) fn failed(mut stderr: String, failure: &Failure, text: bool) -> crate::RunOutput {
    stderr.push_str(&failure.render(text));
    crate::RunOutput {
        stdout: String::new(),
        stderr,
        code: 1,
    }
}

/// The warning line, as [`crate::render_warning`] prints it.
fn render_warning(warning: &RawValue, text: bool) -> String {
    if text {
        let parsed: WarningText =
            serde_json::from_str(warning.get()).expect("checked in Session::call");
        format!(
            "warning: {}: {}\n",
            escape_controls(&parsed.code),
            escape_controls(&parsed.message)
        )
    } else {
        format!("{{\"warning\":{}}}\n", warning.get())
    }
}

/// The JSON lines of an MCP tool result for `command` run on the Broker:
/// the main line (result or error), the warning line, and whether it is
/// an error. Connection failures are tool errors too.
pub(crate) fn tool_lines(
    route: &Route<'_>,
    command: Command,
    env: &HashMap<String, String>,
) -> (String, Option<String>, bool) {
    match call(route, command, env, false) {
        Ok(answer) => {
            let warning = answer
                .warning
                .as_deref()
                .map(|w| format!("{{\"warning\":{}}}", w.get()));
            match answer.result {
                Ok(result) => (result.get().to_string(), warning, false),
                Err(body) => (
                    serde_json::json!({ "error": body }).to_string(),
                    warning,
                    true,
                ),
            }
        }
        Err(error) => {
            let line = crate::render_error(&error, false);
            (line.trim_end().to_string(), None, true)
        }
    }
}

/// The caller's own audit entries from the Broker, each the raw JSON of
/// its log line.
pub(crate) fn audit(route: &Route<'_>) -> Result<Vec<Box<RawValue>>, Failure> {
    #[derive(Deserialize)]
    struct Entries {
        entries: Vec<Box<RawValue>>,
    }
    let mut session = Session::open(route)?;
    let answer = session.call(&Request::Audit {})?;
    let result = answer.result.map_err(Failure::Remote)?;
    let entries: Entries =
        serde_json::from_str(result.get()).map_err(|_| CoreError::BrokerUnavailable)?;
    if entries.entries.iter().any(|e| !e.get().starts_with('{')) {
        return Err(CoreError::BrokerUnavailable.into());
    }
    Ok(entries.entries)
}
