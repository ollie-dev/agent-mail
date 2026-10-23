## Round 1
**Changed:**
- `src/imap.rs`: new `ImapError::AppendRefused` for a tagged `NO`/`BAD` to `APPEND`, before the continuation or after the literal; read errors, drops and missing responses stay `Protocol`.
- `src/core.rs`: new `CoreError::DraftRefused { account, mailbox }` (code `draft_refused`, body with `operation`, `account`, `mailbox`; message says no draft was created, retry later or ask the human, never quotes the server). `draft` maps `AppendRefused` to it with the Drafts mailbox; `connector_error` maps a stray `AppendRefused` to `server_error`. `src/create.rs` lists the new variant in its login-check reasons.
- `src/lib.rs`: `run` renders every clap parse error except help/version (incl. the help shown for a missing subcommand) as `{"error":{"code":"invalid_arguments","message":...}}`, exit 2. The message is built from clap's error context (argument names, validator reason from `InvalidValue`, unknown option cut at `=`, unexpected positionals not echoed, suggested subcommand only) and ends with ``run `agent-mail <path> --help` for usage``, the path found by walking the args against `Cli::command()`. The in-process `mcp` refusal is the same JSON shape, or `error: invalid_arguments: ...` with `--text`.
- `src/mcp.rs`: with no token and invalid arguments, `missing_token`'s message is `no agent token presented; set AGENT_MAIL_TOKEN. The arguments are also wrong: <argument message>`; new `tool_error` helper.
- `tests/support/mod.rs`: `FakeImap::refuse_append(before_literal, reply)`.
- Docs: README new `## Usage errors` section, `draft_refused` paragraph and errors-list entry in `## Draft`, `draft_refused` in the audit `reason` bullet (example list trimmed to stay under 300 characters), MCP bullets split and corrected (`invalid_arguments` is no longer MCP-only; `missing_token` names argument mistakes). SKILL.md: usage-error sentence and a `draft_refused` entry.
- Existing tests updated for the new usage-error text: `tests/audit.rs` (unknown subcommand), `tests/cli.rs` (`--store`), `tests/create.rs` (credential flags); `tests/cli.rs` error samples include `DraftRefused`.

**Commits:** b3cc51f
**Tests:** `cargo test` → all suites `test result: ok` (0 failed); `cargo clippy -- -D warnings` and `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | tests/draft.rs `job020_ac1_trycreate_on_the_drafts_mailbox_fails_with_draft_refused` | pass |
| AC2 | tests/draft.rs `job020_ac2_other_append_refusals_fail_with_draft_refused` | pass |
| AC3 | tests/draft.rs `job020_ac3_a_refused_draft_is_audited_once_without_secrets` | pass |
| AC4 | tests/mcp.rs `job020_ac4_draft_refused_matches_the_cli` | pass |
| AC5 | tests/cli.rs `job020_ac5_an_invalid_draft_recipient_is_a_json_usage_error` | pass |
| AC6 | tests/cli.rs `job020_ac6_every_command_reports_usage_errors_as_json` | pass |
| AC7 | tests/cli.rs `job020_ac7_usage_errors_never_echo_the_callers_values` | pass |
| AC8 | tests/cli.rs `job020_ac8_help_and_version_keep_clap_plain_text` | pass |
| AC9 | tests/cli.rs `job020_ac9_a_usage_error_reads_no_store_connects_nowhere_and_audits_nothing` | pass |
| AC10 | tests/cli.rs `job020_ac10_in_process_mcp_is_refused_as_json_or_text` | pass |
| AC11 | tests/mcp.rs `job020_ac11_missing_token_also_reports_argument_mistakes` | pass |
| AC12 | tests/mcp.rs `job020_ac12_missing_token_alone_keeps_its_message` | pass |
| AC13 | tests/mcp.rs `job020_ac13_token_like_arguments_are_named_never_echoed` | pass |
| AC14 | tests/plugin.rs `job020_ac14_readme_and_skill_document_draft_refused_and_json_usage_errors` | pass |
| AC15 | `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check` | pass |

**Review findings:** none (round 1).

## Nit pass 1
**Commits:** d6c7762
**Tests:** `cargo test` → all 24 suites `test result: ok` (0 failed); `cargo clippy -- -D warnings` → `Finished`, no warnings; `cargo fmt --check` → clean.
**Findings:**
- review-ui.md and review-ux.md [fix-now] (the same problem): the unknown-command message had a stray `?;` joint ("unknown command; did you mean `list`?; run ..."). Fixed in `src/lib.rs` `usage_message`, `InvalidSubcommand` arm: the suggestion now sits in parentheses inside the problem, "unknown command (did you mean `list`?); run `agent-mail --help` for usage". I checked the other places a `;` could repeat before the pointer: no `InvalidValue` validator reason (src/lib.rs, src/search.rs, src/read.rs) contains `;`, and no other arm adds one. Tests: `tests/cli.rs` `job020_ac6_every_command_reports_usage_errors_as_json` now asserts that every case's message has exactly one `;`, and it checks the exact messages for `lsit` and `mandate delet`, with no `?;`. Done-when checks: `target/debug/agent-mail lsit` → `unknown command (did you mean \`list\`?); run \`agent-mail --help\` for usage`, and `agent-mail mandate delet` → `unknown command (did you mean \`delete\`?); run \`agent-mail mandate --help\` for usage`. Each has exactly one `;`, no `?;`, and ends with the pointer. The tests in tests/cli.rs pass.
