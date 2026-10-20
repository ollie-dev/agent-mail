VERDICT: PASS

## Findings
- [nit] src/seal.rs:55 (`mint`): `URL_SAFE_NO_PAD.encode(*secret)` and `secret: *secret` each copy the 32-byte array by value, and those stack copies are not wiped. The copy stored in the token is wiped on drop, so this is a small residue only. → Optionally encode from `secret.as_ref()` and `copy_from_slice` into the struct, as `parse` already does.
- [nit] src/seal.rs:228 (`open`): `serde_json::from_slice` builds the Credential `String` directly. A JSON-escaped credential could pass through a serde scratch buffer that is not wiped. This copy is inside a third-party crate, like the ring and async-imap copies the builder lists, so it is acceptable for now.
- [nit] tests/project.rs: the positive-control build and the default build share `target/release-check` with different feature sets, so every test run relinks both binaries. This is correct (the default build runs last and its output is the one checked) but slow. A separate subdirectory per feature set would keep both cached.
- [nit] Cargo.toml (carried over from round 2): `log` `max_level_off` is what keeps async-imap's LOGIN trace compiled out. Note this in the audit-log ticket (06) so nobody lowers the cap by accident.

## Round 2 nits (requested by the human in human-notes.md)
- Zeroization is resolved. `Credential` and `AgentToken` zeroize on drop through manual `Zeroize`/`Drop`/`ZeroizeOnDrop` impls. `String::zeroize` wipes the full capacity. The derived seal key, the decoded token bytes, the plaintext in `seal` (pre-sized by a counting pass, so it never reallocates) and the decrypted buffer in `open` are all `Zeroizing`. Two unit tests cover the token and the Credential. Adding `Drop` to `Credential` doesn't break serde or `Clone`, and no code moves fields out of it.
- The release marker is resolved. `#[used] static MARKER_BYTES` sits inside `mod memory`, which is still gated by `#[cfg(any(test, feature = "test-support"))]`. The release check now has a positive control: a `test-support` build must contain the marker, so the absence check on the default build means something.
- The log cap and the `--text` escaping were already resolved in round 2 and are unchanged.

## Checked in the round-3 diff (3814838)
- New dependency: `zeroize` 1, with no derive feature. It was already in the lock file through rustls/ring, the lock change only adds it to agent-mail's own dependency list, and it adds no new capability.
- Nothing new is logged, printed or returned. Both `Debug` impls still redact. Test fixtures still use only the fake values.
- Access policy, the store lookup and the TLS paths are unchanged from round 2.
- `cargo test` passes every suite (lib 6, cli 10, list 14, project 5, sealing 6). `cargo clippy -- -D warnings` and `cargo fmt --check` are clean.
