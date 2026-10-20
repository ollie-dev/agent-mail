## Round 1
**Changed:**
- Core entry point `core::execute(token, command, context)` handles the flow: token → store lookup → open sealed record → deny-by-default Policy check (before any connection) → IMAP LIST → modified UTF-7 decode → filter by Grants in server order. Errors are structured (`missing_token`, `invalid_token`, `denied`, `tls_required`, plus `tls_certificate`, `connection_failed`, `login_failed`, `server_error`, `store_unavailable`), and none of them carries a secret. Malformed, unknown and unopenable tokens all map to the same `invalid_token`.
- Mandate model (`src/mandate.rs`): Account (login, host, port, `TlsMode::{Implicit, Starttls}` with no plaintext variant, Credential with a redacted `Debug`), and a Policy of Grants (operation, Account login, mailboxes as exact UTF-8 names or `*`).
- Sealing (`src/seal.rs`): `amt_` tokens carry 32 random bytes in base64url (43 chars). HKDF-SHA256 with distinct info labels derives the store lookup key and the ChaCha20-Poly1305 key. The record is version ‖ random nonce ‖ AEAD(JSON{token_hash, mandate}), with AAD bound to the lookup key. Open checks the tag and the token hash. `AgentToken` has a redacted `Debug`. Crypto is ring, which rustls already pulls in.
- Store port (`src/store.rs`): the `MandateStore` trait and `RuntimeStore` (always empty; `main.rs` hard-wires it). `InMemoryStore` (carrying the marker `agent-mail-in-memory-store`) and the `TlsTrust::OnlyRoots` test-trust hook compile only under `cfg(any(test, feature = "test-support"))`. The `test-support` feature is not in `default`. Integration tests switch it on through a self dev-dependency.
- IMAP connector (`src/imap.rs`): async-imap on a current-thread tokio runtime with tokio-rustls (ring provider) and rustls-platform-verifier, plus a 30 s timeout. STARTTLS is mandatory: no `STARTTLS` capability or a non-OK reply gives `tls_required`, and LOGIN goes only over the TLS stream. A failed TLS handshake that is not a certificate error gives `tls_required`.
- Modified UTF-7 decoder (`src/utf7.rs`) returns an error on malformed input and never panics. The core falls back to the raw name losslessly.
- CLI: `list` subcommand and a global `--text` flag. `agent_mail::run(args, env, context) -> RunOutput{stdout, stderr, code}` is the in-process entry point. `main.rs` stays thin.
- Tests: `tests/support/` has a fake IMAP server: real TCP, an rcgen CA plus a localhost leaf, implicit/STARTTLS/plaintext-only modes, records command verbs only (never arguments, so the Credential never lands in test output) and counts connections. New `tests/list.rs` and `tests/sealing.rs`. Additions to `tests/cli.rs` and `tests/project.rs`. CLAUDE.md layout updated.

**Commits:** 232b06a

**Tests:** `cargo test`: all suites green (lib 2, cli 10, list 14, project 5, sealing 6; 0 failed). `cargo clippy -- -D warnings`, `cargo clippy --all-targets --features test-support -- -D warnings` and `cargo fmt --check` are clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | list.rs `ac1_list_returns_only_granted_mailboxes_implicit_tls`, `ac1_list_returns_only_granted_mailboxes_starttls` (+ `ac1_starttls_logs_in_only_after_the_upgrade`) | pass |
| AC2 | list.rs `ac2_text_flag_prints_human_readable_list` | pass |
| AC3 | list.rs `ac3_empty_policy_is_denied_before_listing`, `ac3_search_only_policy_is_denied_before_listing` | pass |
| AC4 | list.rs `ac4_wildcard_grant_returns_every_mailbox` | pass |
| AC5 | list.rs `ac5_modified_utf7_names_are_decoded_and_matched` (+ unit tests in `src/utf7.rs`) | pass |
| AC6 | list.rs `ac6_missing_token_is_reported`, `ac6_malformed_and_unknown_tokens_are_indistinguishable`; cli.rs `ac6_binary_reports_missing_token`, `ac6_binary_reports_malformed_and_unknown_tokens_identically` | pass |
| AC7 | sealing.rs `ac7_altered_ciphertext_fails_to_open` (every byte flipped), `ac7_record_fails_to_open_with_another_token`; list.rs `ac7_tampered_or_wrongly_keyed_record_fails_through_list` | pass |
| AC8 | sealing.rs `ac8_store_holds_no_clear_text_secrets_or_names` | pass |
| AC9 | list.rs `ac9_starttls_account_refuses_server_without_starttls`, `ac9_implicit_account_refuses_plaintext_server` | pass |
| AC10 | list.rs: every scenario goes through `list_both_modes` (JSON and `--text`), which calls `assert_no_secrets` on stdout and stderr; also `ac10_untrusted_certificate_is_refused_without_leaking_secrets` | pass |
| AC11 | project.rs `ac11_test_support_feature_is_not_a_default`, `ac11_release_binary_has_no_in_memory_store` (`cargo build --release --locked` into `target/release-check`, then a byte search for the marker); cli.rs `ac11_store_flag_is_an_unknown_argument`, `ac11_store_env_var_changes_nothing` | pass |

