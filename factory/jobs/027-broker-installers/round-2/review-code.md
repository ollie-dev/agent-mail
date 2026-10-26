VERDICT: PASS

## Findings
- [follow-up] tests/installers.rs: the elevated AC2 and AC6 tests still cannot be run in this review (shell not elevated). The first Windows CI run must confirm that the TLS login now chains to the test CA in LocalMachine\Root; the new chain test only covers the fixture.

## Checked
- Read: round-1 review (no blocking or fix-now items, so nothing to re-verify); human-notes.md (tls_certificate note); the round-2 commit 8acc80e in full (`tests/support/mod.rs`: `test_certificates`, `tls_config`, `TEST_CA_NAME`, `TEST_LEAF_NAME`; `tests/installers.rs`: `job027_fake_server_certificate_chains_to_the_test_ca_on_windows`); the Round 2 section of build.md.
- The fix matches the note: the CA gets CN=agent-mail test CA, the leaf CN=localhost, so the leaf is no longer self-issued. The SANs (localhost, imap.gmail.com) are unchanged, so the other fake-server tests are unaffected. The new test builds the chain with X509Chain, with the CA only in ExtraStore, and would fail with the old default DNs.
- Ran: `cargo test` in the worktree: every `test result` line ok, 0 failed (the fake-server-based suites all pass; installers: 1 passed, 9 ignored).
- Criteria: AC2 and AC6 (the Mandate stored through the Broker over TLS to the fake server) are the ones this round affects; the fixture fix has a default-run test, the elevated tests run only on CI.
