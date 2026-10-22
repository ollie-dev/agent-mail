VERDICT: PASS

## Findings
- [follow-up] src/core.rs (`Presented::target`, `open_presented`): every presented token costs one store lookup (one Keychain prompt each on macOS) and there is no cap. The spec lists a cap as out of scope. A later job could add a limit on presented tokens.
- [follow-up] tests/tokens.rs `ac5_every_agent_command_warns_and_still_succeeds`: asserts the warning and exit code 0 per command, as the spec asks, but not which Mandate the audit line names. Optional later hardening, not needed for this job.

No blocking or fix-now findings.

## What I read
- Full `git diff main...HEAD` (10 files).
- `src/core.rs`:
  - `Command` (List, Search, Draft with `account`; Whoami), `WhoamiResult`, `WhoamiMandate`.
  - `CoreError::AmbiguousMandate`, `ErrorBody.mandates`, the reworded `account_required` message, and `every_error_message_names_a_next_step`.
  - `Report`, `Warning::invalid_tokens`, `execute`, `execute_reporting`, `run_command`, `whoami`.
  - `is_valid_account`, `Presented`, `open_presented`, `open_token`, `open_for_agent`, `Presented::reachable`, `Presented::target`, `Presented::select`.
  - `list`, `search`, `read`, `flag`, `move_message`, `draft`.
- `src/lib.rs`:
  - `Cli` after_help, `AccountChoice`, `account_login`, `DraftArgs.account` (conflicts with `--reply-to`).
  - Argument-to-`Command` mapping, warning-before-error stderr assembly.
  - `render_whoami`, `push_accounts_and_grants` (shared with `mandate list --text`), `render_warning`, `render_error`.
- `tests/tokens.rs` (all 14 tests), `tests/support/mod.rs` (`mandate_with_id`, `run_raw_tokens`), and the updates to `tests/draft.rs`, `tests/read.rs`, `tests/flag.rs`, `tests/move_message.rs`. The last four change only the audit expectation for uncovered Accounts (no `mandate`) and the new `account` field and command lists.
- README section, Draft and Audit log edits, and the CLAUDE.md layout line.
- Ran `cargo test` (all suites ok, 0 failed), `cargo clippy --all-targets -- -D warnings` (clean) and `cargo fmt --check` (clean).

## Acceptance criteria against code and tests
- AC1 (selection by `--account`): `Presented::select` matches the exact login. `ac1_account_selects_the_mandate_covering_it` checks stdout, empty stderr and audit `mandate`, `account` and `outcome` for both accounts, for both token orders including the `, ` form. It also covers `read` and `search --account other`. It would fail without the change, because `--account` does not exist. `ac1_unacceptable_account_values_are_usage_errors` checks exit code 2 for empty, LF, CR and NUL on list, search and draft, and for draft with `--reply-to`. It also checks 0 connections and no audit line. `ac1_core_refuses_...` covers the core's re-check. Exercised.
- AC2 (account optional or `account_required`): `ac2_...` covers one reachable Account, `<tW>,garbage`, two Mandates, and one Mandate covering two Accounts. It covers list, search and draft. It checks the `accounts` list, the `--account` wording, no "not supported yet", the error having 3 keys, an audit line with reason `account_required` and no mandate or account, and no new connections. `target` runs before `trail.account` is set, so the audit line matches. Exercised.
- AC3 (`ambiguous_mandate`): `select` returns the error before any Grant check. `ac3_two_mandates_...` covers all 7 commands, including `draft --reply-to`, and checks the key order prefix, mandate IDs in token order, message text, audit line (account, denied, reason, no mandate) and 0 connections. It also covers other Accounts being unaffected and `<tX>,<tX>` succeeding. `ac3_ambiguity_is_reported_even_when_only_one_mandate_grants` covers the emptied-Grants case. Exercised. The key order is sorted, not the spec's illustrative order. The builder documented this, and it matches the existing error convention.
- AC4 (no cross-Mandate Grants): Grants are evaluated only on the selected Mandate's Policy. `ac4_...` builds W with Grants on the other login and checks list returns only `Archive`. It checks that search, read and draft are denied with the right operation, account and mailbox, with audit `mandate` `bbbb2222`, and that no connection is made. It also covers an uncovered Account: denied, audit has no mandate. Exercised.
- AC5 (invalid tokens): `open_presented` records field positions, counting empty fields. `ac5_invalid_tokens_...` covers `[1]`, `[2,3]` for unknown and tampered, and `[3]` with an empty field. It checks the exact JSON prefix and key count, the `--text` line, warning-before-error ordering, and the same audit keys. `Env::run` asserts no `garbage` or token text in any output. `ac5_only_invalid_or_no_tokens_...` covers `invalid_token` with a single stderr line, and `missing_token` for `" , ,"`, `""` and `","`. `ac5_every_agent_command_warns_...` covers search, read, flag, move and draft. List is covered elsewhere. Exercised.
- AC6 (`whoami`): the test compares the exact JSON string, including key order and `invalid_tokens:[3]`. It checks that no secret appears in output (Credential, tokens, token hashes, `garbage`), 0 connections, no audit line, P with no Grants still shown, ambiguous Mandates both listed, `missing_token` when unset, only invalid tokens giving `mandates:[]` and positions, and the `--text` form in both flag placements with control characters escaped. Exercised.
- AC7 (docs and help): `ac7_...` asserts the README needles, that the old Draft sentence is gone, `whoami` in the `--help` command list, and `--account <LOGIN>` in the `list`, `search` and `draft` help. The README text matches the behaviour I read. Exercised.

## Other checks
- Scope: the diff stays within the spec's file list. The `render_error` change goes through `json_line`, with the same output as before.
- Error order: `read`, `flag`, `move` and `draft` run selection first and report its error only after the existing input checks, so old error precedence is kept. Ambiguity and uncovered Accounts are still reported before any Grant check or connection.
- `#[allow(clippy::too_many_arguments)]` on `draft` is acceptable.
