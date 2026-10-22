## Round 1
**Changed:**
- `src/search.rs`: `Query.gmail: Option<SearchText>` (`Query::all` sets `None`). When set, `imap_criteria()` returns only `X-GM-RAW <quoted>`, with `CHARSET UTF-8 ` in front for non-ASCII. Quoting uses `SearchText::quoted`. Unit test `gmail_queries_are_sent_raw_and_quoted`.
- `src/lib.rs`: `search --gmail <QUERY>` with `conflicts_with_all` on the eight standard criteria. Its own parser rejects an empty value and CR/LF/NUL. `allow_hyphen_values` is on so Gmail negations such as `-in:spam` parse. The help text says what the spec asks for. `From<SearchArgs> for Query` carries the field over.
- `src/imap.rs`: `search` takes a `gmail: bool`. When it is true, the capabilities are read after login and the call fails with the new `ImapError::GmailUnsupported` before `EXAMINE`, then logs out. The capability read is factored into `capability_atoms`, which `move_in` also uses (still one `CAPABILITY` per move). New public `drafts_mailbox(account, network)`, which shares a new `find_drafts` with `draft_in`, so draft behaviour is unchanged. It returns the raw name as LIST gave it (modified UTF-7, e.g. `[Gmail]/Entw&APw-rfe`), the same value `draft` appends to.
- `src/core.rs`: `CoreError::Unsupported { account, mailbox }` with code `unsupported`, `operation` `search`, `account`, `mailbox`, and a message whose next step is `run agent-mail search with the standard criteria (...)`. The message never repeats the query. It is mapped from `ImapError::GmailUnsupported` and added to `every_error_message_names_a_next_step`. `src/create.rs` lists the new variant in `failure_reason`.
- `tests/support/mod.rs`: `script_gmail_search(query, uids)` (INBOX) and `script_gmail_search_in(mailbox, query, uids)`. Scripted results match the exact query only, and unscripted queries match nothing. `X-GM-RAW` gets `BAD` unless `X-GM-EXT-1` is advertised.
- `tests/gmail_live.rs` (new): the ignored `gmail_live_round_trip` runs steps (a) to (e) plus the audit-log check, and five non-ignored tests cover `live_config` and the ignore attribute. `Credential::expose` is `pub(crate)` and I did not widen it. Instead, the leak checks re-read the password variable into a `zeroize::Zeroizing<String>`. The skip message goes straight to `std::io::stderr()`, because `eprintln!` output is captured and hidden for a passing test. Assert messages print step names, exit codes, error codes and `ImapError` variant names only.
- `tests/draft.rs`: `drafts_mailbox_returns_the_first_listed_drafts_mailbox_or_drafts_not_found` (fake server).
- `README.md`: the Search section gains the `--gmail QUERY` usage line, an option bullet and a "Gmail search" paragraph. A new `## Development` section documents the live test: the variables (the password is read with `read -rs`, so it stays out of shell history), the run command, the preconditions, and that it never writes to the account. `CLAUDE.md` lists `tests/gmail_live.rs` and its run command.

**Commits:** d61a8fe

**Tests:** `cargo test`: every binary `test result: ok` (search 30 passed; gmail_live 5 passed, 1 ignored; keychain 1 ignored; 0 failed overall). `cargo clippy --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean. `cargo test --test gmail_live -- --ignored` without the variables prints `gmail_live skipped: set AGENT_MAIL_LIVE_GMAIL_ADDRESS and AGENT_MAIL_LIVE_GMAIL_APP_PASSWORD to run it` and passes. `cargo test -- --ignored keychain --list` lists only the Keychain test.

| AC | Test | Status |
|----|------|--------|
| AC1 | `tests/search.rs` `ac1_gmail_query_is_sent_unchanged_as_x_gm_raw_and_decides_the_result`, `ac1_gmail_query_is_quoted_and_marked_utf8`, `ac1_bad_gmail_queries_and_combinations_are_usage_errors_without_a_connection`; `src/search.rs` `gmail_queries_are_sent_raw_and_quoted` | pass |
| AC2 | `tests/search.rs` `ac2_gmail_on_a_server_without_x_gm_ext_1_is_unsupported`; `src/core.rs` `every_error_message_names_a_next_step` | pass |
| AC3 | `tests/search.rs` `ac3_gmail_outside_the_granted_mailbox_is_denied`, `ac3_gmail_without_a_search_grant_is_denied`, `ac3_gmail_respects_the_limit`, `ac3_gmail_audit_lines_match_standard_search_and_never_hold_the_query` | pass |
| AC4 | `tests/gmail_live.rs` `gmail_live_round_trip` (ignored; needs a real account, not run here), `live_round_trip_is_ignored_by_default`; `tests/draft.rs` `drafts_mailbox_returns_the_first_listed_drafts_mailbox_or_drafts_not_found` | ignored by default (live run not possible here); helpers pass |
| AC5 | `tests/gmail_live.rs` `live_config_without_either_variable_skips_with_a_clear_message`, `live_config_without_the_password_names_it_and_hides_the_address`, `live_config_without_the_address_names_it`, `live_config_holds_the_password_as_a_redacted_credential` | pass |
| AC6 | `tests/search.rs` `ac6_readme_and_help_document_gmail_search_and_the_live_test` | pass |

**Review findings:** none (round 1).

**Notes for reviewers:** `imap_criteria()` sends only `X-GM-RAW` when `gmail` is set, even if another adapter fills in standard criteria too. The CLI makes that combination a usage error. A future adapter such as MCP should refuse the combination too, not rely on the core.

## Nit pass 1
**Commits:** a86c251
**Tests:** `cargo test`: all 18 `test result: ok`, 0 failed (search 30 passed; gmail_live 5 passed, 1 ignored). `cargo clippy -- -D warnings` and `cargo clippy --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.
**Findings:**
- review-ui `[fix-now]` src/lib.rs `--gmail` help is one ~290-character line:
  - Changed: the doc comment is split into a short first paragraph (`Search in Gmail's search syntax, e.g. "from:alice has:attachment"`) and a detail paragraph (sent unchanged; needs X-GM-EXT-1; `unsupported` elsewhere; cannot be combined with --from, --to, --subject, --text, --since, --before, --unseen or --flagged; works with --mailbox and --limit).
  - Same problem elsewhere in this help: `--mailbox` was 107 characters in `search -h`. Its help is now `Mailbox to search, as `agent-mail list` names it`.
  - Done when: `agent-mail search -h` shows the `--gmail` line at 94 characters, and the longest `-h` line is now 94. `agent-mail search --help` names X-GM-EXT-1, `unsupported`, the eight excluded criteria, and that --mailbox/--limit work.
  - Regression test: `ac6_readme_and_help_document_gmail_search_and_the_live_test` asserts the `--help` details and that every `search -h` line is under 100 characters, without X-GM-EXT-1.
- review-ux `[fix-now]` README audit `reason` list omits `unsupported`:
  - Changed: added `unsupported` to the `reason` bullet. No other README list of search error codes exists. The Search section's Gmail paragraph already names `unsupported`.
  - Done when: `grep -n 'reason' README.md` shows line 165, the `reason` bullet, ending its list with `invalid_draft` or `unsupported`.
  - Regression test: the same test asserts the README Audit log `reason` bullet contains `` `unsupported` ``.
