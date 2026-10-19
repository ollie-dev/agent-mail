# agent-mail v1

Status: ready-for-agent

## Problem Statement

I want AI agents (Claude Code sessions, Claude Desktop) to work with my email: find messages, read them, triage them and prepare replies. Today that means handing an agent my mail password or a full-access token, which gives it everything: every folder, every action, forever, with the secret sitting in a config file or environment variable the agent can read and leak. Agents read untrusted mail by design, so a single prompt-injected message could make one exfiltrate the password or act far beyond what I intended. I have no way to say "this agent may read INBOX and write drafts, nothing else", no way to keep the password out of its reach, and no record of what it did.

## Solution

`agent-mail` is a single native binary that agents call as a CLI (through a Claude Code skill) or as an MCP server (Claude Desktop). I create a **Mandate** in my own terminal through a guided setup: I pick the Accounts, type each Credential (checked by a live login), tick the mailboxes and operations each Account allows, and receive an **Agent Token** once. I give the token to an Agent. From then on the Agent can do exactly what the Mandate's **Policy** grants, and nothing else, without ever seeing the Credential.

Mandates are sealed and immutable: there is no edit command, and creating a new one requires every Credential again, so an Agent that can run `agent-mail` still cannot widen its own rights. On macOS the Mandate lives in the Keychain, readable only by the signed, hardened agent-mail binary, so even a hijacked Agent cannot extract the Credential. On Windows (a later milestone of v1) the same design protects only against honest Agents, and the documentation says so plainly.

v1 reads and organises mail and writes drafts; it never sends. I review and send drafts myself.

## User Stories

### Creating and managing Mandates (human)

