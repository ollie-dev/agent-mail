//! Checks of the Claude Code plugin (`.claude-plugin/marketplace.json` and
//! `plugin/`), the README setup guide and the CLAUDE.md layout line. A real
//! `/plugin install` needs Claude Code, so the manifests are checked instead.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

const INSTALLER: &str =
    "curl -fsSL https://github.com/ollie-dev/agent-mail/releases/latest/download/install.sh | sh";

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &str) -> String {
    fs::read_to_string(root().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn json_file(path: &str) -> Value {
    serde_json::from_str(&read(path)).unwrap_or_else(|e| panic!("{path} is not JSON: {e}"))
}

fn skill() -> String {
    read("plugin/skills/agent-mail/SKILL.md")
}

/// The text of the README section `heading` (such as `## Install`), up to
/// the next heading of the same or a higher level. Same rule as in
/// `tests/release.rs`.
fn readme_section(heading: &str) -> String {
    let readme = read("README.md");
    let start = readme
        .find(&format!("\n{heading}\n"))
        .unwrap_or_else(|| panic!("README has no {heading}"));
    let level = heading.split(' ').next().unwrap();
    let rest = &readme[start + heading.len() + 2..];
    let end = rest
        .match_indices('\n')
        .map(|(i, _)| i)
        .find(|&i| {
            let line = &rest[i + 1..];
            line.starts_with('#')
                && line
                    .split(' ')
                    .next()
                    .is_some_and(|h| h.len() <= level.len())
        })
        .unwrap_or(rest.len());
    rest[..end].to_string()
}

/// The bodies of the ```` ```json ```` blocks in `text`.
fn json_blocks(text: &str) -> Vec<Value> {
    let mut blocks = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("```json\n") {
        let body = &rest[start + 8..];
        let end = body.find("```").expect("unclosed code block");
        blocks.push(
            serde_json::from_str(&body[..end])
                .unwrap_or_else(|e| panic!("JSON block does not parse: {e}\n{}", &body[..end])),
        );
        rest = &body[end + 3..];
    }
    blocks
}

/// Every file under `dir`, recursively.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(files_under(&path));
        } else {
            out.push(path);
        }
    }
    out
}

/// The YAML frontmatter of the skill, between the opening and closing `---`.
fn frontmatter(skill: &str) -> String {
    let rest = skill
        .strip_prefix("---\n")
        .expect("SKILL.md starts with a --- frontmatter block");
    let end = rest.find("\n---\n").expect("frontmatter is closed");
    rest[..end].to_string()
}

#[test]
fn ac1_plugin_manifests_provide_the_skill_and_the_mcp_server() {
    let market = json_file(".claude-plugin/marketplace.json");
    assert_eq!(market["name"], "agent-mail");
    let plugins = market["plugins"].as_array().expect("plugins array");
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0]["name"], "agent-mail");
    assert_eq!(plugins[0]["source"], "./plugin");

    let manifest = json_file("plugin/.claude-plugin/plugin.json");
    assert_eq!(manifest["name"], "agent-mail");
    assert!(
        manifest["description"]
            .as_str()
            .is_some_and(|d| !d.trim().is_empty())
    );
    if let Some(version) = manifest.get("version") {
        let cargo = read("Cargo.toml");
        let cargo_version = cargo
            .lines()
            .find_map(|l| l.strip_prefix("version = "))
            .expect("Cargo.toml version")
            .trim_matches('"');
        assert_eq!(version, cargo_version);
    }

    let front = frontmatter(&skill());
    assert!(front.lines().any(|l| l.trim() == "name: agent-mail"));
    assert!(front.lines().any(|l| {
        l.strip_prefix("description:")
            .is_some_and(|d| !d.trim().is_empty())
    }));

    let mcp = json_file("plugin/.mcp.json");
    let server = &mcp["mcpServers"]["agent-mail"];
    assert_eq!(server["command"], "agent-mail");
    assert_eq!(server["args"], json!(["mcp"]));

    let files = files_under(&root().join("plugin"));
    assert!(!files.is_empty());
    for file in files {
        assert!(
            !file.starts_with(root().join("plugin/bin")),
            "{}",
            file.display()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&file).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0, "{} is executable", file.display());
        }
        let bytes = fs::read(&file).unwrap();
        for magic in [&b"\xCF\xFA\xED\xFE"[..], b"\xCA\xFE\xBA\xBE", b"\x7FELF"] {
            assert!(!bytes.starts_with(magic), "{} is a binary", file.display());
        }
    }

    let section = readme_section("## Claude Code plugin");
    assert!(section.contains("/plugin marketplace add ollie-dev/agent-mail"));
    assert!(section.contains("/plugin install agent-mail@agent-mail"));
}

