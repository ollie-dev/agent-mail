VERDICT: PASS

## Findings
None.

## Checked
- Read: the full diff `git diff main...factory/032-unsupported-providers` (data/providers.json, src/create.rs, src/provider.rs, tests/create.rs, tests/provider.rs); the single commit dce838a.
- Read: `src/provider.rs` `Provider` (now `imap`/`credential` as `Option`, new `unsupported`), `connection()`, `label()`, `parse()` (unsupported XOR imap+credential, empty/over-300 reason rejected, `deny_unknown_fields` kept), `all()` (still `include_str!`, no runtime read), `sorted()`, `by_domain()`.
- Read: `src/create.rs` `ask_provider` loop and `add_account`: an unsupported choice only reaches `terminal.message(reason)` and loops back; `connection()` is the sole gate and requires `unsupported == None` plus both settings, so no Credential prompt, IMAP connect or store write can follow an unsupported pick. Cancel still maps through `cancelled`. Reasons are compiled-in constants, not untrusted input.
- Ran: grep for `provider::`, `by_domain`, `.imap`, `.credential`, `.unsupported` across `src/`: `by_domain` has no callers outside provider.rs, and `create.rs` is the only consumer of Provider, so the Option change opens no other path (export, MCP, Broker untouched).
- Ran: grep of the added test lines for password/secret/token: only the existing placeholder `CREDENTIAL` ("fake-app-password-123" in tests/support/mod.rs); AC4 cancel test asserts no Hidden prompt and empty `store.entries()`.
- Checked: no Cargo.toml/Cargo.lock change, so no new dependencies.
- Ran: `cargo test --test provider --test create`: 30 and 12 passed, 0 failed.
