//! The Mandate file: a Mandate's Accounts (server, port, TLS mode, login) and
//! Grants as TOML, written at the end of `mandate create` and read back by
//! `mandate create --from`.
//!
//! These types are separate from [`crate::mandate::Account`] and have no
//! Credential field: the file never holds a Credential or an Agent Token.
//!
//! A file passed to `--from` is untrusted input. It is checked key by key
//! before it is deserialised, so every problem is reported by key path and
//! line number only and no value from the file is echoed (a pasted secret
//! must not end up in an error message).

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

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
        let table: toml::Table = text.parse().map_err(|e: toml::de::Error| {
            let place = e
                .span()
                .map(|span| {
                    let (line, column) = line_column(text, span.start);
                    format!(" at line {line}, column {column}")
                })
                .unwrap_or_default();
            // `Display` and `message()` may quote the offending source text,
            // so only the position is reported.
            InvalidFile(format!("the file is not valid TOML{place}"))
        })?;
        check_table(&table)?;
        let file: MandateFile = table
            .try_into()
            .map_err(|_| InvalidFile("the file does not match the Mandate file format".into()))?;
        file.validate()?;
        Ok(file)
    }

    /// Reads, parses and validates the file at `path`.
    pub fn load(path: &Path) -> Result<Self, InvalidFile> {
        use std::io::Read;
        let shown = escape_controls(&path.display().to_string());
        let cannot = |e: std::io::Error| InvalidFile(format!("cannot read {shown}: {}", e.kind()));
        let file = std::fs::File::open(path).map_err(cannot)?;
        if !file.metadata().map_err(cannot)?.is_file() {
            return Err(InvalidFile(format!("{shown} is not a regular file")));
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

    /// Checks what types cannot: non-empty names, ports from 1, distinct
    /// logins, Grants naming a file Account, `draft` without mailboxes.
    fn validate(&self) -> Result<(), InvalidFile> {
        if self.name.trim().is_empty() {
            return Err(InvalidFile("`name` is empty".into()));
        }
        if self.accounts.is_empty() {
            return Err(InvalidFile("the file has no [[accounts]]".into()));
        }
        let mut logins = BTreeSet::new();
        for (i, account) in self.accounts.iter().enumerate() {
            let at = format!("accounts[{}]", i + 1);
            if account.login.trim().is_empty() {
                return Err(InvalidFile(format!("`{at}.login` is empty")));
            }
            if account.host.trim().is_empty() {
                return Err(InvalidFile(format!("`{at}.host` is empty")));
            }
            if account.port == 0 {
                return Err(InvalidFile(format!("`{at}.port` must be from 1 to 65535")));
            }
            if !logins.insert(account.login.as_str()) {
                return Err(InvalidFile(format!(
                    "two Accounts have the login {}",
                    escape_controls(&account.login)
                )));
            }
        }
        for (i, grant) in self.grants.iter().enumerate() {
            let at = format!("grants[{}]", i + 1);
            if !logins.contains(grant.account.as_str()) {
                return Err(InvalidFile(format!(
                    "`{at}.account` names {}, which is not among the file's Accounts",
                    escape_controls(&grant.account)
                )));
            }
            if grant.operation == Operation::Draft && !grant.mailboxes.is_empty() {
                return Err(InvalidFile(format!(
                    "`{at}` is a draft Grant, which takes no mailboxes"
                )));
            }
            if grant.mailboxes.iter().any(String::is_empty) {
                return Err(InvalidFile(format!("`{at}.mailboxes` has an empty name")));
            }
        }
        Ok(())
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

fn unknown_key(key: &str, at: &str) -> InvalidFile {
    // Key names are reported (escaped); their values never are.
    let key = escape_controls(key);
    if at.is_empty() {
        InvalidFile(format!("unknown key `{key}`"))
    } else {
        InvalidFile(format!("unknown key `{key}` in `{at}`"))
    }
}

fn wrong(at: &str, expected: &str) -> InvalidFile {
    InvalidFile(format!("`{at}` must be {expected}"))
}

/// Checks keys and value types, naming the key path of the first problem.
fn check_table(table: &toml::Table) -> Result<(), InvalidFile> {
    if let Some(key) = table.keys().find(|k| !TOP_KEYS.contains(&k.as_str())) {
        return Err(unknown_key(key, ""));
    }
    match table.get("name") {
        None => return Err(InvalidFile("the key `name` is missing".into())),
        Some(v) if !v.is_str() => return Err(wrong("name", "a string")),
        Some(_) => {}
    }
    let accounts = match table.get("accounts") {
        None => return Err(InvalidFile("the file has no [[accounts]]".into())),
        Some(toml::Value::Array(items)) => items,
        Some(_) => return Err(wrong("accounts", "an array of tables ([[accounts]])")),
    };
    for (i, item) in accounts.iter().enumerate() {
        let at = format!("accounts[{}]", i + 1);
        let entry = item
            .as_table()
            .ok_or_else(|| wrong(&at, "a table ([[accounts]])"))?;
        check_keys(entry, &ACCOUNT_KEYS, &at)?;
        check_string(entry, "login", &at)?;
        check_string(entry, "host", &at)?;
        match entry.get("port") {
            Some(toml::Value::Integer(port)) if (1..=65535).contains(port) => {}
            Some(toml::Value::Integer(_)) => {
                return Err(wrong(&format!("{at}.port"), "from 1 to 65535"));
            }
            Some(_) => return Err(wrong(&format!("{at}.port"), "an integer")),
            None => unreachable!("checked by check_keys"),
        }
        match entry.get("tls").and_then(toml::Value::as_str) {
            Some("implicit" | "starttls") => {}
            _ => {
                return Err(wrong(&format!("{at}.tls"), "\"implicit\" or \"starttls\""));
            }
        }
    }
    let grants = match table.get("grants") {
        None => return Ok(()),
        Some(toml::Value::Array(items)) => items,
        Some(_) => return Err(wrong("grants", "an array of tables ([[grants]])")),
    };
    for (i, item) in grants.iter().enumerate() {
        let at = format!("grants[{}]", i + 1);
        let entry = item
            .as_table()
            .ok_or_else(|| wrong(&at, "a table ([[grants]])"))?;
        check_keys(entry, &GRANT_KEYS, &at)?;
        let operation = entry.get("operation").and_then(toml::Value::as_str);
        if !Operation::ALL
            .iter()
            .any(|op| Some(op.as_str()) == operation)
        {
            return Err(wrong(
                &format!("{at}.operation"),
                "one of list, search, read, flag, move, draft",
            ));
        }
        check_string(entry, "account", &at)?;
        match entry.get("mailboxes") {
            Some(toml::Value::Array(names)) if names.iter().all(toml::Value::is_str) => {}
            _ => {
                return Err(wrong(
                    &format!("{at}.mailboxes"),
                    "an array of mailbox names",
                ));
            }
        }
    }
    Ok(())
}

fn check_keys(entry: &toml::Table, keys: &[&str], at: &str) -> Result<(), InvalidFile> {
    if let Some(key) = entry.keys().find(|k| !keys.contains(&k.as_str())) {
        return Err(unknown_key(key, at));
    }
    if let Some(key) = keys.iter().find(|k| !entry.contains_key(**k)) {
        return Err(InvalidFile(format!("the key `{key}` is missing in `{at}`")));
    }
    Ok(())
}

fn check_string(entry: &toml::Table, key: &str, at: &str) -> Result<(), InvalidFile> {
    match entry.get(key) {
        Some(v) if v.is_str() => Ok(()),
        _ => Err(wrong(&format!("{at}.{key}"), "a string")),
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
