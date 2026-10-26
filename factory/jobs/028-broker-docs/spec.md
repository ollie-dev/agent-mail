# Docs for the Windows Broker

## Goal
The README, the release notes, the Agent-facing skill and `CLAUDE.md` describe Windows as it now works with the Broker (ADR-0004): both platforms protect against a hijacked Agent, Windows installs elevated into `%ProgramFiles%\agent-mail`, and `uninstall.ps1` removes it. After this ships an operator reading the README or a release page gets correct Windows instructions, and a `v*` release can be tagged again.

## Context
- Stack: Rust, one crate; the docs are checked by Rust integration tests. Test command: `cargo test` (lint: `cargo clippy -- -D warnings`, `cargo fmt --check`).
- Ticket: `.scratch/windows-broker/issues/22-docs.md`. Decision record: `docs/adr/0004-windows-broker.md` (supersedes ADR-0002 for Windows). Terms from `GLOSSARY.md`: Account, Credential, Agent, Mandate, Agent Token, Policy, Grant, Broker.
- Files this job changes:
  - `README.md`: sections "Install", "Creating a Mandate", "What each OS protects against" and "Audit log".
  - `.github/release-notes.md`.
  - `plugin/skills/agent-mail/SKILL.md`: the "Errors and what to do" list.
  - `CLAUDE.md`: the module, test and release-file lists.
  - `tests/release.rs` and `tests/plugin.rs`: tests that pin the old Windows wording (`job024_ac5a_...`, `job024_ac5b_...` and `ac6_readme_states_what_each_os_protects_against` assert "honest Agents", `%USERPROFILE%\.local\bin\agent-mail.exe` and `%LOCALAPPDATA%\agent-mail\audit.log`) are updated, and new tests are added for this job.
- Facts the docs must match (from the code and installers):
  - `install.ps1`: asks for elevation (UAC) once, installs `%ProgramFiles%\agent-mail\agent-mail.exe`, registers and starts the Broker, the service `agent-mail` under `NT SERVICE\agent-mail`, with delayed automatic start, and adds the folder to the machine Path. Rerunning it updates in place and keeps the stored Mandates.
  - `uninstall.ps1` is a release asset, run as `powershell -ExecutionPolicy Bypass -File uninstall.ps1 [-KeepData] [-Force]`. It removes the Broker, the binary and the Path entry, and deletes the stored Mandates and the audit log after a confirmation. `-KeepData` keeps them; `-Force` deletes them without asking.
  - Error codes and their messages (`src/core.rs`): `broker_unavailable` (the service could not be reached; start it with `sc.exe start agent-mail` in an elevated terminal, or rerun `install.ps1`), `broker_version_mismatch` (the binary and the service are different versions; rerun `install.ps1`), and `elevation_required` (`mandate create` on Windows needs an elevated terminal).
  - The Broker's audit log is `%ProgramData%\agent-mail\audit.log`. Only the Broker can write it, Administrators can read it, and `agent-mail audit` shows a user the entries of their own Mandates (`--text` gives one line per entry).

## Acceptance criteria
1. AC1: The README's "What each OS protects against" section says both platforms protect against a hijacked Agent, how (Keychain plus self-check on macOS, the Broker on Windows), and that administrators are out of scope; the old Windows "honest Agents only" caveat is gone.
   Test: in `readme_section("## What each OS protects against")`:
   - the `**macOS.**` paragraph contains "hijacked Agent", "Keychain" and "self-check" (or "hardened runtime");
   - the `**Windows.**` paragraph contains "hijacked Agent", "Broker" and `NT SERVICE\agent-mail`, and does not contain "Credential Manager, where any program running as you" or "can read (extract)";
   - the section contains "administrator" and "out of scope";
   - `README.md`, `.github/release-notes.md` and `plugin/skills/agent-mail/SKILL.md` contain no "honest Agents";
   - the sentence "The audit log can also be read or changed by any program running as you" is no longer unqualified: any paragraph in the section with "audit log" and "any program running as you" also names "macOS".
