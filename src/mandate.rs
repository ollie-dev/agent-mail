//! The Mandate model: the Account it covers, its Credential and its Policy.
//!
//! A Mandate is immutable once sealed (ADR-0003). These types only describe
//! its contents; sealing lives in [`crate::seal`].

use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A sealed, unchangeable authority over one Account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mandate {
    /// The Account this Mandate covers.
    pub account: Account,
    /// What the Agent may do on that Account.
    pub policy: Policy,
}

/// A mail account agent-mail connects to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// IMAP login name; also identifies the Account in Grants and output.
    pub login: String,
    /// IMAP server host name.
    pub host: String,
    /// IMAP server port.
    pub port: u16,
    /// How the connection is secured. There is no plaintext option.
    pub tls: TlsMode,
    /// The secret that unlocks the Account.
    pub credential: Credential,
}

/// How the IMAP connection is secured. Deliberately has no plaintext variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TlsMode {
    /// TLS from the first byte (usually port 993).
    Implicit,
    /// Plain connect, then a mandatory `STARTTLS` upgrade before login.
    Starttls,
}

/// The secret that unlocks an Account. Its `Debug` output is redacted and its
/// memory is wiped when dropped.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Credential(String);

impl Credential {
    /// Wraps a Credential.
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// The secret itself, for the IMAP login only.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl Zeroize for Credential {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl Drop for Credential {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl ZeroizeOnDrop for Credential {}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Credential(<redacted>)")
    }
}

/// The set of Grants inside one Mandate; anything not granted is denied.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// The Grants. An empty list denies everything.
    pub grants: Vec<Grant>,
}

/// One permission: an operation on an Account's mailboxes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    /// The operation allowed.
    pub operation: Operation,
    /// Login name of the Account the Grant applies to.
    pub account: String,
    /// The mailboxes the Grant covers.
    pub mailboxes: Vec<MailboxPattern>,
}

/// Mail operations a Grant can allow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    /// List mailboxes.
    List,
    /// Search messages.
    Search,
    /// Read messages.
    Read,
    /// Set or clear flags.
    Flag,
    /// Move messages.
    Move,
    /// Create drafts.
    Draft,
}

impl Operation {
    /// The operation's wire name, as used in Grants and errors.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Search => "search",
            Self::Read => "read",
            Self::Flag => "flag",
            Self::Move => "move",
            Self::Draft => "draft",
        }
    }
}

/// A mailbox in a Grant: an exact UTF-8 name, or `*` for every mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum MailboxPattern {
    /// Every mailbox (`*`).
    All,
    /// Exactly this decoded UTF-8 mailbox name.
    Exact(String),
}

impl MailboxPattern {
    /// Whether this pattern covers the decoded mailbox `name`.
    pub fn matches(&self, name: &str) -> bool {
        match self {
            Self::All => true,
            Self::Exact(exact) => exact == name,
        }
    }
}

impl From<String> for MailboxPattern {
    fn from(value: String) -> Self {
        if value == "*" {
            Self::All
        } else {
            Self::Exact(value)
        }
    }
}

impl From<&str> for MailboxPattern {
    fn from(value: &str) -> Self {
        Self::from(value.to_string())
    }
}

impl From<MailboxPattern> for String {
    fn from(value: MailboxPattern) -> Self {
        match value {
            MailboxPattern::All => "*".to_string(),
            MailboxPattern::Exact(name) => name,
        }
    }
}

impl Policy {
    /// The Grants for `operation` on the Account with login `account`.
    pub fn grants_for<'a>(
        &'a self,
        operation: Operation,
        account: &'a str,
    ) -> impl Iterator<Item = &'a Grant> + 'a {
        self.grants
            .iter()
            .filter(move |g| g.operation == operation && g.account == account)
    }

    /// Whether any Grant allows `operation` on `mailbox` of `account`.
    pub fn allows_mailbox(&self, operation: Operation, account: &str, mailbox: &str) -> bool {
        self.grants_for(operation, account)
            .any(|g| g.mailboxes.iter().any(|m| m.matches(mailbox)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wiped_on_drop<T: ZeroizeOnDrop>() {}

    #[test]
    fn credential_is_wiped_on_drop_and_zeroizes() {
        wiped_on_drop::<Credential>();
        let mut credential = Credential::new("fake-app-password-123");
        credential.zeroize();
        assert_eq!(credential.expose(), "");
    }
}
