# Follow-ups

Review findings that were real but out of scope for their job. The orchestrator appends to this file from each merged job's `decision.md`; see "Follow-up ledger" in `.claude/skills/factory/SKILL.md`. To work on some of them, name their ids in a backlog line. Lines are ticked off, never deleted.

- [ ] F1 · 012-create-drafts · code · src/core.rs:973: when the core gets an invalid `--to`, the audit entry logs `"recipients":[]` (a deliberate deviation) → when the MCP `draft` tool lands (ticket 15), confirm this is the behaviour wanted for rejected input from that adapter.
- [ ] F2 · 012-create-drafts · code · src/imap.rs:856: an APPEND the server refuses (`NO [TRYCREATE]`, `NO`, `BAD`) becomes a generic `Protocol`/`server_error` → map these to a clearer error that says no draft was created and names a next step.
- [ ] F3 · 012-create-drafts · ui · src/lib.rs:112: the `draft --help` long description is one unwrapped paragraph of about 980 characters, the same style as other subcommands → wrap or split long help text across all commands in one pass.
- [ ] F4 · 012-create-drafts · ux · src/lib.rs:226: usage errors from clap value parsers (bad `--to`, bad `--reply-to`) are plain text on stderr, not the JSON error shape the core uses; the other commands behave the same → make usage errors machine-readable for agents across all commands.
