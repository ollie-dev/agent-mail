//! The MCP adapter: `agent-mail mcp` as an MCP server over stdio.
//!
//! A small synchronous JSON-RPC 2.0 loop, one JSON object per line on the
//! input and the output, handling one request at a time. Each `tools/call`
//! checks its arguments with the CLI's validators, builds the same
//! [`Command`] the CLI builds and runs it through
//! [`core::execute_reporting`], so Mandate selection, Grant checks and the
//! audit entry happen on every call, exactly as on the CLI. Results are the
//! CLI's JSON lines. Nothing opened is kept between calls, and the adapter
//! never builds a Mandate management command.

use std::collections::HashMap;
use std::io::{BufRead, Write};

use serde_json::{Map, Value, json};

use crate::core::{self, Command, Context, Report, Warning};
use crate::flag::FlagAction;
use crate::search::{InvalidValue, Limit, Query};
use crate::terminal::escape_controls;

/// The protocol version answered when the client asks for one this server
/// does not know.
pub const LATEST_PROTOCOL_VERSION: &str = "2025-06-18";

/// Protocol versions this server speaks.
const SUPPORTED_PROTOCOL_VERSIONS: [&str; 2] = ["2025-06-18", "2025-03-26"];

/// JSON-RPC error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// Longest requested method or tool name echoed back in an error message.
const MAX_ECHOED_NAME: usize = 64;

const INSTRUCTIONS: &str = "Agent Mail gives you scoped access to the human's email accounts. \
Start with `whoami` to see your Mandates, their accounts and grants. Then call `list` for the \
mailboxes, `search` for messages, and pass the `ref` values `search` returns to `read`, `flag`, \
`move` and `draft`. Failures are tool errors whose structured content holds `error.code`. \
Mandates cannot be created or changed here; the human manages them with `agent-mail mandate` \
in their own terminal.";

/// Runs the MCP server: reads newline-delimited JSON-RPC messages from
/// `input` until EOF and writes one response line per request to `output`.
/// `env` is the server process's environment, captured once at start; it is
/// the only source of `AGENT_MAIL_TOKEN` and the proxy variables.
pub fn serve(
    mut input: impl BufRead,
    mut output: impl Write,
    env: &HashMap<String, String>,
    context: &Context<'_>,
) -> std::io::Result<()> {
    let mut line = Vec::new();
    loop {
        line.clear();
        if input.read_until(b'\n', &mut line)? == 0 {
            return Ok(());
        }
        let text = match std::str::from_utf8(&line) {
            Ok(text) => text.trim_end_matches(['\n', '\r']),
            Err(_) => {
                write_line(
                    &mut output,
                    &error_response(
                        Value::Null,
                        PARSE_ERROR,
                        "parse error: the line is not UTF-8",
                    ),
                )?;
                continue;
            }
        };
        if text.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_line(text, env, context) {
            write_line(&mut output, &response)?;
        }
    }
}

fn write_line(output: &mut impl Write, value: &Value) -> std::io::Result<()> {
    let mut text = serde_json::to_string(value).expect("responses always serialise");
    text.push('\n');
    output.write_all(text.as_bytes())?;
    output.flush()
}

/// Handles one input line; `None` for a notification, which gets no reply.
fn handle_line(text: &str, env: &HashMap<String, String>, context: &Context<'_>) -> Option<Value> {
    let message: Value = match serde_json::from_str(text) {
        Ok(message) => message,
        Err(_) => {
            return Some(error_response(
                Value::Null,
                PARSE_ERROR,
                "parse error: the line is not a JSON object",
            ));
        }
    };
    let Value::Object(message) = message else {
        return Some(error_response(
            Value::Null,
            INVALID_REQUEST,
            "invalid request: expected one JSON-RPC object per line",
        ));
    };
    let id = message.get("id").cloned();
    if let Some(id) = &id
        && !matches!(id, Value::String(_) | Value::Number(_))
    {
        return Some(error_response(
            Value::Null,
            INVALID_REQUEST,
            "invalid request: `id` must be a string or a number",
        ));
    }
    let method = match (message.get("jsonrpc"), message.get("method")) {
        (Some(Value::String(version)), Some(Value::String(method))) if version == "2.0" => method,
        _ => {
            // A malformed message; without an `id` there is nobody to
            // answer.
            return Some(error_response(
                id?,
                INVALID_REQUEST,
                "invalid request: expected `jsonrpc` \"2.0\" and a string `method`",
            ));
        }
    };
    // Notifications (no `id`) get no reply, whatever their method.
    let id = id?;
    let params = message.get("params");
    let result = match method.as_str() {
        "initialize" => Ok(initialize(params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_list() })),
        "tools/call" => call_tool(params, env, context),
        other => Err((
            METHOD_NOT_FOUND,
            format!("method not found: `{}`", echo_name(other)),
        )),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => error_response(id, code, &message),
    })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

