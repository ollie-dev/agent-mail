APPROVE
Docs-only change brings the README, release notes, skill and CLAUDE.md in line with the Windows Broker; every criterion is tested, all four reviews pass and the full suite is green.

## Acceptance criteria
- AC1: pass (`tests/release.rs::job028_ac1_both_platforms_protect_against_a_hijacked_agent`, plus updated `job024_ac5a_readme_windows_paragraph_says_a_hijacked_agent_cannot_extract_credentials` and `tests/plugin.rs::ac6_readme_states_what_each_os_protects_against`)
- AC2: pass (`tests/release.rs::job028_ac2_install_mandate_and_audit_sections_describe_the_broker`, plus updated `job024_ac5b_readme_and_release_notes_give_the_windows_install`)
- AC3: pass (`tests/release.rs::job028_ac3_release_notes_describe_the_broker`, `tests/plugin.rs::job028_ac3_skill_tells_the_agent_to_hand_broker_errors_to_the_human`, unchanged `ac3_skill_tells_the_human_to_install_a_missing_binary`)
- AC4: pass (`tests/release.rs::job028_ac4_claude_md_lists_the_broker_and_uninstall_ps1`)
- AC5: pass (`tests/release.rs::job028_ac5_no_release_or_plugin_test_still_asserts_the_old_windows_wording`; unchanged `ac8_no_real_credential_or_agent_token_in_plugin_or_readme` and `job024_ac9_token_values_in_the_top_part_are_placeholders` pass)

Ran in the worktree at 96b2edd: `cargo test --locked` (every suite ok, 0 failed; only the opt-in tests ignored), `cargo fmt --check` and `cargo clippy --all-targets --locked -- -D warnings`, all clean.

## For the human
Nothing needs your decision. The change touches only README.md, .github/release-notes.md, SKILL.md, CLAUDE.md and the tests that check them. The security claims match ADR-0004 and the code, which the security reviewer checked against the SDDL builders and `handle_create`/`handle_audit`: administrators are out of scope on both platforms, the plain-text Agent Token caveat stays, and the skill does not tell the Agent to run `sc.exe`, `install.ps1` or anything elevated. Inside the Install section the macOS Keychain and hardened-runtime paragraphs moved up next to the macOS instructions. That is a reorder within the spec's scope. This job closes F49 (CLAUDE.md naming `uninstall.ps1`).

## Follow-ups
- code/ui · README.md "Audit log" (key list): the `mailbox` and `messages` bullets were split into per-operation bullets with repeated key names, only to fit the 300-character limit → restructure the key list as a table (together with F26's full list of reason codes).
- decider · README.md "Audit log" (last paragraph) and "What each OS protects against" (last paragraph): the "any process running as you can read or delete the file" caveat is now limited to macOS. The old text covered all of Unix, and the "Other Unix" path is still listed → word the caveat as "On macOS and other Unix" so the Linux path keeps it.
