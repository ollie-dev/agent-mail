VERDICT: PASS

## Findings
- [nit] src/create.rs:95-101 (carried from round 2): when `reveal` fails, the result of `context.store.delete(&record.key)` is thrown away, and the message still says nothing was stored. If the delete also fails, an orphaned sealed record stays in the Keychain. No secret leaks, because nobody holds the token and so nobody can unseal it. The message is still wrong in that case. Report the delete failure honestly.
- [nit] tests/keychain.rs:36: the add-only check calls `put` with `b"replacement"` under a live key. That is the right test. If an older build without add-only were ever run against this test, it would overwrite the item. The test deletes the item afterwards anyway, so this is harmless.

Round 2 had no blocking items. The human note (2026-10-21, "add OPENCODE, make put add-only") is resolved:
- `OPENCODE` is in `SANDBOX_INDICATORS` (src/create.rs:37), and AC8's test now checks each indicator separately.
- `KeychainStore::put` (src/store.rs:88-99) now calls `SecItemAdd` directly through `ItemAddOptions`, with class generic-password, service `agent-mail` and account = base64url store key. The location is left unset, so the item goes to the default (login) file keychain, as before. No `kSecAttrAccess` is passed, so the new item's access list still trusts only the creating binary. `errSecDuplicateItem` now maps to `StoreError`. `create` then reports `store_unavailable` and shows no token (src/create.rs:81-86). So an existing item and its access list can never be replaced or updated by `put`. A second `put` was the round-1 concern about items with a different access list.
- `InMemoryStore::put` follows the same add-only rule. `tests/sealing.rs::store_put_is_add_only` covers it.
- New dependency: `core-foundation` 0.10, macOS only. It is already in Cargo.lock at 0.10.1 as a dependency of security-framework, so no new code enters the build. It is used only for `CFData`.
- The `put` error carries no detail, so no key, value, token or Credential can reach output through it.
- `cargo test`: every suite passes (lib 8, cli 10, create 20, keychain 1 ignored, list 15, project 5, sealing 7).
