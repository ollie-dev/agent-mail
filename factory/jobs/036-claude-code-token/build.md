# Build: 036-claude-code-token

## Round 1
**Changed:**
- `src/setup.rs`: added `Destination::ClaudeCode` ("A Claude Code project"), offered first. It covers the folder prompt (`PROJECT_FOLDER`, default `Machine::current_dir()`, relative paths resolved against it, "No such folder: <path>" and ask again) and the git-risk check (a `.git` entry in the folder or any ancestor, then `git check-ignore -q -- .claude/settings.local.json`, then `git ls-files -- ...` only when ignored; any failure counts as a risk). It also covers the settings merge and write, Add/Replace (`EXISTING_TOKENS`, `ADD_TOKEN`, `REPLACE_TOKENS`, default Add, no duplicate), the old-Mandate delete offers after Replace (found via `store_key` + `seal::open`, deleted via `Command::MandateDelete`, default No, text ends with `DELETE_OLD_WARNING`) and the fallback to `Wizard::reveal` / `SHOWN_ABOVE` for git risks, bad JSON, the wrong shape and failed writes. On the written path the token is never revealed.
- Final check: for each written Claude Code destination it reads `env.AGENT_MAIL_TOKEN` back from the file and runs `whoami` with it. The check passes when the new Mandate's ID is among the opened ones. The direct check for "I'll configure it myself" is unchanged. The next steps are one per destination in the order chosen, including "Restart Claude Code in <folder>, then ask it to list your mailboxes." There is a test-only hook `setup::hook::set_before_file_check` / `clear` (thread-local, behind `cfg(any(test, feature = "test-support"))`) for AC9b.
- Key order: I did **not** turn on `serde_json`'s `preserve_order`. Turning it on broke `tests/mandates.rs::delete_does_not_claim_success_while_an_unreadable_record_may_survive`, because agent-visible error objects are emitted with sorted keys and would change order. Instead the settings file is handled as an ordered list of members with `Box<RawValue>` values (the `raw_value` feature is already on), so other keys and nested values are kept byte-for-byte. `Cargo.toml` and `Cargo.lock` are unchanged.
- Write: a temporary file in `.claude/` and then a rename. A new file is created with mode 0600 on Unix, and a replaced file keeps its permissions. A `.claude` folder created for a write that then fails is removed again. Messages name only paths, never file content or old tokens.
- `tests/setup.rs`: `run_on` now delegates to a new `run_with(…, machine)`, so tests can script `git` replies and pre-create files. The existing AC5 option-list test now expects `[CLAUDE_CODE_PROJECT, CONFIGURE_MYSELF]`. I added 27 `cc_ac*` tests.
- `CLAUDE.md`: the `src/setup.rs` and `tests/setup.rs` descriptions now name the Claude Code destination.

**Commits:** 5165385

**Tests:** `cargo test` passes in every test binary, 0 failed (`tests/setup.rs`: `test result: ok. 60 passed; 0 failed`). `cargo clippy --all-targets -- -D warnings` is clean. `cargo fmt --check` is clean.

