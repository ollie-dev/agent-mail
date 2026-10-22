//! Moving one message between two granted mailboxes of the same Account.
//!
//! The module is named `move_message` because `move` is a Rust keyword.

use serde::Serialize;

use crate::terminal::escape_controls;

/// Result of `move`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MoveResult {
    /// The message reference, as given.
    #[serde(rename = "ref")]
    pub reference: String,
    /// Login name of the Account.
    pub account: String,
    /// The source mailbox, decoded UTF-8.
    pub mailbox: String,
    /// The target mailbox, as given.
    pub target: String,
    /// The message's reference in the target, built from the server's
    /// `COPYUID` response code; `None` (JSON `null`) when the server
    /// reported none.
    pub new_ref: Option<String>,
}

impl MoveResult {
    /// The `--text` line, without its `\n`: `<ref> -> <new_ref>`, or
    /// `<ref> -> <target> (new reference not reported)`. Every value has its
    /// control characters escaped.
    pub fn text_line(&self) -> String {
        match &self.new_ref {
            Some(new_ref) => format!(
                "{} -> {}",
                escape_controls(&self.reference),
                escape_controls(new_ref)
            ),
            None => format!(
                "{} -> {} (new reference not reported)",
                escape_controls(&self.reference),
                escape_controls(&self.target)
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(target: &str, new_ref: Option<&str>) -> MoveResult {
        MoveResult {
            reference: "agent@example.test/INBOX/1700000001/2".to_string(),
            account: "agent@example.test".to_string(),
            mailbox: "INBOX".to_string(),
            target: target.to_string(),
            new_ref: new_ref.map(str::to_string),
        }
    }

    #[test]
    fn json_keys_are_in_order_and_new_ref_may_be_null() {
        let json = serde_json::to_string(&result("Archive", None)).unwrap();
        assert_eq!(
            json,
            "{\"ref\":\"agent@example.test/INBOX/1700000001/2\",\"account\":\"agent@example.test\",\"mailbox\":\"INBOX\",\"target\":\"Archive\",\"new_ref\":null}"
        );
    }

    #[test]
    fn text_line_escapes_controls() {
        assert_eq!(
            result("Archive", Some("agent@example.test/Archive/1700000002/26")).text_line(),
            "agent@example.test/INBOX/1700000001/2 -> agent@example.test/Archive/1700000002/26"
        );
        assert_eq!(
            result("Ar\u{1b}[2Jchive", None).text_line(),
            "agent@example.test/INBOX/1700000001/2 -> Ar\\u{1b}[2Jchive (new reference not reported)"
        );
    }
}
