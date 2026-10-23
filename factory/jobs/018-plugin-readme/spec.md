# Claude Code plugin and README setup guide

## Goal
A Claude Code user can add this repository as a plugin marketplace and install the `agent-mail` plugin. That gives every session the agent-mail skill and the `agent-mail mcp` server, and the skill tells the human how to install the binary if it is missing. The README walks a human through the whole setup without help: installing, `mandate create`, where the Agent Token goes in Claude Code and Claude Desktop, the sandbox's allowed domains, one Gmail app password per Mandate, and exactly what each OS protects against.

## Context
- Stack: Rust, one binary crate (ADR-0001). Tests: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`. This job changes no Rust under `src/`. It adds Markdown and JSON files and one integration test file.
- Ticket: `.scratch/agent-mail-v1/issues/17-plugin-readme.md`. Feature spec: `.scratch/agent-mail-v1/spec.md`, user stories 25, 57, 60–64 and the "Distribution and hosts" decisions. Glossary terms used here: Account, Credential, Agent, Mandate, Agent Token, Policy, Grant.
- New files:
  - `.claude-plugin/marketplace.json` at the repo root. It is a Claude Code plugin marketplace named `agent-mail` with one plugin, `agent-mail`, whose `source` is `./plugin`. The human installs it with `/plugin marketplace add ollie-dev/agent-mail` and then `/plugin install agent-mail@agent-mail`.
  - `plugin/.claude-plugin/plugin.json`: the plugin manifest, with `name` `agent-mail` and a `description`. If it has a `version`, it equals `version` in `Cargo.toml`.
  - `plugin/.mcp.json`: one MCP server, `agent-mail`, with `command` `agent-mail` and `args` `["mcp"]`. It holds no token. The server inherits `AGENT_MAIL_TOKEN` from Claude Code's environment, which the `env` of `.claude/settings.local.json` sets. If an `env` key is present, each value is only a `${...}` reference, such as `"${AGENT_MAIL_TOKEN:-}"`.
  - `plugin/skills/agent-mail/SKILL.md`: the skill. Its YAML frontmatter has `name: agent-mail` and a `description` saying when to use it (reading, searching, triaging email and drafting replies).
  - The plugin carries no binary: no executable file and nothing under `plugin/bin/`.
  - `tests/plugin.rs`: the checks for this job. It reads the files above, `README.md` and `CLAUDE.md` from the repo root and parses the JSON with `serde_json`. It has its own README section helper, the same rule as `readme_section` in `tests/release.rs`, or moves that helper to `tests/support/`.
- Changed files: `README.md` (new and revised sections, listed per criterion) and `CLAUDE.md` (the layout line names the new files).
- The Agent commands the skill documents, as `src/lib.rs` and the README define them: `whoami`, `list`, `search`, `read`, `flag`, `move`, `draft`, and the matching MCP tools of `src/mcp.rs`. Errors are `{"error":{"code":...}}` on stderr, exit code 1. Usage errors exit with code 2. A denial has `code` `denied` with `operation`, `account` and, for one-mailbox operations, `mailbox`.
- Installer one-liner, exactly as in the README Install section and `tests/release.rs`: `curl -fsSL https://github.com/ollie-dev/agent-mail/releases/latest/download/install.sh | sh`

## Acceptance criteria
1. AC1: Installing the plugin in Claude Code makes the agent-mail skill and the `agent-mail mcp` server available.
   Test (structural, because a real `/plugin install` needs Claude Code):
   - `.claude-plugin/marketplace.json` parses as JSON with `name` `"agent-mail"` and a `plugins` array holding one entry with `name` `"agent-mail"` and `source` `"./plugin"`.
   - `plugin/.claude-plugin/plugin.json` parses with `name` `"agent-mail"` and a non-empty `description`. If `version` is present, it equals `Cargo.toml`'s `version`.
   - `plugin/skills/agent-mail/SKILL.md` starts with a `---` frontmatter block holding `name: agent-mail` and a non-empty `description:`.
   - `plugin/.mcp.json` parses with `mcpServers.agent-mail.command == "agent-mail"` and `mcpServers.agent-mail.args == ["mcp"]`.
   - No file under `plugin/` has an executable bit or is a Mach-O/ELF binary (its first bytes are not `\xCF\xFA\xED\xFE`, `\xCA\xFE\xBA\xBE` or `\x7FELF`).
   - The README has a `## Claude Code plugin` section containing `/plugin marketplace add ollie-dev/agent-mail` and `/plugin install agent-mail@agent-mail`.
