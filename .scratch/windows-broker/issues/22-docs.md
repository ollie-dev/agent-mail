# 22: Docs for the Windows Broker

**What to build:** the README, release notes, skill and CLAUDE.md describe Windows with the Broker (ADR-0004). After this ticket a `v*` release can be tagged again.

**Blocked by:** 21

**Status:** ready-for-agent

- [ ] The README's "What each OS protects against" section says both platforms protect against a hijacked Agent, how (Keychain plus self-check on macOS, the Broker on Windows), and that administrators are out of scope; the old Windows "honest Agents only" caveat is gone
- [ ] The README's install section covers the elevated `install.ps1`, updates and `uninstall.ps1`; the Mandate section says `mandate create` on Windows needs an elevated terminal; the audit section documents `agent-mail audit` and the Windows log location
- [ ] `.github/release-notes.md` and `plugin/skills/agent-mail/SKILL.md` match, including `broker_unavailable` and `broker_version_mismatch`
- [ ] `CLAUDE.md`'s module, test and release-file lists include the Broker, the pipe protocol, `uninstall.ps1` and the new tests
- [ ] The tests in `tests/release.rs` and `tests/plugin.rs` are updated to match, and the placeholder-only token scan still passes
