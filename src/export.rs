//! The Mandate file: a Mandate's Accounts (server, port, TLS mode, login) and
//! Grants as TOML, written at the end of `mandate create` and read back by
//! `mandate create --from`.
//!
//! These types are separate from [`crate::mandate::Account`] and have no
//! Credential field: the file never holds a Credential or an Agent Token.
//!
//! A file passed to `--from` is untrusted input. It is checked key by key
//! with source positions, so every problem is reported by line, and no key
//! name or value from the file is echoed (a pasted secret, even as a key,
//! must not end up in an error message).

use std::ops::Range;
use std::path::Path;

use serde::{Deserialize, Serialize};
use toml::Spanned;
use toml::de::{DeTable, DeValue};

use crate::mandate::{Mandate, Operation, TlsMode};
use crate::terminal::escape_controls;

/// The largest file `--from` reads. A Mandate file is a few hundred bytes.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// A Mandate as exported: name, Accounts and Grants. No secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MandateFile {
    /// The Mandate's name.
    pub name: String,
    /// The Accounts, in order.
    pub accounts: Vec<FileAccount>,
    /// The Grants.
    #[serde(default)]
    pub grants: Vec<FileGrant>,
}

/// An Account's connection settings, without its Credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileAccount {
    /// IMAP login name.
    pub login: String,
    /// IMAP server host name.
    pub host: String,
    /// IMAP server port, 1 to 65535.
    pub port: u16,
    /// `implicit` or `starttls`.
    pub tls: TlsMode,
}

/// One Grant: an operation on decoded UTF-8 mailbox names (`*` for all).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileGrant {
    /// The operation allowed.
    pub operation: Operation,
    /// Login of the Account, one of the file's Accounts.
    pub account: String,
    /// Mailbox names; empty for `draft`.
    pub mailboxes: Vec<String>,
}

/// Why a Mandate file cannot be used. Never quotes a value from the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidFile(pub String);

impl MandateFile {
    /// The exportable part of `mandate`: everything but the Credentials, ID
    /// and creation time.
    pub fn of(mandate: &Mandate) -> Self {
        Self {
            name: mandate.name.clone(),
            accounts: mandate
                .accounts
                .iter()
                .map(|a| FileAccount {
                    login: a.login.clone(),
                    host: a.host.clone(),
                    port: a.port,
                    tls: a.tls,
                })
                .collect(),
            grants: mandate
                .policy
                .grants
                .iter()
                .map(|g| FileGrant {
                    operation: g.operation,
                    account: g.account.clone(),
                    mailboxes: g.mailboxes.iter().cloned().map(String::from).collect(),
                })
                .collect(),
        }
    }

    /// The file as TOML text.
    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("a Mandate file always serialises")
    }

    /// Parses and validates TOML text.
    pub fn from_toml(text: &str) -> Result<Self, InvalidFile> {
        let root = DeTable::parse(text).map_err(|e| {
            // `Display` and `message()` may quote the offending source text,
            // so only the position is reported.
            match e.span() {
                Some(span) => {
                    let (line, column) = line_column(text, span.start);
                    InvalidFile(format!("line {line}, column {column} is not valid TOML"))
                }
                None => InvalidFile("the file is not valid TOML".into()),
            }
        })?;
        let file = Walk { text }.file(root.get_ref())?;
        // Defence in depth: the strict serde types must agree with the walk.
        match toml::from_str::<MandateFile>(text) {
            Ok(parsed) if parsed == file => Ok(file),
            _ => Err(InvalidFile(
                "the file does not match the Mandate file format".into(),
            )),
        }
    }

    /// Reads, parses and validates the file at `path`. The path must name a
    /// regular file (symlinks are followed) before it is opened, so a FIFO or
    /// device is never opened.
    pub fn load(path: &Path) -> Result<Self, InvalidFile> {
        use std::io::Read;
        let shown = escape_controls(&path.display().to_string());
        let cannot = |e: std::io::Error| InvalidFile(format!("cannot read {shown}: {}", e.kind()));
        let not_regular = || InvalidFile(format!("{shown} is not a regular file"));
        if !std::fs::metadata(path).map_err(cannot)?.is_file() {
            return Err(not_regular());
        }
        let file = std::fs::File::open(path).map_err(cannot)?;
        // The path may have been swapped between the check and the open.
        if !file.metadata().map_err(cannot)?.is_file() {
            return Err(not_regular());
        }
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(cannot)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(InvalidFile(format!(
                "{shown} is larger than {MAX_FILE_BYTES} bytes"
            )));
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| InvalidFile(format!("{shown} is not UTF-8 text")))?;
        Self::from_toml(&text)
    }

    /// The decoded mailbox names the file grants on Account `login` for any
    /// non-`draft` operation, and whether `*` is among them.
    pub fn mailboxes_for(&self, login: &str) -> (Vec<String>, bool) {
        let mut names: Vec<String> = Vec::new();
        let mut all = false;
        for grant in self.grants.iter().filter(|g| g.account == login) {
            if grant.operation == Operation::Draft {
                continue;
            }
            for name in &grant.mailboxes {
                if name == "*" {
                    all = true;
                } else if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        (names, all)
    }

    /// The operations the file grants on Account `login`.
    pub fn operations_for(&self, login: &str) -> Vec<Operation> {
        Operation::ALL
            .into_iter()
            .filter(|op| {
                self.grants
                    .iter()
                    .any(|g| g.account == login && g.operation == *op)
            })
            .collect()
    }
}

fn line_column(text: &str, offset: usize) -> (usize, usize) {
    let before = &text.as_bytes()[..offset.min(text.len())];
    let line = before.iter().filter(|b| **b == b'\n').count() + 1;
    let column = before.iter().rev().take_while(|b| **b != b'\n').count() + 1;
    (line, column)
}

const TOP_KEYS: [&str; 3] = ["name", "accounts", "grants"];
const ACCOUNT_KEYS: [&str; 4] = ["login", "host", "port", "tls"];
const GRANT_KEYS: [&str; 3] = ["operation", "account", "mailboxes"];

type Value<'i> = Spanned<DeValue<'i>>;

/// Checks the parsed file key by key and builds the [`MandateFile`].
///
/// Every problem names a line and what is wrong there. No key name or value
/// from the file is repeated, except a login a Grant or a duplicate names
/// (escaped), since that is how the human finds it.
struct Walk<'t> {
    text: &'t str,
}