#[test]
fn ac2_skill_documents_commands_output_denial_and_human_only_mandates() {
    let skill = skill();
    for needle in [
        "agent-mail whoami",
        "agent-mail list",
        "agent-mail search",
        "agent-mail read",
        "agent-mail flag",
        "agent-mail move",
        "agent-mail draft",
        "AGENT_MAIL_TOKEN",
        "--account",
        "ref",
        "--gmail",
        "--text",
        "missing_token",
        "invalid_token",
        "account_required",
        "ambiguous_mandate",
        "stale_reference",
        "proxy_refused",
        "Only the human creates Mandates",
        "agent-mail mandate create",
    ] {
        assert!(skill.contains(needle), "SKILL.md lacks {needle:?}");
    }
    assert!(
        json_blocks(&skill).iter().any(|block| {
            let error = &block["error"];
            error["code"] == "denied"
                && error.get("operation").is_some()
                && error.get("account").is_some()
        }),
        "SKILL.md has no denied error example"
    );
    assert!(skill.contains("\"code\":\"denied\""));
    for line in skill.lines().filter(|l| l.contains("agent-mail mandate")) {
        assert!(line.contains("human"), "line without `human`: {line}");
    }
    // The Agent never asks the human for a token, which would put it in the
    // chat; it asks the human to set one in AGENT_MAIL_TOKEN.
    for line in skill.lines() {
        let lower = line.to_lowercase();
        if lower.contains("ask the human for") || lower.contains("ask for a") {
            assert!(
                !lower.contains("token") || lower.contains("never"),
                "SKILL.md asks for a token: {line}"
            );
        }
    }
    assert!(skill.contains("Ask the human to set a valid token in `AGENT_MAIL_TOKEN`"));
}

#[test]
fn ac3_skill_tells_the_human_to_install_a_missing_binary() {
    let skill = skill();
    let start = skill
        .find("\n## If agent-mail is missing\n")
        .expect("SKILL.md has the section");
    let rest = &skill[start + 1..];
    let section = match rest[1..].find("\n## ") {
        Some(end) => &rest[..end + 1],
        None => rest,
    };
    assert!(section.contains(INSTALLER));
    assert!(section.contains("Do not run the installer yourself"));
    assert_eq!(skill.matches(INSTALLER).count(), 1);

    let front = frontmatter(&skill);
    let mut in_allowed = false;
    for line in front.lines() {
        if line.starts_with("allowed-tools") {
            in_allowed = true;
        } else if !line.starts_with(' ') && !line.starts_with('-') {
            in_allowed = false;
        }
        if in_allowed {
            let words: Vec<&str> = line
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
                .collect();
            assert!(
                !words.contains(&"curl") && !words.contains(&"sh"),
                "allowed-tools names curl or sh: {line}"
            );
        }
    }
}

#[test]
fn ac4_readme_shows_where_agent_tokens_go() {
    let section = readme_section("## Agent Tokens in Claude Code and Claude Desktop");
    assert!(section.contains(".claude/settings.local.json"));
    assert!(section.contains("git-ignored"));
    assert!(section.contains("claude_desktop_config.json"));
    let blocks = json_blocks(&section);
    assert!(blocks.contains(&json!({"env": {"AGENT_MAIL_TOKEN": "<your agent token>"}})));
    assert!(blocks.iter().any(|b| {
        let server = &b["mcpServers"]["agent-mail"];
        server["command"] == "agent-mail"
            && server["args"] == json!(["mcp"])
            && server["env"]["AGENT_MAIL_TOKEN"] == "<your agent token>"
    }));
    assert!(!read("README.md").contains("in the project's `.mcp.json`"));
}

