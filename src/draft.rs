//! Creating a draft: the result type and its `--text` line, the validation
//! of the Agent's recipients, subject and body, the reply helpers, and the
//! RFC 5322 message that is appended to the Drafts mailbox.
//!
//! The recipients, subject and body are untrusted Agent input and the
//! header-injection surface; the original's headers in reply mode are
//! untrusted server data. Nothing from either reaches the header section
//! unchecked: addresses are bare `local@domain`, the subject is re-encoded,
//! message ids are filtered, and the body only ever follows the header
//! section, quoted-printable encoded.

use base64::Engine;
use mail_parser::{HeaderValue, MessageParser};
use ring::rand::{SecureRandom, SystemRandom};
use serde::Serialize;

use crate::terminal::escape_controls;

/// The longest address `--to` accepts.
pub const MAX_ADDRESS_LEN: usize = 254;

/// Domain of the Message-ID when the login has no domain.
pub const FALLBACK_DOMAIN: &str = "agent-mail.invalid";

/// Result of `draft`. Keys serialise in field order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DraftResult {
    /// The new draft's reference, built from the server's `APPENDUID`
    /// response code; `None` (JSON `null`) when the server reported none.
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    /// Login name of the Account.
    pub account: String,
    /// The Drafts mailbox, decoded UTF-8.
    pub mailbox: String,
    /// The recipients, as given.
    pub to: Vec<String>,
    /// The subject as sent: the reply subject in reply mode.
    pub subject: String,
    /// The `--reply-to` reference, or `None`.
    pub original: Option<String>,
}

impl DraftResult {
    /// The `--text` line, without its `\n`: `draft created: <ref>`, or
    /// `draft created in <mailbox> (new reference not reported)`. Every
    /// value has its control characters escaped.
    pub fn text_line(&self) -> String {
        match &self.reference {
            Some(reference) => format!("draft created: {}", escape_controls(reference)),
            None => format!(
                "draft created in {} (new reference not reported)",
                escape_controls(&self.mailbox)
            ),
        }
    }
}

/// Whether `text` is a bare address `local@domain`: exactly one `@` with a
/// non-empty part on each side, printable ASCII only, no space and none of
/// `<>()[]\,;:"`, at most [`MAX_ADDRESS_LEN`] characters.
pub fn is_valid_address(text: &str) -> bool {
    let Some((local, domain)) = text.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && !domain.contains('@')
        && text.len() <= MAX_ADDRESS_LEN
        && text
            .bytes()
            .all(|b| b.is_ascii_graphic() && !b"<>()[]\\,;:\"".contains(&b))
}

/// Whether `text` may be a subject: not empty, no CR, LF or NUL.
pub fn is_valid_subject(text: &str) -> bool {
    !text.is_empty() && !text.contains(['\r', '\n', '\0'])
}

/// Whether `text` may be a body: no NUL.
pub fn is_valid_body(text: &str) -> bool {
    !text.contains('\0')
}

/// The reply subject: `original` with `Re: ` in front, unless it already
/// starts with `re:` (case-insensitive) after leading whitespace, when it
/// is left unchanged.
pub fn reply_subject(original: &str) -> String {
    let trimmed = original.trim_start();
    let has_prefix = trimmed
        .get(..3)
        .is_some_and(|p| p.eq_ignore_ascii_case("re:"));
    if has_prefix {
        original.to_string()
    } else {
        format!("Re: {original}")
    }
}

/// Whether `id` (without angle brackets) may be copied into a header: not
/// empty, at most 900 characters, printable ASCII without space, `<` or
/// `>`.
pub fn is_safe_message_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 900
        && id
            .bytes()
            .all(|b| b.is_ascii_graphic() && b != b'<' && b != b'>')
}

/// The References value of a reply: the original's References ids, in
/// order, then its Message-ID, each in angle brackets. Ids that are not
/// [`is_safe_message_id`] are dropped.
pub fn references(original_references: &[String], message_id: &str) -> Vec<String> {
    original_references
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(message_id))
        .filter(|id| is_safe_message_id(id))
        .map(|id| format!("<{id}>"))
        .collect()
}

