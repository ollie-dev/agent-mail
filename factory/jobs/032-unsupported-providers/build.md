## Round 1
**Changed:**
- `data/providers.json`: copied `.scratch/provider-list/providers.json` unchanged, appending the 5 unsupported entries (`outlook`, `microsoft365`, `proton`, `tuta`, `hey`); 35 entries.
- `src/provider.rs`: `Provider` gains `unsupported: Option<String>`; `imap` and `credential` become `Option`. `parse` requires either `imap` + `credential` with no reason, or a non-empty reason of at most `REASON_MAX` (300) characters with neither. New `connection()` returns the IMAP settings and Credential kind of a supported Provider. `label()` (and so `search_option()`) appends `UNSUPPORTED_SUFFIX` (" – not supported", U+2013).
- `src/create.rs`: `ask_provider` loops: choosing an unsupported Provider shows its reason as a `terminal.message` and asks "Mail provider:" again with the same options and default; it returns the chosen Provider with its IMAP settings and Credential kind, so `add_account` never sees an unsupported one.
- Tests: updated the job031 tests that hard-coded 30/31 (they now filter on `unsupported` to keep checking the 30 supported entries, and expect 35/36); added job032 tests below.

**Commits:** dce838a
**Tests:** `cargo test`: all suites green, 548 passed, 0 failed (ignored opt-in tests unchanged). `cargo clippy -- -D warnings` (also `--all-targets`): clean. `cargo fmt --check`: clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | `tests/provider.rs` `job032_ac1_all_returns_the_35_providers_in_file_order`, `job032_ac1_unsupported_providers_carry_the_starting_reason_only` | pass |
| AC2 | `tests/provider.rs` `ac3_the_built_in_list_follows_the_schema` (imap/credential present exactly when no reason, reasons <= 300), `job032_ac2_the_parser_enforces_the_unsupported_rules` | pass |
| AC3 | `tests/create.rs` `job032_ac3_mail_provider_lists_unsupported_providers_by_name`; `tests/provider.rs` `job032_ac3_unsupported_labels_carry_the_suffix`, `job032_ac3_search_finds_unsupported_providers_by_name_and_domain` | pass |
| AC4 | `tests/create.rs` `job032_ac4_unsupported_provider_shows_the_reason_and_asks_again` (cancel), `job032_ac4_another_provider_after_an_unsupported_one_completes` (continue) | pass |
| AC5 | `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check` | pass |

**Review findings:** none (round 1).
