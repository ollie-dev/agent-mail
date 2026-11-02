## Round 1
**Changed:**
- Prefactor (own commit): `Terminal::search` with `SearchOption { label, keys }` and `SearchOption::matches` (trimmed, case-insensitive substring of label or any key; empty matches all). `InquireTerminal` uses `inquire::Select` with filtering on and a custom scorer built on `matches` (score `-index`, so matches keep list order). `ScriptedTerminal` takes `Answer::search(query, label)` (panics if the option does not match), plus `Choose`, `Accept`, `Cancel`; recorded as `Kind::Choice`. `NoTerminal` cancels.
- `data/providers.json`: the 30 supported entries of `.scratch/provider-list/providers.json`, unchanged and in the same formatting.
- `src/provider.rs`: `include_str!` of the list with the comment on why it is built in; `parse` (serde with `deny_unknown_fields`, then slug, unique id, unique domain case-insensitive, non-empty name and host, port not 0, note at most 300 chars), `all()` (OnceLock), `sorted()`, `Provider::label`, `Provider::search_option`, `CredentialKind::prompt`, `by_domain` (for ticket 03; unit-tested).
- `src/create.rs`: "Mail provider:" (`MAIL_PROVIDER`) replaces "Account type:"; `GMAIL`/`GENERIC` removed, `CUSTOM_SERVER` added. A Provider asks "Email address at <Name>:", shows the note, then "Password"/"App password for <login> on <host>:<port>:". Custom server path unchanged. `--from` kept Accounts skip the picker. `GMAIL_HOST`, `looks_like_gmail` and the warning stay; `GMAIL_PORT` stays with `#[expect(dead_code, reason = ...)]` since nothing reads it any more until ticket 03.
- Tests: `GENERIC` -> `CUSTOM_SERVER` in export, client, proxy, credential_manager; the old Gmail preset test in `tests/create.rs` replaced by the job031 tests; `tests/support` gains `GMAIL_LOGIN` (`someone@gmail.com`) as a known fake login.
- `CLAUDE.md`: layout names `src/provider.rs`, `data/providers.json`, `tests/provider.rs`, plus a Data bullet.

**Commits:** 943a45e, 3d4f9ee
**Tests:** `cargo test` -> every suite `test result: ok` (0 failed); `cargo clippy --all-targets -- -D warnings` and `cargo clippy -- -D warnings` clean; `cargo fmt --check` clean. `tests/credential_manager.rs` is `#![cfg(windows)]` and was not compiled on macOS (rename only).

| AC | Test | Status |
|----|------|--------|
| AC1 | `terminal::tests::search_picks_an_option_found_by_a_hidden_key`, `search_panics_when_the_query_does_not_find_the_label`, `search_can_be_cancelled_chosen_or_accepted`, `matching_trims_ignores_case_and_matches_all_on_empty` (src/terminal.rs) | pass |
| AC2 | `tests/provider.rs`: `ac2_the_list_is_built_in_and_says_why`, `ac2_all_returns_the_30_supported_providers_in_file_order` | pass |
| AC3 | `tests/provider.rs`: `ac3_the_built_in_list_follows_the_schema`, `ac3_the_parser_rejects_each_broken_rule` | pass |
| AC4 | `tests/provider.rs`: `ac4_labels_show_at_most_three_domains`, `ac4_sorted_is_case_insensitive_by_name`, `ac4_search_finds_by_name_and_every_domain`; `tests/create.rs`: `job031_ac4_mail_provider_lists_custom_server_then_providers_by_name` | pass |
| AC5 | `tests/create.rs`: `job031_ac5_gmail_provider_asks_only_address_and_app_password`, `job031_ac5_password_provider_without_note_asks_password_and_shows_no_note` | pass |
| AC6 | `tests/create.rs`: `job031_ac6_no_account_type_or_gmail_preset_remains`, `job031_ac6_gmail_login_on_a_custom_server_is_still_warned`; `tests/export.rs` warning scenarios | pass |
| AC7 | `tests/create.rs`: `job031_ac7_custom_server_asks_host_port_tls_and_login_in_order`; `tests/export.rs`: `job031_ac7_from_asks_no_provider_but_an_added_account_goes_through_the_picker`, `ac3_from_file_pre_selects_the_file_choices` | pass |
| AC8 | `tests/create.rs`: `job031_ac8_provider_and_custom_server_store_the_same_account`; existing create, export, client and proxy suites | pass |
| AC9 | `tests/release.rs`: `job031_ac9_claude_md_lists_the_provider_module_and_data` | pass |
| AC10 | `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check` | pass |

**Review findings:** none (round 1).
