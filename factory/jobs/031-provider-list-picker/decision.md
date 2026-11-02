APPROVE
All ten criteria have passing tests, all four round-1 reviews pass with no fix-now findings, and the diff stays inside the ticket-01 scope.

## Acceptance criteria
- AC1: pass (src/terminal.rs unit tests `search_picks_an_option_found_by_a_hidden_key`, `search_panics_when_the_query_does_not_find_the_label`, `search_can_be_cancelled_chosen_or_accepted`, `matching_trims_ignores_case_and_matches_all_on_empty`)
- AC2: pass (tests/provider.rs `ac2_the_list_is_built_in_and_says_why`, `ac2_all_returns_the_30_supported_providers_in_file_order`; src/provider.rs reads the list only through `include_str!`)
- AC3: pass (tests/provider.rs `ac3_the_built_in_list_follows_the_schema`, `ac3_the_parser_rejects_each_broken_rule`, which covers all 8 broken rules)
- AC4: pass (tests/provider.rs `ac4_labels_show_at_most_three_domains`, `ac4_sorted_is_case_insensitive_by_name`, `ac4_search_finds_by_name_and_every_domain`; tests/create.rs `job031_ac4_mail_provider_lists_custom_server_then_providers_by_name`)
- AC5: pass (tests/create.rs `job031_ac5_gmail_provider_asks_only_address_and_app_password`, `job031_ac5_password_provider_without_note_asks_password_and_shows_no_note`)
- AC6: pass (tests/create.rs `job031_ac6_no_account_type_or_gmail_preset_remains`, `job031_ac6_gmail_login_on_a_custom_server_is_still_warned`; tests/export.rs warning scenarios)
- AC7: pass (tests/create.rs `job031_ac7_custom_server_asks_host_port_tls_and_login_in_order`; tests/export.rs `job031_ac7_from_asks_no_provider_but_an_added_account_goes_through_the_picker`)
- AC8: pass (tests/create.rs `job031_ac8_provider_and_custom_server_store_the_same_account`; the create, export, client and proxy suites pass; tests/credential_manager.rs is Windows-only, so on macOS only the renamed constant was checked, not a compile)
- AC9: pass (tests/release.rs `job031_ac9_claude_md_lists_the_provider_module_and_data`)
- AC10: pass (I ran `cargo test` in the worktree: 30 suites, 0 failed. `cargo clippy -- -D warnings` was clean. `cargo fmt --check` was clean.)

## For the human
Nothing needs a decision from you. The Credential's destination on the Provider path comes only from `data/providers.json`, which is compiled into the binary. The change adds no runtime read and no new dependency (Cargo.toml and Cargo.lock are unchanged). The Broker protocol and the Mandate format are unchanged too. One manual check is still worth doing: no one has driven the real `inquire` picker on a terminal yet. Try `agent-mail mandate create` once and type `web.de` at "Mail provider:".

## Follow-ups
- ui · src/terminal.rs:285: the picker uses inquire's default page size for 31 options, with the help line "type to search, enter to choose", and no one has seen it on a real terminal → when ticket 02 adds the unsupported entries, check how the list looks in an 80-column terminal and whether the "no match" state is clear.
- ux · src/terminal.rs:289: the picker help does not say that "Custom server…" is the first and default entry, or that it disappears when a search matches only Providers → in ticket 02 or 03, mention the Custom server option in the help line.