/// `name` for an error message: control characters escaped, at most
/// [`MAX_ECHOED_NAME`] characters.
fn echo_name(name: &str) -> String {
    let mut short: String = name.chars().take(MAX_ECHOED_NAME).collect();
    if short.len() < name.len() {
        short.push_str("...");
    }
    escape_controls(&short)
}

fn initialize(params: Option<&Value>) -> Value {
    let requested = params
        .and_then(|p| p.get("protocolVersion"))
        .and_then(Value::as_str);
    let version = SUPPORTED_PROTOCOL_VERSIONS
        .iter()
        .find(|v| Some(**v) == requested)
        .copied()
        .unwrap_or(LATEST_PROTOCOL_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "agent-mail", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

/// The JSON type of a tool property.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    String,
    Boolean,
    Integer,
    StringArray,
}

/// One property of a tool's input schema.
struct Property {
    name: &'static str,
    kind: Kind,
    description: &'static str,
}

/// One tool: its name, description, properties and required properties.
struct Tool {
    name: &'static str,
    description: &'static str,
    properties: &'static [Property],
    required: &'static [&'static str],
}

const fn prop(name: &'static str, kind: Kind, description: &'static str) -> Property {
    Property {
        name,
        kind,
        description,
    }
}

const REF_DESCRIPTION: &str = "The message reference, exactly as the `search` tool returns it in `ref`: <account>/<mailbox>/<uidvalidity>/<uid>";
const ACCOUNT_DESCRIPTION: &str = "The account to work on, by its login. May be left out when the tokens reach only one account; with several, the call fails with `account_required` and lists them.";

