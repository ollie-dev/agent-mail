VERDICT: PASS

## Findings
- [nit] src/create.rs (guided): a file Account with a Grant but where the human drops it leaves its Grants unused, which is correct; no change needed. Only note: the name prompt default comes straight from the file, so display escaping depends on the Terminal text prompt; consider escaping it if InquireTerminal prints the default verbatim.
- [nit] src/export.rs: `from_toml` validates keys by hand and then also derives `deny_unknown_fields`; the duplication is deliberate (no value echo) and documented, so acceptable.

All ACs have tests; `cargo test` is green (all suites pass). Export is O_EXCL, errors never quote file values, no Credential field in the export types, refusal checks precede file loading.
