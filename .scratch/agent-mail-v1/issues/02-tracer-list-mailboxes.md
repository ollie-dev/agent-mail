# 02: Tracer bullet: an Agent lists its granted mailboxes

**What to build:** The first end-to-end path. An Agent runs `agent-mail list` with `AGENT_MAIL_TOKEN` set and gets back only the mailboxes its Mandate's Policy grants on the Account, as JSON (or human-readable with `--text`).

This slice establishes every layer later tickets build on (see the spec's Implementation Decisions and Testing Decisions):
- the core as the single entry point (tokens + command → result), with the CLI as a thin adapter;
- the Mandate as one sealed record (Accounts with Credentials, Policy, Agent Token hash), encrypted with an AEAD cipher under a key derived from the Agent Token, located by a lookup key also derived from the token (ADR-0003);
- the Mandate store port, with an in-memory store that exists only in test builds and cannot be selected at runtime;
- the IMAP connection over TLS (implicit TLS or STARTTLS; plaintext refused);
- deny-by-default Policy enforcement with a structured error;
- the in-process fake IMAP server, reached over real TCP with TLS, whose certificate is trusted only in test builds.

Only one token and one Account are needed in this slice.

**Blocked by:** 01

**Status:** done

- [ ] With a sealed Mandate granting `list` on mailboxes INBOX and Archive of an Account, `agent-mail list` returns exactly those two mailboxes from the fake server's larger set, as JSON
- [ ] `--text` prints the same result in human-readable form
- [ ] A Mandate with no `list` Grant on the Account returns a structured `denied` error naming the operation and Account, and a non-zero exit code
- [ ] A Grant with mailbox `*` returns every mailbox the server lists
- [ ] Mailbox names in modified UTF-7 (e.g. `[Gmail]/Entw&APw-rfe`) are returned decoded as UTF-8 (`[Gmail]/Entwürfe`)
- [ ] A missing, malformed or unknown Agent Token returns a structured error without revealing whether any Mandate exists
- [ ] A sealed record whose ciphertext was altered, or which is opened with a different token, fails to open
- [ ] The stored record contains neither the Agent Token nor the Credential in clear text (asserted by inspecting the test store's raw bytes)
- [ ] An Account configured for a server that offers neither implicit TLS nor STARTTLS is refused
- [ ] No Credential or Agent Token appears in any output or error
- [ ] The in-memory store is not present in a release build (a test or build check proves it cannot be selected by flag or environment variable)
