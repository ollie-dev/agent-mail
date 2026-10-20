VERDICT: PASS

## Findings
- [nit] src/core.rs (list): a mailbox name that fails modified UTF-7 decoding falls back to the raw server string and is then matched against Grants. That is lossless and safe, since a Grant can only match it by exact raw name. A short test of this fallback would document the choice.
- [nit] src/store.rs: `use std::fmt;` sits in its own group after `use std::sync::Mutex;` inside `mod memory`. Merge it into the preceding `std` imports.
- [nit] src/lib.rs: `render_outcome` uses `out.push_str(&format!(..))`. `writeln!` or `push_str` with `concat` would avoid the temporary allocation.

Verified:
- `cargo test`: all suites green (lib 2, cli 10, list 14, project 5, sealing 6).
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.
- Security notes hold:
  - HKDF-SHA256 derives the store key and the seal key under distinct labels.
  - ChaCha20-Poly1305 is bound to the store key through AAD.
  - The token hash, Credential and Policy sit in one ciphertext.
  - `TlsMode` has no plaintext variant.
  - STARTTLS is mandatory, and LOGIN is sent only over the TLS stream.
  - `Debug` is redacted on `Credential`, `AgentToken` and `SealedRecord`.
  - The in-memory store and `TlsTrust::OnlyRoots` are gated by `cfg(any(test, feature = "test-support"))`.
  - `test-support` is not in `default`.
  - `main.rs` hard-wires `RuntimeStore`.
- The Policy check happens before any connection.
- Malformed, unknown and unopenable tokens all map to `invalid_token`.
- The UTF-7 decoder returns errors and does not panic.
- Every AC has a test, and the scope matches the spec.
