## Round 1
**Changed:**
- F5: `core::MAX_TOKENS = 8`; `open_presented` now collects the distinct non-empty fields (with their positions) first, fails with `MissingToken` if none and the new `CoreError::TooManyTokens` (`too_many_tokens`, no `operation`/`account`) if more than 8, and only then opens tokens. Message: "AGENT_MAIL_TOKEN holds more than 8 distinct tokens; set AGENT_MAIL_TOKEN to at most 8 tokens". Added to the core's `every_error_message_names_a_next_step` and to `tests/cli.rs` `error_samples`. Documented in README (several-tokens section) and SKILL.md (error list).
- F1: rewrote the `recipients` comment in the `Command::Draft` arm of `src/core.rs` (both adapters reject a bad address before the core runs); core behaviour unchanged. Pinned by a new MCP/CLI test.
- F6: `ac5_every_agent_command_warns_and_still_succeeds` checks that each command adds exactly one audit line with `mandate` `aaaa1111`, its operation, and `allowed`.
- F12: `src/lib.rs` split into `parse(args) -> Result<Cli, RunOutput>` and `run_cli(cli, env, context)`; `run` is `parse` then `run_cli`. `src/main.rs` calls `agent_mail::parse` once (audit redirect still set before it), serves MCP on `CliCommand::Mcp`, else `run_cli`.
- F16: removed `impl GuardedStore<KeychainStore> { pub fn keys }` from `src/store.rs`.
- Ticked F1, F5, F6, F12, F16 in `factory/follow-ups.md` with `→ 022-small-code-cleanups`.

**Commits:** 3d7b76a
**Tests:** `cargo test` → every suite `test result: ok` (0 failed); `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | tests/tokens.rs `ac1_at_most_eight_distinct_tokens_are_accepted` | pass |
| AC2 | tests/tokens.rs `ac2_more_than_eight_distinct_tokens_is_too_many_tokens` | pass |
| AC3 | tests/tokens.rs `ac3_the_cap_is_checked_before_any_store_lookup` | pass |
| AC4 | tests/tokens.rs `ac4_whoami_applies_the_same_cap_and_is_not_audited`; tests/mcp.rs `job022_ac4_too_many_tokens_matches_the_cli` | pass |
| AC5 | tests/mcp.rs `job022_ac5_draft_rejects_an_invalid_recipient_without_audit_or_echo` | pass |
| AC6 | tests/tokens.rs `ac5_every_agent_command_warns_and_still_succeeds` | pass |
| AC7 | tests/cli.rs `job022_ac7_run_cli_on_a_parsed_cli_matches_run` | pass |
| AC8 | tests/cli.rs `job022_ac8_main_parses_its_arguments_once` (plus existing binary tests incl. `ac7_the_binary_serves_mcp_on_stdio`) | pass |
| AC9 | tests/project.rs `job022_ac9_the_guarded_keys_method_is_gone` | pass |

**Review findings:** none (round 1).