/// What a reply takes from the original's headers. Untrusted server data.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Original {
    /// The Message-ID without angle brackets, if present and safe.
    pub message_id: Option<String>,
    /// The References ids without angle brackets, in order (unfiltered).
    pub references: Vec<String>,
    /// The decoded Subject, CR, LF and NUL replaced by spaces; `""` when
    /// absent.
    pub subject: String,
}

/// Parses the original's raw header section (or whole message).
pub fn parse_original(raw: &[u8]) -> Original {
    let Some(message) = MessageParser::default().parse_headers(raw) else {
        return Original::default();
    };
    let strip = |s: &str| {
        s.trim()
            .trim_start_matches('<')
            .trim_end_matches('>')
            .to_string()
    };
    let references = match message.references() {
        HeaderValue::Text(id) => vec![strip(id)],
        HeaderValue::TextList(ids) => ids.iter().map(|id| strip(id)).collect(),
        _ => Vec::new(),
    };
    Original {
        message_id: message
            .message_id()
            .map(strip)
            .filter(|id| is_safe_message_id(id)),
        references,
        subject: message
            .subject()
            .unwrap_or_default()
            .replace(['\r', '\n', '\0'], " "),
    }
}

/// The reply headers and subject for `original`: `(In-Reply-To,
/// References, subject)`. Without a usable Message-ID, neither header is
/// set but the subject is still the reply subject.
pub fn reply_headers(original: &Original) -> (Option<String>, Vec<String>, String) {
    let subject = reply_subject(&original.subject);
    match &original.message_id {
        Some(id) => (
            Some(format!("<{id}>")),
            references(&original.references, id),
            subject,
        ),
        None => (None, Vec::new(), subject),
    }
}

/// A new Message-ID without brackets: 32 lowercase hex characters from the
/// operating system's CSPRNG, `@`, and the login's domain, or
/// [`FALLBACK_DOMAIN`] when the login is not a bare address.
pub fn new_message_id(login: &str) -> String {
    let mut bytes = [0u8; 16];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("the operating system's random source is available");
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let domain = if is_valid_address(login) {
        login.split_once('@').map_or(FALLBACK_DOMAIN, |(_, d)| d)
    } else {
        FALLBACK_DOMAIN
    };
    format!("{hex}@{domain}")
}

/// Unix seconds as an RFC 5322 date in UTC, e.g.
/// `Thu, 22 Oct 2026 09:30:15 +0000`.
pub fn rfc5322_date(secs: u64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    // `YYYY-MM-DDTHH:MM:SSZ`.
    let iso = crate::core::rfc3339_utc(secs);
    let month: usize = iso[5..7].parse().unwrap_or(1);
    format!(
        "{}, {} {} {} {} +0000",
        DAYS[usize::try_from(secs / 86_400 % 7).unwrap_or(0)],
        &iso[8..10],
        MONTHS[month - 1],
        &iso[0..4],
        &iso[11..19]
    )
}

/// Everything that goes into the appended message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft<'a> {
    /// The Account login; used as `From:` only when it is a bare address.
    pub login: &'a str,
    /// The recipients, each [`is_valid_address`].
    pub to: &'a [String],
    /// The subject; any UTF-8 without CR, LF or NUL.
    pub subject: &'a str,
    /// The body; any UTF-8 without NUL.
    pub body: &'a str,
    /// The Message-ID without brackets.
    pub message_id: &'a str,
    /// The `Date:` value.
    pub date: &'a str,
    /// `In-Reply-To:` with brackets, in reply mode.
    pub in_reply_to: Option<&'a str>,
    /// `References:` ids with brackets, in reply mode.
    pub references: &'a [String],
}

