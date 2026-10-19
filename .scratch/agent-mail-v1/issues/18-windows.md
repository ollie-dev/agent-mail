# 18: Windows milestone

**What to build:** agent-mail runs on Windows for honest Agents. Mandates are stored in Windows Credential Manager. CI adds a Windows x64 release build with checksum and attestation, and a PowerShell installer installs it. The README states plainly that on Windows any process of the same user can read stored secrets, so the Windows build protects only against honest Agents (ADR-0002).

**Blocked by:** 03, 16

**Status:** ready-for-agent

- [ ] On Windows, `mandate create` stores the Mandate in Credential Manager and `list` with the printed token works (opt-in test, ignored by default)
- [ ] The hardened-runtime self-check is macOS-only and does not block Windows
- [ ] A release tag produces a Windows x64 binary on GitHub Releases with a SHA-256 checksum and a build attestation
- [ ] The PowerShell installer downloads the binary, verifies the checksum, aborts on a mismatch, and installs to the user's local bin directory
- [ ] The README's Windows section states that a hijacked Agent can read Credentials on Windows
- [ ] All tests that do not need a platform store pass on Windows CI
