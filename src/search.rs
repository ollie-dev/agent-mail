//! Message search: the criteria an Agent may send, the result types, and
//! the message reference with its string form.
//!
//! Every value here that reaches the IMAP server is checked when it is
//! built: [`SearchText`] cannot hold CR, LF or NUL, [`Limit`] is within
//! `1..=1000` and [`Date`] is a real calendar day. The CLI parser builds
//! them through the same constructors, and so will any other adapter, so
//! the core never holds a value that could end or inject an IMAP command.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Results returned when no `--limit` is given.
pub const DEFAULT_LIMIT: u32 = 20;
/// The largest allowed `--limit`.
pub const MAX_LIMIT: u32 = 1000;

/// A value is not allowed in a search. Holds a description, never the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidValue(pub &'static str);

impl fmt::Display for InvalidValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for InvalidValue {}

/// A string criterion or mailbox name: any UTF-8 without CR, LF or NUL.
/// Deserialising checks the value again, through [`SearchText::new`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SearchText(String);

impl SearchText {
    /// Checks `value` and wraps it.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidValue> {
        let value = value.into();
        if value.contains(['\r', '\n', '\0']) {
            return Err(InvalidValue("must not contain CR, LF or NUL"));
        }
        Ok(Self(value))
    }

    /// The value.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The value as an IMAP quoted string, `"` and `\` escaped.
    pub fn quoted(&self) -> String {
        let mut out = String::with_capacity(self.0.len() + 2);
        out.push('"');
        for c in self.0.chars() {
            if c == '"' || c == '\\' {
                out.push('\\');
            }
            out.push(c);
        }
        out.push('"');
        out
    }
}

impl TryFrom<String> for SearchText {
    type Error = InvalidValue;

    fn try_from(value: String) -> Result<Self, InvalidValue> {
        Self::new(value)
    }
}

impl From<SearchText> for String {
    fn from(value: SearchText) -> Self {
        value.0
    }
}

/// A calendar date, parsed from `YYYY-MM-DD`. Serialised in that form, and
/// deserialising checks it again, through [`Date::parse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Date {
    year: u16,
    month: u8,
    day: u8,
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

impl Date {
    /// Parses `YYYY-MM-DD`, rejecting days that do not exist.
    pub fn parse(text: &str) -> Result<Self, InvalidValue> {
        const BAD: InvalidValue = InvalidValue("must be a date as YYYY-MM-DD, e.g. 2026-10-05");
        let b = text.as_bytes();
        let shape = b.len() == 10
            && b[4] == b'-'
            && b[7] == b'-'
            && b.iter()
                .enumerate()
                .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
        if !shape {
            return Err(BAD);
        }
        let year: u16 = text[0..4].parse().map_err(|_| BAD)?;
        let month: u8 = text[5..7].parse().map_err(|_| BAD)?;
        let day: u8 = text[8..10].parse().map_err(|_| BAD)?;
        if year == 0 || !(1..=12).contains(&month) || day == 0 || day > days_in(year, month) {
            return Err(BAD);
        }
        Ok(Self { year, month, day })
    }

    /// The date in IMAP `date` syntax, e.g. `05-Oct-2026`.
    pub fn imap(&self) -> String {
        format!(
            "{:02}-{}-{:04}",
            self.day,
            MONTHS[usize::from(self.month - 1)],
            self.year
        )
    }
}

impl TryFrom<String> for Date {
    type Error = InvalidValue;

    fn try_from(value: String) -> Result<Self, InvalidValue> {
        Self::parse(&value)
    }
}

impl From<Date> for String {
    fn from(value: Date) -> Self {
        format!("{:04}-{:02}-{:02}", value.year, value.month, value.day)
    }
}

fn days_in(year: u16, month: u8) -> u8 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// How many results a search returns at most: `1..=1000`. Deserialising
/// checks it again, through [`Limit::new`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct Limit(u32);

impl Limit {
    /// Checks `n` and wraps it.
    pub fn new(n: u32) -> Result<Self, InvalidValue> {
        if (1..=MAX_LIMIT).contains(&n) {
            Ok(Self(n))
        } else {
            Err(InvalidValue("must be an integer from 1 to 1000"))
        }
    }

