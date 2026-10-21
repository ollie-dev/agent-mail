//! Reading one message: the result types, the conversion of a raw MIME
//! message into headers, a plain-text body and an attachment list, and the
//! body's line-ending normalisation and truncation.
//!
//! The message is untrusted server data. Parsing never panics on malformed
//! MIME (a message that cannot be parsed at all is reported as `None`),
//! HTML is converted to text without fetching or running anything, and no
//! attachment content ever reaches a [`ReadResult`].

use std::borrow::Cow;

use mail_parser::{Address, MessageParser, MessagePart, MimeHeaders, PartType};
use serde::Serialize;

use crate::search::{InvalidValue, MessageRef};

/// Bodies longer than this many characters are cut unless `--full` is given.
pub const BODY_LIMIT: usize = 20_000;

/// Result of `read`. Keys serialise in field order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadResult {
    /// The message reference that was read.
    #[serde(rename = "ref")]
    pub reference: String,
    /// Login name of the Account.
    pub account: String,
    /// The mailbox, decoded UTF-8.
    pub mailbox: String,
    /// The `Date:` header, RFC 3339 UTC; `""` when absent or unparseable.
    pub date: String,
    /// The first From address; `""` when absent.
    pub from: String,
    /// Every To address, in header order.
    pub to: Vec<String>,
    /// Every Cc address, in header order.
    pub cc: Vec<String>,
    /// The decoded Subject; `""` when absent.
    pub subject: String,
    /// The Message-ID with its angle brackets; `""` when absent.
    pub message_id: String,
    /// The plain-text body, possibly cut with a truncation marker.
    pub body: String,
    /// Whether `body` was cut.
    pub truncated: bool,
    /// The attachments, in message order. Never their content.
    pub attachments: Vec<Attachment>,
}

/// One attachment of a [`ReadResult`]: metadata only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Attachment {
    /// The decoded file name; `""` when none is given.
    pub name: String,
    /// Lower-case `type/subtype`, without parameters.
    #[serde(rename = "type")]
    pub content_type: String,
    /// Decoded size in bytes.
    pub size: usize,
}

/// What a raw message holds, before truncation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMessage {
    /// Unix seconds of the `Date:` header, if it parsed.
    pub date: Option<i64>,
    /// The first From address.
    pub from: String,
    /// Every To address.
    pub to: Vec<String>,
    /// Every Cc address.
    pub cc: Vec<String>,
    /// The decoded Subject.
    pub subject: String,
    /// The Message-ID with angle brackets.
    pub message_id: String,
    /// The whole normalised body.
    pub body: String,
    /// The attachments.
    pub attachments: Vec<Attachment>,
}

/// Parses a message reference given on the command line: the form
/// [`MessageRef::parse`] accepts, with no CR, LF or NUL in the decoded
/// account or mailbox.
pub fn parse_reference(text: &str) -> Result<MessageRef, InvalidValue> {
    const BAD: InvalidValue = InvalidValue(
        "must be a message reference as `agent-mail search` prints it: <account>/<mailbox>/<uidvalidity>/<uid>",
    );
    let reference = MessageRef::parse(text).map_err(|_| BAD)?;
    if !is_valid_reference(&reference) {
        return Err(BAD);
    }
    Ok(reference)
}

/// Whether `reference` may be sent to a server: non-empty account and
/// mailbox without CR, LF or NUL, and a UID of at least 1. The core checks
/// this again, since adapters other than the CLI build references too.
pub fn is_valid_reference(reference: &MessageRef) -> bool {
    let clean = |s: &str| !s.is_empty() && !s.contains(['\r', '\n', '\0']);
    clean(&reference.account) && clean(&reference.mailbox) && reference.uid >= 1
}

/// Parses a raw RFC 5322 message. `None` if it cannot be parsed at all.
pub fn parse_message(raw: &[u8]) -> Option<ParsedMessage> {
    let message = MessageParser::default().parse(raw)?;
    if message.parts.is_empty() {
        return None;
    }
    let addresses = |address: Option<&Address<'_>>| -> Vec<String> {
        address
            .map(|a| a.iter().filter_map(format_addr).collect())
            .unwrap_or_default()
    };
    let date = message
        .date()
        .filter(|d| d.is_valid())
        .map(mail_parser::DateTime::to_timestamp);
    let (body, attachments) = body_and_attachments(&message.parts);
    Some(ParsedMessage {
        date,
        from: addresses(message.from())
            .into_iter()
            .next()
            .unwrap_or_default(),
        to: addresses(message.to()),
        cc: addresses(message.cc()),
        subject: message.subject().unwrap_or_default().to_string(),
        message_id: message
            .message_id()
            .map(|id| format!("<{id}>"))
            .unwrap_or_default(),
        body: normalise(&body),
        attachments,
    })
}