#[test]
fn ac5_readme_lists_the_sandbox_allowed_domains() {
    let section = readme_section("## Sandbox and proxies");
    assert!(
        json_blocks(&section).iter().any(|b| {
            b["sandbox"]["network"]["allowedDomains"]
                .as_array()
                .is_some_and(|d| d.contains(&json!("imap.gmail.com")))
        }),
        "no sandbox.network.allowedDomains block with imap.gmail.com"
    );
    assert!(
        section.contains(".claude/settings.local.json")
            || section.contains(".claude/settings.json")
    );
    assert!(section.contains("agent-mail whoami"));
    assert!(section.contains("host"));
    assert!(section.contains("proxy_refused"));
}

#[test]
fn ac6_readme_states_what_each_os_protects_against() {
    let section = readme_section("## What each OS protects against");
    for needle in [
        "macOS",
        "cannot extract",
        "Credential",
        "red flag",
        "same user",
        "Agent Token",
        "Windows",
        "honest Agents",
    ] {
        assert!(section.contains(needle), "section lacks {needle:?}");
    }
}

#[test]
fn ac7_readme_recommends_one_gmail_app_password_per_mandate() {
    let section = readme_section("## Gmail app passwords");
    for needle in [
        "one app password per Mandate",
        "https://myaccount.google.com/apppasswords",
        "revoke",
        "agent-mail mandate delete",
    ] {
        assert!(section.contains(needle), "section lacks {needle:?}");
    }
}

