VERDICT: PASS

## Findings

None new. Round 1 had no `[blocking]` or `[fix-now]` items. Its one `[follow-up]` is about the elevated copy reusing the caller's host exe and the user-writable `PSModulePath` (install.ps1:349, uninstall.ps1:219). It is already in decision.md's follow-ups, and this round leaves it unchanged.

## Checked
- Read: `round-1/review-security.md`, `human-notes.md` (the 2026-10-26 note on `tls_certificate`) and `decision.md`.
- Read: `git log main..factory/027-broker-installers`. The only new commit is 8acc80e.
- Read: the full diff of 8acc80e, in `tests/support/mod.rs` and `tests/installers.rs`. After that, the Round 2 section of `build.md`.
- tests/support/mod.rs, `test_certificates()` and `tls_config()`:
  - The CA and the leaf now have distinct CNs. The CA key is still generated per run and stays in memory: it goes into `Issuer` and is never returned or written to disk. Only the leaf's PKCS #8 key is returned, which the fake server needs.
  - The leaf's SANs are unchanged (`localhost`, `imap.gmail.com`). The CA's constraints are unchanged.
  - No Credential and no real secret is in the fixture.
- tests/installers.rs, the new `job027_fake_server_certificate_chains_to_the_test_ca_on_windows`:
  - It writes only the public CA and leaf DER files to a temp dir. It builds the chain with the CA in `ExtraStore` only, so it touches no certificate store and needs no rights.
  - The PowerShell it runs gets only paths the test generated, through `quoted()`.
- The `LocalMachine\Root` guard (tests/installers.rs:204-229) still removes the CA by thumbprint on drop. This round does not change it.
- Ran: `cargo test --locked --test installers`: 1 passed (the new chain test), 9 ignored (the elevated tests).
- Ran: a grep of `tests/` for `test_certificates`, `serialize_der` and `ca_key`. The CA key appears only inside `test_certificates()`, and only the leaf key is serialized.
