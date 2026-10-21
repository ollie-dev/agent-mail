//! Modified UTF-7 (RFC 3501 section 5.1.3) decoding and encoding of IMAP
//! mailbox names.
//!
//! Mailbox names come from the server and are untrusted: malformed input
//! returns an error and never panics.

/// A mailbox name that is not valid modified UTF-7.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utf7Error;

/// Decodes a modified UTF-7 mailbox name into UTF-8.
///
/// Characters outside printable ASCII are passed through unchanged, so names
/// from servers that already send UTF-8 survive.
pub fn decode(input: &str) -> Result<String, Utf7Error> {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c != '&' {
            out.push(c);
            continue;
        }
        let mut units: Vec<u16> = Vec::new();
        let mut bits: u32 = 0;
        let mut nbits: u32 = 0;
        let mut empty = true;
        loop {
            let Some(c) = chars.next() else {
                return Err(Utf7Error);
            };
            if c == '-' {
                break;
            }
            empty = false;
            let value = sextet(c).ok_or(Utf7Error)?;
            bits = (bits << 6) | value;
            nbits += 6;
            if nbits >= 16 {
                nbits -= 16;
                units.push((bits >> nbits) as u16);
                bits &= (1 << nbits) - 1;
            }
        }
        if empty {
            out.push('&');
            continue;
        }
        // Leftover padding must be fewer than six bits, all zero.
        if nbits >= 6 || bits != 0 || units.is_empty() {
            return Err(Utf7Error);
        }
        for decoded in char::decode_utf16(units) {
            let ch = decoded.map_err(|_| Utf7Error)?;
            // Printable ASCII must not be base64-encoded.
            if (' '..='~').contains(&ch) {
                return Err(Utf7Error);
            }
            out.push(ch);
        }
    }
    Ok(out)
}

/// Encodes a UTF-8 mailbox name as modified UTF-7, for sending to the
/// server. Printable ASCII stays as it is (`&` becomes `&-`); every other
/// run of characters becomes `&`, modified base64 of its UTF-16, `-`.
pub fn encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut run: Vec<u16> = Vec::new();
    for c in input.chars() {
        if (' '..='~').contains(&c) {
            flush(&mut run, &mut out);
            if c == '&' {
                out.push_str("&-");
            } else {
                out.push(c);
            }
        } else {
            let mut units = [0u16; 2];
            run.extend_from_slice(c.encode_utf16(&mut units));
        }
    }
    flush(&mut run, &mut out);
    out
}

/// Appends `run` as one base64 section and empties it.
fn flush(run: &mut Vec<u16>, out: &mut String) {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+,";
    if run.is_empty() {
        return;
    }
    out.push('&');
    let mut bits: u32 = 0;
    let mut nbits: u32 = 0;
    for unit in run.drain(..) {
        bits = (bits << 16) | u32::from(unit);
        nbits += 16;
        while nbits >= 6 {
            nbits -= 6;
            out.push(char::from(ALPHABET[((bits >> nbits) & 0x3f) as usize]));
        }
        bits &= (1 << nbits) - 1;
    }
    if nbits > 0 {
        out.push(char::from(
            ALPHABET[((bits << (6 - nbits)) & 0x3f) as usize],
        ));
    }
    out.push('-');
}

fn sextet(c: char) -> Option<u32> {
    let v = match c {
        'A'..='Z' => c as u32 - 'A' as u32,
        'a'..='z' => c as u32 - 'a' as u32 + 26,
        '0'..='9' => c as u32 - '0' as u32 + 52,
        '+' => 62,
        ',' => 63,
        _ => return None,
    };
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_examples() {
        assert_eq!(decode("INBOX").unwrap(), "INBOX");
        assert_eq!(decode("[Gmail]/Entw&APw-rfe").unwrap(), "[Gmail]/Entwürfe");
        assert_eq!(decode("A&-B").unwrap(), "A&B");
        assert_eq!(decode("&ZeVnLIqe-").unwrap(), "日本語");
        assert_eq!(decode("&2D3eAA-").unwrap(), "\u{1F600}");
    }

    #[test]
    fn encodes_examples() {
        assert_eq!(encode("INBOX"), "INBOX");
        assert_eq!(encode("[Gmail]/Entwürfe"), "[Gmail]/Entw&APw-rfe");
        assert_eq!(encode("A&B"), "A&-B");
        assert_eq!(encode("日本語"), "&ZeVnLIqe-");
        assert_eq!(encode("\u{1F600}"), "&2D3eAA-");
        assert_eq!(encode("a\tb"), "a&AAk-b");
    }

    #[test]
    fn encode_round_trips_through_decode() {
        for name in [
            "",
            "INBOX",
            "[Gmail]/Entwürfe",
            "A&B&&",
            "日本語/Ünterordner",
            "\u{1F600}x\u{1F601}",
            "ü",
            "üü",
            "üüü",
            "Tab\there",
            "~!\"#$%'()*+,-./",
        ] {
            assert_eq!(decode(&encode(name)).as_deref(), Ok(name), "{name:?}");
        }
    }

    #[test]
    fn rejects_malformed_without_panicking() {
        for bad in [
            "&", "&APw", "&A-", "&AP!-", "&APx-", "&AEE-", "&2D0-", "&3gA-", "&&-",
        ] {
            assert_eq!(decode(bad), Err(Utf7Error), "{bad:?}");
        }
    }
}