/// `README.md`, the marketplace and every file under `plugin/`, by name.
fn published_texts() -> Vec<(String, String)> {
    let mut texts = vec![
        ("README.md".to_string(), read("README.md")),
        (
            ".claude-plugin/marketplace.json".to_string(),
            read(".claude-plugin/marketplace.json"),
        ),
    ];
    for file in files_under(&root().join("plugin")) {
        texts.push((
            file.display().to_string(),
            fs::read_to_string(&file).unwrap(),
        ));
    }
    texts
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Matches of `amt_[A-Za-z0-9_-]{20,}`.
fn token_shapes(text: &str) -> Vec<String> {
    text.match_indices("amt_")
        .filter_map(|(i, _)| {
            let tail = &text[i + 4..];
            let n = tail
                .bytes()
                .take_while(|&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                .count();
            (n >= 20).then(|| text[i..i + 4 + n].to_string())
        })
        .collect()
}

/// Matches of `\b[a-z]{4} [a-z]{4} [a-z]{4} [a-z]{4}\b`.
fn app_password_shapes(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut found = Vec::new();
    for i in 0..b.len() {
        if i > 0 && is_word(b[i - 1]) {
            continue;
        }
        let mut p = i;
        let mut ok = true;
        for group in 0..4 {
            if group > 0 {
                if b.get(p) != Some(&b' ') {
                    ok = false;
                    break;
                }
                p += 1;
            }
            if p + 4 > b.len() || !b[p..p + 4].iter().all(u8::is_ascii_lowercase) {
                ok = false;
                break;
            }
            p += 4;
        }
        if ok && b.get(p).is_none_or(|&c| !is_word(c)) {
            found.push(text[i..p].to_string());
        }
    }
    found
}

/// The `AGENT_MAIL_TOKEN` values given in `text`, after `AGENT_MAIL_TOKEN=`
/// or as the JSON value of `"AGENT_MAIL_TOKEN"`, split at commas.
fn token_values(text: &str) -> Vec<String> {
    let mut raw = Vec::new();
    for (i, _) in text.match_indices("AGENT_MAIL_TOKEN=") {
        let tail = &text[i + "AGENT_MAIL_TOKEN=".len()..];
        let value = if let Some(q) = tail.strip_prefix('"') {
            &q[..q.find('"').unwrap_or(q.len())]
        } else if let Some(q) = tail.strip_prefix('\'') {
            &q[..q.find('\'').unwrap_or(q.len())]
        } else if tail.starts_with('<') {
            &tail[..tail.find('>').map_or(tail.len(), |e| e + 1)]
        } else if tail.starts_with("${") {
            &tail[..tail.find('}').map_or(tail.len(), |e| e + 1)]
        } else {
            let end = tail
                .find(|c: char| c.is_whitespace() || c == '`')
                .unwrap_or(tail.len());
            &tail[..end]
        };
        raw.push(value.to_string());
    }
    for (i, _) in text.match_indices("\"AGENT_MAIL_TOKEN\"") {
        let tail = text[i + "\"AGENT_MAIL_TOKEN\"".len()..].trim_start();
        if let Some(tail) = tail.strip_prefix(':') {
            let tail = tail.trim_start();
            let value: String = serde_json::Deserializer::from_str(tail)
                .into_iter::<String>()
                .next()
                .expect("a JSON value")
                .expect("a JSON string value");
            raw.push(value);
        }
    }
    raw.iter()
        .flat_map(|v| v.split(',').map(|t| t.trim().to_string()))
        .filter(|t| !t.is_empty())
        .collect()
}

fn is_placeholder(token: &str) -> bool {
    (token.starts_with('<') && token.ends_with('>'))
        || (token.starts_with("amt_") && token.contains("..."))
        || (token.starts_with("${") && token.ends_with('}'))
}

/// Email addresses in `text`: a local part, `@`, and a dotted domain.
fn emails(text: &str) -> Vec<String> {
    let local = |c: char| c.is_ascii_alphanumeric() || "._%+-".contains(c);
    let domain = |c: char| c.is_ascii_alphanumeric() || c == '.' || c == '-';
    let mut found = Vec::new();
    for (i, _) in text.match_indices('@') {
        let start = text[..i]
            .char_indices()
            .rfind(|&(_, c)| !local(c))
            .map_or(0, |(s, c)| s + c.len_utf8());
        let end = text[i + 1..]
            .find(|c: char| !domain(c))
            .map_or(text.len(), |e| i + 1 + e);
        let host = text[i + 1..end].trim_end_matches('.');
        if start < i && host.contains('.') {
            found.push(format!("{}@{host}", &text[start..i]));
        }
    }
    found
}

#[test]
fn ac8_no_real_credential_or_agent_token_in_plugin_or_readme() {
    let mut values = 0;
    for (name, text) in published_texts() {
        assert_eq!(token_shapes(&text), Vec::<String>::new(), "{name}");
        assert_eq!(app_password_shapes(&text), Vec::<String>::new(), "{name}");
        for token in token_values(&text) {
            values += 1;
            assert!(is_placeholder(&token), "{name}: token value {token:?}");
        }
        for address in emails(&text) {
            let (local, domain) = address.split_once('@').unwrap();
            let ok = domain.ends_with("example.test")
                || domain.ends_with("example.com")
                || (domain == "gmail.com" && local.starts_with("you"));
            assert!(ok, "{name}: address {address}");
        }
    }
    assert!(values > 0, "no AGENT_MAIL_TOKEN example found");

    // The scanners catch what they are meant to.
    assert_eq!(token_shapes("x amt_abcdefghijklmnopqrstu y").len(), 1);
    assert!(token_shapes("amt_...work...").is_empty());
    assert_eq!(app_password_shapes("pw: abcd efgh ijkl mnop.").len(), 1);
    assert_eq!(
        token_values(r#"AGENT_MAIL_TOKEN=amt_real {"AGENT_MAIL_TOKEN": "x"}"#),
        ["amt_real", "x"]
    );
    assert!(!is_placeholder("amt_real"));
    assert_eq!(
        emails("mail bob@corp.example.org now"),
        ["bob@corp.example.org"]
    );
}

#[test]
fn ac9_readme_covers_creating_a_mandate() {
    let section = readme_section("## Creating a Mandate");
    for needle in [
        "agent-mail mandate create",
        "mandate create --from",
        "agent-mail mandate list",
        "agent-mail mandate delete",
        "own terminal",
        "shown once",
    ] {
        assert!(section.contains(needle), "section lacks {needle:?}");
    }
}

#[test]
fn ac10_claude_md_names_the_plugin_files() {
    let claude = read("CLAUDE.md");
    for needle in [
        "plugin/",
        ".claude-plugin/marketplace.json",
        "plugin/skills/agent-mail/SKILL.md",
        "tests/plugin.rs",
    ] {
        assert!(claude.contains(needle), "CLAUDE.md lacks {needle:?}");
    }
}

/// The blank-line-separated paragraph of `text` holding `needle`.
fn paragraph_with<'a>(text: &'a str, needle: &str) -> &'a str {
    text.split("\n\n")
        .find(|p| p.contains(needle))
        .unwrap_or_else(|| panic!("no paragraph holds {needle:?}"))
}

#[test]
fn job020_ac14_readme_and_skill_document_draft_refused_and_json_usage_errors() {
    let draft = readme_section("## Draft");
    let errors = paragraph_with(&draft, "Errors besides those above:");
    assert!(errors.contains("`draft_refused`"), "{errors}");
    let refused = paragraph_with(&draft, "**When the server refuses the draft.**");
    assert!(refused.contains("`draft_refused`"), "{refused}");

    let audit = readme_section("## Audit log");
    let reasons = paragraph_with(&audit, "- `reason`:");
    assert!(reasons.contains("`draft_refused`"), "{reasons}");

    let readme = read("README.md");
    let usage = paragraph_with(&readme, "A usage error, such as");
    assert!(
        usage.contains("JSON error with code `invalid_arguments` and exit code 2"),
        "{usage}"
    );
    let skill = skill();
    let skill_usage = paragraph_with(&skill, "A usage error");
    assert!(
        skill_usage.contains("JSON error with code `invalid_arguments` and exit code 2"),
        "{skill_usage}"
    );
    let skill_refused = skill
        .lines()
        .find(|l| l.starts_with("- `draft_refused`"))
        .expect("SKILL.md lists draft_refused");

    let mcp = readme_section("## MCP server");
    let missing = paragraph_with(&mcp, "names the argument");
    let missing = missing
        .split("\n- ")
        .find(|item| item.contains("names the argument"))
        .unwrap();

    let reasons = reasons
        .lines()
        .find(|l| l.starts_with("- `reason`:"))
        .unwrap();
    for paragraph in [
        errors,
        refused,
        reasons,
        usage,
        skill_usage,
        skill_refused,
        missing,
    ] {
        // Bullet lists are measured item by item.
        for item in paragraph.split("\n- ") {
            assert!(
                item.chars().count() <= 300,
                "{} characters: {item}",
                item.chars().count()
            );
        }
    }
}

#[test]
fn job021_ac14_readme_and_skill_document_proxy_auth_failed_and_whoami_proxy() {
    let proxies = readme_section("## Sandbox and proxies");
    let list = paragraph_with(&proxies, "- `proxy_refused`:");
    let item = |code: &str| -> &str {
        list.split("\n- ")
            .find(|i| i.trim_start_matches("- ").starts_with(&format!("`{code}`")))
            .unwrap_or_else(|| panic!("no {code} bullet"))
    };
    let refused = item("proxy_refused");
    assert!(refused.contains("403"), "{refused}");
    assert!(refused.contains("not allowed by ruleset"), "{refused}");
    assert!(!refused.contains("credential"), "{refused}");
    let auth = item("proxy_auth_failed");
    assert!(auth.contains("credentials"), "{auth}");
    assert!(auth.contains("variable"), "{auth}");
    let other = paragraph_with(&proxies, "Any other proxy refusal");
    assert!(other.contains("`connection_failed`"), "{other}");

    let whoami = readme_section("## Several Mandates and whoami");
    let proxy = paragraph_with(&whoami, "**`proxy` in `whoami`.**");
    let proxy_text = paragraph_with(&whoami, "With `--text` this is a line `proxy");

    let skill = skill();
    assert!(read("README.md").contains("proxy_auth_failed"));
    assert!(skill.contains("proxy_auth_failed"));
    let skill_auth = skill
        .lines()
        .find(|l| l.starts_with("- `proxy_auth_failed`"))
        .expect("SKILL.md lists proxy_auth_failed");
    assert!(skill_auth.contains("Tell the human"), "{skill_auth}");
    let skill_whoami = skill
        .lines()
        .find(|l| l.starts_with("- `agent-mail whoami`"))
        .unwrap();
    assert!(skill_whoami.contains("`proxy`"), "{skill_whoami}");

    for text in [
        refused,
        auth,
        other,
        proxy,
        proxy_text,
        skill_auth,
        skill_whoami,
    ] {
        assert!(
            text.chars().count() <= 300,
            "{} characters: {text}",
            text.chars().count()
        );
    }
}

#[test]
fn job023_ac1_mcp_server_section_offers_claude_mcp_add_in_local_scope() {
    let section = readme_section("## MCP server");
    for needle in [
        "claude mcp add",
        "--scope local",
        "AGENT_MAIL_TOKEN=<your agent token>",
    ] {
        assert!(
            section.contains(needle),
            "MCP server section lacks {needle:?}"
        );
    }
    let block = section
        .split("```sh\n")
        .skip(1)
        .map(|b| b.split("```").next().unwrap())
        .find(|b| b.contains("claude mcp add"))
        .expect("no sh block with claude mcp add");
    for needle in [
        "--scope local",
        "--env AGENT_MAIL_TOKEN=<your agent token>",
        "-- agent-mail mcp",
    ] {
        assert!(block.contains(needle), "sh block lacks {needle:?}: {block}");
    }
    let prose = paragraph_with(&section, "The local scope");
    for needle in ["only for you", "in this project", "outside the repository"] {
        assert!(prose.contains(needle), "{prose}");
    }
}

#[test]
fn job023_ac2_mcp_server_section_shows_a_project_mcp_json_with_a_reference() {
    let section = readme_section("## MCP server");
    assert!(json_blocks(&section).iter().any(|b| {
        let server = &b["mcpServers"]["agent-mail"];
        server["command"] == "agent-mail"
            && server["args"] == json!(["mcp"])
            && server["env"] == json!({"AGENT_MAIL_TOKEN": "${AGENT_MAIL_TOKEN}"})
    }));
    let prose = paragraph_with(&section, "from the environment");
    assert!(prose.contains(".claude/settings.local.json"), "{prose}");
    assert!(prose.contains("`env`"), "{prose}");
    assert!(
        prose.contains("Agent Tokens in Claude Code and Claude Desktop"),
        "{prose}"
    );
}

#[test]
fn job023_ac3_mcp_server_section_warns_never_to_commit_a_token() {
    let section = readme_section("## MCP server");
    assert!(section.contains(".mcp.json"));
    let sentence = section
        .split(". ")
        .flat_map(|s| s.split("\n\n"))
        .find(|s| s.to_lowercase().contains("never") && s.contains("commit"))
        .expect("no sentence says never to commit a token");
    assert!(sentence.contains(".mcp.json"), "{sentence}");
    // The placeholder-only scan covers the README.
    let readme = read("README.md");
    let tokens = token_values(&readme_section("## MCP server"));
    assert!(tokens.contains(&"${AGENT_MAIL_TOKEN}".to_string()));
    for token in token_values(&readme) {
        assert!(is_placeholder(&token), "README token value {token:?}");
    }
}

// Job 024: the README start page.

/// The README text before its first line that starts with `## `.
fn readme_header() -> String {
    let readme = read("README.md");
    let end = readme
        .match_indices('\n')
        .map(|(i, _)| i + 1)
        .find(|&i| readme[i..].starts_with("## "))
        .expect("README has no ## heading");
    readme[..end].to_string()
}

/// The `## ` headings of the README, in order, without the `## `.
fn readme_h2s() -> Vec<String> {
    read("README.md")
        .lines()
        .filter_map(|l| l.strip_prefix("## "))
        .map(str::to_string)
        .collect()
}

/// The README text before the line `## Install`.
fn readme_top() -> String {
    let readme = read("README.md");
    let end = readme
        .find("\n## Install\n")
        .expect("README has no ## Install");
    readme[..end].to_string()
}

/// The GitHub anchor of a heading: lower-cased, spaces turned into `-`,
/// punctuation other than `-` dropped.
fn github_anchor(heading: &str) -> String {
    let slug: String = heading
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            '-' | '_' => Some(c),
            c if c.is_alphanumeric() => Some(c),
            _ => None,
        })
        .collect();
    format!("#{slug}")
}

