VERDICT: PASS

## Findings
- None.

## Checked
- Read: full `git diff main...factory/021-proxy-error-messages`: `src/proxy.rs` (`Proxy.variable`, `summary`, `TunnelError::Auth`, `status_result`, SOCKS5 mapping), `src/imap.rs` (`ProxyAuthFailed`, `proxy_summary`, `login` mapping), `src/core.rs` (`CoreError::ProxyAuthFailed`, code, message, `connector_error`, `WhoamiProxy`, `whoami`), `src/create.rs` `failure_reason`, `src/lib.rs` (`render_whoami`, help), `src/mcp.rs` description, README and SKILL.md changes. Messages never carry the value, host or port; the proxy's reply is not echoed; `Debug` for `Proxy` stays custom.
- Ran: `cargo test`: all suites green, 0 failed; `cargo clippy -- -D warnings`: clean; `cargo fmt --check`: clean.
- Criteria: AC1 and AC6 via `status_result` unit test and `ac6_*` in tests/proxy.rs; AC2, AC3, AC4, AC5, AC7 via `ac2_*` to `ac7_*` in tests/proxy.rs; AC8 `ac8_proxy_auth_failed_matches_the_cli` in tests/mcp.rs; AC9 `ac9_mandate_create_through_a_proxy_rejecting_credentials`; AC10 to AC12 in tests/tokens.rs; AC13 `ac13_whoami_tool_reports_the_proxy_like_the_cli`; AC14 `job021_ac14_*` in tests/plugin.rs. Each would fail without the change (new code and variant).
