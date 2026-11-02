VERDICT: PASS

## Findings
None.

## Checked
- Read: full `git diff main...factory/032-unsupported-providers` (`data/providers.json`, `src/provider.rs`: `Provider`, `connection`, `label`, `parse`; `src/create.rs`: `add_account`, `ask_provider` loop; test names added in `tests/provider.rs` and `tests/create.rs`).
- Ran: `cargo test` in the worktree: all suites passed, 0 failed. `cargo clippy -- -D warnings`: clean. `cargo fmt --check`: clean. Compared `data/providers.json` with `.scratch/provider-list/providers.json` as parsed JSON: equal, 35 entries.
- Criteria:
  - AC1: `job032_ac1_all_returns_the_35_providers_in_file_order` and `job032_ac1_unsupported_providers_carry_the_starting_reason_only`. Both fail without the 5 entries.
  - AC2: `job032_ac2_the_parser_enforces_the_unsupported_rules`. It covers the `parse` Err/Ok cases (300 vs 301 characters, with `imap`, with `credential`). The `parse` match arms enforce the rules.
  - AC3: `job032_ac3_mail_provider_lists_unsupported_providers_by_name`, `job032_ac3_unsupported_labels_carry_the_suffix` and `job032_ac3_search_finds_unsupported_providers_by_name_and_domain`. They fail without the suffix and entries.
  - AC4: `job032_ac4_unsupported_provider_shows_the_reason_and_asks_again` (cancel path) and `job032_ac4_another_provider_after_an_unsupported_one_completes` (continue path). The loop in `ask_provider` returns only supported choices, so no Credential prompt or store call is reachable.
  - AC5: the three commands above pass.
