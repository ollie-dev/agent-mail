## Round 1
**Changed:**
- `agent-mail search` (`src/lib.rs`, `src/core.rs`): options per spec; `Command::Search`, `Outcome::Messages`; Policy check (`search` on the Account and the exact decoded mailbox) before any connection; `CoreError::Denied` gains optional `mailbox`; new `mailbox_not_found`; one audit entry per call with `mailbox`, `messages` (refs) and new `results` (= number returned, never `total`).
- `src/search.rs` (new): validated value types (`SearchText` rejects CR/LF/NUL, `Date` strict `YYYY-MM-DD`, `Limit` 1..=1000), `Query::imap_criteria` (quoted strings with `"`/`\` escaped, `CHARSET UTF-8` for non-ASCII, `ALL` when empty), result types, `MessageRef` format/parse, RFC 2047 decoder, `Name <addr>` formatting.
- `src/imap.rs`: connect/TLS/login shared by `list_mailboxes` and new `search`: `EXAMINE` (modified UTF-7 name), `UID SEARCH`, `UID FETCH <top-limit UIDs> (UID FLAGS INTERNALDATE ENVELOPE)`, logout. `EXAMINE` `NO` → `MailboxNotFound`. `src/utf7.rs`: `encode`, round-trip tested.
- Fake server (`tests/support/mod.rs`): spec fixtures; `EXAMINE`/`SELECT`, `UID SEARCH` evaluated by the fake, `UID FETCH` with per-fetch UID counts; records `SELECT`/`STORE`; sets `\Seen` on non-peek `BODY[...]`/`RFC822`/`RFC822.TEXT` (even after `EXAMINE`, to catch a missing `.PEEK`).
- README "Search" section and updated Audit log fields; CLAUDE.md layout names `search.rs` and `tests/search.rs`.

**Spec deviations to check:**
- *`--text` clash:* the global `--text` flag and the `search --text S` criterion can't both exist in clap (duplicate long is a debug-assert panic, same id is a type-mismatch panic). So the top-level `--text` is no longer `global`. `list` and `mandate` (and its subcommands) declare their own `--text`, so every position that worked before still works (`text_flag_still_works_after_list_and_mandate`). For search, text output is `agent-mail --text search ...`. `search --help` and the README say so.
- *Reference encoding:* the spec's rule ("`%`, `/` and bytes outside 0x21–0x7E") contradicts its own example and AC6, which encode `[` and `]` as `%5B`/`%5D`. I followed the ACs and used RFC 3986 path-segment encoding: `pchar` (letters, digits, `-._~!$&'()*+,;=:@`) stays as is and every other byte becomes `%XX` in upper-case hex. This still yields exactly three unencoded `/`. Parse is strict: it requires the canonical form, so lower-case hex, raw `[` and the like are refused.
- *Core re-validation:* the core does not re-check strings at runtime. Instead the `Query` fields are types that can only be built through the validating constructors, and clap's value parsers call those same constructors. An MCP adapter therefore can't hand the core an unchecked CR/LF/NUL value, an out-of-range limit or an invalid date.
- `mailbox_not_found` carries `account` and `mailbox` but no `operation`. The audit `mailbox` is the requested mailbox on every search entry, including token failures.

**Commits:** 789f1c7
**Tests:** `cargo test`: all suites `ok`, 159 passed, 0 failed, 1 ignored (keychain opt-in). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | `tests/search.rs::ac1_search_by_from_returns_matching_messages_newest_first` | pass |
| AC2 | `tests/search.rs::ac2_each_criterion_narrows_and_criteria_combine_with_and` | pass |
| AC3 | `tests/search.rs::ac3_default_limit_is_20_and_limit_adjusts_it`, `ac3_bad_limits_are_usage_errors_without_a_connection` | pass |
| AC4 | `tests/search.rs::ac4_search_outside_the_granted_mailbox_is_denied`, `ac4_search_without_a_search_grant_is_denied`, `ac4_empty_policy_denies_the_default_mailbox`, `ac4_list_denial_still_has_no_mailbox_key`; `src/core.rs::every_error_message_names_a_next_step` | pass |
| AC5 | `tests/search.rs::ac5_search_appends_one_entry_with_references_and_count`, `ac5_no_matches_default_limit_and_missing_mailbox_are_logged`, `ac5_log_never_holds_subjects_bodies_or_secrets` | pass |
| AC6 | `tests/search.rs::ac6_references_carry_uidvalidity_and_uid`; `src/search.rs::reference_round_trips_and_encodes_separators`, `malformed_references_are_refused` | pass |

Extra tests the spec asks for: `quotes_in_a_criterion_cannot_inject_search_keys`, `search_never_changes_mailbox_state`, `usage_errors_for_dates_and_line_breaks_make_no_connection`, `non_ascii_criteria_are_sent_as_utf8_with_charset`, `non_ascii_mailbox_is_granted_by_its_decoded_name_and_sent_as_utf7`, `search_help_lists_every_option_and_default`, `readme_documents_search_and_its_audit_fields`, `src/lib.rs::search_text_output_keeps_each_message_on_one_escaped_line`, `src/utf7.rs::encode_round_trips_through_decode`.

**Review findings:** none (round 1).