/// `Name <addr>` when there is a display name, else `addr`; `None` when the
/// entry has neither.
fn format_addr(addr: &mail_parser::Addr<'_>) -> Option<String> {
    let name = addr
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty());
    match (name, addr.address.as_deref()) {
        (Some(name), Some(address)) => Some(format!("{name} <{address}>")),
        (None, Some(address)) => Some(address.to_string()),
        (Some(name), None) => Some(name.to_string()),
        (None, None) => None,
    }
}

/// A leaf part, with the index of its parent multipart (if any).
struct Leaf {
    index: usize,
    parent: Option<usize>,
}

/// Every leaf part in message order. Nested messages are leaves.
fn leaves(parts: &[MessagePart<'_>]) -> Vec<Leaf> {
    let mut out = Vec::new();
    // Depth-first, children in order; guarded against cycles and bad ids.
    let mut stack = vec![(0usize, None)];
    let mut visited = vec![false; parts.len()];
    while let Some((index, parent)) = stack.pop() {
        let Some(part) = parts.get(index) else {
            continue;
        };
        if std::mem::replace(&mut visited[index], true) {
            continue;
        }
        match &part.body {
            PartType::Multipart(children) => {
                for &child in children.iter().rev() {
                    stack.push((child as usize, Some(index)));
                }
            }
            _ => out.push(Leaf { index, parent }),
        }
    }
    out
}

fn is_attachment_disposition(part: &MessagePart<'_>) -> bool {
    part.content_disposition()
        .is_some_and(mail_parser::ContentType::is_attachment)
}

fn is_subtype(part: &MessagePart<'_>, subtype: &str) -> bool {
    match part.content_type() {
        Some(ct) => {
            ct.ctype().eq_ignore_ascii_case("text")
                && ct
                    .subtype()
                    .is_some_and(|s| s.eq_ignore_ascii_case(subtype))
        }
        // RFC 2045: no Content-Type means text/plain.
        None => subtype == "plain",
    }
}

fn is_alternative(part: &MessagePart<'_>) -> bool {
    part.content_type().is_some_and(|ct| {
        ct.ctype().eq_ignore_ascii_case("multipart")
            && ct
                .subtype()
                .is_some_and(|s| s.eq_ignore_ascii_case("alternative"))
    })
}

/// Selects the body (first `text/plain` leaf, else the first `text/html`
/// leaf converted to text) and lists every other leaf as an attachment,
/// except the body's siblings in a `multipart/alternative`.
fn body_and_attachments(parts: &[MessagePart<'_>]) -> (String, Vec<Attachment>) {
    let leaves = leaves(parts);
    let candidate = |subtype: &str| {
        leaves.iter().position(|leaf| {
            let part = &parts[leaf.index];
            !is_attachment_disposition(part)
                && matches!(part.body, PartType::Text(_) | PartType::Html(_))
                && is_subtype(part, subtype)
        })
    };
    let selected = candidate("plain").or_else(|| candidate("html"));
    let body = selected.map_or(String::new(), |at| {
        let part = &parts[leaves[at].index];
        match &part.body {
            PartType::Html(html) => mail_parser::decoders::html::html_to_text(html),
            PartType::Text(text) if is_subtype(part, "html") => {
                mail_parser::decoders::html::html_to_text(text)
            }
            PartType::Text(text) => text.to_string(),
            _ => String::new(),
        }
    });
    // The other alternatives of the selected body: direct siblings under a
    // multipart/alternative parent.
    let alternative_parent = selected
        .and_then(|at| leaves[at].parent)
        .filter(|&parent| is_alternative(&parts[parent]));
    let attachments = leaves
        .iter()
        .enumerate()
        .filter(|(at, leaf)| {
            Some(*at) != selected
                && (alternative_parent.is_none() || leaf.parent != alternative_parent)
        })
        .map(|(_, leaf)| attachment(&parts[leaf.index]))
        .collect();
    (body, attachments)
}

fn attachment(part: &MessagePart<'_>) -> Attachment {
    let content_type = match part.content_type() {
        Some(ct) => match ct.subtype() {
            Some(sub) => format!("{}/{}", ct.ctype(), sub),
            None => ct.ctype().to_string(),
        },
        None if part.is_message() => "message/rfc822".to_string(),
        None => "text/plain".to_string(),
    };
    Attachment {
        name: part.attachment_name().unwrap_or_default().to_string(),
        content_type: content_type.to_ascii_lowercase(),
        size: part.len(),
    }
}

/// Line endings normalised to `\n` (CRLF and lone CR), trailing line breaks
/// removed.
pub fn normalise(text: &str) -> String {
    let text: Cow<'_, str> = if text.contains('\r') {
        Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(text)
    };
    text.trim_end_matches('\n').to_string()
}

/// The body to return and whether it was cut: more than [`BODY_LIMIT`]
/// characters and not `full` gives the first [`BODY_LIMIT`] followed by the
/// truncation marker.
pub fn truncate(body: String, full: bool) -> (String, bool) {
    if full {
        return (body, false);
    }
    let total = body.chars().count();
    if total <= BODY_LIMIT {
        return (body, false);
    }
    let cut = body
        .char_indices()
        .nth(BODY_LIMIT)
        .map_or(body.len(), |(at, _)| at);
    let mut out = body[..cut].to_string();
    out.push_str(&format!(
        "\n\n[truncated: showing {BODY_LIMIT} of {total} characters; read again with --full for the complete body]"
    ));
    (out, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_turns_crlf_and_cr_into_lf_and_trims_trailing_breaks() {
        assert_eq!(normalise("a\r\nb\rc\n\r\n\n"), "a\nb\nc");
        assert_eq!(normalise("  x  \n"), "  x  ");
        assert_eq!(normalise(""), "");
    }

    #[test]
    fn truncate_counts_chars_not_bytes() {
        let body = "ü".repeat(BODY_LIMIT);
        assert_eq!(truncate(body.clone(), false), (body.clone(), false));
        let long = format!("{body}x");
        let (cut, truncated) = truncate(long.clone(), false);
        assert!(truncated);
        assert!(cut.starts_with(&body));
        assert!(cut.ends_with(
            "\n\n[truncated: showing 20000 of 20001 characters; read again with --full for the complete body]"
        ));
        assert_eq!(truncate(long.clone(), true), (long, false));
    }

    #[test]
    fn references_with_line_breaks_or_nul_are_refused() {
        assert!(parse_reference("agent@example.test/Read/1700000006/1").is_ok());
        for bad in [
            "agent@example.test/Re%0Aad/1700000006/1",
            "agent@example.test/Read%0D/1700000006/1",
            "agent%00@example.test/Read/1700000006/1",
            "agent@example.test/Read/1700000006/0",
            "agent@example.test/Read/1700000006",
            "not-a-ref",
        ] {
            assert!(parse_reference(bad).is_err(), "{bad}");
        }
        assert!(!is_valid_reference(&MessageRef {
            account: "a".to_string(),
            mailbox: "b\nc".to_string(),
            uidvalidity: 1,
            uid: 1,
        }));
        assert!(!is_valid_reference(&MessageRef {
            account: "a".to_string(),
            mailbox: "b".to_string(),
            uidvalidity: 1,
            uid: 0,
        }));
    }

    #[test]
    fn garbage_never_panics() {
        for raw in [
            &b""[..],
            b"\0\0\0",
            b"Content-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\n--x--",
            b"Content-Type: multipart/mixed\r\n\r\nno boundary",
            &[0xff, 0xfe, b'\r', b'\n', b'\r', b'\n', 0x80],
        ] {
            let _ = parse_message(raw);
        }
    }

    #[test]
    fn nested_alternative_with_related_lists_only_real_attachments() {
        let raw = b"From: a@example.com\r\n\
Content-Type: multipart/mixed; boundary=outer\r\n\r\n\
--outer\r\n\
Content-Type: multipart/alternative; boundary=alt\r\n\r\n\
--alt\r\n\
Content-Type: text/plain\r\n\r\n\
plain\r\n\
--alt\r\n\
Content-Type: text/html\r\n\r\n\
<p>html</p>\r\n\
--alt--\r\n\
--outer\r\n\
Content-Type: image/PNG; name=\"=?UTF-8?Q?bild_=C3=BC.png?=\"\r\n\
Content-Transfer-Encoding: base64\r\n\r\n\
AAEC\r\n\
--outer--\r\n";
        let parsed = parse_message(raw).unwrap();
        assert_eq!(parsed.body, "plain");
        assert_eq!(
            parsed.attachments,
            vec![Attachment {
                name: "bild ü.png".to_string(),
                content_type: "image/png".to_string(),
                size: 3,
            }]
        );
    }

    #[test]
    fn rfc2231_filenames_decode() {
        let raw = b"From: a@example.com\r\n\
Content-Type: multipart/mixed; boundary=b\r\n\r\n\
--b\r\n\
Content-Type: text/plain\r\n\r\n\
hi\r\n\
--b\r\n\
Content-Type: application/octet-stream\r\n\
Content-Disposition: attachment; filename*=UTF-8''Gr%C3%BC%C3%9Fe.txt\r\n\r\n\
xyz\r\n\
--b--\r\n";
        let parsed = parse_message(raw).unwrap();
        assert_eq!(parsed.attachments[0].name, "Grüße.txt");
        assert_eq!(
            parsed.attachments[0].content_type,
            "application/octet-stream"
        );
    }
}
