VERDICT: PASS

## Findings
- [follow-up] src/proxy.rs: `connect_handshake` maps every 4xx (e.g. 404, 429) to `proxy_refused`, and SOCKS5 maps "no acceptable method" (0xFF) and auth failure to Refused too. This is documented in the README and reasonable for v1; a later job could narrow it if agents get misleading "allow this host" advice for non-allowlist 4xx.

## Checked
- Read: full diff `git diff main...factory/015-sandbox-proxy` (src/proxy.rs whole, src/imap.rs, src/core.rs, src/create.rs, src/lib.rs, Cargo.toml, README.md, CLAUDE.md); tests/proxy.rs ACs 4 and 5 plus the test list; confirmed `create::run` receives the context rebuilt in `execute_reporting`, so the live login check uses the env-derived proxy.
- Checked: URL parsing (first non-empty var, schemes, ports, percent-decoding, Zeroizing), CRLF/space host rejection before sending, bounded 8 KiB byte-at-a-time CONNECT reply, length-checked SOCKS5 replies, redacting Debug on Proxy/ProxySetting/Network, `OnlyRoots` winning over built-in roots, invalid proxy returning before any connect.
- Ran: `cargo test` all suites green (71 unit, tests/proxy.rs 15 passed, no failures); `cargo clippy -- -D warnings` clean; `cargo fmt --check` clean.
- Criteria: AC1 `ac1_*` (implicit, STARTTLS, lowercase); AC2 `ac2_*` (SOCKS5, precedence); AC3 `ac3_*` (CONNECT, SOCKS5, refusal, invalid URL; secret forms absent from stdout/stderr/audit); AC4 `ac4_proxy_refusal_names_the_host` (both protocols, host field, audit reason); AC5 `ac5_*` (empty vars, HTTP_PROXY ignored); AC6 `ac6_*` integration plus unit tests `proxy_mode_trusts_the_built_in_roots` and `direct_mode_keeps_the_platform_verifier`. Each would fail without the change.
