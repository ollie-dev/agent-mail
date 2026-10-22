APPROVE
All four round-1 reviews pass with no fix-now findings, every criterion has a passing test, and the change stays within the spec (only new dependency is the static-data webpki-roots).

## Acceptance criteria
- AC1: pass (tests/proxy.rs `ac1_list_through_connect_proxy_implicit_tls`, `ac1_list_through_connect_proxy_starttls`, `ac1_lowercase_https_proxy_is_used`)
- AC2: pass (tests/proxy.rs `ac2_list_through_socks5_proxy`, `ac2_https_proxy_wins_over_all_proxy`)
- AC3: pass (tests/proxy.rs `ac3_connect_proxy_credentials_are_sent_and_never_shown`, `ac3_socks5_proxy_credentials_are_sent_and_never_shown`, `ac3_credentials_never_shown_when_the_proxy_refuses`, `ac3_invalid_proxy_url_names_the_variable_only`)
- AC4: pass (tests/proxy.rs `ac4_proxy_refusal_names_the_host`, CONNECT 403 and SOCKS5 0x02)
- AC5: pass (tests/proxy.rs `ac5_without_proxy_variables_connections_are_direct`)
- AC6: pass (tests/proxy.rs `ac6_proxy_mode_rejects_a_certificate_not_chained_to_a_built_in_root`; src/imap.rs `imap::tests::proxy_mode_trusts_the_built_in_roots`, `imap::tests::direct_mode_keeps_the_platform_verifier`)

Ran in the worktree: `cargo test` (every suite ok, tests/proxy.rs 15 passed, 71 unit tests passed, 2 opt-in tests ignored), `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --check` clean.

## For the human
Nothing needs a decision. Things that are intended but worth knowing: in proxy mode, TLS is verified against the built-in webpki roots, not the platform verifier. The spec requires this, and it happens only when `HTTPS_PROXY`/`ALL_PROXY` is set. An operator who already has one of these variables set in their shell will now see `mandate create` and every command go through that proxy. Proxy credentials from the URL are sent only to the proxy. They are held in `Zeroizing`, never written to disk, and tests check that they never appear in output or the audit log. Security review confirmed the test-only `OnlyRoots` hook is absent from the release binary.

## Follow-ups
- code · src/proxy.rs (connect_handshake / socks5_handshake): every CONNECT 4xx (e.g. 404, 429) and SOCKS5 "no acceptable method" or auth failure map to `proxy_refused`, so the Agent may be told to "allow this host" when that is not the cause → narrow `proxy_refused` to allowlist refusals (403, SOCKS 0x02) and give 407/auth failures their own message.
- ui · src/core.rs:~700: the `proxy_refused` message is one line of about 300 characters, like other long messages in the file → shorten the error messages across core.rs in one pass.
- ux · src/core.rs (execute_reporting): an invalid proxy variable is only reported when a command connects, so `whoami` and `mandate list` never flag it → consider a proxy-setting hint in `whoami`.
