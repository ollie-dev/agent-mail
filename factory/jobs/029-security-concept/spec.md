# Security concept documents for macOS and Windows

## Goal
A reader can open `docs/security/` and see, for each platform, who holds what, where the trust boundaries are, how one Agent request and `mandate create` flow, and what is and is not protected against. The README's "What each OS protects against" section links to these documents, and a test keeps the links and the diagrams in place.

## Context
- Stack: Rust, one crate `agent-mail` (ADR-0001). Test command: `cargo test` (the new checks run with `cargo test --test release`). Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Ticket: `.scratch/windows-broker/issues/23-security-concept.md` (blocked by 22, which is done). There is no `.scratch/windows-broker/spec.md`; the source of truth is ADR-0002, ADR-0003, ADR-0004 and `GLOSSARY.md`.
- New files: `docs/security/README.md`, `docs/security/macos.md`, `docs/security/windows.md`.
- Changed files: `README.md` (section `## What each OS protects against`, lines about 190-202), `tests/release.rs` (new `job029_*` tests; reuse `read`, `readme_section` and `prose_paragraphs`).
- Code the documents describe (read, do not change): `src/seal.rs` (Agent Token prefix `amt_`, HKDF-SHA256 store key and seal key, ChaCha20-Poly1305 sealed record with the listing as associated data), `src/store.rs` (Keychain store, `CredentialManagerStore`, `GuardedStore`), `src/selfcheck.rs` (hardened-runtime self-check, macOS release builds only), `src/core.rs` (opening tokens, Mandate selection, deny-by-default Policy), `src/create.rs`, `src/audit.rs` (per-user log on macOS, `%ProgramData%\agent-mail\audit.log` with `sid` on Windows), `src/broker.rs` (pipe protocol, 1 MiB frame cap, version handshake, `Caller`, `pipe_sddl`, `audit_dir_sddl`), `src/broker/windows.rs` (impersonation to read the SID, revert before the store, elevation check for `create`), `src/client.rs` (`PipeConnector` server-identity check, `broker_unavailable`, `broker_version_mismatch`), `install.ps1` / `uninstall.ps1`.

## Acceptance criteria
1. AC1: `docs/security/README.md` compares macOS and Windows side by side and explains what both share: the sealed Mandate, the Agent Token, the Policy, and that administrators are out of scope.
   Test: the file exists; it contains a Markdown table (a line starting with `|` and a separator line containing `---`) whose header row names both `macOS` and `Windows`; its text contains `Mandate`, `Agent Token`, `Policy`, `administrator` and `out of scope`; it links to `macos.md` and `windows.md`.
2. AC2: `docs/security/macos.md` and `docs/security/windows.md` each cover who holds what, the trust boundaries, the path of one Agent request, `mandate create`, and the threats protected against and not protected against.
   Test: each file has these six `##` headings, exactly as written: `## Who holds what`, `## Trust boundaries`, `## One Agent request`, ``## `mandate create` ``, `## Protected against`, `## Not protected against`. `macos.md` mentions `Keychain`, `hardened runtime` and `self-check`; `windows.md` mentions `Broker`, `NT SERVICE\agent-mail`, `\\.\pipe\agent-mail`, `SID`, `elevated` (or `elevation`), `broker_unavailable` and `broker_version_mismatch`. Each `## Not protected against` section mentions `administrator`.
3. AC3: Each platform document has Mermaid diagrams: a component diagram with the trust boundaries, and sequence diagrams for one request and for `mandate create`.
   Test: each of `macos.md` and `windows.md` has at least three fenced blocks opened with ```` ```mermaid ````; at least one starts with `flowchart` or `graph` and contains `subgraph` (the trust boundaries); at least two start with `sequenceDiagram`, one inside `## One Agent request` and one inside ``## `mandate create` ``.
4. AC4: The documents match the code and ADR-0002, ADR-0003 and ADR-0004, and follow the CLAUDE.md text convention.
   Test (the automatable part): in all three documents, every prose paragraph and list item outside code blocks (as split by `prose_paragraphs`) is at most 300 characters; `macos.md` links to `0002-no-background-service.md` and `0003-immutable-mandates.md`, `windows.md` links to `0004-windows-broker.md` and `0003-immutable-mandates.md`; none of the three uses the glossary's avoided words `vault`, `daemon` or `API key`; no document contains an `amt_` string other than a placeholder (`amt_` followed only by `.`, `<`, `x` or `X` characters, e.g. `amt_...`), and none contains the phrase `honest Agents`. Matching the code and ADRs beyond this is checked by review.
5. AC5: The README's "What each OS protects against" section links to them; a test in `tests/release.rs` checks the links and that each platform document contains Mermaid blocks.
   Test: `readme_section("## What each OS protects against")` contains the links `docs/security/README.md`, `docs/security/macos.md` and `docs/security/windows.md`; the existing `job028_ac1_*` test and the README paragraph-length and placeholder-only token scans in `tests/plugin.rs` still pass; the AC3 checks live in `tests/release.rs`.

## Out of scope
- Any change to Rust source, the installers, workflows, the plugin or the skill.
- New threat mitigations; the documents describe what exists today.
- Rendering or validating Mermaid syntax beyond the checks above (no Mermaid toolchain in CI).
- Listing the new documents in `CLAUDE.md` or the release notes, unless an existing test requires it.
- Linux and any platform other than macOS and Windows.

## Notes for reviewers
- Security: documentation only, but it states security claims, so check each against the code and ADRs: the Agent Token is the only authorization (the client executable is not checked); the sealed record binds Credential, Policy and token hash (ADR-0003); on macOS the Keychain access list plus the self-check (release builds only) keep Credentials from a hijacked Agent; on Windows no Credential, sealed or not, leaves the Broker, the Broker serves a Mandate only to its owner's SID, reverts impersonation before the store, refuses `create` below high integrity, and the client checks the pipe owner and server identity; deletion by an Agent is accepted; token files in host configs are plain text readable by the same user; the macOS audit log is writable by the user, the Windows log only by the Broker; administrators are out of scope. No real token or Credential may appear in examples or diagrams. AC4's "match the code and ADRs" is only partly automatable; the rest relies on the security review.
- UX: readers are operators (humans deciding whether to trust agent-mail and setting up Mandates) and security reviewers. The flow: README "What each OS protects against" links to `docs/security/README.md`, which compares the platforms and links to each platform document. Use the glossary terms (Account, Credential, Agent, Mandate, Agent Token, Policy, Grant, Broker) throughout.
- UI: rendered Markdown on GitHub, including Mermaid diagrams; no terminal output or application UI changes.
