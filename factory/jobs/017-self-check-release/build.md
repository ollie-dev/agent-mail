## Round 1
**Changed:**
- `src/selfcheck.rs` (new): `hardened_runtime_ok(Option<u32>)`, `process_status()` via a hand-declared `csops(getpid(), CS_OPS_STATUS, &mut u32, 4)` (non-zero return gives `None`), and `check()` cached in a `OnceLock`. Active only under `cfg(all(target_os = "macos", not(debug_assertions)))`; no env var, flag, file or feature is consulted. `Cargo.toml` pins `[profile.release] debug-assertions = false`.
- `src/store.rs`: `StoreError` is now an enum (`Unavailable`, `HardenedRuntimeMissing`). New `GuardedStore<S>` runs the check before `get`/`put`/`delete`/`entries` and never forwards on failure; `with_status` (test-support only), `inner()` (test-support only), `check()`, and a guarded `keys()` for `GuardedStore<KeychainStore>`. `MandateStore` gained a default `check()` (passes) that `GuardedStore` overrides, so the core can ask through `&dyn MandateStore`.
- `src/core.rs`: `CoreError::HardenedRuntimeMissing` (`hardened_runtime_missing`, exit 1), `From<StoreError> for CoreError`, all store mappings go through it; message: "this agent-mail binary is not signed with the hardened runtime, so it refused to access the Keychain and the Keychain was not accessed; ask the human to reinstall agent-mail from https://github.com/ollie-dev/agent-mail/releases". Audit `reason` follows automatically from the code.
- `src/create.rs`: `context.store.check()?` after the interactive/sandbox refusals and before the `--from` file load and the first prompt; a failed `put` now keeps its error kind. `src/main.rs` wraps `RuntimeStore` in `GuardedStore::new`.
- Release files: `.github/workflows/release.yml` (tag/version check, `cargo test`, both targets, `scripts/sign-macos.sh`, tarball + `.sha256`, `actions/attest-build-provenance`, `gh release create --notes-file`, `verify` job; actions pinned to commit SHAs; top-level `contents: read`, publish job only `contents/id-token/attestations: write`), `.github/release-notes.md`, `scripts/sign-macos.sh` (parses the `CodeDirectory ... flags=0x...(...)` list exactly for `adhoc` and `runtime`), `scripts/verify-release.sh`, `install.sh` (also rejects a `.sha256` file that does not name exactly the expected asset, extracts only the `agent-mail` member, refuses symlinks, stages next to the destination for an atomic `mv`).
- Docs: README "Install" section (one-liner, `~/.local/bin`, checksum, attestation verification and what the checksum does not cover, Keychain note, `hardened_runtime_missing`), "Release-binary tests" and "Releasing" paragraphs under Development, `hardened_runtime_missing`/`store_unavailable` in the audit `reason` list. CLAUDE.md layout updated.

**Commits:** 38f6059
**Tests:** `cargo test` → 344 passed, 0 failed, 6 ignored (23 test binaries, every `test result: ok`). `cargo test --test selfcheck -- --ignored` → `test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out` (run on macOS arm64). `cargo clippy -- -D warnings` clean (also with `--all-targets`), `cargo fmt --check` clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | `tests/selfcheck.rs` `release::ac1_hardened_release_binary_reaches_the_keychain` (ignored, opt-in) | pass (with `--ignored`) |
| AC2 | `tests/selfcheck.rs` `release::ac2_unhardened_release_binary_refuses_every_keychain_access` (ignored, opt-in) | pass (with `--ignored`) |
| AC3 | `tests/release.rs` `ac3_workflow_triggers_on_version_tags`, `ac3_workflow_builds_signs_attests_and_verifies`, `ac3_workflow_checks_the_tag_against_cargo_toml`, `ac3_scripts_are_executable_and_parse`; CI `verify` job on first tag push | pass (local); CI part pending a real tag |
| AC4 | `tests/selfcheck.rs` `release::ac4_sign_script_applies_adhoc_runtime_and_check_only_verifies` (ignored, opt-in); CI via `scripts/verify-release.sh` | pass (with `--ignored`) |
| AC5 | `tests/release.rs` `installer::ac5_installs_the_host_binary_with_mode_755`, `ac5_checksum_mismatch_aborts_without_installing`, `ac5_missing_asset_installs_nothing`, `ac5_malformed_checksum_file_aborts_without_installing`, `ac5_replaces_an_existing_install`, `ac5_no_sudo_and_temporary_directory_removed` | pass |
| AC6 | `tests/selfcheck.rs` `release::ac6_release_binary_needs_no_rust_toolchain` (ignored, opt-in) | pass (with `--ignored`) |
| AC7 | `tests/release.rs` `ac7_release_notes_hold_the_one_liner_and_the_keychain_note`, `ac7_readme_install_section_has_the_keychain_note` | pass |
| AC8 | `src/selfcheck.rs` unit tests; `tests/selfcheck.rs` `ac8_hardened_runtime_ok_decides_on_the_runtime_bit`, `ac8_failed_guard_never_forwards_a_call`, `ac8_whoami_and_list_are_refused_without_connecting_or_changing_the_store`, `ac8_mandate_list_and_delete_are_refused_and_w_stays_stored`, `ac8_mandate_create_is_refused_before_the_first_prompt`, `ac8_mcp_whoami_is_a_tool_error_with_the_same_code`, `ac8_refused_list_is_audited_like_store_unavailable`, `ac8_passing_guard_matches_the_unwrapped_store`, `ac8_store_free_commands_work_on_an_unhardened_build` | pass |
| AC9 | `tests/selfcheck.rs` `ac9_check_passes_in_debug_builds`, `ac9_debug_binary_reaches_the_store`; `src/selfcheck.rs` `debug_builds_pass_whatever_the_status` | pass |
| AC10 | `tests/release.rs` `ac10_readme_install_section_and_releasing_paragraph`, `ac10_claude_md_lists_the_new_files`, `release_profile_pins_debug_assertions_off`; full suite + clippy + fmt green | pass |

