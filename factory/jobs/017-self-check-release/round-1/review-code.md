VERDICT: PASS

## Findings
- [follow-up] src/store.rs: `GuardedStore::keys` is an inherent method on `GuardedStore<KeychainStore>` that nothing calls today; if the core never enumerates through `keys()`, drop it, or route any future use through it. No fix needed now.

## Checked
- Read: the full `git diff main...factory/017-self-check-release` for src (`selfcheck.rs`, `store.rs`: `GuardedStore`, `Gate`, `check`, `StoreError` enum; `core.rs`: `From<StoreError>`, mapping, message; `create.rs` early check; `main.rs`; `lib.rs`), `Cargo.toml` (`debug-assertions = false` pinned), `install.sh`, `scripts/sign-macos.sh`, `scripts/verify-release.sh`, `.github/workflows/release.yml` (actions pinned by SHA, minimal permissions), `.github/release-notes.md`.
- Security points: the guard runs before every get/put/delete/entries and never forwards on failure. The check is lazy and cached, with no env/flag/feature switch. `csops` is called with a u32 buffer and its exact size, and a nonzero return gives `None`, which fails. The installer verifies the checksum before extracting and extracts only the `agent-mail` member.
- Ran: `cargo test`: all suites ok, 0 failed. `cargo clippy -- -D warnings`: clean. `cargo fmt --check`: clean. `cargo test --test selfcheck -- --ignored`: 4 passed (release-binary AC1, AC2, AC4 and AC6 tests on arm64 macOS).
- Criteria: AC1, AC2, AC4 local and AC6 are covered by the ignored release-binary tests, which passed. AC5, AC7 and the AC3 workflow text are covered by `tests/release.rs`. AC8 is covered by the `selfcheck.rs` unit tests plus `tests/selfcheck.rs` in-process guard tests (e.g. `ac8_failed_guard_never_forwards_a_call`), which would fail without the guard. AC9 is covered by the debug unit test and the binary test. AC10 docs: README and CLAUDE.md changes are in the diff.