    /// Parses a decimal integer from 1 to 1000.
    pub fn parse(text: &str) -> Result<Self, InvalidValue> {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(InvalidValue("must be an integer from 1 to 1000"));
        }
        let n = text
            .parse()
            .map_err(|_| InvalidValue("must be an integer from 1 to 1000"))?;
        Self::new(n)
    }

    /// The limit.
    pub fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for Limit {
    type Error = InvalidValue;

    fn try_from(value: u32) -> Result<Self, InvalidValue> {
        Self::new(value)
    }
}

impl From<Limit> for u32 {
    fn from(value: Limit) -> Self {
        value.0
    }
}

impl Default for Limit {
    fn default() -> Self {
        Self(DEFAULT_LIMIT)
    }
}

/// One search: a mailbox and criteria, combined with AND.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    /// The decoded UTF-8 mailbox name, as `list` prints it.
    pub mailbox: SearchText,
    /// `FROM`.
    pub from: Option<SearchText>,
    /// `TO`.
    pub to: Option<SearchText>,
    /// `SUBJECT`.
    pub subject: Option<SearchText>,
    /// `TEXT`.
    pub text: Option<SearchText>,
    /// `SINCE`: internal date on or after.
    pub since: Option<Date>,
    /// `BEFORE`: internal date before.
    pub before: Option<Date>,
    /// `UNSEEN`.
    pub unseen: bool,
    /// `FLAGGED`.
    pub flagged: bool,
    /// A Gmail search query, sent unchanged as `X-GM-RAW`. When set, it is
    /// the whole search: the standard criteria above are not sent, and the
    /// server must advertise `X-GM-EXT-1`.
    pub gmail: Option<SearchText>,
    /// At most this many results.
    pub limit: Limit,
}

impl Query {
    /// Every message of `mailbox`, default limit.
    pub fn all(mailbox: SearchText) -> Self {
        Self {
            mailbox,
            from: None,
            to: None,
            subject: None,
            text: None,
            since: None,
            before: None,
            unseen: false,
            flagged: false,
            gmail: None,
            limit: Limit::default(),
        }
    }

    /// The `UID SEARCH` arguments: `CHARSET UTF-8` first when any string is
    /// not ASCII, then each criterion, or `ALL` when there is none. A Gmail
    /// query is sent alone as `X-GM-RAW <quoted query>`.
    pub fn imap_criteria(&self) -> String {
        if let Some(gmail) = &self.gmail {
            let raw = format!("X-GM-RAW {}", gmail.quoted());
            return if gmail.as_str().is_ascii() {
                raw
            } else {
                format!("CHARSET UTF-8 {raw}")
            };
        }
        let mut keys = Vec::new();
        let mut ascii = true;
        for (name, value) in [
            ("FROM", &self.from),
            ("TO", &self.to),
            ("SUBJECT", &self.subject),
            ("TEXT", &self.text),
        ] {
            if let Some(value) = value {
                ascii &= value.as_str().is_ascii();
                keys.push(format!("{name} {}", value.quoted()));
            }
        }
        if let Some(date) = self.since {
            keys.push(format!("SINCE {}", date.imap()));
        }
        if let Some(date) = self.before {
            keys.push(format!("BEFORE {}", date.imap()));
        }
        if self.unseen {
            keys.push("UNSEEN".to_string());
        }
        if self.flagged {
            keys.push("FLAGGED".to_string());
        }
        if keys.is_empty() {
            keys.push("ALL".to_string());
        }
        let criteria = keys.join(" ");
        if ascii {
            criteria
        } else {
            format!("CHARSET UTF-8 {criteria}")
        }
    }
}

/// Result of `search`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchResult {
    /// Login name of the Account.
    pub account: String,
    /// The searched mailbox, decoded UTF-8.
    pub mailbox: String,
    /// How many messages matched.
    pub total: usize,
    /// Whether more matched than are returned.
    pub more: bool,
    /// The newest matches (UID descending), at most `limit`.
    pub messages: Vec<MessageSummary>,
}

