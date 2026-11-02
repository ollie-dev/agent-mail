//! The built-in list of Providers: mail services whose IMAP settings
//! `mandate create` fills in, so the human types only the address and the
//! Credential.
//!
//! This module loads `data/providers.json`, validates it, builds the picker
//! labels and search keys, and looks a Provider up by mail domain.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::mandate::TlsMode;
use crate::terminal::SearchOption;

/// The Provider list, compiled into the binary. It is deliberately not read
/// at runtime: a list read from a file, the environment or the network could
/// send the Credential the human is about to type to another server. The
/// host a Credential goes to comes only from data inside the signed binary,
/// so changing the list means a new release.
const PROVIDERS_JSON: &str = include_str!("../data/providers.json");

/// The longest `note` allowed, in characters.
pub const NOTE_MAX: usize = 300;

/// The most domains a picker label shows.
const LABEL_DOMAINS: usize = 3;

/// A mail service with known IMAP settings.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provider {
    /// A unique lowercase slug.
    pub id: String,
    /// The display name.
    pub name: String,
    /// The service's own mail domains, in data order. May be empty.
    pub domains: Vec<String>,
    /// Where and how to connect.
    pub imap: ImapSettings,
    /// Which Credential the service wants.
    pub credential: CredentialKind,
    /// Shown before the Credential prompt, if present.
    #[serde(default)]
    pub note: Option<String>,
}

/// A Provider's IMAP server.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImapSettings {
    /// The server host.
    pub host: String,
    /// The server port, never 0.
    pub port: u16,
    /// How the connection is secured.
    pub tls: TlsMode,
}

/// The kind of Credential a Provider wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    /// The account password.
    Password,
    /// An app-specific password.
    AppPassword,
}

impl CredentialKind {
    /// The Credential prompt's opening words.
    pub fn prompt(self) -> &'static str {
        match self {
            Self::Password => "Password",
            Self::AppPassword => "App password",
        }
    }
}

impl Provider {
    /// The picker label: `Name (d1, d2, d3, …)`, at most three domains in
    /// data order, `, …` only when there are more, just `Name` without any.
    pub fn label(&self) -> String {
        if self.domains.is_empty() {
            return self.name.clone();
        }
        let shown: Vec<&str> = self
            .domains
            .iter()
            .take(LABEL_DOMAINS)
            .map(String::as_str)
            .collect();
        let more = if self.domains.len() > LABEL_DOMAINS {
            ", …"
        } else {
            ""
        };
        format!("{} ({}{more})", self.name, shown.join(", "))
    }

    /// The picker option: the label, searchable by the name and every
    /// domain, shown or not.
    pub fn search_option(&self) -> SearchOption {
        let mut keys = vec![self.name.clone()];
        keys.extend(self.domains.iter().cloned());
        SearchOption {
            label: self.label(),
            keys,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderList {
    providers: Vec<Provider>,
}

/// Parses and validates a Provider list in the `{ "providers": [...] }`
/// shape. Fails on the first broken rule.
pub fn parse(json: &str) -> Result<Vec<Provider>, String> {
    let list: ProviderList =
        serde_json::from_str(json).map_err(|e| format!("invalid provider list: {e}"))?;
    let mut ids = HashSet::new();
    let mut domains = HashSet::new();
    for provider in &list.providers {
        let id = &provider.id;
        if !is_slug(id) {
            return Err(format!("provider id {id:?} is not a lowercase slug"));
        }
        if !ids.insert(id.clone()) {
            return Err(format!("provider id {id:?} appears twice"));
        }
        if provider.name.trim().is_empty() {
            return Err(format!("provider {id:?} has no name"));
        }
        for domain in &provider.domains {
            if !domains.insert(domain.to_lowercase()) {
                return Err(format!("domain {domain:?} appears twice"));
            }
        }
        if provider.imap.host.trim().is_empty() {
            return Err(format!("provider {id:?} has an empty IMAP host"));
        }
        if provider.imap.port == 0 {
            return Err(format!("provider {id:?} has IMAP port 0"));
        }
        if let Some(note) = &provider.note
            && note.chars().count() > NOTE_MAX
        {
            return Err(format!(
                "provider {id:?} has a note over {NOTE_MAX} characters"
            ));
        }
    }
    Ok(list.providers)
}

/// Whether `id` matches `^[a-z0-9]+(-[a-z0-9]+)*$`.
fn is_slug(id: &str) -> bool {
    !id.is_empty()
        && id.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// Every built-in Provider, in data order.
///
/// # Panics
///
/// If the built-in list is invalid, which a test rules out.
pub fn all() -> &'static [Provider] {
    static PROVIDERS: OnceLock<Vec<Provider>> = OnceLock::new();
    PROVIDERS.get_or_init(|| parse(PROVIDERS_JSON).expect("the built-in provider list is valid"))
}

/// The built-in Providers sorted case-insensitively by name, as the picker
/// shows them.
pub fn sorted() -> Vec<&'static Provider> {
    let mut providers: Vec<&Provider> = all().iter().collect();
    providers.sort_by_key(|p| p.name.to_lowercase());
    providers
}

/// The built-in Provider whose domains include `domain` (case-insensitive).
pub fn by_domain(domain: &str) -> Option<&'static Provider> {
    let domain = domain.trim().to_lowercase();
    all()
        .iter()
        .find(|p| p.domains.iter().any(|d| d.to_lowercase() == domain))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_follow_the_pattern() {
        for good in ["gmx", "gmx-com", "1und1", "a-b-c"] {
            assert!(is_slug(good), "{good}");
        }
        for bad in ["", "GMX", "-a", "a-", "a--b", "a_b", "a.b"] {
            assert!(!is_slug(bad), "{bad}");
        }
    }

    #[test]
    fn domain_lookup_is_case_insensitive() {
        assert_eq!(by_domain("GMX.de").map(|p| p.id.as_str()), Some("gmx"));
        assert_eq!(by_domain("example.test"), None);
    }
}