/// Asserts that every needle appears in `text`, in the given order; panics if
/// one is missing or out of order.
fn assert_in_order(text: &str, needles: &[&str]) {
    let mut from = 0;
    for needle in needles {
        let at = text[from..]
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} missing or out of order"));
        from += at + needle.len();
    }
}

#[test]
fn job024_ac1_header_is_centred_with_icon_name_and_tagline() {
    let header = readme_header();
    assert!(header.contains("align=\"center\""), "{header}");
    let img = header
        .split("<img")
        .skip(1)
        .find(|t| t.contains("src=\"assets/icon.png\""))
        .expect("no <img> with src=\"assets/icon.png\"");
    let img = &img[..img.find('>').unwrap()];
    assert!(img.contains("width=\"128\""), "{img}");
    let alt = img
        .split("alt=\"")
        .nth(1)
        .map(|a| &a[..a.find('"').unwrap()])
        .expect("icon has no alt");
    assert!(!alt.trim().is_empty(), "{img}");
    assert!(
        header.contains("<h1 align=\"center\">Agent Mail</h1>"),
        "{header}"
    );
    assert!(header.contains("Connects your agents securely to your email"));
    assert!(root().join("assets/icon.png").is_file());
}

#[test]
fn job024_ac2_old_title_is_gone_and_first_line_is_not_a_markdown_heading() {
    let readme = read("README.md");
    for gone in ["Connects you agents", "E-Mails"] {
        assert!(!readme.contains(gone), "README still holds {gone:?}");
    }
    assert!(!readme.lines().any(|l| l.starts_with("# Agent Mail -")));
    let first = readme.lines().find(|l| !l.trim().is_empty()).unwrap();
    assert!(!first.starts_with("# "), "{first}");
}

