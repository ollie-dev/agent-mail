APPROVE
All four round-2 reviews pass, the round-1 symlink blocker is fixed and tested, every criterion has a passing test, and the diff stays in scope.

## Acceptance criteria
- AC1: pass (cc_ac1a_folder_prompt_defaults_to_the_current_folder, cc_ac1b_another_folder_absolute_or_relative, cc_ac1c_missing_folder_or_a_file_is_refused_and_asked_again, cc_ac1d_cancel_at_the_folder_prompt_follows_the_cancel_path)
- AC2: pass (cc_ac2a_not_ignored_falls_back_to_configure_myself, cc_ac2a_fallback_after_a_reveal_points_above, cc_ac2b_tracked_falls_back, cc_ac2c_git_that_cannot_run_falls_back, cc_ac2d_git_file_counts_as_a_repository, cc_ac2e_ignored_and_untracked_is_written, cc_ac2_folder_reached_through_a_symlink_into_a_repository_is_checked)
- AC3: pass (cc_ac3a_risk_leaves_the_repository_untouched, cc_ac3b_write_adds_only_the_settings_file)
- AC4: pass (cc_ac4_outside_git_is_written_without_running_git)
- AC5: pass (cc_ac5a_creates_the_folder_and_file_with_the_token, cc_ac5b_keeps_every_other_key_in_order, cc_ac5c_env_is_appended_last_when_missing, cc_ac5d_written_path_never_shows_the_token)
- AC6: pass (cc_ac6a_add_appends_with_a_comma, cc_ac6b_replace_swaps_the_tokens, cc_ac6c_same_project_twice_lists_the_token_once (includes the empty-value case), cc_ac6e_cancel_at_existing_tokens_leaves_the_file; AC6d via assert_nowhere in the AC6/AC7 tests)
- AC7: pass (cc_ac7a_replace_offers_to_delete_the_old_mandate_default_no, cc_ac7b_yes_deletes_the_old_mandate, cc_ac7c_one_offer_per_old_mandate_none_for_malformed, cc_ac7d_unknown_old_token_gives_no_offer, cc_ac7e_add_and_failed_writes_never_offer_a_delete)
- AC8: pass (cc_ac8a_invalid_json_falls_back, cc_ac8b_json_that_cannot_hold_the_token_falls_back, cc_ac8c_unwritable_file_falls_back)
- AC9: pass (cc_ac9a_final_check_reads_the_file_back_and_lists_both_mandates, cc_ac9b_a_file_that_does_not_open_the_new_mandate_fails_the_check)
- AC10: pass (cc_ac10_one_next_step_per_written_destination_in_order)
- AC11: pass (I ran `cargo test` at 0ff9bd0: every binary ok, tests/setup.rs 61 passed with 0 failed, tests/create.rs 39 passed, tests/release.rs 60 passed; `cargo clippy -- -D warnings` clean; `cargo fmt --check` clean; ac5 option-list test asserts `[CLAUDE_CODE_PROJECT, CONFIGURE_MYSELF]`)

## For the human
Nothing needs a human decision. The change writes the Agent Token into a project's `.claude/settings.local.json`, which the spec asks for. The git-risk check guards it and fails safe. Round 1 security found that a typed path through a symlink skipped the check. Round 2 fixed that by also walking the canonicalized folder, and a test covers it. No dependency changed: `preserve_order` was not turned on, and the existing `raw_value` feature keeps the key order. New files get mode 0600 on Unix and are written with a temp file plus a rename. No `[fix-now]` findings were raised.

## Follow-ups
- security · src/setup.rs (`write_settings`): a kill or crash between creating the temp file `.claude/.settings.local.json.agent-mail-<pid>.tmp` and the rename leaves a file holding the full token, and the user's git excludes may not cover it → clean up stale `agent-mail-*.tmp` files on the next run, or name the temp file so the same ignore rule covers it (and git check-ignore it too).
- security · src/setup.rs (`git_risk`): a repository is found only by a `.git` entry, so a work tree driven by `GIT_DIR`/`GIT_WORK_TREE` or `core.worktree` is missed and git is never asked → when git is available, always run `git rev-parse --is-inside-work-tree` in the folder and treat "true" like a found `.git`.
- ux · src/setup.rs (`write_settings`/`Settings::to_bytes`): an existing settings file is rewritten with 2-space pretty printing while untouched nested values keep their raw text, so a hand-formatted file can end up with mixed indentation → keep the original formatting, or note it in the docs ticket (10).
- ui · src/setup.rs (`GitRisk::warning`, `SettingsProblem::message`, the write-failure message): five messages repeat the tail "so the token was not written there." → build them with one shared helper.
- code · src/setup.rs (`claude_code`): `if replace || old.is_empty()` repeats choice logic that `replace` already encodes → move the value building into a small helper.
