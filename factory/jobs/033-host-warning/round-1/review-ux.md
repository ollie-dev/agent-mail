VERDICT: PASS

## Findings
(none)

## Checked
- Read: spec.md, the full diff of src/create.rs, src/provider.rs and tests/create.rs stat, build.md claims checked against code.
- Ran: `grep GMAIL|looks_like_gmail src/create.rs` shows nothing; `cargo test --test create --test export` in the worktree: 39 and 21 passed, including the unchanged Gmail warning tests. Warning text is the same format as the spec, login and host go through escape_controls, domain is the exact text after the last `@` (unit test covers gmail.com.evil.test, no-@, a@b@web.de, cox/yahoo shared host).