**Review findings:** none (first round).

Notes for reviewers:
- Error codes beyond the four the spec fixes (`tls_certificate`, `connection_failed`, `login_failed`, `server_error`, `store_unavailable`) were added because the connector needs some answer for those failures. Error JSON field order is `code`, then `operation`/`account` when present, then `message`.
- Under `--text`, errors print as `error: <code>: <message>` on stderr (the spec fixes only that the code is present).
- Because of the self dev-dependency, `cargo test` builds the binary with `test-support` on. The binary still never selects the in-memory store, because `main.rs` hard-wires `RuntimeStore` and the platform verifier. The release check builds without the feature.

## Round 2
**Changed:**
- Every error message now ends with a next step after `; `, for example `invalid_token`: "the agent token is not valid; check AGENT_MAIL_TOKEN or ask the human for a new token". The codes are unchanged. The `invalid_token` message is still one fixed string, so malformed and unknown tokens stay indistinguishable.
- `--text` escapes control characters in server-supplied names and the account (`\u{a}`, `\u{1b}` and so on), so a hostile server cannot forge lines or ANSI styling. Non-ASCII such as `ü` is unchanged. JSON was already escaped. This addresses the security nit.
- `log` is now a dependency with `max_level_off` and `release_max_level_off`, so async-imap's trace of raw commands (including LOGIN) is compiled out. This addresses the security nit.
- `list --help` now says the token comes from AGENT_MAIL_TOKEN and that output is JSON by default (UX nit). It still does not contain "mandate". The `std` imports in `src/store.rs` are merged (code nit).

**Commits:** 797ffe9

**Tests:** `cargo test`: all suites green (lib 4, cli 10, list 14, project 5, sealing 6; 0 failed). `cargo clippy -- -D warnings`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | list.rs `ac1_list_returns_only_granted_mailboxes_implicit_tls`, `ac1_list_returns_only_granted_mailboxes_starttls` | pass |
| AC2 | list.rs `ac2_text_flag_prints_human_readable_list` | pass |
| AC3 | list.rs `ac3_empty_policy_is_denied_before_listing`, `ac3_search_only_policy_is_denied_before_listing` | pass |
| AC4 | list.rs `ac4_wildcard_grant_returns_every_mailbox` | pass |
| AC5 | list.rs `ac5_modified_utf7_names_are_decoded_and_matched` | pass |
| AC6 | list.rs `ac6_missing_token_is_reported`, `ac6_malformed_and_unknown_tokens_are_indistinguishable`; cli.rs `ac6_binary_reports_missing_token`, `ac6_binary_reports_malformed_and_unknown_tokens_identically` | pass |
| AC7 | sealing.rs `ac7_altered_ciphertext_fails_to_open`, `ac7_record_fails_to_open_with_another_token`; list.rs `ac7_tampered_or_wrongly_keyed_record_fails_through_list` | pass |
| AC8 | sealing.rs `ac8_store_holds_no_clear_text_secrets_or_names` | pass |
| AC9 | list.rs `ac9_starttls_account_refuses_server_without_starttls`, `ac9_implicit_account_refuses_plaintext_server` | pass |
| AC10 | list.rs: every scenario goes through `list_both_modes` + `assert_no_secrets`; `ac10_untrusted_certificate_is_refused_without_leaking_secrets` | pass |
| AC11 | project.rs `ac11_test_support_feature_is_not_a_default`, `ac11_release_binary_has_no_in_memory_store`; cli.rs `ac11_store_flag_is_an_unknown_argument`, `ac11_store_env_var_changes_nothing` | pass |

New supporting tests: `core::tests::every_error_message_names_a_next_step`, `tests::text_output_escapes_control_characters_from_the_server` (src/lib.rs).

