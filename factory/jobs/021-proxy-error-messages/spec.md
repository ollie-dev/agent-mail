# Proxy error messages: narrow `proxy_refused`, add `proxy_auth_failed`, proxy hint in `whoami`

## Goal
When a call goes through the sandbox proxy and fails, the Agent gets the right next step. `proxy_refused` ("ask the human to allow this host") is returned only when the proxy's allowlist refused the host. Rejected or missing proxy credentials get their own error, `proxy_auth_failed`. Any other proxy refusal is `connection_failed`. `whoami` also shows which proxy variable is in effect and whether it holds a usable proxy URL, so an Agent can spot a broken proxy setting before it makes a call that connects. This covers follow-ups F8 and F10 in `factory/follow-ups.md`.

## Context
- Stack: Rust, one binary crate `agent-mail`. Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- `src/proxy.rs`: `TunnelError` gets a third variant for authentication failures, next to `Connection` and `Refused`. The mapping becomes:
  - `connect_handshake`: 403 is `Refused`, 407 is the new auth variant, and every other non-2xx status (including other 4xx such as 400, 404 and 429) is `Connection`.
  - `socks5_handshake`: method choice `0xFF` and a non-zero RFC 1929 status are the auth variant. CONNECT reply `0x02` stays `Refused`.
  - `Proxy` (or `ProxySetting::Through`) learns the name of the variable it was read from, so errors can name the variable. They never show its value.
- `src/imap.rs`: a new `ImapError` variant for proxy authentication failures, carrying the variable name. `login` maps the new `TunnelError` variant to it. `Network` exposes the proxy setting (variable name and whether it is usable, never the value) so `whoami` can report it.
- `src/core.rs`:
  - a new `CoreError::ProxyAuthFailed { account, host, port, variable }` with code `proxy_auth_failed` and a short message, mapped in `connector_error`.
  - `WhoamiResult` gets an optional `proxy` field.
  - `whoami` fills it from `context.network`, which `execute_reporting` has already built from `env`.
- `src/create.rs` (`failure_reason`): one line for the new `ImapError` variant.
- `src/lib.rs`: `render_whoami` gets a `proxy` line, and the `Whoami` help text mentions `proxy`.
- `src/mcp.rs`: the `whoami` tool description mentions `proxy`. Errors already pass through `CoreError::body`.
- `tests/support/proxy.rs`: the fake proxies learn to require credentials and to answer a fixed CONNECT status. For example, `require_credentials(user, password)`:
  - CONNECT answers `407 Proxy Authentication Required` when `Proxy-Authorization` is missing or wrong.
  - SOCKS5 answers method `0xFF` when `0x02` is not offered, and RFC 1929 status `0x01` for wrong credentials.

  Also `answer_status(code)` makes CONNECT reply with that status line. All values are fake.
- Tests: `tests/proxy.rs` (the proxy errors and `mandate create` through a proxy), `tests/tokens.rs` (`whoami`), `tests/mcp.rs` (MCP parity), `tests/plugin.rs` (SKILL.md and README checks), and unit tests in `src/proxy.rs`.
- Docs: `README.md` ("Sandbox and proxies" error list and the `whoami` paragraph) and `plugin/skills/agent-mail/SKILL.md` (error list).

## Acceptance criteria
In the criteria below, the Account is `LOGIN` on `localhost:<fake IMAP port>`, the Mandate grants `list` on `INBOX`, and "the call" is `agent-mail list` run in-process with the given proxy variable.

1. AC1: Allowlist refusals stay `proxy_refused`.
   - Setup: a CONNECT proxy in `HTTPS_PROXY` answers `403 Forbidden`, or a SOCKS5 proxy in `ALL_PROXY` answers CONNECT reply `0x02`.
   - Expected: the call exits 1 with error `code` `proxy_refused`, `account` `LOGIN` and `host` `localhost`. The message contains `localhost:<port>`, "ask the human" and "allowed domains".
   - The fake IMAP server sees 0 connections. The audit line has `outcome` `allowed` and `reason` `proxy_refused`. The existing `ac4_proxy_refusal_names_the_host` keeps passing.