2. AC2: The README's install section covers the elevated `install.ps1`, updates and `uninstall.ps1`; the Mandate section says `mandate create` on Windows needs an elevated terminal; the audit section documents `agent-mail audit` and the Windows log location.
   Test:
   - `## Install` still contains the Windows one-liner `irm https://github.com/ollie-dev/agent-mail/releases/latest/download/install.ps1 | iex`, plus `%ProgramFiles%\agent-mail`, "UAC" or "administrator", "Broker", "Path", "rerun" (any case) together with "Mandates", `uninstall.ps1` and `-KeepData`.
   - `## Install` no longer contains `%USERPROFILE%\.local\bin\agent-mail.exe` or "never asks for administrator rights".
   - `## Creating a Mandate` has a paragraph containing "Windows", "elevated" and `elevation_required`, and the section no longer says the Mandate is stored "in Windows Credential Manager on Windows" without naming the Broker.
   - `## Audit log` contains `agent-mail audit` and `%ProgramData%\agent-mail\audit.log`, says the Broker writes it, and no longer contains `%LOCALAPPDATA%\agent-mail\audit.log`.
   - Every prose paragraph in these four sections is at most 300 characters long, as in `job026_ac6_...` (list items count as their own paragraph, code blocks are skipped).
3. AC3: `.github/release-notes.md` and `plugin/skills/agent-mail/SKILL.md` match, including `broker_unavailable` and `broker_version_mismatch`.
   Test:
   - The release notes contain the Windows one-liner, `%ProgramFiles%\agent-mail`, `uninstall.ps1`, "Broker", "hijacked Agent", `broker_unavailable` and `broker_version_mismatch`, still hold `ONE_LINER` and `KEYCHAIN_NOTE`, and no longer contain `%USERPROFILE%\.local\bin\agent-mail.exe`.
   - The skill's "## Errors and what to do" section has an item for `broker_unavailable` and one for `broker_version_mismatch`, each telling the Agent to tell the human (the item contains "human") and not to fix it itself. Neither item contains `sc.exe` or tells the Agent to run `install.ps1`; the human runs those.
   - The skill still names `install.sh`'s one-liner exactly once (the existing `ac3_skill_tells_the_human_to_install_a_missing_binary` passes unchanged).
4. AC4: `CLAUDE.md`'s module, test and release-file lists include the Broker, the pipe protocol, `uninstall.ps1` and the new tests.
   Test: `CLAUDE.md` contains `src/broker.rs`, `src/broker/windows.rs`, `src/client.rs`, "length-prefixed", `tests/broker.rs`, `tests/client.rs` and `tests/installers.rs`, and its "Release files" bullet contains `uninstall.ps1` (as a release asset next to `install.ps1`).
5. AC5: The tests in `tests/release.rs` and `tests/plugin.rs` are updated to match, and the placeholder-only token scan still passes.
   Test: `cargo test --test release --test plugin` passes, including `ac8_no_real_credential_or_agent_token_in_plugin_or_readme` and `job024_ac9_token_values_in_the_top_part_are_placeholders` unchanged; no test in either file still asserts "honest Agents", `%USERPROFILE%\.local\bin\agent-mail.exe` or `%LOCALAPPDATA%\agent-mail\audit.log` as present; the whole `cargo test` run stays green.

## Out of scope
- The security concept documents under `docs/security/` and the README links to them (ticket 23).
- Any change to code, installers or workflows; this job changes documentation and the tests that check it.
- The README header's platform badge (`platform-macOS`) and the "Why Agent Mail" bullet that names only the macOS Keychain; they may stay as they are unless a reviewer asks (the existing header tests pin the badge).
- A Windows install line in the skill's "If agent-mail is missing" section; the existing test requires the `install.sh` one-liner to appear exactly once and is left unchanged.
- The "Windows named pipe" paragraph in "Sandbox and proxies" (job 026), which stays as is.

## Notes for reviewers
- Security: no code path changes. The docs must not overstate protection: administrators stay out of scope on both platforms (ADR-0004), and the Agent Token in a host config file is still plain text readable by any same-user process, so the "Agent Tokens" paragraph stays. The skill must not tell an Agent to run `sc.exe`, `install.ps1` or an elevated terminal. No real Credential or Agent Token may appear; the placeholder scan over the README and `plugin/` must keep passing.
- UX: two readers. The operator (human) reads the README and release notes to install, update, uninstall, create Mandates in an elevated terminal on Windows and read the audit log with `agent-mail audit`. The Agent reads `SKILL.md` and, on `broker_unavailable` or `broker_version_mismatch`, stops and tells the human. Follow the CLAUDE.md text convention: each thing said once, paragraphs of at most about 300 characters.
- UI: no UI surface beyond rendered Markdown (README on GitHub, the release page body, the skill shown to the Agent).
- Ticket wording: AC3's "match" is read as both files describing the same Windows behaviour and both naming the two Broker error codes; the release notes are human-facing, so they say what to do (rerun `install.ps1`, start the service), while the skill tells the Agent to tell the human.