**Review findings:**
- [blocking] review-ux, src/core.rs error messages lack next steps → fixed. Each message now ends with an action ("check AGENT_MAIL_TOKEN or ask the human for a new token", "ask the human to fix the account's server or TLS settings", "ask the human to check the account's credential", "check the network and retry, or ask the human to check the account's host and port", and so on). The unit test `every_error_message_names_a_next_step` covers every variant. The codes are unchanged, and `invalid_token` stays one fixed string.
- Nits not taken this round:
  - Zeroizing the Credential and token. Deferred: it is a hardening item and nothing here depends on it.
  - A `#[used]` marker static. The `cfg` gate is the guarantee, as the security review notes.
  - A UTF-7 fallback test.
  - A "(none)" marker in `--text` output.
  - Account details in `--text` errors.

## Round 3
Human feedback (2026-10-20): "fix all security nits remaining from round 2 mentioned by security agent". Round 2 had already fixed two of the four security nits, logging and `--text` escaping. This round fixes the other two.

**Changed:**
- **Zeroization (security nit 3):**
  - `Credential` and `AgentToken` implement `Zeroize` and `ZeroizeOnDrop` (manual `Drop` impls with the `zeroize` crate), so the Credential, the token text and the 256-bit token secret are wiped on drop.
  - The derived ChaCha20-Poly1305 key is held in `Zeroizing` while the ring key is built.
  - Token parsing decodes into a `Zeroizing` buffer.
  - In `seal`, the plaintext (token hash, Credential, Policy) is serialised into a `Zeroizing` buffer. The buffer is pre-sized by a counting pass so it never reallocates and leaves no unwiped copies.
  - In `open`, the decrypted buffer is `Zeroizing` too. The opened `Mandate` wipes its Credential on drop.
- **Release marker check (security nit 4):**
  - The gated `memory` module in `src/store.rs` now carries a `#[used] static MARKER_BYTES`, so the marker sits in any binary the in-memory store is compiled into.
  - `ac11_release_binary_has_no_in_memory_store` now builds twice into `target/release-check`. First, a positive control with `--features test-support` must contain the marker. Second, the default release build must not.

**Commits:** 3814838

**Tests:** `cargo test`: all suites green (lib 6, cli 10, list 14, project 5, sealing 6; 0 failed). `cargo clippy -- -D warnings`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | list.rs `ac1_list_returns_only_granted_mailboxes_implicit_tls`, `ac1_list_returns_only_granted_mailboxes_starttls` | pass |
| AC2 | list.rs `ac2_text_flag_prints_human_readable_list` | pass |
| AC3 | list.rs `ac3_empty_policy_is_denied_before_listing`, `ac3_search_only_policy_is_denied_before_listing` | pass |
| AC4 | list.rs `ac4_wildcard_grant_returns_every_mailbox` | pass |
| AC5 | list.rs `ac5_modified_utf7_names_are_decoded_and_matched` | pass |
| AC6 | list.rs `ac6_missing_token_is_reported`, `ac6_malformed_and_unknown_tokens_are_indistinguishable`; cli.rs `ac6_binary_reports_missing_token`, `ac6_binary_reports_malformed_and_unknown_tokens_identically` | pass |
| AC7 | sealing.rs `ac7_altered_ciphertext_fails_to_open`, `ac7_record_fails_to_open_with_another_token`; list.rs `ac7_tampered_or_wrongly_keyed_record_fails_through_list` | pass |
| AC8 | sealing.rs `ac8_store_holds_no_clear_text_secrets_or_names` | pass |
| AC9 | list.rs `ac9_starttls_account_refuses_server_without_starttls`, `ac9_implicit_account_refuses_plaintext_server` | pass |
| AC10 | list.rs: every scenario goes through `list_both_modes` + `assert_no_secrets`; `ac10_untrusted_certificate_is_refused_without_leaking_secrets` | pass |
| AC11 | project.rs `ac11_test_support_feature_is_not_a_default`, `ac11_release_binary_has_no_in_memory_store` (now with a positive control); cli.rs `ac11_store_flag_is_an_unknown_argument`, `ac11_store_env_var_changes_nothing` | pass |

New supporting tests: `mandate::tests::credential_is_wiped_on_drop_and_zeroizes` and `seal::tests::token_is_wiped_on_drop_and_zeroizes`.

**Review findings:** Round 2 had no blocking findings. The four security nits:
- Log trace leak: fixed in round 2 (`log` with `max_level_off`).
- `--text` control characters: fixed in round 2.
- Zeroization: fixed in this round.
- Marker robustness: fixed in this round.

Limits a reviewer can check: copies held inside third-party crates are outside our control and are not wiped. That covers ring's internal key schedule and async-imap's formatted LOGIN command buffer. The process environment that holds `AGENT_MAIL_TOKEN` is not wiped either. I used manual `Zeroize` impls because the `zeroize_derive` version that zeroize 1.9.1 requires could not be resolved from the registry here.