#[test]
fn job024_ac3_header_has_release_platform_and_license_badges_in_links() {
    let header = readme_header();
    let links: Vec<(&str, &str)> = header
        .split("<a href=\"")
        .skip(1)
        .map(|t| {
            let href = &t[..t.find('"').unwrap()];
            let inner = &t[..t.find("</a>").expect("unclosed <a>")];
            (href, inner)
        })
        .collect();
    let badge = |fragment: &str| {
        links
            .iter()
            .find(|(_, inner)| inner.contains("<img") && inner.contains(fragment))
            .map(|(href, _)| *href)
            .unwrap_or_else(|| panic!("no linked badge with {fragment:?}"))
    };
    assert_eq!(
        badge("img.shields.io/github/v/release/ollie-dev/agent-mail"),
        "https://github.com/ollie-dev/agent-mail/releases/latest"
    );
    badge("img.shields.io/badge/platform-macOS");
    assert_eq!(badge("img.shields.io/badge/license-Apache"), "LICENSE");
}

#[test]
fn job024_ac4_why_section_has_short_bullets_covering_the_promises() {
    let h2s = readme_h2s();
    let why = h2s.iter().position(|h| h == "Why Agent Mail").unwrap();
    let quick = h2s.iter().position(|h| h == "Quick start").unwrap();
    assert_eq!(why, 0, "## Why Agent Mail must follow the header");
    assert!(why < quick);
    let section = readme_section("## Why Agent Mail");
    let bullets: Vec<&str> = section.lines().filter(|l| l.starts_with("- ")).collect();
    let others: Vec<&str> = section
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with("- "))
        .collect();
    assert!(others.is_empty(), "not a list: {others:?}");
    assert!(!bullets.is_empty() && bullets.len() <= 6, "{bullets:?}");
    for b in &bullets {
        assert!(b.len() <= 120, "bullet too long: {b}");
    }
    let joined = bullets.join("\n");
    for needle in ["Keychain", "Mandate", "audit", "draft", "never sends"] {
        assert!(joined.contains(needle), "Why misses {needle:?}");
    }
    assert!(!read("README.md").contains("simple and secure way"));
}