| AC | Test | Status |
|----|------|--------|
| AC1a | cc_ac1a_folder_prompt_defaults_to_the_current_folder | pass |
| AC1b | cc_ac1b_another_folder_absolute_or_relative | pass |
| AC1c | cc_ac1c_missing_folder_or_a_file_is_refused_and_asked_again | pass |
| AC1d | cc_ac1d_cancel_at_the_folder_prompt_follows_the_cancel_path | pass |
| AC2a | cc_ac2a_not_ignored_falls_back_to_configure_myself, cc_ac2a_fallback_after_a_reveal_points_above | pass |
| AC2b | cc_ac2b_tracked_falls_back | pass |
| AC2c | cc_ac2c_git_that_cannot_run_falls_back (not_started, exit 128, and ls-files failures) | pass |
| AC2d | asserted in every AC2 fallback test (`assert_git_fallback`); plus cc_ac2d_git_file_counts_as_a_repository | pass |
| AC2e | cc_ac2e_ignored_and_untracked_is_written | pass |
| AC3a/c | cc_ac3a_risk_leaves_the_repository_untouched | pass |
| AC3b/c | cc_ac3b_write_adds_only_the_settings_file | pass |
| AC4 | cc_ac4_outside_git_is_written_without_running_git | pass |
| AC5a | cc_ac5a_creates_the_folder_and_file_with_the_token | pass |
| AC5b | cc_ac5b_keeps_every_other_key_in_order | pass |
| AC5c | cc_ac5c_env_is_appended_last_when_missing | pass |
| AC5d | cc_ac5d_written_path_never_shows_the_token | pass |
| AC6a | cc_ac6a_add_appends_with_a_comma | pass |
| AC6b | cc_ac6b_replace_swaps_the_tokens | pass |
| AC6c | cc_ac6c_same_project_twice_lists_the_token_once | pass |
| AC6d | `assert_nowhere(old)` in cc_ac6a/b/e, cc_ac7b/c | pass |
| AC6e | cc_ac6e_cancel_at_existing_tokens_leaves_the_file | pass |
| AC7a | cc_ac7a_replace_offers_to_delete_the_old_mandate_default_no | pass |
| AC7b | cc_ac7b_yes_deletes_the_old_mandate | pass |
| AC7c | cc_ac7c_one_offer_per_old_mandate_none_for_malformed | pass |
| AC7d | cc_ac7d_unknown_old_token_gives_no_offer | pass |
| AC7e | cc_ac7e_add_and_failed_writes_never_offer_a_delete | pass |
| AC8a | cc_ac8a_invalid_json_falls_back | pass |
| AC8b | cc_ac8b_json_that_cannot_hold_the_token_falls_back | pass |
| AC8c | cc_ac8c_unwritable_file_falls_back | pass |
| AC8d | asserted in the AC8 tests (`assert_unusable`, cc_ac8c) | pass |
| AC9a | cc_ac9a_final_check_reads_the_file_back_and_lists_both_mandates | pass |
| AC9b | cc_ac9b_a_file_that_does_not_open_the_new_mandate_fails_the_check | pass |
| AC10 | cc_ac10_one_next_step_per_written_destination_in_order (also cc_ac1b, cc_ac9a) | pass |
| AC11 | full `cargo test`, clippy, fmt; ac5_configure_myself_reveals_the_token_and_both_snippets updated to two options | pass |

**Review findings:** none (round 1).

**Notes for reviewers:**
- The settings file is rewritten pretty-printed at the top level. Nested values that were already in the file keep their original text, and the `env` object is written compactly.
- When "Add" finds that the new token is already listed (AC6c), the value is rewritten from the trimmed, non-empty fields.
- Cancelling an old-Mandate delete offer counts as No.
- The old-Mandate offer is skipped if the old Mandate's ID equals the new one's, because deletion is by ID.

## Round 2
**Changed:**
- `src/setup.rs` `git_risk`: the `.git` ancestor walk now runs on the folder as typed and on `std::fs::canonicalize(folder)`. A folder that cannot be canonicalized counts as a risk (`GitFailed`). Git still runs in the folder as typed.
- `tests/setup.rs`: added the Unix-only test `cc_ac2_folder_reached_through_a_symlink_into_a_repository_is_checked`. It sets up `<root>/repo/.git/` and `<root>/repo/sub/` with `<root>/link -> <root>/repo/sub`, types `<root>/link`, and scripts check-ignore to exit 1. It asserts one "not ignored by git" warning, the recorded check-ignore in `<root>/link`, no `.claude` under `repo/sub`, the token revealed once, and `NEXT_CONFIGURE_MYSELF` as the next step.

**Commits:** 0ff9bd0

**Tests:** `cargo test` passes in every test binary, 0 failed (`tests/setup.rs`: `test result: ok. 61 passed; 0 failed`). `cargo clippy --all-targets -- -D warnings` is clean. `cargo fmt --check` is clean.

| AC | Test | Status |
|----|------|--------|
| AC2 (symlinked folder) | cc_ac2_folder_reached_through_a_symlink_into_a_repository_is_checked | pass |
| AC1–AC11 | all Round 1 tests, unchanged | pass |

**Review findings:**
- [blocking] security, `git_risk` used a lexical-only ancestor walk → fixed. The walk now also covers the canonicalized folder, and a canonicalize failure counts as a risk. The symlink test above covers it.
- No `[fix-now]` findings were raised. The `[follow-up]` findings (stale temp file, `GIT_DIR`/`core.worktree` detection, mixed indentation, shared message tail, value-building helper) are left for the factory.