/// The tools, in the order `tools/list` returns them. Mandate management is
/// deliberately absent.
const TOOLS: [Tool; 7] = [
    Tool {
        name: "whoami",
        description: "Show what the presented agent tokens allow: each Mandate's ID, name, accounts (login, host, port, tls) and grants (operation, account, mailboxes), in token order, and invalid_tokens, the positions in AGENT_MAIL_TOKEN of tokens that are not valid, and proxy, the proxy variable in effect and whether it is usable (omitted without one). Needs no grant, makes no connection and never shows a credential or token. Call this first.",
        properties: &[],
        required: &[],
    },
    Tool {
        name: "list",
        description: "List the mailboxes this agent may access on one account. Returns account and mailboxes, each with name; pass a name to `search` as mailbox or to `move` as to.",
        properties: &[prop("account", Kind::String, ACCOUNT_DESCRIPTION)],
        required: &[],
    },
    Tool {
        name: "search",
        description: "Search one mailbox this agent may search, newest first. Criteria combine with AND; string criteria are case-insensitive substring matches. With no criterion every message matches. Returns account, mailbox, total (how many matched), more (whether more matched than are shown) and messages, each with ref, date, from, subject and flags. Pass a message's ref to `read`, `flag`, `move` or `draft` (as reply_to).",
        properties: &[
            prop("account", Kind::String, ACCOUNT_DESCRIPTION),
            prop(
                "mailbox",
                Kind::String,
                "Mailbox to search, as `list` names it",
            ),
            prop(
                "from",
                Kind::String,
                "Messages whose From header contains this",
            ),
            prop("to", Kind::String, "Messages whose To header contains this"),
            prop(
                "subject",
                Kind::String,
                "Messages whose Subject contains this",
            ),
            prop(
                "text",
                Kind::String,
                "Messages whose headers or body contain this (the TEXT criterion)",
            ),
            prop(
                "since",
                Kind::String,
                "Messages received on or after this date (YYYY-MM-DD)",
            ),
            prop(
                "before",
                Kind::String,
                "Messages received before this date (YYYY-MM-DD)",
            ),
            prop("unseen", Kind::Boolean, "Only messages not yet seen"),
            prop("flagged", Kind::Boolean, "Only flagged messages"),
            prop(
                "gmail",
                Kind::String,
                "Search in Gmail's search syntax, e.g. \"from:alice has:attachment\". Works only on servers advertising Gmail's extensions (X-GM-EXT-1) and fails with `unsupported` elsewhere. Cannot be combined with from, to, subject, text, since, before, unseen or flagged, but works with mailbox and limit.",
            ),
            prop(
                "limit",
                Kind::Integer,
                "Return at most this many messages, 1 to 1000",
            ),
        ],
        required: &[],
    },
    Tool {
        name: "read",
        description: "Read one message by the reference `search` returned, without marking it seen. Needs a `read` grant on the reference's account and mailbox. Returns ref, account, mailbox, date, from, to, cc, subject, message_id, body (the plain-text part, or the HTML part converted to text), truncated, and attachments (name, type and size only). A body longer than 20,000 characters is cut, with truncated true and a marker at the end, unless full is true. If the mailbox was renumbered since the search, the read fails with `stale_reference`: search again for a fresh reference.",
        properties: &[
            prop("ref", Kind::String, REF_DESCRIPTION),
            prop(
                "full",
                Kind::Boolean,
                "Return the whole body; by default bodies are cut at 20,000 characters",
            ),
        ],
        required: &["ref"],
    },
    Tool {
        name: "flag",
        description: "Mark one message read or unread, or starred or unstarred, by the reference `search` returned. Needs a `flag` grant on the reference's account and mailbox. Give exactly one of set or clear. Only `seen` (or `\\Seen`, the message is read) and `flagged` (or `\\Flagged`, the message is starred) can be changed; names are case-insensitive, and any other flag fails with `unsupported_flag`. Returns ref, account, mailbox and flags (every flag of the message after the change). If the mailbox was renumbered since the search, nothing is changed and the call fails with `stale_reference`.",
        properties: &[
            prop("ref", Kind::String, REF_DESCRIPTION),
            prop(
                "set",
                Kind::String,
                "Set this flag: `seen` marks the message read, `flagged` stars it",
            ),
            prop(
                "clear",
                Kind::String,
                "Clear this flag: `seen` marks the message unread, `flagged` unstars it",
            ),
        ],
        required: &["ref"],
    },
    Tool {
        name: "move",
        description: "Move one message, by the reference `search` returned, to another mailbox of the same account. Needs a `move` grant on both the source mailbox and the target. Returns ref, account, mailbox (the source), target, and new_ref, the message's reference in the target, which may be null when the server does not report it (then search the target for it). A server with neither MOVE nor UIDPLUS fails with `move_unsupported`. If the mailbox was renumbered since the search, nothing is moved and the call fails with `stale_reference`.",
        properties: &[
            prop("ref", Kind::String, REF_DESCRIPTION),
            prop(
                "to",
                Kind::String,
                "The target mailbox, by its name as `list` returns it; `move` must be granted on it too",
            ),
        ],
        required: &["ref", "to"],
    },
    Tool {
        name: "draft",
        description: "Create a draft in the account's Drafts mailbox for the human to review and send, optionally as a reply. Needs a `draft` grant on the account; a reply (reply_to) also needs a `read` grant on the original's mailbox. Give subject for a new message, or reply_to with a reference exactly as `search` returns it, but not both: the draft then threads with the original and its subject is the original's with `Re: ` in front. The draft goes to the mailbox the server marks \\Drafts; with none marked, the call fails with `drafts_not_found`. Returns ref (null when the server does not report it), account, mailbox, to, subject and original. Drafts cannot be changed or removed afterwards; the human reviews and sends them in their own mail client.",
        properties: &[
            prop(
                "to",
                Kind::StringArray,
                "The recipients, each a bare address local@domain (no display names)",
            ),
            prop(
                "subject",
                Kind::String,
                "The subject of a new message; required without reply_to and not allowed with it",
            ),
            prop(
                "body",
                Kind::String,
                "The plain-text body; may span several lines",
            ),
            prop(
                "reply_to",
                Kind::String,
                "Reply to this message, by its reference exactly as `search` returns it in `ref`",
            ),
            prop(
                "account",
                Kind::String,
                "The account to work on, by its login; not allowed with reply_to, as a reply works on the reference's account. May be left out when the tokens reach only one account.",
            ),
        ],
        required: &["to", "body"],
    },
];

