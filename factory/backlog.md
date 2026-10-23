# Backlog

`/factory next` runs the first unchecked item and ticks it off. Add one feature per line.

- [x] Ticket 01: Project skeleton (.scratch/agent-mail-v1/issues/01-project-skeleton.md) → 001-project-skeleton
- [x] CLI polish: running `agent-mail` with no arguments prints a usage hint; the `--help` description avoids the word "mandate" until mandate commands exist → 002-cli-polish
- [x] Ticket 02: Tracer bullet: an Agent lists its granted mailboxes (.scratch/agent-mail-v1/issues/02-tracer-list-mailboxes.md) → 003-tracer-list-mailboxes
- [x] Ticket 03: `mandate create`: guided setup stored in the Keychain (.scratch/agent-mail-v1/issues/03-mandate-create.md) → 004-mandate-create
- [x] Ticket 04: `mandate list` and `mandate delete` (.scratch/agent-mail-v1/issues/04-mandate-list-delete.md) → 005-mandate-list-delete
- [x] Ticket 05: Export a Mandate's choices and re-create from file (.scratch/agent-mail-v1/issues/05-mandate-export-from.md) → 006-mandate-export-from
- [x] Ticket 06: Audit log (.scratch/agent-mail-v1/issues/06-audit-log.md) → 007-audit-log
- [x] Ticket 07: Search messages (.scratch/agent-mail-v1/issues/07-search.md) → 008-search-messages
- [x] Ticket 08: Read a message (.scratch/agent-mail-v1/issues/08-read.md) → 009-read-message
- [x] Ticket 09: Set and clear read and starred flags (.scratch/agent-mail-v1/issues/09-flag.md) → 010-flag-messages
- [x] Ticket 10: Move a message between granted mailboxes (.scratch/agent-mail-v1/issues/10-move.md) → 011-move-message
- [x] Ticket 11: Create drafts, including replies (.scratch/agent-mail-v1/issues/11-draft.md) → 012-create-drafts
- [x] Ticket 12: Gmail search syntax and opt-in live Gmail test (.scratch/agent-mail-v1/issues/12-gmail-search-live-test.md) → 013-gmail-search-live-test
- [x] Ticket 13: Several Mandates per Agent and `whoami` (.scratch/agent-mail-v1/issues/13-multiple-tokens-whoami.md) → 014-multiple-mandates-whoami
- [x] Ticket 14: Work inside the Claude Code sandbox via its proxy (.scratch/agent-mail-v1/issues/14-sandbox-proxy.md) → 015-sandbox-proxy
- [x] Ticket 15: MCP server with all Agent commands (.scratch/agent-mail-v1/issues/15-mcp-server.md) → 016-mcp-server
- [x] Ticket 16: Hardened-runtime self-check and release pipeline (.scratch/agent-mail-v1/issues/16-self-check-release.md) → 017-self-check-release
- [x] Ticket 17: Claude Code plugin and README (.scratch/agent-mail-v1/issues/17-plugin-readme.md) → 018-plugin-readme
- [ ] Ticket 18: Windows milestone (.scratch/agent-mail-v1/issues/18-windows.md)
- [ ] Follow-ups F3, F7, F9, F13, F17: short, consistent help text and error messages per the CLAUDE.md convention (at most about 300 characters per paragraph, each thing said once), consistent global-versus-subcommand `--text` wording, and a ui reviewer check for the convention (factory/follow-ups.md)
- [ ] Follow-ups F2, F4, F14, F15: errors an Agent can read: clear errors for a refused APPEND (no draft created, next step), JSON-shaped usage errors across all commands, mention argument mistakes alongside `missing_token` where practical, JSON shape for the in-process `mcp` refusal (factory/follow-ups.md)
- [ ] Follow-ups F8, F10: proxy errors: narrow `proxy_refused` to allowlist refusals (CONNECT 403, SOCKS5 0x02), give 407 and SOCKS5 auth failures their own message, and consider a proxy-setting hint in `whoami` (factory/follow-ups.md)
- [ ] Follow-ups F1, F5, F6, F12, F16: small code and test cleanups: confirm the rejected-`--to` audit entry for the MCP `draft` tool, cap presented tokens in `AGENT_MAIL_TOKEN`, assert the audit `mandate` per command in tests/tokens.rs, parse arguments once in main.rs, drop the unused guarded `keys()` (factory/follow-ups.md)
- [ ] Follow-ups F11, F18, F19 (decided): README recommends `claude mcp add` local scope or `${AGENT_MAIL_TOKEN}` env expansion and warns not to commit the token; install.sh accepts only `arm64`, not `aarch64`; drop `list` from the release-binary loop in tests/selfcheck.rs so the opt-in test never writes to the real audit log (factory/follow-ups.md)
- [ ] Intel Mac (x86_64) coverage: the x86_64-apple-darwin binary is already built, signed, attested, published and picked by install.sh, but it is cross-compiled on an arm64 runner and never run. Run it under Rosetta in the release workflow (`arch -x86_64`): `cargo test --locked --target x86_64-apple-darwin` in the `test` job, and `--version` for both targets (not only the host one) in `scripts/verify-release.sh`, with tests/release.rs checks to match