#[test]
fn job024_ac5_quick_start_lists_install_create_and_token_link_in_order() {
    let h2s = readme_h2s();
    let quick = h2s.iter().position(|h| h == "Quick start").unwrap();
    let contents = h2s.iter().position(|h| h == "Contents").unwrap();
    assert_eq!(h2s[quick - 1], "Why Agent Mail");
    assert!(quick < contents);
    let section = readme_section("## Quick start");
    assert_in_order(&section, &[INSTALLER, "agent-mail mandate create"]);
    let after = &section[section.find("agent-mail mandate create").unwrap()..];
    let step = paragraph_with(after, "#agent-tokens-in-claude-code-and-claude-desktop");
    assert!(step.contains("Claude Code"), "{step}");
    assert!(step.contains("Claude Desktop"), "{step}");
    assert!(
        step.contains("](#agent-tokens-in-claude-code-and-claude-desktop)"),
        "{step}"
    );
    assert!(!section.contains("{\"env\""), "{section}");
    assert!(!section.contains("AGENT_MAIL_TOKEN="), "{section}");
    let fences = section
        .lines()
        .filter(|l| l.trim_start().starts_with("```"))
        .count();
    assert!(fences / 2 <= 3, "{fences} fences");
}

#[test]
fn job024_ac6_contents_links_every_later_heading_in_order() {
    let h2s = readme_h2s();
    let contents = h2s.iter().position(|h| h == "Contents").unwrap();
    let install = h2s.iter().position(|h| h == "Install").unwrap();
    assert_eq!(h2s[contents - 1], "Quick start");
    assert!(contents < install);
    let section = readme_section("## Contents");
    assert_eq!(
        github_anchor("What each OS protects against"),
        "#what-each-os-protects-against"
    );
    let targets: Vec<String> = h2s[contents + 1..]
        .iter()
        .map(|h| format!("]({})", github_anchor(h)))
        .collect();
    let targets: Vec<&str> = targets.iter().map(String::as_str).collect();
    assert_in_order(&section, &targets);
}