impl Walk<'_> {
    fn at(&self, span: Range<usize>, what: &str) -> InvalidFile {
        let (line, _) = line_column(self.text, span.start);
        InvalidFile(format!("line {line}: {what}"))
    }

    fn file(&self, table: &DeTable<'_>) -> Result<MandateFile, InvalidFile> {
        self.known_keys(table, &TOP_KEYS, "unknown key at the top level")?;
        let name = match table.get("name") {
            None => return Err(InvalidFile("the Mandate name is missing".into())),
            Some(v) => self.text_of(v, "the Mandate name must be a non-empty string")?,
        };
        let entries = match table.get("accounts") {
            None => return Err(InvalidFile("the file has no [[accounts]] table".into())),
            Some(v) => self.tables(v, "accounts must be [[accounts]] tables")?,
        };
        if entries.is_empty() {
            return Err(InvalidFile("the file has no [[accounts]] table".into()));
        }
        let mut accounts: Vec<FileAccount> = Vec::new();
        for (span, entry) in entries {
            let account = self.account(span, entry)?;
            if accounts.iter().any(|a| a.login == account.login) {
                let login = entry.get("login").expect("checked").span();
                return Err(self.at(
                    login,
                    &format!(
                        "two Accounts have the login {}",
                        escape_controls(&account.login)
                    ),
                ));
            }
            accounts.push(account);
        }
        let entries = match table.get("grants") {
            None => Vec::new(),
            Some(v) => self.tables(v, "grants must be [[grants]] tables")?,
        };
        let mut grants = Vec::new();
        for (span, entry) in entries {
            grants.push(self.grant(span, entry, &accounts)?);
        }
        Ok(MandateFile {
            name,
            accounts,
            grants,
        })
    }

    fn account(&self, span: Range<usize>, entry: &DeTable<'_>) -> Result<FileAccount, InvalidFile> {
        self.known_keys(entry, &ACCOUNT_KEYS, "unknown key in an [[accounts]] entry")?;
        let field = |key: &str| {
            entry.get(key).ok_or_else(|| {
                self.at(span.clone(), &format!("an [[accounts]] entry has no {key}"))
            })
        };
        let login = self.text_of(field("login")?, "the login must be a non-empty string")?;
        let host = self.text_of(field("host")?, "the host must be a non-empty string")?;
        let port = field("port")?;
        let port = port
            .get_ref()
            .as_integer()
            .and_then(|n| i64::from_str_radix(n.as_str(), n.radix()).ok())
            .and_then(|n| u16::try_from(n).ok())
            .filter(|n| *n != 0)
            .ok_or_else(|| self.at(port.span(), "the port must be a number from 1 to 65535"))?;
        let tls = field("tls")?;
        let tls = match tls.get_ref().as_str() {
            Some("implicit") => TlsMode::Implicit,
            Some("starttls") => TlsMode::Starttls,
            _ => {
                return Err(self.at(tls.span(), "the TLS mode must be implicit or starttls"));
            }
        };
        Ok(FileAccount {
            login,
            host,
            port,
            tls,
        })
    }

    fn grant(
        &self,
        span: Range<usize>,
        entry: &DeTable<'_>,
        accounts: &[FileAccount],
    ) -> Result<FileGrant, InvalidFile> {
        self.known_keys(entry, &GRANT_KEYS, "unknown key in a [[grants]] entry")?;
        let field = |key: &str| {
            entry
                .get(key)
                .ok_or_else(|| self.at(span.clone(), &format!("a [[grants]] entry has no {key}")))
        };
        let operation = field("operation")?;
        let operation = Operation::ALL
            .into_iter()
            .find(|op| operation.get_ref().as_str() == Some(op.as_str()))
            .ok_or_else(|| {
                self.at(
                    operation.span(),
                    "the operation must be one of list, search, read, flag, move, draft",
                )
            })?;
        let account = field("account")?;
        let login = self.text_of(account, "the Grant's account must be a non-empty string")?;
        if !accounts.iter().any(|a| a.login == login) {
            return Err(self.at(
                account.span(),
                &format!(
                    "the Grant names Account {}, which is not among the file's Accounts",
                    escape_controls(&login)
                ),
            ));
        }
        let list = field("mailboxes")?;
        let bad = || self.at(list.span(), "the mailboxes must be a list of mailbox names");
        let mut mailboxes = Vec::new();
        for item in list.get_ref().as_array().ok_or_else(bad)?.iter() {
            match item.get_ref().as_str() {
                Some(name) if !name.is_empty() => mailboxes.push(name.to_string()),
                _ => return Err(bad()),
            }
        }
        if operation == Operation::Draft && !mailboxes.is_empty() {
            return Err(self.at(list.span(), "a draft Grant takes no mailboxes"));
        }
        Ok(FileGrant {
            operation,
            account: login,
            mailboxes,
        })
    }

    fn known_keys(
        &self,
        table: &DeTable<'_>,
        keys: &[&str],
        what: &str,
    ) -> Result<(), InvalidFile> {
        match table
            .iter()
            .find(|(k, _)| !keys.contains(&k.get_ref().as_ref()))
        {
            Some((key, _)) => Err(self.at(key.span(), what)),
            None => Ok(()),
        }
    }

    fn text_of(&self, value: &Value<'_>, what: &str) -> Result<String, InvalidFile> {
        match value.get_ref().as_str() {
            Some(text) if !text.trim().is_empty() => Ok(text.to_string()),
            _ => Err(self.at(value.span(), what)),
        }
    }

    /// The tables of an array of tables, each with its span.
    fn tables<'v, 'i>(
        &self,
        value: &'v Value<'i>,
        what: &str,
    ) -> Result<Vec<(Range<usize>, &'v DeTable<'i>)>, InvalidFile> {
        let items = value
            .get_ref()
            .as_array()
            .ok_or_else(|| self.at(value.span(), what))?;
        items
            .iter()
            .map(|item| match item.get_ref().as_table() {
                Some(table) => Ok((item.span(), table)),
                None => Err(self.at(item.span(), what)),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
name = "triage"

[[accounts]]
login = "agent@example.test"
host = "localhost"
port = 993
tls = "implicit"

[[grants]]
operation = "list"
account = "agent@example.test"
mailboxes = ["INBOX", "*"]

[[grants]]
operation = "draft"
account = "agent@example.test"
mailboxes = []
"#;

    #[test]
    fn round_trips_and_summarises_per_account() {
        let file = MandateFile::from_toml(GOOD).unwrap();
        assert_eq!(MandateFile::from_toml(&file.to_toml()).unwrap(), file);
        assert_eq!(
            file.mailboxes_for("agent@example.test"),
            (vec!["INBOX".to_string()], true)
        );
        assert_eq!(
            file.operations_for("agent@example.test"),
            [Operation::List, Operation::Draft]
        );
    }

    #[test]
    fn problems_never_quote_values() {
        let secret = "hunter2-file-secret";
        for text in [
            GOOD.replace("port = 993", &format!("port = \"{secret}\"")),
            GOOD.replace("tls = \"implicit\"", &format!("tls = \"{secret}\"")),
            GOOD.replace("name = \"triage\"", &format!("name = {secret}")),
            GOOD.replace("operation = \"list\"", &format!("operation = \"{secret}\"")),
            GOOD.replace("mailboxes = []", &format!("mailboxes = [\"{secret}\"]")),
            GOOD.replace("mailboxes = []", &format!("mailboxes = [1, \"{secret}\"]")),
        ] {
            let InvalidFile(problem) = MandateFile::from_toml(&text).unwrap_err();
            assert!(!problem.contains(secret), "{problem}");
            assert!(!problem.contains("; "), "{problem}");
        }
    }

    #[test]
    fn line_column_counts_from_one() {
        assert_eq!(line_column("a\nbc", 0), (1, 1));
        assert_eq!(line_column("a\nbc", 3), (2, 2));
    }
}