**Deviation from the spec (for reviewers to check):** AC1 and AC9 expect `whoami` with an unknown token to exit 1 with `invalid_token`. The existing `whoami` contract (job 014, `tests/tokens.rs` `ac6_whoami_lists_ambiguous_mandates_and_handles_bad_tokens`) answers exit 0 with `{"mandates":[],"invalid_tokens":[1]}` when every token is unknown, and changing that is out of scope. The tests keep the spec's intent (proof that the Keychain lookup ran, never `hardened_runtime_missing`): AC1 asserts exit 0 with exactly that stdout and no `hardened_runtime_missing`; AC9 asserts the same for `whoami` and also runs `list` on the debug binary, which does exit 1 with `invalid_token` (its audit line goes to a temporary file via `AGENT_MAIL_TEST_AUDIT_LOG`).

**Notes:** The opt-in AC2 test runs `list` on the unhardened release binary, which has no audit redirect, so it appends one `denied` line with `reason` `hardened_runtime_missing` to the developer's real audit log (no token in it). This is documented in the README and in the test. AC3/AC4's CI parts run only on the first real `v*` tag push (out of scope here).

**Review findings:** none (round 1).

## Round 2
**Changed:**
- `.github/workflows/release.yml`: new `test` job (permissions `contents: read` only) runs the tag-versus-`Cargo.toml` check and `cargo test --locked`. `publish` now has `needs: test`, no longer runs the tests, and keeps the same three write scopes for build, sign, attest and publish. `verify` is unchanged (`needs: publish`). This follows the human note of 2026-10-23 and resolves the security reviewer's round-1 `[follow-up]` on the OIDC token being exposed to test code.
- `tests/release.rs`: new `ac3_tests_run_in_a_read_only_job_that_publish_depends_on`. It parses the job blocks and asserts that `test` runs `cargo test --locked` with exactly `contents: read` and no write scope, and that `publish` has `needs: test`, runs no `cargo test` and keeps `id-token: write`.
- `README.md` "Releasing" paragraph now describes the read-only `test` job, the `publish` job that depends on it, and the final `verify` job.

**Commits:** 256130f
**Tests:** `cargo test` → 345 passed, 0 failed, 6 ignored (every `test result: ok`). `cargo clippy -- -D warnings` clean, `cargo fmt --check` clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | `tests/selfcheck.rs` `release::ac1_hardened_release_binary_reaches_the_keychain` (ignored, opt-in) | pass (with `--ignored`, round 1; untouched) |
| AC2 | `tests/selfcheck.rs` `release::ac2_unhardened_release_binary_refuses_every_keychain_access` (ignored, opt-in) | pass (with `--ignored`, round 1; untouched) |
| AC3 | `tests/release.rs` `ac3_workflow_triggers_on_version_tags`, `ac3_workflow_builds_signs_attests_and_verifies`, `ac3_workflow_checks_the_tag_against_cargo_toml`, `ac3_tests_run_in_a_read_only_job_that_publish_depends_on`, `ac3_scripts_are_executable_and_parse`; CI `verify` job on first tag push | pass (local); CI part pending a real tag |
| AC4 | `tests/selfcheck.rs` `release::ac4_sign_script_applies_adhoc_runtime_and_check_only_verifies` (ignored, opt-in) | pass (with `--ignored`, round 1; untouched) |
| AC5 | `tests/release.rs` `installer::ac5_*` (6 tests) | pass |
| AC6 | `tests/selfcheck.rs` `release::ac6_release_binary_needs_no_rust_toolchain` (ignored, opt-in) | pass (with `--ignored`, round 1; untouched) |
| AC7 | `tests/release.rs` `ac7_release_notes_hold_the_one_liner_and_the_keychain_note`, `ac7_readme_install_section_has_the_keychain_note` | pass |
| AC8 | `src/selfcheck.rs` unit tests; `tests/selfcheck.rs` `ac8_*` (9 tests) | pass |
| AC9 | `tests/selfcheck.rs` `ac9_check_passes_in_debug_builds`, `ac9_debug_binary_reaches_the_store`; `src/selfcheck.rs` `debug_builds_pass_whatever_the_status` | pass |
| AC10 | `tests/release.rs` `ac10_readme_install_section_and_releasing_paragraph`, `ac10_claude_md_lists_the_new_files`, `release_profile_pins_debug_assertions_off`; full suite + clippy + fmt green | pass |

**Review findings:** Round 1 had no `[blocking]` findings. Human note (2026-10-23), "move the test run into its own job with contents: read only and make publish depend on it": done as described above. This was the security review's `[follow-up]` at `.github/workflows/release.yml:67`.
