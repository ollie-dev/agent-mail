ESCALATE
The round-3 security nits are fixed and all 11 criteria and all four reviews pass, but the human has not yet signed off the new credential storage format and the secret-handling dependencies.

## Acceptance criteria
I ran `cargo test` in the worktree at 3814838. Every suite passed: lib 6, cli 10, list 14, project 5, sealing 6, 0 failed. `cargo clippy -- -D warnings` and `cargo fmt --check` are clean.
- AC1: pass (list.rs `ac1_list_returns_only_granted_mailboxes_implicit_tls`, `ac1_list_returns_only_granted_mailboxes_starttls`, plus `ac1_starttls_logs_in_only_after_the_upgrade`)
- AC2: pass (list.rs `ac2_text_flag_prints_human_readable_list`)
- AC3: pass (list.rs `ac3_empty_policy_is_denied_before_listing`, `ac3_search_only_policy_is_denied_before_listing`)
- AC4: pass (list.rs `ac4_wildcard_grant_returns_every_mailbox`)
- AC5: pass (list.rs `ac5_modified_utf7_names_are_decoded_and_matched`)
- AC6: pass (list.rs `ac6_missing_token_is_reported`, `ac6_malformed_and_unknown_tokens_are_indistinguishable`; cli.rs `ac6_binary_reports_missing_token`, `ac6_binary_reports_malformed_and_unknown_tokens_identically`)
- AC7: pass (sealing.rs `ac7_altered_ciphertext_fails_to_open`, `ac7_record_fails_to_open_with_another_token`; list.rs `ac7_tampered_or_wrongly_keyed_record_fails_through_list`)
- AC8: pass (sealing.rs `ac8_store_holds_no_clear_text_secrets_or_names`)
- AC9: pass (list.rs `ac9_starttls_account_refuses_server_without_starttls`, `ac9_implicit_account_refuses_plaintext_server`)
- AC10: pass (list.rs: every scenario runs through `list_both_modes`, which calls `assert_no_secrets` on JSON and `--text` output; plus `ac10_untrusted_certificate_is_refused_without_leaking_secrets`)
- AC11: pass (project.rs `ac11_test_support_feature_is_not_a_default`, `ac11_release_binary_has_no_in_memory_store`, which now includes a positive control that a `test-support` build does contain the marker; cli.rs `ac11_store_flag_is_an_unknown_argument`, `ac11_store_env_var_changes_nothing`)

Scope: the round-3 commit only changes `Cargo.toml`/`Cargo.lock` (adds `zeroize`), `src/mandate.rs`, `src/seal.rs`, `src/store.rs` and `tests/project.rs`. That is exactly what your human-notes request covered. The branch as a whole stays inside the spec.

## For the human
You asked for the remaining round-2 security nits to be fixed, and they are:
- **Zeroization:** `Credential` and `AgentToken` are wiped on drop. The derived seal key, the decoded token bytes, the seal plaintext and the opened plaintext are held in `Zeroizing` buffers.
- **Release marker:** a `#[used]` static now carries the marker inside the cfg-gated module, and the release check has a positive control.

Security review round 3 confirmed both fixes.

Your note didn't say whether you accept the rest of the round-2 escalation, so I'm escalating again. These items are still yours to decide:
1. **A new way Credentials are stored.** `src/seal.rs` defines the sealed-record format that ticket 03 will put in the Keychain. The token secret is 256 bits. HKDF-SHA256 derives separate keys for the store lookup and for ChaCha20-Poly1305. The nonce is random, the AAD is bound to the store key, and the token hash, Credential and Policy are sealed together in one ciphertext (ADR-0003).
2. **New dependencies that handle secrets:**
   - `ring` (AEAD and HKDF)
   - `rustls`, `tokio-rustls` and `rustls-platform-verifier`
   - `async-imap`, which carries the Credential in LOGIN
   - `zeroize`, new in this round. It was already in the dependency tree through rustls, and it adds no capability.
3. **Residual caveats reviewers accepted as nits:**
   - Some copies sit inside third-party code and are not wiped: ring's key schedule, async-imap's LOGIN buffer, a possible serde scratch buffer, and the process environment holding `AGENT_MAIL_TOKEN`.
   - `mint` leaves small stack copies of the token secret that are not wiped.
   - The crate-wide `log` `max_level_off` cap is what keeps async-imap's LOGIN trace out of the build. Ticket 06 (audit log) must not lower it. Consider writing that down in an ADR or in ticket 06.
   - Five extra error codes become part of the Agent-facing contract: `tls_certificate`, `connection_failed`, `login_failed`, `server_error` and `store_unavailable`.

Your options:
- **Approve:** merge `factory/003-tracer-list-mailboxes` as is. The sealing format and the dependency set become the base for ticket 03. The caveats under item 3 go to the backlog.
- **Rework:** send it back with specific changes, for example a different crypto crate, wiping the `mint` stack copies, or fewer error codes. That means another review round.
