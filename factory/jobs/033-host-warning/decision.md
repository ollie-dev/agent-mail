APPROVE
The general host warning replaces the Gmail-only check, every criterion has a passing test, all four round-1 reviews pass, and the diff stays in scope.

## Acceptance criteria
- AC1: pass (`job033_ac1_web_de_login_on_a_custom_server_is_warned`)
- AC2: pass (`job033_ac2_other_providers_address_at_a_chosen_provider_is_warned`)
- AC3: pass (`job033_ac3a_providers_sharing_a_host_are_not_warned`, `job033_ac3b_matching_host_in_another_case_is_not_warned`, `job033_ac3c_unknown_domain_is_not_warned`, `job033_ac3d_login_without_at_is_not_warned`, plus the extra `job033_ac3c_domain_is_matched_exactly_not_as_a_suffix` and the unit test `provider::tests::expected_host_follows_the_exact_domain_after_the_last_at`)
- AC4: pass (`job033_ac4_gmail_only_check_and_constants_are_gone`. The unchanged `job031_ac6_gmail_login_on_a_custom_server_is_still_warned`, `gmail_login_on_another_host_is_warned_before_the_credential` and `credential_prompt_names_the_server_host_and_port` also pass. The diff removes no lines from tests/, and grep finds none of the names in src/create.rs.)
- AC5: pass (`job033_ac5_host_and_login_in_the_warning_are_escaped`)
- AC6: pass. I ran `cargo test` in the worktree: every suite ok, 0 failed. `cargo clippy -- -D warnings` is clean and `cargo fmt --check` is clean.

## For the human
Nothing needs a decision. The change touches src/create.rs, src/provider.rs and tests/create.rs, adds no dependency, and changes no credential storage. Login and host are still escaped. Domain matching is exact (on the text after the last `@`), never a suffix match. No `[fix-now]` findings were raised.

## Follow-ups
None.