fn tool_list() -> Vec<Value> {
    TOOLS.iter().map(tool_definition).collect()
}

fn tool_definition(tool: &Tool) -> Value {
    let mut properties = Map::new();
    for p in tool.properties {
        let mut schema = match p.kind {
            Kind::String => json!({ "type": "string" }),
            Kind::Boolean => json!({ "type": "boolean" }),
            Kind::Integer => json!({ "type": "integer" }),
            Kind::StringArray => {
                json!({ "type": "array", "items": { "type": "string" }, "minItems": 1 })
            }
        };
        schema["description"] = json!(p.description);
        match (tool.name, p.name) {
            ("search", "limit") => {
                schema["minimum"] = json!(1);
                schema["maximum"] = json!(crate::search::MAX_LIMIT);
                schema["default"] = json!(20);
            }
            ("search", "mailbox") => schema["default"] = json!("INBOX"),
            _ => {}
        }
        properties.insert(p.name.to_string(), schema);
    }
    let mut schema = json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false,
    });
    if !tool.required.is_empty() {
        schema["required"] = json!(tool.required);
    }
    json!({
        "name": tool.name,
        "description": tool.description,
        "inputSchema": schema,
    })
}

type RpcError = (i64, String);

fn call_tool(
    params: Option<&Value>,
    env: &HashMap<String, String>,
    context: &Context<'_>,
) -> Result<Value, RpcError> {
    let Some(name) = params.and_then(|p| p.get("name")).and_then(Value::as_str) else {
        return Err((
            INVALID_PARAMS,
            "invalid params: tools/call needs a string `name`".to_string(),
        ));
    };
    let Some(tool) = TOOLS.iter().find(|t| t.name == name) else {
        return Err((
            INVALID_PARAMS,
            format!(
                "unknown tool `{}`; call tools/list for the tools this server offers. Mandates are not managed over MCP: the human manages them with the `agent-mail mandate` CLI in their own terminal.",
                echo_name(name)
            ),
        ));
    };
    // `_meta` and every other field besides `name` and `arguments` are
    // never read: the token comes only from `env`.
    let empty = Map::new();
    let arguments = match params.and_then(|p| p.get("arguments")) {
        None | Some(Value::Null) => Ok(&empty),
        Some(Value::Object(map)) => Ok(map),
        Some(_) => Err("invalid arguments: `arguments` must be an object".to_string()),
    };
    let command = arguments.and_then(|args| build_command(tool, args));
    match command {
        Ok(command) => Ok(render_report(core::execute_reporting(
            env, &command, context,
        ))),
        // Without any token the call fails as every Agent command does,
        // whatever the arguments hold; there is no command to audit. The
        // argument mistake is reported too, so one retry can fix both. It
        // names argument keys only, never values.
        Err(message) if core::no_token_presented(env) => {
            let missing = core::CoreError::MissingToken.body().message;
            Ok(tool_error(
                "missing_token",
                &format!("{missing}. The arguments are also wrong: {message}"),
            ))
        }
        Err(message) => Ok(invalid_arguments(&message)),
    }
}

/// The tool result for a core report: the CLI's JSON lines, the warning
/// first for an error and second for a success, as the CLI prints them.
fn render_report(report: Report) -> Value {
    let warning = Warning::invalid_tokens(&report.invalid_tokens)
        .map(|w| line(crate::render_warning(&w, false)));
    let (main, is_error) = match &report.result {
        Ok(outcome) => (line(crate::render_outcome(outcome, false)), false),
        Err(error) => (line(crate::render_error(error, false)), true),
    };
    let structured: Value = serde_json::from_str(&main).expect("the CLI prints JSON");
    let mut content = Vec::new();
    if is_error && let Some(w) = &warning {
        content.push(text_item(w));
    }
    content.push(text_item(&main));
    if !is_error && let Some(w) = &warning {
        content.push(text_item(w));
    }
    json!({
        "content": content,
        "structuredContent": structured,
        "isError": is_error,
    })
}

