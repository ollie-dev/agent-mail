VERDICT: PASS

## Findings

None. The docs-only change does not overstate protection. On both platforms administrators are out of scope, and the "Agent Tokens" paragraph (plain-text token readable by same-user processes) stays. The "any program running as you" caveat for the audit log is now limited to macOS, which matches the Windows audit folder ACL. The skill tells the Agent to hand both Broker errors to the human and names neither `sc.exe`, `install.ps1` nor an elevated terminal. No Credential or real token appears anywhere in the diff.

## Checked
- Read: the full diff `git diff main...factory/028-broker-docs` (one commit, 96b2edd): `README.md` (Install, Creating a Mandate, What each OS protects against, Audit log), `.github/release-notes.md`, `plugin/skills/agent-mail/SKILL.md` (Errors and what to do), `CLAUDE.md` (Release files bullet), `tests/release.rs` (job024_ac5a/ac5b updates, new job028_* tests and helpers) and `tests/plugin.rs` (ac6 update, job028_ac3 skill test).
- Read, to check the docs' security claims against the code:
  - `src/broker.rs`: `pipe_dacl_sddl`, `audit_dir_sddl` (Broker full access, BA read only), which backs "only the Broker can write it, Administrators can read it".
  - `src/broker.rs`: the `Request` enum (`mandate list`/`delete` go through `Run` with no elevation check), `handle_create` (refuses below high integrity with `ElevationRequired`, before any login or store access) and `handle_audit` (filters entries on the caller's SID), which back "your own Mandates".
  - `src/broker/windows.rs`: `process_elevated`; the service uses `SerializedStore::new(CredentialManagerStore)`, which backs "that account's own Credential Manager".
  - `docs/adr/0004-windows-broker.md`: administrators out of scope; medium integrity cannot read an elevated console, which backs the README's elevated-window sentence.
  - `uninstall.ps1`: header and the `-KeepData`/`-Force`/RunAs handling.
- Read after the diff: `spec.md` and `build.md`. Its claims about the files changed and tests passing match what I saw.
- Ran: `cargo test --locked --test release --test plugin`. Release: 40 passed. Plugin: 26 passed, including the unchanged placeholder scans `ac8_no_real_credential_or_agent_token_in_plugin_or_readme` and `job024_ac9_token_values_in_the_top_part_are_placeholders`.
- Ran: a grep of the added diff lines for password, secret, `am_`-style token and `token = "..."` patterns. No matches.
