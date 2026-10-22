VERDICT: PASS

## Findings
None.

## Checked
- Read: full `git diff main...factory/015-sandbox-proxy` (one commit, 4c9d926), then spec.md and build.md; build.md's claims match the code.
- Read `src/proxy.rs` in full:
  - `ProxySetting::from_env`: variable order, empty counts as unset, `HTTP_PROXY`/`NO_PROXY` never read.
  - `parse`: an unknown scheme, including `https://`, is `Invalid(variable)`. A path, query or fragment is rejected. Credentials are percent-decoded into `Zeroizing` and the user must not be empty. RFC 1929 credentials are capped at 255 bytes. The proxy host must pass `is_plain_host` or be an IPv6 literal. Port 0 and ports over 65535 are rejected.
  - The hand-written `Debug` for `Proxy`/`ProxySetting` hides the address and the credentials.
  - `tunnel`: the Account host must be a plain host or IPv6 before anything is connected, so no CR/LF, space or colon can reach the CONNECT line or the SOCKS length-prefixed name. Covered by `hosts_with_line_breaks_are_never_sent`.
  - `connect_handshake`: credentials are only base64-encoded into the header, so CR/LF in decoded credentials cannot inject headers. The request buffer is `Zeroizing`. The reply is read one byte at a time with an 8 KiB cap, and the status line is parsed strictly.
  - `socks5_handshake`: offers exactly one method and checks the version on every reply. The bound-address read is bounded by ATYP (at most 255+2 bytes). Unknown ATYP fails as `connection_failed`.
  - No proxy crate was added.
- Read `src/imap.rs`:
  - `chosen_trust` and `tls_connector`: the `OnlyRoots` arm is still behind `cfg(any(test, feature = "test-support"))`. `Platform` plus `Direct` gives the platform verifier. `Platform` plus a proxy gives the webpki roots. Built-in roots are never used without a proxy.
  - `login`: `Invalid` returns before any TCP connect. TLS still runs end to end, verifying `account.host`. The Credential is sent only after TLS is up.
  - `ImapError::ProxyRefused`/`InvalidProxy` hold only the Account host/port and a `&'static str` variable name, never the URL.
  - Every `login` caller is still wrapped in `tokio::time::timeout(TIMEOUT, …)`, so a hostile proxy that stalls cannot hang the run.
- Read `src/core.rs`:
  - `execute_reporting` builds the proxy from the passed `env` map; `main.rs` fills that map from `vars_os`.
  - `CoreError::ProxyRefused`/`InvalidProxy`: the error bodies and messages name only the variable or the Account host:port.
  - `connector_error` mapping and the audit path: the audit `reason` is the error code.
- Read `src/create.rs` `failure_reason`: the host goes through `escape_controls`, and the variable name only. The live login check uses `context.network`, which now carries the env-derived proxy.
- Read `Cargo.toml`/`Cargo.lock`: the only new dependency is `webpki-roots` 1.0.10. It depends only on `rustls-pki-types`, is static data with no build script or I/O, and `cargo tree -i webpki-roots -e normal` shows only `agent-mail` uses it.
- Read `tests/support/proxy.rs` (the fake CONNECT/SOCKS5 proxies, 127.0.0.1 only) and `tests/proxy.rs`:
  - The `assert_no_proxy_secrets` checks cover stdout, stderr and audit lines for `proxyuser`, `pr@xy-secret`, `pr%40xy-secret` and the base64 form, on the success, refusal and invalid-URL paths, including `--text`.
  - The fixtures hold only obviously fake values.
- Read README "Sandbox and proxies" and the CLAUDE.md layout change: no secrets, and the behaviour described matches the code.
- Ran `cargo test`: all suites ok (tests/proxy.rs 15 passed, including the AC3 leak checks and `tests/project.rs`, which contains the release-binary marker check).
- Ran `cargo clippy --all-targets -- -D warnings`: clean.
- Ran `cargo build --release`, then `strings target/release/agent-mail | grep -ci "OnlyRoots\|trusting_only"`: 0. The test hook is not in the release binary.
- Ran a grep of the diff for `password|secret` outside the fake test constants: only docs, comments and redaction tests matched. No real credentials were committed.