fn line(mut text: String) -> String {
    if text.ends_with('\n') {
        text.pop();
    }
    text
}

fn text_item(text: &str) -> Value {
    json!({ "type": "text", "text": text })
}

fn invalid_arguments(message: &str) -> Value {
    tool_error("invalid_arguments", message)
}

/// A tool error result with only `code` and `message`.
fn tool_error(code: &str, message: &str) -> Value {
    let error = json!({ "error": { "code": code, "message": message } });
    json!({
        "content": [text_item(&error.to_string())],
        "structuredContent": error,
        "isError": true,
    })
}

/// The tool arguments, checked against the tool's schema.
struct Arguments<'a> {
    map: &'a Map<String, Value>,
}

impl<'a> Arguments<'a> {
    fn get(&self, name: &str) -> Option<&'a Value> {
        self.map.get(name)
    }

    fn string(&self, name: &str) -> Result<Option<&'a str>, String> {
        match self.get(name) {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(_) => Err(format!("invalid argument `{name}`: must be a string")),
        }
    }

    fn required_string(&self, name: &str) -> Result<&'a str, String> {
        self.string(name)?
            .ok_or_else(|| format!("missing argument `{name}`: it is required"))
    }

    fn boolean(&self, name: &str) -> Result<bool, String> {
        match self.get(name) {
            None => Ok(false),
            Some(Value::Bool(b)) => Ok(*b),
            Some(_) => Err(format!("invalid argument `{name}`: must be true or false")),
        }
    }

    /// `name` checked with the CLI's `validator`.
    fn checked<T>(
        &self,
        name: &str,
        validator: fn(&str) -> Result<T, InvalidValue>,
    ) -> Result<Option<T>, String> {
        self.string(name)?
            .map(|value| validator(value).map_err(|e| invalid(name, &e)))
            .transpose()
    }

    fn required_checked<T>(
        &self,
        name: &str,
        validator: fn(&str) -> Result<T, InvalidValue>,
    ) -> Result<T, String> {
        let value = self.required_string(name)?;
        validator(value).map_err(|e| invalid(name, &e))
    }
}

fn invalid(name: &str, error: &InvalidValue) -> String {
    format!("invalid argument `{name}`: {error}")
}

/// Checks `args` against `tool` and builds the CLI's command for it, or
/// the `invalid_arguments` message naming the offending argument.
fn build_command(tool: &Tool, map: &Map<String, Value>) -> Result<Command, String> {
    for key in map.keys() {
        if !tool.properties.iter().any(|p| p.name == key) {
            let allowed: Vec<&str> = tool.properties.iter().map(|p| p.name).collect();
            let takes = if allowed.is_empty() {
                "no arguments".to_string()
            } else {
                format!("only {}", allowed.join(", "))
            };
            return Err(format!(
                "unknown argument `{}`: the `{}` tool takes {takes}",
                echo_name(key),
                tool.name
            ));
        }
    }
    let args = Arguments { map };
    match tool.name {
        "whoami" => Ok(Command::Whoami),
        "list" => Ok(Command::List {
            account: args.checked("account", crate::account_login)?,
        }),
        "search" => search_command(&args),
        "read" => Ok(Command::Read {
            reference: args.required_checked("ref", crate::reference)?,
            full: args.boolean("full")?,
        }),
        "flag" => {
            let reference = args.required_checked("ref", crate::reference)?;
            let (action, flag) = match (args.string("set")?, args.string("clear")?) {
                (Some(flag), None) => (FlagAction::Set, flag),
                (None, Some(flag)) => (FlagAction::Clear, flag),
                (Some(_), Some(_)) => {
                    return Err(
                        "invalid arguments `set` and `clear`: give exactly one of them, not both"
                            .to_string(),
                    );
                }
                (None, None) => {
                    return Err(
                        "missing argument `set` or `clear`: give exactly one of them".to_string(),
                    );
                }
            };
            Ok(Command::Flag {
                reference,
                action,
                flag: flag.to_string(),
            })
        }
        "move" => Ok(Command::Move {
            reference: args.required_checked("ref", crate::reference)?,
            target: args.required_checked("to", crate::mailbox_name)?,
        }),
        "draft" => draft_command(&args),
        other => unreachable!("tool {other} has no command"),
    }
}

