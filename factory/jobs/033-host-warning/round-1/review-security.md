VERDICT: PASS

## Findings
None.

## Checked
- Read: the full diff `git diff main...factory/033-host-warning` (src/create.rs, src/provider.rs, tests/create.rs; one commit b305fd2). No Cargo.toml/Cargo.lock change, so no new dependencies.
- Read: `src/create.rs` `add_account` in full. Login and host still go through `escape_controls` before they appear in the warning, in the hidden prompt and in the "Checking"/"failed" lines. Only the trusted compiled-in Provider `name` and IMAP host go into the message unescaped. The Credential is read only after the warning, and nothing in the change reads, logs or formats it. The check runs on all three paths: chosen Provider, "Custom server…", and `--from` (the `saved` branch).
- Read: `src/provider.rs` `by_domain` and the new `expected_for_login`. The domain is the text after the last `@` (`rsplit_once`) and must equal a listed domain exactly (trimmed, lowercased), so it is never a suffix match: `someone@gmail.com.evil.test` gives None. Unsupported Providers give None through `connection()`. Host comparison is `eq_ignore_ascii_case`. The edge cases left (e.g. a trailing-dot domain like `max@web.de.`) only skip an informational warning, the same as the removed Gmail check. Nothing new grants access.
- Read: `src/terminal.rs` `escape_controls`/`is_format`, which escape control and Cf characters.
- Read: tests/create.rs job033 tests, including AC5 (no raw ESC/BEL in the warning) and the suffix-match test. Fixtures hold no real credentials.
- Read: build.md, checked against the code above. Its claims match.
- Ran: `git diff main...factory/033-host-warning | grep -inE "password|secret|token|app.?pass"`: no matches.
- Ran: `cargo test --test create job033`: 9 passed. `cargo test --lib provider`: 3 passed, including `expected_host_follows_the_exact_domain_after_the_last_at`. `cargo test --test export`: 21 passed, including the existing Gmail warning tests.
