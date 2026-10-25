# 23: Security concept documents

**What to build:** a detailed, visual explanation of the security concept for each platform, so a reader can see who holds what, where the trust boundaries are and what is and is not protected against.

**Blocked by:** 22

**Status:** ready-for-agent

- [ ] `docs/security/README.md` compares macOS and Windows side by side and explains what both share: the sealed Mandate, the Agent Token, the Policy, and that administrators are out of scope
- [ ] `docs/security/macos.md` and `docs/security/windows.md` each cover who holds what, the trust boundaries, the path of one Agent request, `mandate create`, and the threats protected against and not protected against
- [ ] Each platform document has Mermaid diagrams: a component diagram with the trust boundaries, and sequence diagrams for one request and for `mandate create`
- [ ] The documents match the code and ADR-0002, ADR-0003 and ADR-0004, and follow the CLAUDE.md text convention
- [ ] The README's "What each OS protects against" section links to them; a test in `tests/release.rs` checks the links and that each platform document contains Mermaid blocks
