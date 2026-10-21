VERDICT: PASS

## Findings
- [nit] src/create.rs (guided): a Draft-only operation tick still forces at least one mailbox tick first; harmless, but the mailbox prompt is irrelevant for a draft-only Grant set.
- [nit] src/create.rs (add_account): the retry loop keeps the Credential in `account` clones per attempt; they are wiped on drop, so fine, just noting.

Reviewed all changed source and ran the suite: cargo test green (lib 8, cli 10, create 18, keychain 1 ignored, list 15, project 5, sealing 6), clippy -D warnings and fmt --check clean. Each AC has a covering test; the TTY and sandbox checks run before any prompt or store access; the connect override and ScriptedTerminal sit behind test-support; the token goes only to the Terminal.
