VERDICT: PASS

## Findings
- [nit] src/lib.rs:111: `fn reference` is a one-line wrapper around `crate::read::parse_reference`; `value_parser = read::parse_reference` could be used directly.
- [nit] src/core.rs (CoreError body for StaleReference/MessageNotFound): the `matches!` inside an `if let ... | ...` block re-tests the variant; two separate message arms would read more directly.
- [nit] src/read.rs `attachment`: an untyped non-message leaf defaults to `text/plain`, which is right per RFC 2045. A short unit test for it would pin the behaviour.

Checked: `cargo test` is green in the worktree, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean. Each AC1-AC8 has a test in tests/read.rs. The Policy check and Account match run before any connection. The UIDVALIDITY check follows EXAMINE and precedes the fetch. The fetch uses BODY.PEEK[]. The audit entry holds the reference only. `--text` output escapes controls. The extra `invalid_reference` error is reasonable and covered by the next-step test.
