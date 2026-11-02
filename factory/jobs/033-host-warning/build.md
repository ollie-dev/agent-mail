## Round 1
**Changed:**
- `src/provider.rs`: new `expected_for_login(login)` returns the supported built-in Provider and its IMAP host for the exact domain after the login's last `@` (via `by_domain`, so case-insensitive and never a suffix match); `None` without `@`, for unknown domains and for unsupported Providers. Unit test `expected_host_follows_the_exact_domain_after_the_last_at` (incl. `someone@gmail.com.evil.test`, `x@cox.net`).
- `src/create.rs`: `add_account` warns with the spec's text (`{Name}`, `{expected}`) whenever `expected_for_login` gives a host that differs ASCII case-insensitively from the chosen host; login and host still go through `escape_controls`. Applies to a chosen Provider, "Custom server…" and `--from`. Removed `GMAIL_HOST`, `GMAIL_PORT`, `looks_like_gmail` and the unit test `gmail_addresses_are_recognised`.
- `tests/create.rs`: job033 tests for AC1-AC5, plus an extra suffix-match test (`someone@gmail.com.evil.test`).

**Commits:** b305fd2
**Tests:** `cargo test` -> all suites ok, 557 passed; 0 failed; 6 ignored. `cargo clippy -- -D warnings` (and `--all-targets`) clean, `cargo fmt --check` clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | `job033_ac1_web_de_login_on_a_custom_server_is_warned` | pass |
| AC2 | `job033_ac2_other_providers_address_at_a_chosen_provider_is_warned` | pass |
| AC3 | `job033_ac3a_providers_sharing_a_host_are_not_warned`, `job033_ac3b_matching_host_in_another_case_is_not_warned`, `job033_ac3c_unknown_domain_is_not_warned`, `job033_ac3c_domain_is_matched_exactly_not_as_a_suffix`, `job033_ac3d_login_without_at_is_not_warned` | pass |
| AC4 | `job033_ac4_gmail_only_check_and_constants_are_gone`, unchanged `job031_ac6_gmail_login_on_a_custom_server_is_still_warned` (tests/create.rs), `gmail_login_on_another_host_is_warned_before_the_credential` and `credential_prompt_names_the_server_host_and_port` (tests/export.rs) | pass |
| AC5 | `job033_ac5_host_and_login_in_the_warning_are_escaped` | pass |
| AC6 | `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check` | pass |

**Review findings:** none (round 1).