2. AC2: The skill documents every Agent command, the JSON output, the denial error, and that Mandates are created only by the human.
   Test: `SKILL.md` contains:
   - each of `agent-mail whoami`, `agent-mail list`, `agent-mail search`, `agent-mail read`, `agent-mail flag`, `agent-mail move`, `agent-mail draft`;
   - `AGENT_MAIL_TOKEN`, `--account`, `ref`, `--gmail` and `--text`;
   - an example error object containing `"code":"denied"` together with `"operation"` and `"account"`;
   - the error codes `missing_token`, `invalid_token`, `account_required`, `ambiguous_mandate`, `stale_reference` and `proxy_refused`;
   - the sentence `Only the human creates Mandates` and the command `agent-mail mandate create`.
   It never tells the Agent to run `mandate create` or `mandate delete` itself. Test: every line containing `agent-mail mandate` also contains `human`.
3. AC3: When the binary is missing, the skill tells the human to run the installer and does not try to install it itself.
   Test:
   - `SKILL.md` has a section headed `## If agent-mail is missing` that contains the installer one-liner verbatim and the sentence `Do not run the installer yourself`.
   - The one-liner appears nowhere else in `SKILL.md`.
   - The frontmatter has no `allowed-tools` entry that names `curl` or `sh`.
4. AC4: The README shows where to put the Agent Token for Claude Code and for Claude Desktop, using placeholder values only.
   Test: the README has a `## Agent Tokens in Claude Code and Claude Desktop` section containing:
   - `.claude/settings.local.json` and the words `git-ignored`, with a JSON block that parses to `{"env":{"AGENT_MAIL_TOKEN":"<your agent token>"}}`;
   - `claude_desktop_config.json`, with a JSON block whose `mcpServers.agent-mail` has `command` `"agent-mail"`, `args` `["mcp"]` and `env.AGENT_MAIL_TOKEN` `"<your agent token>"`.
   The README no longer contains the string ``in the project's `.mcp.json` ``: a checked-in `.mcp.json` must not hold a token.
5. AC5: The README lists the sandbox allowed-domain settings for Gmail's IMAP server and how to add other hosts.
   Test: the README's `## Sandbox and proxies` section:
   - holds a JSON block that parses and contains the string `imap.gmail.com` in the setting Claude Code's sandbox reads its allowed network domains from;
   - names the settings file it goes in (`.claude/settings.local.json` or `.claude/settings.json`);
   - contains `agent-mail whoami` and `host`, saying that another Account's IMAP host, as `whoami` shows it, is added to the same list;
   - contains `proxy_refused`, as the error an Agent sees when the host is missing.
6. AC6: The README states that on macOS a hijacked Agent cannot extract Credentials, that a Keychain prompt outside an update is a red flag, and that tokens in host config files are readable by other Agents of the same user.
   Test: the README has a `## What each OS protects against` section containing:
   - `macOS`, `cannot extract` and `Credential`;
   - `red flag`;
   - `same user` and `Agent Token`;
   - `Windows`, stating that the Windows milestone, when it ships, protects only against honest Agents (the words `honest Agents`).
7. AC7: The README recommends one Gmail app password per Mandate and explains revoking on Google's side.
   Test: the README has a `## Gmail app passwords` section containing `one app password per Mandate`, `https://myaccount.google.com/apppasswords`, `revoke` and `agent-mail mandate delete`.
8. AC8: No example in the plugin or README contains a real Credential or Agent Token.
   Test, over `README.md`, `.claude-plugin/marketplace.json` and every file under `plugin/`:
   - no match for the Agent Token shape `amt_[A-Za-z0-9_-]{20,}`;
   - no match for a Gmail app password shape `\b[a-z]{4} [a-z]{4} [a-z]{4} [a-z]{4}\b`;
   - every `AGENT_MAIL_TOKEN` value given in an example (after `AGENT_MAIL_TOKEN=`, or as the JSON value of `"AGENT_MAIL_TOKEN"`) is a `<...>` placeholder, an `amt_...` placeholder containing `...`, or a `${...}` reference;
   - every email address in these files ends in `example.test`, `example.com` or `gmail.com`, and the `gmail.com` ones are only `you@gmail.com`/`you.test@gmail.com`-style placeholders starting with `you`.
