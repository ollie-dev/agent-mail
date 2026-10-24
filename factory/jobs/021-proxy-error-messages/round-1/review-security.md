VERDICT: PASS

## Findings
None. The new error, the `whoami` `proxy` field, its `--text` line and the `mandate create` failure line carry only the `&'static str` variable name, taken from the fixed list in `ProxySetting::from_env`. They never carry the proxy URL, its host or port, or its credentials. `Proxy`'s `Debug` stays redacted. The proxy's status line and reason phrase are reduced to a numeric code in `status_result` and never reach a message. `whoami` reads only the already-parsed `Network` setting and opens no connection. The change adds no dependencies (`base64` in the fake proxy was already a dependency) and does not touch access policy.

## Checked
- Read: the full diff `git diff main...factory/021-proxy-error-messages` (src and tests, README, SKILL.md).
- Read in src/proxy.rs: the `Proxy` struct and its redacted `Debug` impl, `parse(value, variable)`, `ProxySetting::from_env`/`summary`, `status_result` (403 Refused, 407 Auth, all else Connection, reason phrase discarded), and the `socks5_handshake` 0xFF and RFC 1929 status mapping.
- Read in src/imap.rs: `ImapError::ProxyAuthFailed`, the `login` mapping (`proxy.variable()` only) and `Network::proxy_summary`.
- Read in src/core.rs: `CoreError::ProxyAuthFailed` and its body/message (variable and account only, no `host`), the `ProxyRefused` body for comparison, the `connector_error` mapping, `whoami` filling `WhoamiProxy` from `context.network`, and the `WhoamiResult`/`WhoamiProxy` serialisation.
- Read in src/create.rs: the `failure_reason` line (variable only). In src/lib.rs: `render_whoami` and the help text. In src/mcp.rs: the tool description.
- Read tests/support/proxy.rs (`require_credentials` and `answer_status`, fake values only), tests/support/mod.rs (`run_logged_with`, `mcp_lines_with`), and the new tests in tests/proxy.rs (AC1 to AC7 and AC9, including `assert_no_proxy_secrets` and `assert_no_proxy_address` over stdout, stderr and audit) and tests/tokens.rs (AC10 to AC12 secret and no-connection checks). The fixtures `proxyuser`, `pr@xy-secret` and `proxy-secret` are obvious placeholders.
- Read the README and SKILL.md additions: no values or credentials, only variable names.
- Read build.md after the diff. Its claims match the code.
- Ran: `cargo test`: every suite ok, 0 failed. `cargo clippy -- -D warnings`: clean.