#[test]
fn job024_ac7_sections_after_contents_are_kept_in_place_and_order() {
    let h2s = readme_h2s();
    let contents = h2s.iter().position(|h| h == "Contents").unwrap();
    assert_eq!(
        h2s[contents + 1..],
        [
            "Install",
            "Creating a Mandate",
            "Agent Tokens in Claude Code and Claude Desktop",
            "Claude Code plugin",
            "Gmail app passwords",
            "What each OS protects against",
            "Several Mandates and whoami",
            "Usage errors",
            "Search",
            "Read",
            "Flag",
            "Move",
            "Draft",
            "MCP server",
            "Sandbox and proxies",
            "Audit log",
            "Development",
        ]
    );
}

#[test]
fn job024_ac8_prose_paragraphs_before_install_are_short() {
    let top = readme_top();
    let mut in_fence = false;
    let mut paragraphs = Vec::new();
    let mut prose = String::new();
    // Drop fenced code blocks first, keeping blank lines as separators.
    for line in top.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            prose.push('\n');
            continue;
        }
        if !in_fence {
            prose.push_str(line);
        }
        prose.push('\n');
    }
    for block in prose.split("\n\n") {
        let block = block.trim();
        if block.is_empty() || block.starts_with('<') {
            continue;
        }
        if block.starts_with("- ") || block.starts_with("* ") {
            paragraphs.extend(block.split("\n- ").map(str::to_string));
        } else {
            paragraphs.push(block.to_string());
        }
    }
    assert!(!paragraphs.is_empty());
    for p in paragraphs {
        assert!(p.chars().count() <= 300, "paragraph too long: {p}");
    }
}

#[test]
fn job024_ac9_token_values_in_the_top_part_are_placeholders() {
    for token in token_values(&readme_top()) {
        assert!(is_placeholder(&token), "README token value {token:?}");
    }
}

#[test]
fn job026_ac6_readme_records_whether_the_sandbox_reaches_the_pipe() {
    let section = readme_section("## Sandbox and proxies");
    let pipe = paragraph_with(&section, r"`\\.\pipe\agent-mail`");
    assert!(pipe.contains("broker_unavailable"), "{pipe}");
    assert!(pipe.contains("agent-mail mcp"), "{pipe}");
    for block in section.split("\n\n") {
        // The section's code blocks hold no blank line.
        if block.trim_start().starts_with("```") {
            assert!(block.trim_end().ends_with("```"), "{block}");
            continue;
        }
        // Each list item is its own paragraph.
        for item in format!("\n{block}").split("\n- ") {
            let joined: Vec<&str> = item.lines().map(str::trim).collect();
            let text = joined.join(" ");
            assert!(
                text.trim().chars().count() <= 300,
                "a paragraph of {} characters: {text}",
                text.trim().chars().count()
            );
        }
    }
}
