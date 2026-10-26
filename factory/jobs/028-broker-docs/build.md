## Round 1
**Changed:**
- `README.md`: "Install" covers the elevated `install.ps1` (UAC, `%ProgramFiles%\agent-mail`, the Broker `agent-mail` as `NT SERVICE\agent-mail`, machine Path), updating by rerunning it (Mandates kept), the attestation and `uninstall.ps1` with `-KeepData`/`-Force`; long paragraphs split to at most 300 characters.
- `README.md`: "Creating a Mandate" names the Keychain on macOS and the Broker on Windows, and adds the elevated-terminal paragraph with `elevation_required` (`mandate list`/`delete` need no elevation).
- `README.md`: "What each OS protects against" has both platforms protecting against a hijacked Agent (Keychain plus self-check; the Broker), a new "Administrators" paragraph (out of scope on both), and the audit-log sentence now limited to macOS. The "honest Agents" caveat is gone.
- `README.md`: "Audit log" documents `agent-mail audit` (`--text`), `%ProgramData%\agent-mail\audit.log` written by the Broker (only the Broker writes it; besides the Broker, only Administrators can read it) and the `sid` key. The per-key list items are split per operation so that each is at most 300 characters.
- `.github/release-notes.md`: the Windows install, update and uninstall, plus a "Windows" section (Broker, hijacked Agent, elevated `mandate create`, administrators out of scope, `broker_unavailable` and `broker_version_mismatch` with what the human does).
- `plugin/skills/agent-mail/SKILL.md`: items for `broker_unavailable` and `broker_version_mismatch` telling the Agent to tell the human and not to fix it itself (no `sc.exe`, no `install.ps1`).
- `CLAUDE.md`: the Release files bullet names `uninstall.ps1` as a release asset next to `install.ps1`. The module and test lists already named the Broker files and tests.
- Tests: `job024_ac5a`, `job024_ac5b` (release.rs) and `ac6_readme_states_what_each_os_protects_against` (plugin.rs) updated to the new wording; new `job028_*` tests added.

**Commits:** 96b2edd
**Tests:** `cargo test`: all green (release 40 passed, plugin 26 passed, every other suite ok). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | `tests/release.rs::job028_ac1_both_platforms_protect_against_a_hijacked_agent` | pass |
| AC2 | `tests/release.rs::job028_ac2_install_mandate_and_audit_sections_describe_the_broker` | pass |
| AC3 | `tests/release.rs::job028_ac3_release_notes_describe_the_broker`, `tests/plugin.rs::job028_ac3_skill_tells_the_agent_to_hand_broker_errors_to_the_human` (+ unchanged `ac3_skill_tells_the_human_to_install_a_missing_binary`) | pass |
| AC4 | `tests/release.rs::job028_ac4_claude_md_lists_the_broker_and_uninstall_ps1` | pass |
| AC5 | `tests/release.rs::job028_ac5_no_release_or_plugin_test_still_asserts_the_old_windows_wording`, plus `cargo test --test release --test plugin` (including the unchanged `ac8_no_real_credential_or_agent_token_in_plugin_or_readme` and `job024_ac9_token_values_in_the_top_part_are_placeholders`) | pass |

**Review findings:** none (first round).