9. AC9 (from the ticket's "What to build": the README covers `mandate create`): the README has a `## Creating a Mandate` section containing `agent-mail mandate create`, `mandate create --from`, `agent-mail mandate list` and `agent-mail mandate delete`, and the words `own terminal` and `shown once`.
10. AC10 (CLAUDE.md's layout rule): `CLAUDE.md` contains `plugin/`, `.claude-plugin/marketplace.json`, `plugin/skills/agent-mail/SKILL.md` and `tests/plugin.rs`. `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass.

## Out of scope
- A real `/plugin install` run inside Claude Code in CI. AC1 checks the manifests instead.
- Bundling or downloading the binary from the plugin, and a plugin `bin/` directory.
- Hooks, slash commands or agents in the plugin, beyond the one skill and the MCP entry.
- The Windows milestone itself (ticket 18): installer, Credential Manager store, Windows README instructions. Only the protection statement in AC6 is written now.
- Codex, Copilot and VS Code hosts, and a one-click `.mcpb` package for Claude Desktop.
- Changing CLI help text, error messages or MCP tool descriptions.
- Rewriting the existing command reference sections (Search, Read, Flag, Move, Draft, Audit log). They stay; only the MCP server section's token advice changes (AC4).

## Notes for reviewers
- Security:
  - The plugin and README are copied by users, so any token, app password or real address in them would be a leak. AC8 scans for the token shape (`amt_` plus base64url, per `src/seal.rs`) and the 16-letter app password shape.
  - The plugin's `.mcp.json` must never hold a literal token. A plugin file is shared and versioned, so the token must come from the per-project, git-ignored `.claude/settings.local.json`.
  - The current README tells users to put the token in the project's `.mcp.json`, which is usually committed. AC4 removes that advice.
  - The skill must not have the Agent run `curl | sh` (AC3) or `mandate create`/`mandate delete` (AC2). Reviewers should read the skill as a possibly prompt-injected Agent would and check that nothing invites it to widen its own access or install software.
  - The "What each OS protects against" text must match ADR-0002. On macOS, a hijacked Agent cannot extract a Credential, but the human can approve a Keychain prompt. Agent Tokens in host config files are readable by any process of the same user. Windows protects only against honest Agents.
- Unverified host details for the builder to confirm against current Claude Code documentation, not guess:
  - The exact settings key for the sandbox's allowed network domains (AC5). It could be a `sandbox.network` allow-list or `WebFetch(domain:...)` permission rules, depending on the Claude Code version. The test only requires parseable JSON containing `imap.gmail.com`, so the builder can follow whatever the current schema says.
  - Whether `env` from `.claude/settings.local.json` reaches plugin MCP servers. If it does not, the `${AGENT_MAIL_TOKEN:-}` reference in `plugin/.mcp.json` is the fallback, and the README must say how the token reaches the server.
  - The marketplace and plugin manifest field names.
- AC1 is structural. Making the skill and server actually show up is confirmed only by a manual `/plugin install` in Claude Code. Reviewers should note in the review whether one was done.
- AC9 and AC10 are additions beyond the ticket's checklist. AC9 comes from the ticket's "What to build" (the README covers `mandate create`) and AC10 from CLAUDE.md's layout rule. The ticket's AC1–AC8 are carried over unchanged, in order.
- The required phrases in the tests (`Only the human creates Mandates`, `Do not run the installer yourself`, `one app password per Mandate`) are fixed so that one test per criterion can check them. Reviewers may suggest better wording, but changing it means changing the test too.
- UX: there are two users.
  - The human (operator) follows this flow: install the binary with the one-liner; run `agent-mail mandate create` in their own terminal and copy the Agent Token shown once; put it in `.claude/settings.local.json` `env` (Claude Code) or the Claude Desktop MCP entry's `env`; allow `imap.gmail.com` (or their host) in the sandbox; install the plugin.
  - The Agent reads the skill and follows this flow: `whoami`, then `list`/`search`, then `read`/`flag`/`move`/`draft` using `ref` values. On `denied` or `proxy_refused` it tells the human what to grant or allow. If the binary is missing, it tells the human to run the installer.
  - Following CLAUDE.md, skill and README paragraphs stay short and say each thing once, at most about 300 characters per paragraph or line.
- UI: no rendered UI. The output is Markdown (README, SKILL.md) and JSON manifests that Claude Code reads. The skill text is what the Agent sees.
