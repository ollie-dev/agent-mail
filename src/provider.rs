//! The built-in list of Providers: mail services whose IMAP settings
//! `mandate create` fills in, so the human types only the address and the
//! Credential, plus unsupported Providers that the picker lists with the
//! reason they cannot work.
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

/// The longest `unsupported` reason allowed, in characters.
pub const REASON_MAX: usize = 300;

/// What an unsupported Provider's picker label ends with: a space, an en
/// dash, a space, then `not supported`.
pub const UNSUPPORTED_SUFFIX: &str = " \u{2013} not supported";

/// The most domains a picker label shows.
const LABEL_DOMAINS: usize = 3;

/// A mail service: either one with known IMAP settings and Credential kind,
/// or an unsupported one with the reason it cannot work.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provider {
    /// A unique lowercase slug.
    pub id: String,
    /// The display name.
    pub name: String,
    /// The service's own mail domains, in data order. May be empty.
    pub domains: Vec<String>,
    /// Where and how to connect. Present exactly when `unsupported` is not.
    #[serde(default)]
    pub imap: Option<ImapSettings>,
    /// Which Credential the service wants. Present exactly when
    /// `unsupported` is not.
    #[serde(default)]
    pub credential: Option<CredentialKind>,
    /// Why the service cannot work with agent-mail, if it cannot.
    #[serde(default)]
    pub unsupported: Option<String>,
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
    /// The IMAP settings and Credential kind of a supported Provider; `None`
    /// for an unsupported one.
    pub fn connection(&self) -> Option<(&ImapSettings, CredentialKind)> {
        match (&self.unsupported, &self.imap, self.credential) {
            (None, Some(imap), Some(credential)) => Some((imap, credential)),
            _ => None,
        }
    }

    /// The picker label: `Name (d1, d2, d3, …)`, at most three domains in
    /// data order, `, …` only when there are more, just `Name` without any.
    /// An unsupported Provider's label ends with [`UNSUPPORTED_SUFFIX`].
    pub fn label(&self) -> String {
        let suffix = if self.unsupported.is_some() {
            UNSUPPORTED_SUFFIX
        } else {
            ""
        };
        if self.domains.is_empty() {
            return format!("{}{suffix}", self.name);
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
        format!("{} ({}{more}){suffix}", self.name, shown.join(", "))
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
        match (&provider.unsupported, &provider.imap, &provider.credential) {
            (Some(reason), None, None) => {
                if reason.trim().is_empty() {
                    return Err(format!("provider {id:?} has an empty unsupported reason"));
                }
                if reason.chars().count() > REASON_MAX {
                    return Err(format!(
                        "provider {id:?} has an unsupported reason over {REASON_MAX} characters"
                    ));
                }
            }
            (Some(_), _, _) => {
                return Err(format!(
                    "provider {id:?} is unsupported but has IMAP settings or a credential kind"
                ));
            }
            (None, Some(imap), Some(_)) => {
                if imap.host.trim().is_empty() {
                    return Err(format!("provider {id:?} has an empty IMAP host"));
                }
                if imap.port == 0 {
                    return Err(format!("provider {id:?} has IMAP port 0"));
                }
            }
            (None, _, _) => {
                return Err(format!(
                    "provider {id:?} needs both IMAP settings and a credential kind, or an unsupported reason"
                ));
            }
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

/// The supported built-in Provider a login belongs to, with its IMAP host.
/// The domain is the exact text after the login's last `@`; a login without
/// `@`, an unknown domain or an unsupported Provider gives `None`.
pub fn expected_for_login(login: &str) -> Option<(&'static Provider, &'static str)> {
    let (_, domain) = login.trim().rsplit_once('@')?;
    let provider = by_domain(domain)?;
    let (imap, _) = provider.connection()?;
    Some((provider, imap.host.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_host_follows_the_exact_domain_after_the_last_at() {
        let host = |login: &str| expected_for_login(login).map(|(_, h)| h);
        assert_eq!(host("Someone@GMail.com"), Some("imap.gmail.com"));
        assert_eq!(host("someone@googlemail.com"), Some("imap.gmail.com"));
        assert_eq!(host("a@b@web.de"), Some("imap.web.de"));
        assert_eq!(host("x@cox.net"), Some("imap.mail.yahoo.com"));
        assert_eq!(host("agent@example.test"), None);
        assert_eq!(host("someone@gmail.com.evil.test"), None);
        assert_eq!(host("gmail.com"), None);
    }

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
