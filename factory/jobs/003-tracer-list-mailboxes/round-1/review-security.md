VERDICT: PASS

## Findings
- [nit] src/imap.rs:134 (via async-imap 0.12 `src/imap_stream.rs:51`): async-imap `log::trace!`s every encoded command, so the `LOGIN` line carrying the Credential is in the trace output. Nothing installs a logger today, so nothing leaks yet. A later ticket that adds `env_logger`/`tracing-log` (for example the audit log in ticket 06) would leak it at trace level. → Add `log = { version = "0.4", features = ["max_level_off", "release_max_level_off"] }` (or at least `max_level_debug`) to compile these calls out. Alternatively, add a test that runs the fake server with a capturing logger and asserts the Credential is absent.
- [nit] src/lib.rs:99: `--text` prints server-supplied mailbox names verbatim. Under a `*` Grant, a hostile server can embed `\n`, `\r` or ANSI escapes and forge extra lines, such as a fake account header, in output the Agent or a human reads. JSON mode escapes them correctly. → Escape or replace control characters in text rendering.
- [nit] src/mandate.rs:47, src/seal.rs:31-34,88-94: the Credential, token secret and derived keys are plain `String`/arrays and are not zeroized on drop. `zeroize` is already in the lock file through rustls. → Consider `Zeroizing`/`ZeroizeOnDrop` for `Credential` and `AgentToken`.
- [nit] tests/project.rs:166: the release-binary marker only proves that `InMemoryStore`'s `Debug` impl is absent. The `cfg` gate is the real guarantee, and it is correct (store.rs:34-38, core.rs:30, imap.rs:30). A sturdier check would be a `const`/`#[used]` static carrying the marker inside the gated module.

Checked and fine:
- **Sealing:** HKDF-SHA256 from the 256-bit token secret uses distinct info labels for the store key and the ChaCha20-Poly1305 key. The nonce is random. AAD binds the store key. `token_hash`, the Credential and the Policy sit in one ciphertext, and the hash is re-checked on open. Version, length and nonce failures all return `OpenError` without panicking.
- **Token errors:** malformed, unknown and unopenable tokens all map to the same `invalid_token`. Error bodies and `ImapError` carry no secrets. `Credential`, `AgentToken` and `SealedRecord` have redacted `Debug`.
- **Policy:** it is deny by default and checked before any connection. Matching uses the decoded UTF-8 name, and the account is compared on Grants too.
- **TLS:**
  - `TlsMode` has no plaintext variant.
  - STARTTLS is mandatory: a missing capability or a non-OK reply gives `tls_required`.
  - Bytes buffered before the upgrade are dropped, because `into_inner` discards the `ImapStream` buffer. This blocks STARTTLS command injection.
  - LOGIN runs only on the TLS stream.
  - The platform verifier is the only trust path in non-test builds. No env var or flag picks the store or the trust roots, and `main.rs` hard-wires `RuntimeStore`.
- **Injection:** the login, host and Credential come from the sealed Mandate, not from Agent input. async-imap rejects CR/LF and quotes LOGIN arguments. The UTF-7 decoder is panic-free on malformed input and falls back to the raw name losslessly.
- **Fixtures:** test fixtures are fake. The fake server records verbs only, never arguments.
- **Dependencies:** they are mainstream (rustls/ring, tokio, async-imap, serde, base64, rustls-platform-verifier). rcgen is dev-only. None adds runtime network access or file access beyond the IMAP connection and the OS trust store.
- **Tests:** `cargo test` is all green.
