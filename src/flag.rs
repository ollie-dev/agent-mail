//! Setting and clearing the two flags an Agent may change: `\Seen` (the
//! message is read) and `\Flagged` (the message is starred).
//!
//! The flag name an Agent passes is untrusted. [`Flag::parse`] maps it to
//! one of two fixed values; only their canonical IMAP names ever reach the
//! server, never the Agent's text.

use serde::{Deserialize, Serialize};

/// A flag an Agent may set or clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    /// `\Seen`: the message is read.
    Seen,
    /// `\Flagged`: the message is starred.
    Flagged,
}

impl Flag {
    /// Parses an untrusted flag name: `seen` or `\Seen`, `flagged` or
    /// `\Flagged`, ASCII case-insensitively. `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        let name = text.strip_prefix('\\').unwrap_or(text);
        if name.eq_ignore_ascii_case("seen") {
            Some(Self::Seen)
        } else if name.eq_ignore_ascii_case("flagged") {
            Some(Self::Flagged)
        } else {
            None
        }
    }

    /// The canonical IMAP name, the only form sent to the server.
    pub fn imap_name(self) -> &'static str {
        match self {
            Self::Seen => "\\Seen",
            Self::Flagged => "\\Flagged",
        }
    }
}

/// Whether a flag is set or cleared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlagAction {
    /// Add the flag (`+FLAGS`).
    Set,
    /// Remove the flag (`-FLAGS`).
    Clear,
}

impl FlagAction {
    /// `+` for [`FlagAction::Set`], `-` for [`FlagAction::Clear`].
    pub fn sign(self) -> char {
        match self {
            Self::Set => '+',
            Self::Clear => '-',
        }
    }
}

/// The audit log's `change` value, e.g. `+\Seen` or `-\Flagged`.
pub fn change(action: FlagAction, flag: Flag) -> String {
    format!("{}{}", action.sign(), flag.imap_name())
}

/// The `UID STORE` data item and value, e.g. `+FLAGS (\Seen)`. Built only
/// from the two fixed flag names.
pub fn store_item(action: FlagAction, flag: Flag) -> String {
    format!("{}FLAGS ({})", action.sign(), flag.imap_name())
}

/// Result of `flag`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlagResult {
    /// The message reference, as given.
    #[serde(rename = "ref")]
    pub reference: String,
    /// Login name of the Account.
    pub account: String,
    /// The mailbox, decoded UTF-8.
    pub mailbox: String,
    /// Every flag of the message after the change, in server order.
    pub flags: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_seen_and_flagged() {
        for (text, flag) in [
            ("seen", Flag::Seen),
            ("SEEN", Flag::Seen),
            ("\\Seen", Flag::Seen),
            ("\\SEEN", Flag::Seen),
            ("flagged", Flag::Flagged),
            ("\\Flagged", Flag::Flagged),
            ("FlAgGeD", Flag::Flagged),
        ] {
            assert_eq!(Flag::parse(text), Some(flag), "{text:?}");
        }
        for text in [
            "",
            "\\",
            "\\\\Seen",
            "deleted",
            "\\Deleted",
            "\\Answered",
            "\\Draft",
            "$Junk",
            "seen\r\n",
            " seen",
            "seen)",
            "Seen \\Deleted",
        ] {
            assert_eq!(Flag::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn change_and_store_item_use_canonical_names() {
        assert_eq!(change(FlagAction::Set, Flag::Seen), "+\\Seen");
        assert_eq!(change(FlagAction::Clear, Flag::Flagged), "-\\Flagged");
        assert_eq!(store_item(FlagAction::Set, Flag::Seen), "+FLAGS (\\Seen)");
        assert_eq!(
            store_item(FlagAction::Clear, Flag::Flagged),
            "-FLAGS (\\Flagged)"
        );
    }
}