1. As a mail owner, I want to create a Mandate through a guided terminal UI, so that I never have to hand-write a config file.
2. As a mail owner, I want Gmail to be offered as a preset with its server and port filled in, so that I only type my address and app password.
3. As a mail owner, I want to add a generic IMAP Account by entering host, port and login, so that I can use any provider with password login.
4. As a mail owner, I want agent-mail to accept only TLS connections (implicit TLS or STARTTLS), so that my Credential never crosses the network in clear text.
5. As a mail owner, I want to type each Credential with hidden input, so that it never appears on screen, in shell history or in an agent's view.
6. As a mail owner, I want agent-mail to refuse Credentials passed as arguments, environment variables or piped input, so that no script or Agent can feed them in unseen.
7. As a mail owner, I want each Credential checked by a live login during creation, so that typos fail immediately instead of during an Agent's work.
8. As a mail owner, I want to pick the mailboxes for each Account from a checklist fetched from the server, so that I don't need to know exact folder names.
9. As a mail owner, I want mailbox names shown in readable UTF-8 (including Gmail's localised names such as `[Gmail]/Entwürfe`), so that I recognise them.
10. As a mail owner, I want to tick the allowed operations per Account (list, search, read, flag, move, draft), so that each Agent gets only what it needs.
11. As a mail owner, I want everything I did not tick to be denied, so that new or unforeseen actions are never allowed by accident.
12. As a mail owner, I want one Mandate to cover several Accounts, so that one Agent can work across my mail with one token.
13. As a mail owner, I want to create several Mandates (for example one per Account), so that I can split access however I like.
14. As a mail owner, I want to see a summary of the Mandate and confirm it before it is sealed, so that I can catch mistakes.
15. As a mail owner, I want the Agent Token shown exactly once at the end, so that it is never stored or retrievable from agent-mail afterwards.
16. As a mail owner, I want to give each Mandate a name, so that I can tell them apart in listings and the audit log.
17. As a mail owner, I want to export the chosen Accounts and Grants as a TOML file at the end of setup, so that I can recreate a similar Mandate later.
18. As a mail owner, I want to start a new Mandate from an exported file, which pre-fills the guided UI, so that "changing" a Mandate is export, adjust, create, delete.
19. As a mail owner, I want Credentials to be asked again even when starting from a file, so that the file never needs to hold a secret.
20. As a mail owner, I want `mandate create` to refuse to run without a real terminal or inside an agent sandbox, with a clear message, so that an Agent cannot drive it.
21. As a mail owner, I want to list my Mandates with ID, name, Accounts, Grants and creation date, so that I can see who may do what.
22. As a mail owner, I want listings never to show Credentials or Agent Tokens, so that listing is always safe.
23. As a mail owner, I want to delete a Mandate by ID, so that I can revoke an Agent instantly.
24. As a mail owner, I want deleting a Mandate to need no Credential, so that revoking is always quick (accepting that an Agent can also delete its own Mandate).
25. As a mail owner, I want the README to recommend one Gmail app password per Mandate, so that I can also revoke an Agent on Google's side and see which Agent is which.

### Using mail (Agent)

26. As an Agent, I want to present my Agent Token through an environment variable, so that my host config can supply it without command-line clutter.
27. As an Agent holding several Mandates, I want to pass several tokens comma-separated, so that I can work across them.
28. As an Agent, I want to name the Account for each command, and omit it when only one Account is available, so that commands stay short.
29. As an Agent, I want agent-mail to use the Mandate that covers the named Account, so that I don't have to track which token belongs where.
30. As an Agent, I want a clear error when two of my Mandates cover the same Account, so that ambiguity is never resolved silently.
31. As an Agent, I want `whoami` to show the Policy of the tokens I present, so that I know what I may do before trying.
32. As an Agent, I want to list the mailboxes I am granted, so that I know where to look.
33. As an Agent, I want to search a granted mailbox by from, to, subject, text, date range, unseen and flagged, so that I can find messages on any IMAP server.
34. As an Agent, I want to search Gmail with Gmail's own search syntax when the server supports it, so that I can use queries like `has:attachment newer_than:7d`.
35. As an Agent, I want searches to return at most 20 results by default, adjustable with a limit, so that my context window doesn't fill up.
36. As an Agent, I want search results to include enough header data (date, from, subject, flags, message reference) to decide what to read.
37. As an Agent, I want to read a message's headers and plain-text body, with HTML converted to text, so that I can understand it without parsing markup.
38. As an Agent, I want long bodies cut at 20,000 characters unless I ask for the full text, with a marker saying it was truncated, so that I stay within budget but know when I'm missing content.
39. As an Agent, I want attachments listed by name, type and size, so that I can tell the human about them.
40. As an Agent, I want to mark messages read or unread and starred or unstarred, so that I can triage.
41. As an Agent, I want to move a message from one granted mailbox to another granted mailbox, so that I can archive and file mail.
42. As an Agent, I want to create a new draft, so that the human can review and send it.
43. As an Agent, I want to create a draft as a reply to a message, so that it lands in the right thread with correct reply headers.
44. As an Agent, I want drafts to go to the Account's Drafts folder found by its special-use flag, so that it works with localised folder names.
45. As an Agent, I want reading not to mark messages as seen, so that my reading doesn't change the human's inbox state.
46. As an Agent, I want a stale message reference (mailbox changed underneath me) to fail with a clear error, so that I never act on the wrong message.
47. As an Agent, I want every denied action to fail with a clear, machine-readable error naming what was missing, so that I can tell the human what to grant.
48. As an Agent, I want JSON output by default, so that I can parse results reliably.
49. As an Agent in Claude Desktop, I want the same operations as MCP tools returning the same structures, so that behaviour is identical regardless of how I'm connected.

### Security guarantees

50. As a mail owner on macOS, I want the Credential readable only by the agent-mail binary, so that an Agent running `security` or its own code gets a Keychain prompt I can deny.
51. As a mail owner, I want the Credential, Policy and token hash sealed in one item, encrypted with a key derived from the Agent Token, so that a forged Policy can never be paired with my real Credential.
52. As a mail owner, I want agent-mail to refuse to touch the Keychain if its own hardened-runtime signature is missing, so that a carelessly built binary fails safe instead of being injectable.
53. As a mail owner, I want agent-mail never to print, log or return a Credential or Agent Token after creation, so that nothing leaks through output.
54. As a mail owner, I want an audit log of every call (time, Mandate ID, operation, Account, mailbox, message references, allowed or denied), so that I can see what each Agent did.
55. As a mail owner, I want the audit log to contain no Credentials, tokens or message bodies, so that the log itself is not a leak.
56. As a mail owner, I want no agent-mail command to expose the audit log to Agents, so that an Agent cannot study its own trail.
57. As a mail owner, I want the README to state exactly what each OS protects against, so that I don't overestimate the Windows build.

### Hosts, sandboxes and installation

58. As a Claude Code user, I want agent-mail to work with the Claude Code sandbox on or off, so that I don't have to weaken my sandbox.
59. As a Claude Code user with the sandbox on, I want agent-mail to connect through the sandbox's proxy (HTTP CONNECT or SOCKS5) automatically, so that only the allowed mail hosts are reachable.
60. As a Claude Code user, I want setup docs listing the exact sandbox settings (allowed domains) for my mail hosts, so that I can enable access precisely.
61. As a Claude Code user, I want a plugin that installs the agent-mail skill and MCP entry, so that agents know how to use it.
62. As a Claude Code user, I want the skill to tell me how to install the binary when it's missing, so that setup is self-explanatory.
63. As a Claude Code user, I want to put the Agent Token in a project's local, git-ignored settings, so that each project can have its own Mandate.
64. As a Claude Desktop user, I want to add agent-mail as an MCP server with the token in its environment, so that Desktop chats and Cowork can use my mail.
65. As a user without a Rust toolchain, I want to install agent-mail with a one-line installer, so that I never compile anything.
66. As a user, I want the installer to verify the download's checksum, so that a tampered binary is rejected.
67. As a cautious user, I want build attestations on each release, so that I can prove a binary came from this repo's CI.
68. As a macOS user, I want the release notes to warn that the first run after an update triggers one Keychain prompt, so that I know which prompt is expected.
69. As a Windows user (later milestone), I want a PowerShell installer and Credential Manager storage, so that I can use agent-mail on Windows with honest Agents.

### Developer

70. As a developer, I want the whole command behaviour testable in-process against a fake IMAP server, so that tests need no network and no real Credentials.
71. As a developer, I want the test Mandate store to exist only in test builds, so that no release binary can be pointed at a forged store.
72. As a developer, I want an opt-in live test against a dedicated Gmail account, so that I can confirm real-world behaviour without putting Credentials in the repo.

## Implementation Decisions

### Architecture (ADR-0001, ADR-0002, ADR-0003)

- One Rust binary. No background service: every CLI invocation, and the MCP server process the host app starts over stdio, reads the Mandate from the OS credential store and enforces the Policy in-process.
- The **core** is one deep module: given the presented Agent Tokens and a command, it selects the Mandate, enforces the Policy, performs the mail operation, writes the audit entry and returns a result structure. The CLI and MCP adapters are thin translations into and out of it.
- The core depends on exactly two ports:
  - **Mandate store**: Keychain (macOS), Credential Manager (Windows milestone), and an in-memory store compiled only into test builds. No environment variable, flag or config can select a store at runtime.
  - **Terminal**: the real TTY and a scripted terminal for tests. Used only by `mandate create`.
- Network access is not a port: the real IMAP client always runs, and tests point it at a fake server.

### Mandate model and sealing

- A Mandate is one sealed record: name, short ID, creation time, one or more Accounts (each with server settings and Credential), the Policy, and the hash of its Agent Token.
- Agent Token: at least 256 bits of randomness with a recognisable prefix. Only its hash is stored. A lookup key and the encryption key are both derived from the token, so the store can find a Mandate by token without storing the token.
- The record is encrypted with an AEAD cipher under the token-derived key, which gives both confidentiality and tamper detection.
- Stored as a single OS credential-store item per Mandate. On macOS the item is created by the agent-mail binary itself, so its access list trusts only that binary.
- Immutable: there is no update path. Changing anything means a new Mandate. Deleting is allowed without authentication.

### Policy and Grants

- A Grant names an operation, an Account and a mailbox list (exact names or `*`). Operations in v1: `list`, `search`, `read`, `flag`, `move`, `draft`.
- `move` requires the operation on both the source and the target mailbox. `draft` takes no mailbox list and always targets the `\Drafts` special-use mailbox.
- Deny by default. A denial returns a structured error naming the operation, Account and mailbox that lacked a Grant.
- Token selection: `AGENT_MAIL_TOKEN` holds one or more tokens, comma-separated. Each command targets one Account (optional when unambiguous). The Mandate covering that Account is used. If two presented Mandates cover it, that is an error. Grants never combine across Mandates.

### Mail operations

- IMAP via async-imap on tokio. TLS is required (implicit TLS or STARTTLS); plaintext is refused.
- Messages are referenced by Account, mailbox, UIDVALIDITY and UID. A UIDVALIDITY mismatch is an error.
- Search: standard criteria (from, to, subject, text, since, before, unseen, flagged). When the server advertises `X-GM-EXT-1`, a Gmail query string is passed through as `X-GM-RAW`. Default limit 20.
- Read uses `BODY.PEEK`, so messages aren't marked seen. Output: headers, the plain-text body (from the text part, or HTML converted to text), and an attachment list (name, type, size). The body is truncated at 20,000 characters with a truncation marker unless full output is requested.
- Flag: set or clear `\Seen` and `\Flagged` only.
- Move: IMAP MOVE where advertised, otherwise copy, mark deleted and expunge that UID only (UIDPLUS).
- Draft: build a message and APPEND it to the `\Drafts` mailbox with the `\Draft` flag. Reply mode sets In-Reply-To and References from the original message and prefixes the subject.
- Mailbox names: modified UTF-7 is decoded for display and encoded for commands. Gmail labels appear as mailboxes.

### `mandate create` guided flow

- Interactive prompts with multi-select checklists (a lightweight prompt library such as `inquire`, not a full-screen TUI).
- Flow:
  1. name;
  2. add Accounts (Gmail preset or generic host, port and TLS mode; login; hidden Credential; live login check);
  3. per Account, tick mailboxes from the server's list;
  4. per Account, tick operations;
  5. confirm the summary;
  6. seal and store;
  7. show the token once;
  8. offer a TOML export of Accounts and Grants (never Credentials).
- `--from <file>` pre-fills the flow from an exported file. Credentials are always typed.
- Refuses to run without a real TTY on stdin and stdout, or when an agent sandbox is detected.

### Other commands

- `mandate list`: ID, name, Accounts, Grants, created. Never secrets.
- `mandate delete <id>`.
- `whoami`: the Policies of the presented tokens.
- Output: JSON by default, one object per result; `--text` for humans. MCP tools mirror the agent commands (`whoami`, `list`, `search`, `read`, `flag`, `move`, `draft`) and return the same structures. Mandate management is CLI-only.

### Audit log

- On by default. One line per call: time, Mandate short ID, operation, Account, mailbox, message references, allowed or denied, and reason.
- Never Credentials, tokens or bodies.
- Stored in the platform's per-user data directory. No command exposes it.

### Network and TLS

- Direct connection by default, with system certificate verification.
- When `HTTPS_PROXY` or `ALL_PROXY` is set, connect through it (HTTP CONNECT or SOCKS5, honouring credentials in the proxy URL) and verify TLS against built-in root certificates (webpki-roots), because the system verifier is unavailable inside the Claude Code sandbox.
- Setup docs list the allowed-domain settings for Claude Code's sandbox.

### macOS hardening

- Release binaries are ad-hoc signed with hardened runtime in CI. At startup, before any Keychain access, the binary verifies its own signature has the hardened-runtime flag and refuses otherwise.
- No Developer ID in v1. Each update changes the code signature, so the first Keychain access after an update prompts once, and release notes say so.

### Distribution and hosts

- GitHub Releases built by GitHub Actions: macOS arm64 and x86_64 first, Windows x64 in the Windows milestone. Each release ships SHA-256 checksums and GitHub build attestations.
- A `curl | sh` installer (PowerShell on Windows) downloads to `~/.local/bin`, verifying the checksum. `dist` (formerly cargo-dist) is the candidate tool; add a custom CI step if it can't apply ad-hoc hardened-runtime signing.
- A Claude Code plugin in the repo provides the agent-mail skill and an MCP entry. It carries no binary.
- Agent Tokens live in the `env` of a project's `.claude/settings.local.json`, or the `env` of the Claude Desktop MCP entry.
- v1 hosts: Claude Code (CLI, sandbox on or off) and Claude Desktop (MCP).

### Dependencies

- async-imap (tokio runtime), tokio, rustls with webpki-roots, mail-parser, rmcp, clap, keyring 4, inquire, plus a small HTML-to-text crate. Roughly 150 crates at build time, a single binary, nothing at runtime.

### Milestones

1. **macOS end to end**: everything above.
2. **Windows**: Credential Manager store, PowerShell installer, Windows release build, and README stating the honest-Agent-only protection.

## Testing Decisions

- **What a good test is**: it drives the core the way an Agent or the human would (tokens plus a command, or a scripted terminal session) and asserts on the returned structure, the fake server's resulting state, and the audit log. Tests never assert on internal types, call order or private helpers.
- **Primary seam: the core.** Nearly all tests go through it, using the test-only in-memory Mandate store and the scripted terminal. Covered:
  - Mandate selection across several tokens, including the ambiguity error;
  - every Grant rule, including `move` needing both mailboxes and deny by default;
  - every mail operation;
  - truncation;
  - Gmail detection (`X-GM-EXT-1`, `\Drafts` lookup with localised names);
  - UIDVALIDITY mismatch;
  - the guided create flow, including live-login failure, TTY refusal, export and `--from`;
  - deletion and listing never exposing secrets;
  - sealing (a tampered or re-paired record fails to open);
  - the audit log's content and absence of secrets.
- **Fake IMAP server.** An in-process IMAP server that the tests script, reached over real TCP with TLS. Its certificate is trusted only in test builds. It can advertise or withhold capabilities (MOVE, UIDPLUS, SPECIAL-USE, X-GM-EXT-1) so both Gmail and generic paths are exercised.
- **Thin adapter tests.** The CLI and MCP adapters are tested in-process only to check that arguments and tool calls map to the right core command and that output matches the core's structures, JSON or text.
- **Opt-in platform tests**, ignored by default and run explicitly: real Keychain store round-trip, the hardened-runtime self-check, and a live Gmail test against a dedicated test account whose Credential comes from the developer's environment and never from the repo.
- **Proxy tests.** A minimal in-process CONNECT and SOCKS5 proxy verifies the proxied path.
- **Prior art.** None: the repo has no code yet. The first ticket establishes the test layout and records it in `CLAUDE.md` (Stack: Rust, test command `cargo test`, lint `cargo clippy -- -D warnings` and `cargo fmt --check`).

## Out of Scope

- Sending mail (SMTP, lettre, the `send` operation with its required recipient allowlist): v2.
- OAuth2 / XOAUTH2, Microsoft 365, Google Workspace accounts: later.
- Codex, Copilot CLI and VS Code Copilot hosts: v2.
- Linux (libsecret): v2.
- Editing or deleting drafts, permanent delete, attachment download, editing Gmail labels directly, IDLE/push.
- A background service or separate-OS-account broker (rejected, ADR-0002).
- An edit command for Mandates (rejected, ADR-0003).
- Developer ID signing and notarisation, Homebrew tap, one-click `.mcpb` package for Claude Desktop.
- Touch ID / Windows Hello confirmation.
- Protection against a hijacked Agent on Windows (not achievable without a service, ADR-0002).

## Further Notes

- **Protection is OS-dependent.** On macOS, the remaining risk is a human approving an unexpected Keychain prompt; because ad-hoc builds prompt after every update, the README must teach users that a prompt outside an update is a red flag.
- **Agent Tokens are only as private as the host config files.** Other Agents running as the same user can read them, so token separation stops honest Agents only. This is accepted, consistent with ADR-0002.
- **Sandbox behaviour.** Keychain writes are blocked inside the Claude Code and Codex sandboxes, which is consistent with `mandate create` running only in the human's own terminal.
- **Tracking.** The factory backlog will reference this spec's tickets one per line, and the `spec-writer` carries each ticket's acceptance criteria over unchanged.
- **Research.** The research behind these decisions (Bun.secrets ACLs, sandbox reachability, crate footprints, Gmail and M365 auth) lives in the session scratchpad and is not in the repo. The facts that matter are captured in ADR-0001 to ADR-0003 and this spec.
