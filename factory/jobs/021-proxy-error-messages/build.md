## Round 1
**Changed:**
- `src/proxy.rs`: `TunnelError::Auth`; CONNECT status mapping moved to `status_result` (403 `Refused`, 407 `Auth`, every other non-2xx `Connection`); SOCKS5 method `0xFF` and a non-zero RFC 1929 status are `Auth`, reply `0x02` stays `Refused`. `Proxy` keeps the variable it was read from (`variable()`); `ProxySetting::summary()` gives `(variable, usable)`, never the value.
- `src/imap.rs`: `ImapError::ProxyAuthFailed { host, port, variable }`, mapped in `login`; `Network::proxy_summary()`.
- `src/core.rs`: `CoreError::ProxyAuthFailed { account, host, port, variable }`, code `proxy_auth_failed`, body without `host`, message naming the variable and "ask the human" (no "allowed domains", no proxy reply text); `WhoamiResult.proxy: Option<WhoamiProxy { variable, usable }>`, omitted when direct, filled from `context.network`.
- `src/create.rs`: failure line for the new variant (mentions proxy credentials and the variable). `src/lib.rs`: `--text` `proxy ...` line and `whoami` help paragraph. `src/mcp.rs`: `whoami` tool description mentions `proxy`.
- `tests/support/proxy.rs`: `require_credentials(user, password)` and `answer_status(code)`; `tests/support/mod.rs`: `run_logged_with` and `mcp_lines_with` (extra env vars).
- README "Sandbox and proxies" (three bullets, other refusals give `connection_failed`), new `proxy` in `whoami` paragraphs; SKILL.md `proxy_auth_failed` bullet ("Tell the human") and `proxy` in the `whoami` line. `tests/cli.rs` error samples include the new error (300-character check).

**Commits:** 4003b15
**Tests:** `cargo test` -> all suites ok, 395 passed, 0 failed (ignored opt-in tests unchanged); `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | tests/proxy.rs `ac1_allowlist_refusals_stay_proxy_refused` (+ existing `ac4_proxy_refusal_names_the_host`) | pass |
| AC2 | tests/proxy.rs `ac2_connect_407_is_proxy_auth_failed` | pass |
| AC3 | tests/proxy.rs `ac3_wrong_connect_credentials_are_proxy_auth_failed_and_never_shown` | pass |
| AC4 | tests/proxy.rs `ac4_socks5_no_acceptable_method_is_proxy_auth_failed` | pass |
| AC5 | tests/proxy.rs `ac5_socks5_rejected_credentials_are_proxy_auth_failed_and_never_shown` | pass |
| AC6 | tests/proxy.rs `ac6_other_connect_4xx_answers_are_connection_failed`; src/proxy.rs `connect_statuses_map_to_tunnel_errors` | pass |
| AC7 | tests/proxy.rs `ac7_text_output_of_proxy_auth_failed` | pass |
| AC8 | tests/mcp.rs `ac8_proxy_auth_failed_matches_the_cli` | pass |
| AC9 | tests/proxy.rs `ac9_mandate_create_through_a_proxy_rejecting_credentials` | pass |
| AC10 | tests/tokens.rs `ac10_whoami_reports_a_usable_proxy_without_connecting` | pass |
| AC11 | tests/tokens.rs `ac11_whoami_reports_an_invalid_proxy_variable` | pass |
| AC12 | tests/tokens.rs `ac12_whoami_without_a_proxy_has_no_proxy_key` (+ existing exact-output whoami tests unchanged) | pass |
| AC13 | tests/mcp.rs `ac13_whoami_tool_reports_the_proxy_like_the_cli` | pass |
| AC14 | tests/plugin.rs `job021_ac14_readme_and_skill_document_proxy_auth_failed_and_whoami_proxy` | pass |
| AC15 | `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check` | pass |

**Review findings:** none (round 1).

Notes: the README `whoami` paragraph was already over 300 characters, so the `proxy` field is documented in a new paragraph right after it (`**`proxy` in `whoami`.**`) instead of lengthening it. `CoreError::ProxyAuthFailed` keeps `host` and `port` as the spec asks, but the error body deliberately omits `host`.