2. AC2: A CONNECT `407` is `proxy_auth_failed`.
   - Setup: a CONNECT proxy in `HTTPS_PROXY` (no credentials in the URL) requires credentials and answers `407 Proxy Authentication Required`.
   - Expected: the call exits 1 with empty stdout and error `code` `proxy_auth_failed`, `account` `LOGIN` and no `host` key.
   - The message names `HTTPS_PROXY`, contains "ask the human" and does not contain "allowed domains". The message is at most 300 characters.
   - The fake IMAP server sees 0 connections. The audit line has `outcome` `allowed` and `reason` `proxy_auth_failed`.
3. AC3: Wrong CONNECT credentials are `proxy_auth_failed` and never shown.
   - Setup: the `HTTPS_PROXY` URL carries a user and password the proxy rejects with `407`.
   - Expected: both `list` and `--text list` exit 1 with `proxy_auth_failed`.
   - Nothing in stdout, stderr or the audit log holds the proxy user, the password (plain, percent-encoded or Base64 `user:password`), the proxy's address or the agent token. This is the existing `assert_no_proxy_secrets` check.
4. AC4: SOCKS5 "no acceptable method" is `proxy_auth_failed`.
   - Setup: a SOCKS5 proxy in `ALL_PROXY` (no credentials in the URL) requires username/password and answers method `0xFF`.
   - Expected: the call exits 1 with `code` `proxy_auth_failed`, `account` `LOGIN`, and a message naming `ALL_PROXY` and not containing "allowed domains".
   - The fake IMAP server sees 0 connections.
5. AC5: SOCKS5 rejected credentials are `proxy_auth_failed` and never shown.
   - Setup: the `ALL_PROXY` SOCKS5 URL carries credentials the proxy rejects with RFC 1929 status `0x01`.
   - Expected: `list` and `--text list` exit 1 with `proxy_auth_failed`.
   - None of the secrets listed in AC3 appear in stdout, stderr or the audit log.
6. AC6: Other CONNECT 4xx answers are `connection_failed`, not `proxy_refused`.
   - Setup: a CONNECT proxy answers `400 Bad Request`, `404 Not Found` and `429 Too Many Requests` in turn.
   - Expected: each call exits 1 with `code` `connection_failed` and a message without "allowed domains". The audit `reason` is `connection_failed`.
   - Unit test in `src/proxy.rs`: status 403 maps to `Refused`, 407 to the auth variant, and 400, 404, 429 and 502 to `Connection`.
7. AC7: `--text` output.
   - Setup: the 407 proxy of AC2.
   - Expected: `--text list` writes stderr starting with `error: proxy_auth_failed: ` and exits 1.
8. AC8: MCP parity.
   - Setup: the 407 proxy of AC2 in the MCP server's environment.
   - Expected: the `list` tool returns an error result whose error JSON equals the CLI's for the same call (`code` `proxy_auth_failed`, same `account` and message).
9. AC9: `mandate create` through a proxy that rejects credentials.
   - Setup: the guided flow's live login check goes through the 407 CONNECT proxy in `HTTPS_PROXY`.
   - Expected: the reported failure reason mentions the proxy credentials and `HTTPS_PROXY`. It does not say to allow the host, and nothing is stored.
10. AC10: `whoami` with a usable proxy.
    - Setup: `HTTPS_PROXY=http://<user>:<password>@127.0.0.1:<port>` and one valid token.
    - Expected: `whoami` exits 0 and stdout is the existing JSON with `"proxy":{"variable":"HTTPS_PROXY","usable":true}` added after `invalid_tokens`.
    - No connection is made to the proxy or the IMAP server, and no audit line is written. stdout and stderr hold neither the proxy's address, user or password nor the token.