/// One message in a [`SearchResult`]. Header data only, never a body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MessageSummary {
    /// The message reference, see [`MessageRef`].
    #[serde(rename = "ref")]
    pub reference: String,
    /// Internal date, RFC 3339 UTC.
    pub date: String,
    /// First From address, `Name <addr>` or `addr`; `""` when absent.
    pub from: String,
    /// RFC 2047-decoded subject; `""` when absent.
    pub subject: String,
    /// The server's flags, in server order.
    pub flags: Vec<String>,
}

/// A message reference: Account, mailbox, UIDVALIDITY and UID.
///
/// Its string form is `<account>/<mailbox>/<uidvalidity>/<uid>`. The
/// account and mailbox are encoded as RFC 3986 path segments: ASCII letters
/// and digits and `-._~!$&'()*+,;=:@` stay as they are, every other byte of
/// the UTF-8 (including `%`, `/`, `[`, `]`, space and every byte outside
/// printable ASCII) becomes `%XX` with upper-case hex. So the form always
/// has exactly three unencoded `/`.
///
/// Serialised as its four fields. A deserialised reference is untrusted:
/// the core checks it again with [`crate::read::is_valid_reference`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageRef {
    /// Login name of the Account.
    pub account: String,
    /// Decoded UTF-8 mailbox name.
    pub mailbox: String,
    /// The mailbox's UIDVALIDITY.
    pub uidvalidity: u32,
    /// The message's UID.
    pub uid: u32,
}

/// A string is not a message reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidReference;

impl MessageRef {
    /// The reference's string form.
    pub fn format(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            encode_segment(&self.account),
            encode_segment(&self.mailbox),
            self.uidvalidity,
            self.uid
        )
    }

    /// Parses a string made by [`MessageRef::format`]. Anything else, such
    /// as a missing or extra segment, an empty account or mailbox, a zero
    /// or non-decimal number, or non-canonical encoding, is refused.
    pub fn parse(text: &str) -> Result<Self, InvalidReference> {
        let parts: Vec<&str> = text.split('/').collect();
        let [account, mailbox, uidvalidity, uid] = parts.as_slice() else {
            return Err(InvalidReference);
        };
        let reference = Self {
            account: decode_segment(account)?,
            mailbox: decode_segment(mailbox)?,
            uidvalidity: decimal(uidvalidity)?,
            uid: decimal(uid)?,
        };
        if reference.account.is_empty()
            || reference.mailbox.is_empty()
            || reference.format() != text
        {
            return Err(InvalidReference);
        }
        Ok(reference)
    }
}

fn encode_segment(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if is_pchar(b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// RFC 3986 `pchar` without `pct-encoded`: unreserved, sub-delims, `:`, `@`.
fn is_pchar(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@".contains(&b)
}

fn decode_segment(text: &str) -> Result<String, InvalidReference> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3).ok_or(InvalidReference)?;
            let hex = std::str::from_utf8(hex).map_err(|_| InvalidReference)?;
            out.push(u8::from_str_radix(hex, 16).map_err(|_| InvalidReference)?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| InvalidReference)
}

fn decimal(text: &str) -> Result<u32, InvalidReference> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(InvalidReference);
    }
    match text.parse() {
        Ok(0) | Err(_) => Err(InvalidReference),
        Ok(n) => Ok(n),
    }
}