fn search_command(args: &Arguments<'_>) -> Result<Command, String> {
    let account = args.checked("account", crate::account_login)?;
    let mailbox = args
        .checked("mailbox", crate::search_text)?
        .map_or_else(|| crate::search_text("INBOX"), Ok)
        .map_err(|e| invalid("mailbox", &e))?;
    let limit = match args.get("limit") {
        None => Limit::new(20).expect("20 is a valid limit"),
        Some(value) => value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(InvalidValue("must be an integer from 1 to 1000"))
            .and_then(Limit::new)
            .map_err(|e| invalid("limit", &e))?,
    };
    let query = Query {
        mailbox,
        from: args.checked("from", crate::search_text)?,
        to: args.checked("to", crate::search_text)?,
        subject: args.checked("subject", crate::search_text)?,
        text: args.checked("text", crate::search_text)?,
        since: args.checked("since", crate::date)?,
        before: args.checked("before", crate::date)?,
        unseen: args.boolean("unseen")?,
        flagged: args.boolean("flagged")?,
        gmail: args.checked("gmail", crate::gmail_query)?,
        limit,
    };
    if query.gmail.is_some() {
        let criteria = [
            ("from", query.from.is_some()),
            ("to", query.to.is_some()),
            ("subject", query.subject.is_some()),
            ("text", query.text.is_some()),
            ("since", query.since.is_some()),
            ("before", query.before.is_some()),
            ("unseen", query.unseen),
            ("flagged", query.flagged),
        ];
        if let Some((name, _)) = criteria.iter().find(|(_, given)| *given) {
            return Err(format!(
                "invalid arguments `gmail` and `{name}`: gmail cannot be combined with from, to, subject, text, since, before, unseen or flagged"
            ));
        }
    }
    Ok(Command::Search { query, account })
}

fn draft_command(args: &Arguments<'_>) -> Result<Command, String> {
    let to = match args.get("to") {
        None => return Err("missing argument `to`: it is required".to_string()),
        Some(Value::Array(items)) if !items.is_empty() => items
            .iter()
            .map(|item| match item {
                Value::String(s) => crate::address(s).map_err(|e| invalid("to", &e)),
                _ => Err("invalid argument `to`: every item must be a string".to_string()),
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(Value::Array(_)) => {
            return Err("invalid argument `to`: must hold at least one address".to_string());
        }
        Some(_) => {
            return Err("invalid argument `to`: must be an array of addresses".to_string());
        }
    };
    let subject = args.checked("subject", crate::subject)?;
    let body = args.required_checked("body", crate::body)?;
    let reply_to = args.checked("reply_to", crate::reference)?;
    let account = args.checked("account", crate::account_login)?;
    if reply_to.is_some() {
        if subject.is_some() {
            return Err(
                "invalid arguments `subject` and `reply_to`: a reply takes the original's subject, so give only one"
                    .to_string(),
            );
        }
        if account.is_some() {
            return Err(
                "invalid arguments `account` and `reply_to`: a reply works on the reference's account, so give only one"
                    .to_string(),
            );
        }
    } else if subject.is_none() {
        return Err(
            "missing argument `subject` or `reply_to`: give a subject for a new message, or reply_to for a reply"
                .to_string(),
        );
    }
    Ok(Command::Draft {
        to,
        subject,
        body,
        reply_to,
        account,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echoed_names_are_escaped_and_capped() {
        assert_eq!(echo_name("a\nb\u{1b}"), "a\\u{a}b\\u{1b}");
        let long = "x".repeat(500);
        assert_eq!(echo_name(&long).len(), MAX_ECHOED_NAME + 3);
    }

    #[test]
    fn every_tool_schema_forbids_extra_properties() {
        for tool in tool_list() {
            assert_eq!(tool["inputSchema"]["additionalProperties"], json!(false));
        }
    }
}