11. AC11: `whoami` with an invalid proxy variable.
    - Setup: `ALL_PROXY=ftp://<user>:<password>@127.0.0.1:1`, nothing else set.
    - Expected: `whoami` exits 0 with `"proxy":{"variable":"ALL_PROXY","usable":false}`. The output contains neither `ftp://`, `127.0.0.1` nor the credentials.
    - With `--text`, stdout has the line `proxy ALL_PROXY is not a usable proxy URL; ask the human to fix or unset it`. With `HTTPS_PROXY` usable, the `--text` line is `proxy HTTPS_PROXY`.
12. AC12: `whoami` without a proxy.
    - Setup: no proxy variable, or all four empty, or only `HTTP_PROXY` set.
    - Expected: the `whoami` JSON has no `proxy` key and no `--text` proxy line. The existing exact-output tests in `tests/tokens.rs` pass unchanged.
13. AC13: The MCP `whoami` tool matches the CLI.
    - Setup: the proxy environment of AC10, then that of AC11.
    - Expected: the tool's result JSON equals `agent-mail whoami` output, including `proxy`.
14. AC14: Docs.
    - `README.md` "Sandbox and proxies":
      - the `proxy_refused` bullet names only allowlist refusals (CONNECT `403`, SOCKS5 "not allowed by ruleset") and no longer mentions credentials;
      - a `proxy_auth_failed` bullet says the proxy rejected or asked for proxy credentials, and names the variable to check;
      - other proxy refusals are said to give `connection_failed`.
    - The README `whoami` paragraph and the `whoami` help text mention `proxy`.
    - `plugin/skills/agent-mail/SKILL.md` lists `proxy_auth_failed` with the next step "tell the human".
    - A test in `tests/plugin.rs` checks that README.md and SKILL.md contain `proxy_auth_failed`. Every new or changed paragraph is at most 300 characters.
15. AC15: `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` all pass.

## Out of scope
- A proxy hint in `mandate list` (F10 also mentions it). Only `whoami` reports the proxy setting.
- Reporting the proxy setting in any error other than `invalid_proxy`, `proxy_refused` and `proxy_auth_failed`.
- Distinct codes for other SOCKS5 reply codes (`0x01`, `0x03` to `0x08`) or CONNECT 5xx. They stay `connection_failed`.
- Retry or back-off on `429`.
- `NO_PROXY`/`HTTP_PROXY` support, proxy authentication schemes other than Basic and RFC 1929, and any check in `whoami` that actually connects to the proxy.
- Other open follow-ups (F11 and later).

## Notes for reviewers
- Security:
  - The proxy URL is untrusted input and may carry proxy credentials. The new error, the `whoami` `proxy` field, its `--text` line, the `mandate create` failure line, `Debug` output and the audit log may name only the variable (`HTTPS_PROXY`, `https_proxy`, `ALL_PROXY`, `all_proxy`). They never show the value, the proxy host or port, or the credentials. AC3, AC5, AC10 and AC11 check this.
  - `whoami` must stay connection-free: it reports only whether the variable parses (AC10).
  - The proxy's reply (status line, reason phrase) is untrusted and must not be echoed into messages.
- UX:
  - The Agent is the user, reading JSON errors from the CLI or MCP tools. The flow is: the Agent calls `whoami` first and sees whether the proxy setting is usable. A connecting call then fails with a code whose next step fits the cause:
    - `proxy_refused`: allow the host;
    - `proxy_auth_failed`: the human checks the proxy credentials in the named variable;
    - `connection_failed`: a network or proxy fault, so retry or tell the human.
  - Inside the Claude Code sandbox the proxy variable is set by the host, not the human. The `proxy_auth_failed` message should say to tell the human, not to edit the variable.
  - Not carrying `host` on `proxy_auth_failed` is deliberate: `host` on `proxy_refused` means "the host to allow".
- UI:
  - JSON error bodies and `--text` `error:` lines.
  - The `proxy` key in `whoami` JSON and its `--text` line.
  - The `mandate create` failure line, the `whoami` help text, the MCP `whoami` description, and the README and SKILL.md bullets.
  - All text keeps the CLAUDE.md rule: short, each thing said once, at most about 300 characters per paragraph or line.