/// Decodes RFC 2047 encoded-words (`=?charset?Q|B?text?=`) in a header
/// value. UTF-8, US-ASCII and ISO-8859-1 are decoded; other charsets and
/// malformed words are left as they are. Whitespace between two adjacent
/// encoded-words is dropped.
pub fn decode_rfc2047(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    let mut after_word = false;
    while let Some(at) = rest.find("=?") {
        let (before, from) = rest.split_at(at);
        match encoded_word(from) {
            Some((decoded, len)) => {
                if !(after_word && before.chars().all(char::is_whitespace)) {
                    out.push_str(before);
                }
                out.push_str(&decoded);
                rest = &from[len..];
                after_word = true;
            }
            None => {
                out.push_str(before);
                out.push_str("=?");
                rest = &from[2..];
                after_word = false;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Decodes the encoded-word at the start of `text`; returns it and its
/// length.
fn encoded_word(text: &str) -> Option<(String, usize)> {
    let body = text.strip_prefix("=?")?;
    let charset_end = body.find('?')?;
    let charset = &body[..charset_end];
    let after = &body[charset_end + 1..];
    // Both bytes ASCII, so slicing after them stays on a char boundary.
    let (&encoding, &mark) = (after.as_bytes().first()?, after.as_bytes().get(1)?);
    if mark != b'?' {
        return None;
    }
    let encoding = char::from(encoding).to_ascii_uppercase();
    let payload_all = &after[2..];
    let payload_end = payload_all.find('?')?;
    if !payload_all[payload_end..].starts_with("?=") {
        return None;
    }
    let payload = &payload_all[..payload_end];
    if charset.is_empty() || charset.contains(char::is_whitespace) {
        return None;
    }
    if payload.contains(char::is_whitespace) {
        return None;
    }
    let bytes = match encoding {
        'Q' => decode_q(payload)?,
        'B' => {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(payload)
                .ok()?
        }
        _ => return None,
    };
    // RFC 2231 allows a language suffix: `UTF-8*en`.
    let charset = charset.split('*').next()?.to_ascii_lowercase();
    let decoded = match charset.as_str() {
        "utf-8" | "utf8" => String::from_utf8(bytes).ok()?,
        "us-ascii" if bytes.is_ascii() => String::from_utf8(bytes).ok()?,
        "iso-8859-1" | "latin1" => bytes.into_iter().map(char::from).collect(),
        _ => return None,
    };
    let len = 2 + charset_end + 1 + 2 + payload_end + 2;
    Some((decoded, len))
}

fn decode_q(payload: &str) -> Option<Vec<u8>> {
    let bytes = payload.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'_' => {
                out.push(b' ');
                i += 1;
            }
            b'=' => {
                let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    Some(out)
}

/// `Name <addr>` when the address has a display name, else `addr`. The
/// name is RFC 2047-decoded.
pub fn format_address(name: Option<&str>, address: &str) -> String {
    match name.map(decode_rfc2047) {
        Some(name) if !name.trim().is_empty() => format!("{name} <{address}>"),
        _ => address.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_round_trips_and_encodes_separators() {
        let reference = MessageRef {
            account: "a/b%c@example.test".to_string(),
            mailbox: "[Gmail]/Entwürfe".to_string(),
            uidvalidity: 4_294_967_295,
            uid: 1,
        };
        let text = reference.format();
        assert_eq!(
            text,
            "a%2Fb%25c@example.test/%5BGmail%5D%2FEntw%C3%BCrfe/4294967295/1"
        );
        assert_eq!(text.matches('/').count(), 3);
        assert_eq!(MessageRef::parse(&text), Ok(reference));
    }

    #[test]
    fn reference_encodes_spaces_and_controls() {
        let reference = MessageRef {
            account: "agent@example.test".to_string(),
            mailbox: "My Box\n\"x\"".to_string(),
            uidvalidity: 7,
            uid: 9,
        };
        let text = reference.format();
        assert_eq!(text, "agent@example.test/My%20Box%0A%22x%22/7/9");
        assert_eq!(MessageRef::parse(&text), Ok(reference));
    }

    #[test]
    fn malformed_references_are_refused() {
        for bad in [
            "INBOX/1/2",
            "a@b/INBOX/x/2",
            "a@b/INBOX/1/0",
            "a@b/INBOX/1/2/3",
            "a@b/INBOX/0/2",
            "/INBOX/1/2",
            "a@b//1/2",
            "a@b/INBOX/+1/2",
            "a@b/INBOX/1/4294967296",
            "a@b/IN%2/1/2",
            "a@b/IN%zz/1/2",
            "a@b/Entw%c3%bcrfe/1/2",
            "a@b/My Box/1/2",
            "a@b/[Gmail]/1/2",
            "a@b/%41/1/2",
            "a@b/%FF/1/2",
            "",
        ] {
            assert_eq!(MessageRef::parse(bad), Err(InvalidReference), "{bad:?}");
        }
    }

    #[test]
    fn search_text_refuses_line_breaks_and_nul() {
        for bad in ["a\rb", "a\nb", "a\0b"] {
            assert!(SearchText::new(bad).is_err(), "{bad:?}");
        }
        let text = SearchText::new("x\" ALL \"y\\").unwrap();
        assert_eq!(text.quoted(), "\"x\\\" ALL \\\"y\\\\\"");
    }

    #[test]
    fn dates_parse_strictly() {
        assert_eq!(Date::parse("2026-10-05").unwrap().imap(), "05-Oct-2026");
        assert_eq!(Date::parse("2024-02-29").unwrap().imap(), "29-Feb-2024");
        for bad in [
            "2026-13-01",
            "10/05/2026",
            "2026-02-29",
            "2026-04-31",
            "2026-00-10",
            "2026-10-00",
            "0000-01-01",
            "2026-1-01",
            "2026-10-05 ",
            "+026-10-05",
        ] {
            assert!(Date::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn limits_are_bounded() {
        assert_eq!(Limit::parse("1").unwrap().get(), 1);
        assert_eq!(Limit::parse("1000").unwrap().get(), 1000);
        assert_eq!(Limit::default().get(), 20);
        for bad in ["0", "1001", "x", "", "-1", "+5", "99999999999"] {
            assert!(Limit::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn criteria_combine_and_mark_utf8() {
        let mailbox = SearchText::new("INBOX").unwrap();
        assert_eq!(Query::all(mailbox.clone()).imap_criteria(), "ALL");
        let query = Query {
            from: Some(SearchText::new("alice").unwrap()),
            subject: Some(SearchText::new("für").unwrap()),
            since: Some(Date::parse("2026-10-05").unwrap()),
            unseen: true,
            flagged: true,
            ..Query::all(mailbox)
        };
        assert_eq!(
            query.imap_criteria(),
            "CHARSET UTF-8 FROM \"alice\" SUBJECT \"für\" SINCE 05-Oct-2026 UNSEEN FLAGGED"
        );
    }

    #[test]
    fn gmail_queries_are_sent_raw_and_quoted() {
        let gmail = |q: &str| Query {
            gmail: Some(SearchText::new(q).unwrap()),
            ..Query::all(SearchText::new("INBOX").unwrap())
        };
        assert_eq!(
            gmail("from:alice has:attachment").imap_criteria(),
            "X-GM-RAW \"from:alice has:attachment\""
        );
        assert_eq!(
            gmail("von:jürgen").imap_criteria(),
            "CHARSET UTF-8 X-GM-RAW \"von:jürgen\""
        );
        assert_eq!(
            gmail("subject:\"a\\b\"").imap_criteria(),
            "X-GM-RAW \"subject:\\\"a\\\\b\\\"\""
        );
    }

    #[test]
    fn rfc2047_words_decode() {
        assert_eq!(
            decode_rfc2047("=?UTF-8?Q?Rechnung_f=C3=BCr_Oktober?="),
            "Rechnung für Oktober"
        );
        assert_eq!(decode_rfc2047("=?utf-8?B?w7w=?= =?utf-8?q?x?="), "üx");
        assert_eq!(
            decode_rfc2047("Re: =?ISO-8859-1?Q?f=FCr?= you"),
            "Re: für you"
        );
        assert_eq!(decode_rfc2047("plain =? text"), "plain =? text");
        assert_eq!(decode_rfc2047("=?KOI8-R?Q?x?="), "=?KOI8-R?Q?x?=");
        assert_eq!(decode_rfc2047("=?UTF-8?Q?bad=ZZ?="), "=?UTF-8?Q?bad=ZZ?=");
    }

    #[test]
    fn addresses_format_with_and_without_names() {
        assert_eq!(
            format_address(Some("Alice Example"), "alice@example.com"),
            "Alice Example <alice@example.com>"
        );
        assert_eq!(
            format_address(None, "alice@example.com"),
            "alice@example.com"
        );
        assert_eq!(format_address(Some(""), "a@b"), "a@b");
    }
}
