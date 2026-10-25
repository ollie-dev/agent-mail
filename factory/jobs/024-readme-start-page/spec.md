# README start page

## Goal
A visitor who opens the repository on GitHub sees a centred header with the Agent Mail icon, name, tagline and badges. Below it they get a short "why", a quick start of a few commands and a table of contents. The existing setup and reference sections follow, unchanged in content and reachable through the table of contents.

## Context
- Stack: Rust, one binary crate. Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Files touched:
  - `README.md`: new top part (header, Why, Quick start, Contents) in place of the current title line and intro paragraph. Everything from `## Install` to the end stays as it is.
  - `tests/plugin.rs`: new tests for the start page, next to the existing README setup-section tests. They reuse its `readme_section`, `paragraph_with`, `token_values` and `is_placeholder` helpers.
  - `assets/icon.png` already exists (480×480 PNG, committed in 2a79ad4). It is only referenced, not changed.
- Decision: the reference sections stay in `README.md` as `##` sections, not moved to `docs/`. They already sit below the setup sections, and keeping them as `##` keeps every `readme_section("## …")` check in `tests/plugin.rs` and `tests/release.rs` valid without edits. The table of contents makes them reachable.
- The license is Apache 2.0 (`LICENSE`); `Cargo.toml` has no `license` field. The repository is `ollie-dev/agent-mail`.

## Acceptance criteria
1. AC1: Header. Before the first line that starts with `## `, the README holds an element with `align="center"`. Inside it are: an `<img>` with `src="assets/icon.png"`, `width="128"` and a non-empty `alt`; the name as `<h1 align="center">Agent Mail</h1>` (or an `<h1>` inside the centred element, with text exactly `Agent Mail`); and the tagline `Connects your agents securely to your email`. A test checks each of these strings in the text before the first `## ` line, and that `assets/icon.png` exists.
2. AC2: Old title gone. The README no longer contains `Connects you agents`, `E-Mails`, or a line starting with `# Agent Mail -`. Its first non-blank line is not a Markdown `# ` heading, because the header is HTML.
3. AC3: Badges. The header holds three badge images, each wrapped in a link:
   - latest release: an image URL containing `img.shields.io/github/v/release/ollie-dev/agent-mail`, linking to `https://github.com/ollie-dev/agent-mail/releases/latest`;
   - platform: an image URL containing `img.shields.io/badge/platform-macOS`;
   - license: an image URL containing `img.shields.io/badge/license-Apache` (or `img.shields.io/github/license/ollie-dev/agent-mail`), linking to `LICENSE`.

   A test checks each URL fragment and link target in the text before the first `## ` line.
4. AC4: Why section. A `## Why Agent Mail` section comes after the header and before `## Quick start`. It is a list of at most six short bullets that together contain `Keychain`, `Mandate`, `audit`, `draft` and `never sends`. The current intro sentence ("Agent Mail is a simple and secure way …") is removed, so the README does not contain `simple and secure way`.
5. AC5: Quick start. A `## Quick start` section comes after `## Why Agent Mail` and before the table of contents. In order, it holds:
   - the installer one-liner `curl -fsSL https://github.com/ollie-dev/agent-mail/releases/latest/download/install.sh | sh`;
   - `agent-mail mandate create`;
   - a step that names Claude Code and Claude Desktop and links to `#agent-tokens-in-claude-code-and-claude-desktop`.

   It has no JSON config block and no `AGENT_MAIL_TOKEN=` value, so the token setup is said once, in the Agent Tokens section. It holds at most three fenced code blocks. A test checks the three items appear in this order and that the section has no `{"env"` and no more than three ```` ``` ```` fences opening code blocks.
6. AC6: Table of contents. A `## Contents` section comes after `## Quick start` and before `## Install`. For every `## ` heading below it, it holds a Markdown link whose target is that heading's GitHub anchor: lower-cased, spaces turned into `-`, punctuation other than `-` dropped. Example: `## What each OS protects against` becomes `#what-each-os-protects-against`. The links are in the same order as the headings. A test derives the anchors from the headings and checks their presence and order in the Contents section.
7. AC7: Sections kept in place and order. After `## Contents`, the README has exactly these `##` headings in this order, with no others: Install, Creating a Mandate, Agent Tokens in Claude Code and Claude Desktop, Claude Code plugin, Gmail app passwords, What each OS protects against, Several Mandates and whoami, Usage errors, Search, Read, Flag, Move, Draft, MCP server, Sandbox and proxies, Audit log, Development. A test checks the list of headings after `## Contents` equals this list.
8. AC8: Short paragraphs in the new top part. In the text before `## Install`, every prose paragraph is at most 300 characters. A paragraph is a `\n\n`-separated block that is not a fenced code block, a list or an HTML block; each list item counts as its own paragraph. A test checks this.
9. AC9: Existing checks and the scan still pass. All existing tests in `tests/plugin.rs` and `tests/release.rs` pass unchanged, including the README placeholder-only token scan (`ac8_no_real_credential_or_agent_token_in_plugin_or_readme`). Every `AGENT_MAIL_TOKEN=` value in the new top part, if any, is a placeholder. `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` are green.

## Out of scope
- Moving reference sections into `docs/`, or splitting them into separate files.
- Rewriting reference sections that already exceed about 300 characters per paragraph (such as the Releasing and audit-log paragraphs). Their wording stays unchanged.
- Changing the content of any section from `## Install` on. The only allowed edits there are broken anchor fixes.
- Screenshots, demo GIFs, a dark-mode icon variant, or changes to `assets/icon.png`.
- Adding a `license` field to `Cargo.toml`, a CI badge, or edits to `CLAUDE.md`, the plugin skill, or the release notes.
- The Intel/Rosetta coverage and Windows backlog items.

## Notes for reviewers
- Security: no code paths change. The README is the only risk. It must hold no real Agent Token, Credential or real email address, and the existing placeholder-only scan covers the whole README. The Why bullets must not overclaim: "never sends" is accurate, since there is no send operation and `draft` only appends to Drafts. Keychain protection applies to macOS only, so the platform badge says macOS. The Quick start must not tell users to put a token in a committed file; it links to the Agent Tokens section, which already warns against that.
- UX: the reader is the operator, a person setting up Agent Mail for their Agents. Their flow: read the Why, run the installer, run `agent-mail mandate create` in their own terminal, then follow the link to put the token into Claude Code or Claude Desktop. The install one-liner and `mandate create` appear both in Quick start and in their full sections. That is deliberate: the Quick start repeats commands but not explanations. Reviewers should check that no prose is repeated.
- UI: GitHub-rendered Markdown. Check the centred header (icon at about 128px, h1, tagline, badges on one line) renders on github.com: GitHub honours `align="center"` on `<p>`, `<div>` and `<h1>`, and needs a blank line between an HTML block and the Markdown that follows. Check that every Contents link resolves to its heading.
