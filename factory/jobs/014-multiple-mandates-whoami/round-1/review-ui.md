VERDICT: PASS

## Findings
- [follow-up] src/lib.rs:70: the `--text` flag help still says "(for `search`, put it before the subcommand)"; harmless here, but a later job could tidy the global-versus-subcommand `--text` wording now that `whoami` and `list` accept it after the subcommand.

Checked: `--help` (after_help on AGENT_MAIL_TOKEN, `whoami` listed), `whoami --help`, `list --help` (`--account <LOGIN>`), the `missing_token` result for blank tokens, and the `account_required`, `ambiguous_mandate` and `invalid_token` messages. Each names its next step. Warning and error lines are one line each, with the warning first and positions only. Text output reuses the `mandate list` account and grant lines with control characters escaped. The 14 tests in `tests/tokens.rs` pass.
