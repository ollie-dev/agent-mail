## Round 1
**Changed:**
- `src/core.rs`: `open_mandate` is replaced by `open_presented`. It splits `AGENT_MAIL_TOKEN` on `,`, trims ASCII whitespace, skips empty fields, counts a repeated token once and records the 1-based positions of invalid tokens. A store read failure still fails the call with `store_unavailable`. `open_for_agent` turns "no token opened" into `invalid_token`. `single_account` is replaced by `Presented::target` (the `--account` value, or the only reachable Account, otherwise `account_required` listing the reachable logins) and `Presented::select` (exact login match; `ambiguous_mandate` when several Mandates cover the login, `None` when none does, which callers turn into `denied` before any connection). Every Grant check, the `list` filter, the Credential and the audit `mandate` come from the selected Mandate only.
- In `read`, `flag`, `move` and `draft`, selection runs first and its error is held until after the existing input checks (`invalid_reference`, `unsupported_flag`, `same_mailbox`, `invalid_draft`). That keeps the old error order, and the audit line still names the selected Mandate on those failures. An ambiguity or uncovered Account is still reported before any Grant check or connection.
- `Command::List { account }`, `Command::Search { query, account }`, `Command::Draft { .., account }`, `Command::Whoami` and `Outcome::Whoami(WhoamiResult)` are new. So are `CoreError::AmbiguousMandate { account, mandates }` (`ambiguous_mandate`), `ErrorBody.mandates` and a reworded `account_required` message naming `--account`. `execute_reporting` returns `Report { result, invalid_tokens }` for ticket 15; `execute` remains as a wrapper. The warning text and shape are in `Warning::invalid_tokens`. `is_valid_account` is checked again in the core: an unacceptable login from another adapter gives `account_required`.
- `src/lib.rs`: `--account <LOGIN>` on `list`, `search` and `draft`; on `draft` it conflicts with `--reply-to`. An empty value, or one holding CR, LF or NUL, is a usage error. Also new: the `whoami` subcommand with JSON and `--text` output, sharing the account and grant line helper with `mandate list --text`, and the stderr warning line printed before any error. Help text now says AGENT_MAIL_TOKEN takes comma-separated tokens.
- New `tests/tokens.rs` (14 tests). `tests/support` gains `mandate_with_id` and `run_raw_tokens`.
- Updated existing tests:
  - The `flag`, `read` and `move` denial helpers no longer expect `mandate` in the audit line when the reference names an Account no presented Mandate covers. This is the behaviour AC4 requires.
  - `tests/draft.rs` adds the new `account` field, `whoami` in the command list and `--account` in the `draft --help` options.
- README gains the "Several Mandates and whoami" section. The Draft section, the search synopsis and the Audit log `mandate`, `account` and `reason` entries are updated. CLAUDE.md now lists `tests/tokens.rs`.
- Key order: error objects keep their existing sorted-key convention, which `tests/mandates.rs` asserts. So `ambiguous_mandate` serialises as `account, code, mandates, message`, not the order shown in the spec's example; the test asserts that exact prefix. The new warning object has no such convention, so it keeps the spec's order `code, positions, message`.

**Commits:** 4dbc2b0
**Tests:** `cargo test`: all suites ok, 253 passed, 0 failed (2 ignored opt-in tests). `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --check` clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | `tokens::ac1_account_selects_the_mandate_covering_it`, `tokens::ac1_unacceptable_account_values_are_usage_errors`, `tokens::ac1_core_refuses_unacceptable_account_values_from_other_adapters` | pass |
| AC2 | `tokens::ac2_account_may_be_omitted_only_when_one_account_is_reachable` | pass |
| AC3 | `tokens::ac3_two_mandates_covering_one_account_are_ambiguous`, `tokens::ac3_ambiguity_is_reported_even_when_only_one_mandate_grants` | pass |
| AC4 | `tokens::ac4_grants_never_combine_across_mandates` | pass |
| AC5 | `tokens::ac5_invalid_tokens_are_reported_without_blocking_valid_ones`, `tokens::ac5_only_invalid_or_no_tokens_are_the_existing_errors`, `tokens::ac5_every_agent_command_warns_and_still_succeeds` | pass |
| AC6 | `tokens::ac6_whoami_shows_each_presented_mandate_and_never_secrets`, `tokens::ac6_whoami_lists_ambiguous_mandates_and_handles_bad_tokens`, `tokens::ac6_whoami_text_output` | pass |
| AC7 | `tokens::ac7_readme_and_help_document_several_tokens_and_whoami` | pass |

**Review findings:** none (first round).

## Nit pass 1
**Commits:** 9fc3dda
**Tests:**
- `cargo test`: all suites ok, 254 passed, 0 failed.
- `cargo clippy -- -D warnings` and `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo fmt --check`: clean.

**Findings:**
- [fix-now] review-ux: the `denied` message for an Account no presented Mandate covers told the Agent to ask for a Grant. Fixed.
  - `src/core.rs`: new `CoreError::Uncovered { operation, account, mailbox }`. It keeps code `denied` and the same fields as `Denied`, so the JSON shape and audit `reason` do not change. Its message reads: "no presented Mandate covers account X, so no grant can allow `op` on it; run `agent-mail whoami` to see the accounts the presented tokens cover, or ask the human for a token whose Mandate covers account X". A new `uncovered()` helper builds it.
  - `src/core.rs`: every place that turned an empty selection into `denied` now uses it. That is `list`, `search`, `read`, `flag`, `move`, `draft --account`, and `draft --reply-to`, which keeps operation `read` and the mailbox. Grant-missing denials still use `Denied` with "ask the human to grant it".
  - `every_error_message_names_a_next_step` now covers two `Uncovered` cases.
  - `README.md`: the Several Mandates section says the uncovered-Account denial points to `whoami` or a new token.
  - New test `tokens::ac4_uncovered_account_denial_points_to_whoami_not_to_a_grant`. With only `<tW>`, it runs `list`, `search` and `draft --account other@example.test`, plus `read`, `flag`, `move` and `draft --reply-to` on an `other@example.test` reference. Each exits 1 with code `denied`, the right operation and account, a message containing "no presented Mandate covers account other@example.test" and "agent-mail whoami" and not "grant it", no audit `mandate`, and the `--text` form `error: denied: no presented Mandate covers...`. No connection is made.
  - The same test checks that Grant-missing denials on the covered Account (`list` and `search --mailbox Archive`) keep "ask the human to grant it". Done-when check: passes.