impl Draft<'_> {
    /// The RFC 5322 message, CRLF line endings. The header section is ASCII
    /// and no line exceeds 998 octets.
    pub fn build(&self) -> String {
        let mut out = String::new();
        if is_valid_address(self.login) {
            out.push_str(&format!("From: {}\r\n", self.login));
        }
        out.push_str(&fold("To:", self.to, ","));
        out.push_str(&format!("Subject: {}\r\n", encode_subject(self.subject)));
        out.push_str(&format!("Date: {}\r\n", self.date));
        out.push_str(&format!("Message-ID: <{}>\r\n", self.message_id));
        if let Some(id) = self.in_reply_to {
            out.push_str(&format!("In-Reply-To: {id}\r\n"));
        }
        if !self.references.is_empty() {
            out.push_str(&fold("References:", self.references, ""));
        }
        out.push_str("MIME-Version: 1.0\r\n");
        out.push_str("Content-Type: text/plain; charset=utf-8\r\n");
        out.push_str("Content-Transfer-Encoding: quoted-printable\r\n");
        out.push_str("\r\n");
        out.push_str(&quoted_printable(self.body));
        out
    }
}

/// A header line `name item<sep> item...`, folded before an item when the
/// line would pass 78 characters.
fn fold(name: &str, items: &[String], sep: &str) -> String {
    let mut out = name.to_string();
    let mut line = name.len();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(sep);
            line += sep.len();
        }
        if i > 0 && line + 1 + item.len() > 78 {
            out.push_str("\r\n");
            line = 0;
        }
        out.push(' ');
        out.push_str(item);
        line += 1 + item.len();
    }
    out.push_str("\r\n");
    out
}

/// The subject as header text: unchanged when it is short printable ASCII
/// that holds no `=?`, else RFC 2047 `B` encoded words of at most 45
/// octets each, folded onto continuation lines.
fn encode_subject(subject: &str) -> String {
    let plain = subject.len() <= 900
        && !subject.contains("=?")
        && subject.bytes().all(|b| (0x20..0x7f).contains(&b));
    if plain {
        return subject.to_string();
    }
    let engine = base64::engine::general_purpose::STANDARD;
    let mut words = Vec::new();
    let mut chunk = String::new();
    for c in subject.chars() {
        if chunk.len() + c.len_utf8() > 45 {
            words.push(format!("=?UTF-8?B?{}?=", engine.encode(&chunk)));
            chunk.clear();
        }
        chunk.push(c);
    }
    if !chunk.is_empty() {
        words.push(format!("=?UTF-8?B?{}?=", engine.encode(&chunk)));
    }
    words.join("\r\n ")
}

