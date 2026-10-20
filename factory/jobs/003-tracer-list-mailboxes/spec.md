# Tracer bullet: an Agent lists its granted mailboxes

## Goal
An Agent runs `agent-mail list` with `AGENT_MAIL_TOKEN` set and gets back only the mailboxes its Mandate's Policy grants on the Account, as JSON by default or human-readable with `--text`. This first end-to-end path sets up every layer later tickets build on: the core as the single entry point, the sealed Mandate record, the Mandate store port with a test-only in-memory store, TLS-only IMAP, deny-by-default Policy enforcement, and an in-process fake IMAP server for tests.

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`. Both lints must stay clean, and every existing test in `tests/cli.rs` and `tests/project.rs` must stay green. In particular, nothing in `--help` (including the new `list` subcommand's help text) may contain the word "mandate". A bare `agent-mail` call must still exit 2 with the usage hint, and `build.rs` must not be added.
- Source ticket: `.scratch/agent-mail-v1/issues/02-tracer-list-mailboxes.md`, from `.scratch/agent-mail-v1/spec.md` (sections "Architecture", "Mandate model and sealing", "Policy and Grants", "Mail operations", "Network and TLS", "Testing Decisions"). ADRs: 0002 (no background service; each call enforces the Policy in-process), 0003 (immutable Mandate, sealed as one item under a key derived from the Agent Token).
- Files touched:
  - `Cargo.toml` / `Cargo.lock`: new dependencies. Suggested: tokio, async-imap (tokio runtime), rustls + tokio-rustls with system certificate verification, serde + serde_json, an AEAD crate (e.g. chacha20poly1305 or aes-gcm), hkdf + sha2, getrandom or rand. Dev: rcgen for the fake server's certificate. Also a cargo feature (e.g. `test-support`) that is **not** in `default` and gates test-only code (see AC11).
  - `src/lib.rs` (and new modules under `src/`):
    - CLI definition with a `list` subcommand and a global `--text` flag.
    - The core entry point: (presented Agent Tokens + command) → result structure or structured error.
    - The Mandate model: Account with login name, host, port, TLS mode `implicit` | `starttls` (no plaintext variant), and Credential; Policy of Grants (operation, Account, mailbox list of exact UTF-8 names or `*`).
    - Sealing and opening.
    - The Mandate store port and the in-memory store gated by `cfg(any(test, feature = "test-support"))`.
    - The IMAP connector.
    - Modified UTF-7 decoding.
    - A library entry point the tests can call in-process with arguments, environment and store, getting back stdout, stderr and exit code.
  - `src/main.rs`: stays thin; wires the real arguments, environment and the runtime store into the library.
  - `tests/list.rs` (new): drives the CLI adapter and core in-process against the fake IMAP server.
  - `tests/sealing.rs` (new): sealed-record tests.
  - `tests/support/` (new): the in-process fake IMAP server (real TCP, TLS with a generated certificate trusted only via the test-support hook; implicit-TLS, STARTTLS and plaintext-without-STARTTLS modes; scripted mailbox list; records received commands).
  - `tests/cli.rs`: binary-level checks.
  - `tests/project.rs`: Cargo feature check.
- Fixtures used below (all test-only, all fake): Account login `agent@example.test`, Credential `fake-app-password-123`. The fake server lists `INBOX`, `Archive`, `Sent`, `Spam`, `[Gmail]/Entw&APw-rfe` in that order. Agent Tokens are minted by the sealing code: a recognisable prefix plus at least 256 bits of randomness, for example `amt_` followed by 43 base64url characters.
- Output contract fixed by this spec:
  - Success, JSON: stdout is one JSON object `{"account":"agent@example.test","mailboxes":[{"name":"INBOX"},{"name":"Archive"}]}`, with mailboxes in server order, followed by a newline; exit 0.
  - Errors: stdout empty, stderr one JSON object `{"error":{"code":"<code>", ...,"message":"<text>"}}`, exit 1.
  - Error codes in this slice: `missing_token`, `invalid_token`, `denied`, `tls_required`.
- Runtime store in this slice: the OS-backed Keychain store arrives with ticket 03. Until then the store a non-test build uses holds no Mandates, so every well-formed token yields `invalid_token`.

## Acceptance criteria
1. AC1: With a sealed Mandate granting `list` on mailboxes INBOX and Archive of an Account, `agent-mail list` returns exactly those two mailboxes from the fake server's larger set, as JSON.
   - Setup: in-memory store with one Mandate (Account `agent@example.test`, Grant `list` on `["INBOX","Archive"]`) and `AGENT_MAIL_TOKEN` set to its token.
   - Expected: stdout parses to `{"account":"agent@example.test","mailboxes":[{"name":"INBOX"},{"name":"Archive"}]}` and exit code is 0.
   - Run the test once with the fake server in implicit-TLS mode and once in STARTTLS mode (Account TLS mode set to match). Both give the same result.
2. AC2: `--text` prints the same result in human-readable form.
   - Setup: same as AC1, invoked as `agent-mail --text list`.
   - Expected: stdout is exactly `agent@example.test\n  INBOX\n  Archive\n`, stdout is not valid JSON, and exit code is 0.
3. AC3: A Mandate with no `list` Grant on the Account returns a structured `denied` error naming the operation and Account, and a non-zero exit code.
   - Setup: a Mandate whose Policy is empty, and separately a Mandate whose only Grant is `search` on `["INBOX"]`.
   - Expected in both cases: stdout is empty, stderr parses to an object with `error.code == "denied"`, `error.operation == "list"` and `error.account == "agent@example.test"`, and exit code is 1.
   - The fake server records no `LIST` command. The Policy is checked before any mailbox data is fetched.
4. AC4: A Grant with mailbox `*` returns every mailbox the server lists.
   - Setup: Grant `list` on `["*"]`.
   - Expected: `mailboxes` names are `["INBOX","Archive","Sent","Spam","[Gmail]/Entwürfe"]` in that order, and exit code is 0.
5. AC5: Mailbox names in modified UTF-7 (e.g. `[Gmail]/Entw&APw-rfe`) are returned decoded as UTF-8 (`[Gmail]/Entwürfe`).
   - Setup: Grant `list` on `["[Gmail]/Entwürfe"]`. The Grant holds the UTF-8 name, and the server lists `[Gmail]/Entw&APw-rfe`.
   - Expected: `mailboxes == [{"name":"[Gmail]/Entwürfe"}]`, and the raw string `&APw-` does not appear in stdout.
6. AC6: A missing, malformed or unknown Agent Token returns a structured error without revealing whether any Mandate exists.
   - Inputs:
     - `AGENT_MAIL_TOKEN` unset, or set to an empty string. Expected: `error.code == "missing_token"`.
     - Set to `hello` (malformed). Expected: `error.code == "invalid_token"`.
     - Set to a freshly minted, well-formed token that was never stored (unknown). Expected: `error.code == "invalid_token"`.
   - The malformed and unknown cases produce byte-identical stderr. The unknown-token stderr is byte-identical whether the store is empty or holds another Mandate.
   - All cases: stdout empty, exit 1, and the fake server receives no connection.
   - The real binary (`tests/cli.rs`) shows the same `missing_token` and `invalid_token` behaviour.
7. AC7: A sealed record whose ciphertext was altered, or which is opened with a different token, fails to open.
   - Inputs: seal a Mandate, then flip one byte in the stored ciphertext and open it with the original token. Separately, open the unmodified record with a second freshly minted token.
   - Expected: both opens return an error and no Mandate.
   - Through `agent-mail list`, both cases end in a structured error with exit 1, and the fake server receives no connection.
8. AC8: The stored record contains neither the Agent Token nor the Credential in clear text (asserted by inspecting the test store's raw bytes).
   - Inputs: seal a Mandate with Credential `fake-app-password-123` and store it in the in-memory store.
   - Expected: none of the store's raw bytes contain the token string, the Credential, the login `agent@example.test`, or the mailbox name `Archive`. Checked for the store key and value, both as raw bytes and as UTF-8.
9. AC9: An Account configured for a server that offers neither implicit TLS nor STARTTLS is refused.
   - Setup: the fake server runs plaintext and does not advertise `STARTTLS`. Test it once with the Account in `starttls` mode and once in `implicit` mode against that server.
   - Expected: `error.code == "tls_required"`, exit 1, and the fake server records no `LOGIN` or `AUTHENTICATE` command, so the Credential never crosses the connection.
10. AC10: No Credential or Agent Token appears in any output or error.
    - Inputs: every scenario above (AC1–AC9), each in JSON and `--text` mode.
    - Expected: the presented token string and `fake-app-password-123` appear in neither stdout nor stderr.
11. AC11: The in-memory store is not present in a release build (a test or build check proves it cannot be selected by flag or environment variable).
    - Check (a), in `tests/project.rs`: `Cargo.toml`'s `[features] default` does not list the test-support feature.
    - Check (b), in `tests/cli.rs`: `agent-mail list --store memory` exits 2 with an unknown-argument usage error. `agent-mail list` with `AGENT_MAIL_STORE=memory` and a well-formed token gives stderr byte-identical to the same call without that variable (`invalid_token`).
    - Check (c): the in-memory store embeds a unique marker string, e.g. `agent-mail-in-memory-store`. A test runs `cargo build --release` into a separate target directory (so it does not deadlock on the test build's lock) and asserts the marker is absent from the resulting binary's bytes.

## Out of scope
- Keychain and Credential Manager stores, and `mandate create`, `list` and `delete` (tickets 03, 04, 18). The runtime store in this slice holds no Mandates.
- Several tokens in `AGENT_MAIL_TOKEN`, Mandate ambiguity, `--account` selection, and `whoami` (ticket 13). This slice handles one token and one Account.
- The audit log (ticket 06).
- search, read, flag, move and draft (tickets 07–11), and Gmail-specific behaviour beyond decoding mailbox names (ticket 12).
- Proxy support and webpki-roots inside the sandbox (ticket 14), the MCP adapter (ticket 15), and the hardened-runtime self-check (ticket 16).
- Mailbox attributes in the output (special-use flags, delimiters). The `{"name": ...}` object shape leaves room to add them later.
- Human-readable formatting of errors under `--text` beyond containing the error code. The ACs fix the JSON error shape only.

## Notes for reviewers
- Security:
  - This slice creates the credential-bearing core.
  - Check that the sealed record is AEAD-encrypted under a key derived from the Agent Token, and that the store lookup key is a separate derivation from the same token (e.g. HKDF with distinct info labels). The token has at least 256 bits of entropy, so no slow KDF is needed.
  - Check that the token hash, Credentials and Policy sit inside the one ciphertext, so a forged Policy cannot be paired with a real Credential (ADR-0003).
  - Check that the in-memory store and the test certificate trust hook compile only under `cfg(any(test, feature = "test-support"))`, and that no environment variable, flag or config chooses the store or adds trusted certificates at runtime.
  - The TLS mode type must have no plaintext variant. STARTTLS must be required, never opportunistic, and LOGIN is sent only after the TLS handshake.
  - Mailbox names from the server are untrusted input. Decoding malformed modified UTF-7 must not panic; return an error or a lossless fallback. Grants are matched on the decoded UTF-8 name.
  - Error messages and `Debug` output of Mandate, Account and Credential types must not include secrets. Prefer a redacting `Debug` on the Credential and token types.
  - Fixtures are fake values only, per CLAUDE.md.
  - Wording in AC6: the ticket says "without revealing whether any Mandate exists". This spec reads that as: malformed and unknown tokens are indistinguishable, and the answer does not depend on what else is in the store. A missing token gets its own code, because that reveals nothing about stored Mandates.
- UX:
  - The user is an Agent (Claude Code via the CLI). The flow: the host sets `AGENT_MAIL_TOKEN`, the Agent runs `agent-mail list`, parses the JSON, and on `denied` tells the human which operation and Account to grant.
  - The operator (human) is not involved in this slice, because Mandates can only be created in tests until ticket 03.
  - Errors are machine-readable on stderr with stable codes, and stdout carries only results.
  - Size: the ticket's 11 criteria are kept unchanged, as required, so this branch will likely run to several hundred lines, mostly the fake IMAP server and sealing. Reviewers should accept the size rather than ask for a split.
- UI:
  - Terminal output only. JSON on stdout for results, a two-level indented list under `--text`, and JSON errors on stderr.
  - `--help` gains the `list` subcommand and `--text` flag, and must still avoid the word "mandate".
