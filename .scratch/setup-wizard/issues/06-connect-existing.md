# 06: Connect an existing Mandate

**What to build:** instead of creating a new Mandate, the human picks "Connect an existing Mandate" and gives the wizard a token it can reuse: taken from a Claude Code project or the Claude Desktop config that already holds it, or pasted hidden. The wizard shows which Mandate it is and then connects it to new destinations as usual. See `../spec.md`, user stories 22–27.

**Blocked by:** 03, 05

**Status:** ready-for-agent

- [ ] The Mandate step offers "Create a new Mandate" and "Connect an existing Mandate"
- [ ] Sources: a Claude Code project folder (its `.claude/settings.local.json`), the Claude Desktop config, or a hidden paste
- [ ] Each candidate token is opened against the store as `whoami` does; the Mandates they open are listed by name, and the human picks one when there are several
- [ ] A token that opens no Mandate is refused with a short message and nothing is written
- [ ] Before the first destination, the wizard says once that every Agent App holding the token gets the same Grants and that deleting the Mandate disconnects all of them
- [ ] Tokens are never echoed; the paste is hidden
- [ ] `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass

## Comments