/// `body` quoted-printable encoded (RFC 2045): line breaks (CRLF, LF or a
/// lone CR) become CRLF, every other byte outside printable ASCII, `=` and
/// a space at the end of a line are escaped, and lines are cut with soft
/// breaks at 76 characters. Ends with CRLF.
pub fn quoted_printable(body: &str) -> String {
    let normalised = body.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(normalised.len() + normalised.len() / 8);
    for (i, line) in normalised.split('\n').enumerate() {
        if i > 0 {
            out.push_str("\r\n");
        }
        let bytes = line.as_bytes();
        let mut column = 0;
        for (j, &b) in bytes.iter().enumerate() {
            let last = j + 1 == bytes.len();
            let literal = (b.is_ascii_graphic() && b != b'=') || (b == b' ' && !last);
            let piece = if literal {
                char::from(b).to_string()
            } else {
                format!("={b:02X}")
            };
            if column + piece.len() > 75 {
                out.push_str("=\r\n");
                column = 0;
            }
            out.push_str(&piece);
            column += piece.len();
        }
    }
    out.push_str("\r\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_bare_local_at_domain() {
        for good in ["carol@example.com", "a@b", "x.y+z@sub.example.org"] {
            assert!(is_valid_address(good), "{good}");
        }
        let long = format!("{}@example.com", "a".repeat(250));
        for bad in [
            "",
            "carol",
            "@example.com",
            "carol@",
            "a@b@c",
            "Carol <carol@example.com>",
            "a@b.c\r\nBcc: x@y.z",
            "a b@c",
            "a,b@c",
            "a;b@c",
            "a:b@c",
            "\"a\"@c",
            "a(b)@c",
            "[a]@c",
            "a\\b@c",
            "ü@example.com",
            long.as_str(),
        ] {
            assert!(!is_valid_address(bad), "{bad:?}");
        }
    }

    #[test]
    fn subject_and_body_rules() {
        assert!(is_valid_subject("Grüße"));
        assert!(!is_valid_subject(""));
        assert!(!is_valid_subject("Hi\r\nBcc: x@y.z"));
        assert!(!is_valid_subject("a\0"));
        assert!(is_valid_body("line\r\nline\n"));
        assert!(!is_valid_body("a\0b"));
    }

    #[test]
    fn reply_subject_adds_re_once() {
        assert_eq!(reply_subject("Budget"), "Re: Budget");
        assert_eq!(reply_subject("Re: Budget"), "Re: Budget");
        assert_eq!(reply_subject("RE: Agenda"), "RE: Agenda");
        assert_eq!(reply_subject("  re:x"), "  re:x");
        assert_eq!(reply_subject("Ré"), "Re: Ré");
        assert_eq!(reply_subject(""), "Re: ");
    }

    #[test]
    fn references_drop_unsafe_ids() {
        let refs = vec![
            "a@x".to_string(),
            "bad id@x".to_string(),
            "b<@x".to_string(),
            "ü@x".to_string(),
            "c@x".to_string(),
        ];
        assert_eq!(references(&refs, "m@x"), vec!["<a@x>", "<c@x>", "<m@x>"]);
    }

    #[test]
    fn rfc5322_date_formats_utc() {
        assert_eq!(rfc5322_date(0), "Thu, 01 Jan 1970 00:00:00 +0000");
        assert_eq!(
            rfc5322_date(1_792_575_015),
            "Wed, 21 Oct 2026 09:30:15 +0000"
        );
    }

    #[test]
    fn message_ids_are_random_hex_at_the_login_domain() {
        let a = new_message_id("agent@example.test");
        let b = new_message_id("agent@example.test");
        assert_ne!(a, b);
        let (hex, domain) = a.split_once('@').unwrap();
        assert_eq!(domain, "example.test");
        assert_eq!(hex.len(), 32);
        assert!(
            hex.bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert!(new_message_id("agent").ends_with("@agent-mail.invalid"));
    }

    #[test]
    fn quoted_printable_keeps_lines_short_and_escapes() {
        let body = format!("{}\nfür = x \n", "a".repeat(2000));
        let qp = quoted_printable(&body);
        assert!(qp.split("\r\n").all(|l| l.len() <= 76));
        assert!(qp.contains("f=C3=BCr =3D x=20\r\n"));
        assert!(qp.is_ascii());
    }

    #[test]
    fn long_or_non_ascii_subjects_are_encoded_and_folded() {
        let encoded = encode_subject(&"ü".repeat(100));
        assert!(encoded.is_ascii());
        assert!(encoded.split("\r\n").all(|l| l.len() <= 78));
        assert_eq!(encode_subject("Status"), "Status");
    }

    #[test]
    fn text_line_escapes_controls() {
        let mut result = DraftResult {
            reference: Some("agent@example.test/Drafts/1700000010/1".to_string()),
            account: "agent@example.test".to_string(),
            mailbox: "Dr\u{1b}[2Jafts".to_string(),
            to: vec!["carol@example.com".to_string()],
            subject: "Status".to_string(),
            original: None,
        };
        assert_eq!(
            result.text_line(),
            "draft created: agent@example.test/Drafts/1700000010/1"
        );
        result.reference = None;
        assert_eq!(
            result.text_line(),
            "draft created in Dr\\u{1b}[2Jafts (new reference not reported)"
        );
    }
}
