## Round 1
**Changed:**
- New `src/flag.rs`: `Flag` (`Seen`/`Flagged`, canonical `\Seen`/`\Flagged`), `FlagAction`, ASCII case-insensitive parsing of the untrusted name (optional single leading backslash), the audit `change` string, the `UID STORE` item, and `FlagResult` (`ref`, `account`, `mailbox`, `flags`).
- `src/core.rs`: `Command::Flag { reference, action, flag: String }`, `Outcome::Flagged`, `CoreError::UnsupportedFlag` (code `unsupported_flag`, operation `flag`, message names `seen`/`flagged` and never echoes the value). Order in the core: open Mandate, `is_valid_reference`, parse flag, exact-login Account match plus `flag` Grant on the decoded mailbox, then connect. `StaleReference`, `MessageNotFound` and also `InvalidReference` now carry `operation` (so `read` still reports `read`, `flag` reports `flag`; messages say "the message was not read" or "no flag was changed"). `connector_error` takes the operation. Audit entry for `flag` logs the reference and `change` (computed only from a supported flag name, so it is absent for `unsupported_flag`).
- `src/audit.rs`: optional `change` key between `messages` and `results`.
- `src/imap.rs`: `imap::flag` does LOGIN, SELECT, UIDVALIDITY check, `UID FETCH <uid> (UID FLAGS)` existence check, `UID STORE <uid> ±FLAGS (<flag>)`, reads flags from the store's FETCH or a follow-up `UID FETCH (UID FLAGS)`, then LOGOUT. No CLOSE, EXPUNGE or body fetch.
- `src/lib.rs`: `flag` subcommand with `--set`/`--clear` in a required, non-multiple clap group, `--help` text, `--text` rendering (`<ref>: <flags>` / `<ref>: no flags`, escaped); unit tests for text escaping and usage errors.
- `tests/support/mod.rs`: `Triage` fixture (UIDVALIDITY 1700000007, not in `SERVER_MAILBOXES`); `STORE`/`UID STORE` applies `+FLAGS`/`-FLAGS`/`FLAGS` (with `.SILENT`) after SELECT, NO after EXAMINE, one FETCH per message whose flags actually changed; new `stores()` and `fetches()` accessors.
- `tests/flag.rs` (new), README "Flag" section and Audit log updates, CLAUDE.md layout line names `src/flag.rs` and `tests/flag.rs`.

**Commits:** 99214f1
**Tests:** `cargo test` -> all suites `test result: ok` (189 passed, 0 failed, 1 ignored); `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | `tests/flag.rs::ac1_marking_read_sets_and_unread_clears_seen` | pass |
| AC2 | `tests/flag.rs::ac2_starring_sets_and_unstarring_clears_flagged` | pass |
| AC3 | `tests/flag.rs::ac3_any_other_flag_is_rejected_with_a_structured_error` (plus `core::tests::every_error_message_names_a_next_step`, `flag::tests::parses_only_seen_and_flagged`) | pass |
| AC4 | `tests/flag.rs::ac4_flagging_without_a_flag_grant_is_denied_and_audited` | pass |
| AC5 | `tests/flag.rs::ac5_stale_reference_changes_no_flag` | pass |
| AC6 | `tests/flag.rs::ac6_each_change_is_audited_with_its_reference_and_change` | pass |

**Review findings:** none (first round).

Notes for reviewers:
- The fake answers a non-silent store only for messages whose flags changed, so the "already unread" case in AC1 exercises the connector's follow-up `UID FETCH (UID FLAGS)` path.
- `InvalidReference` also gained `operation` (not required by the spec) so a malformed reference passed to the core by a future adapter reports `flag` instead of a hard-coded `read`.
- An unsupported flag name is checked after reference validation and before the Policy check, so a call that is both unsupported and ungranted reports `unsupported_flag`.
