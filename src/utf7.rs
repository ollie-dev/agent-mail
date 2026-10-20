//! Modified UTF-7 (RFC 3501 section 5.1.3) decoding of IMAP mailbox names.
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
    fn rejects_malformed_without_panicking() {
        for bad in [
            "&", "&APw", "&A-", "&AP!-", "&APx-", "&AEE-", "&2D0-", "&3gA-", "&&-",
        ] {
            assert_eq!(decode(bad), Err(Utf7Error), "{bad:?}");
        }
    }
}
