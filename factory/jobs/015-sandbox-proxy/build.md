## Round 1
**Changed:**
- `src/proxy.rs` (new): `ProxySetting::from_env` (first non-empty of `HTTPS_PROXY`, `https_proxy`, `ALL_PROXY`, `all_proxy`; `HTTP_PROXY`/`NO_PROXY` ignored), hand-written URL parsing (`http://` or no scheme = CONNECT on port 80, `socks5://`/`socks5h://` = SOCKS5 on port 1080, anything else is `Invalid(variable)`), percent-decoded credentials held in `Zeroizing`, redacting `Debug`. `tunnel()` does the HTTP CONNECT handshake (Basic auth, reply read one byte at a time up to `\r\n\r\n`, 8 KiB cap, so a STARTTLS greeting is never consumed) and SOCKS5 (offers only 0x02 with credentials, only 0x00 without; RFC 1929; DOMAINNAME; reply length-checked per ATYP). The Account host is rejected (`connection_failed`) unless it is a plain host name or IP address, so CR/LF can never reach the request. No proxy crate was added; the only new dependency is `webpki-roots`.
- `src/imap.rs`: `Network` gains a `proxy: ProxySetting` (its `Debug` is redacted) and `with_proxy_from_env`; `login` tunnels through the proxy when one is set, and returns `ImapError::InvalidProxy { variable }` before any connection when the setting is invalid; `chosen_trust` picks the platform verifier for direct connections, the webpki built-in roots in proxy mode, and the test-only `OnlyRoots` hook over both. New `ImapError::ProxyRefused { host, port }`.
- `src/core.rs`: `execute_reporting` derives the run's `Network` from the `env` map (so `agent_mail::run` and every command, including the live login check of `mandate create`, get it); new `CoreError::ProxyRefused` (`proxy_refused`, with `account`, `host` and a message naming `host:port` and asking the human to allow the host in the sandbox's allowed domains) and `CoreError::InvalidProxy` (`invalid_proxy`, naming the variable, never its value); `ErrorBody` gains an optional `host` field.
- `src/create.rs`: login-check failure text for the two new connector errors.
- Mapping choices to check: CONNECT 2xx = tunnel, 4xx (e.g. 403, 407) = `proxy_refused`, anything else (5xx, malformed, over 8 KiB) = `connection_failed`. SOCKS5 reply 0x02, "no acceptable method" (0xFF) and a failed RFC 1929 login = `proxy_refused`; other non-zero replies = `connection_failed`.
- `tests/support/proxy.rs` (new): in-process CONNECT and SOCKS5 proxies on 127.0.0.1 that record targets, `Proxy-Authorization`, offered methods, address type and RFC 1929 credentials, and can refuse a host. `tests/proxy.rs` (new). README "Sandbox and proxies" section; CLAUDE.md layout lists `proxy.rs`, `tests/proxy.rs` and `tests/support/proxy.rs`.

**Commits:** 4c9d926
**Tests:** `cargo test`: every suite `ok`, 280 passed, 0 failed, 2 ignored (the opt-in keychain and live Gmail tests). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | `tests/proxy.rs::ac1_list_through_connect_proxy_implicit_tls`, `ac1_list_through_connect_proxy_starttls` (also `ac1_lowercase_https_proxy_is_used`) | pass |
| AC2 | `tests/proxy.rs::ac2_list_through_socks5_proxy` (socks5 implicit TLS, socks5h STARTTLS), `ac2_https_proxy_wins_over_all_proxy` | pass |
| AC3 | `tests/proxy.rs::ac3_connect_proxy_credentials_are_sent_and_never_shown`, `ac3_socks5_proxy_credentials_are_sent_and_never_shown`, `ac3_credentials_never_shown_when_the_proxy_refuses`, `ac3_invalid_proxy_url_names_the_variable_only` | pass |
| AC4 | `tests/proxy.rs::ac4_proxy_refusal_names_the_host` (CONNECT 403 and SOCKS5 0x02) | pass |
| AC5 | `tests/proxy.rs::ac5_without_proxy_variables_connections_are_direct` (no variables; empty `HTTPS_PROXY`/`ALL_PROXY` plus `HTTP_PROXY` and `NO_PROXY`) | pass |
| AC6 | `tests/proxy.rs::ac6_proxy_mode_rejects_a_certificate_not_chained_to_a_built_in_root`; unit tests `src/imap.rs::tests::proxy_mode_trusts_the_built_in_roots`, `direct_mode_keeps_the_platform_verifier` (also `test_roots_win_over_the_built_in_roots`, `debug_output_hides_the_proxy`) | pass |

Extra tests: `unreachable_proxy_is_connection_failed`, `malformed_connect_reply_is_connection_failed` (a reply that never ends its headers hits the 8 KiB cap), `mandate_create_login_check_goes_through_the_proxy`, and URL parsing and CR/LF host rejection unit tests in `src/proxy.rs::tests`.

**Review findings:** none (round 1).
