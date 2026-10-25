# 19: Windows Broker service

**What to build:** on Windows, `agent-mail` can run as the Broker (ADR-0004): a Windows service under `NT SERVICE\agent-mail` that listens on the named pipe `\\.\pipe\agent-mail`, stores sealed Mandates in its own Credential Manager, and carries out every request itself: it opens the presented tokens, selects the Mandate, connects to IMAP through the client's proxy setting, enforces the Policy, writes the audit log and replies with the JSON result or error the CLI prints today. No Credential, sealed or not, is ever sent to a client. This ticket builds the Broker and its request handling; the clients that call it are Ticket 20.

**Blocked by:** 18

**Status:** ready-for-agent

- [ ] The same binary has a service entry point that registers with the Windows service control manager and stops cleanly on a stop request
- [ ] The pipe's access list allows authenticated local users and the Broker, and nothing else
- [ ] Requests and replies are length-prefixed JSON; a request carries the parsed command, the presented Agent Tokens and `HTTPS_PROXY`/`ALL_PROXY`; the first exchange compares versions and a mismatch fails with `broker_version_mismatch`
- [ ] Each Mandate records the SID of the user who created it; the Broker serves a Mandate only to a pipe client with that SID, and reverts impersonation before any store call
- [ ] `mandate create` is refused unless the pipe client's token is at high integrity (elevated); the Broker runs the live login, generates the Agent Token, seals and stores the Mandate, and returns the token once
- [ ] `mandate list` and `mandate delete` need no elevation and see only the caller's own Mandates
- [ ] Records are stored with `CredentialManagerStore` under the Broker's account; writes are serialized inside the Broker so add-only cannot race (closes F31); `ERROR_NO_SUCH_LOGON_SESSION` is reported under its own name instead of a generic unavailable store
- [ ] The Broker writes the audit log to `%ProgramData%\agent-mail\audit.log`, creating the folder with an access list that lets only the Broker write and Administrators read; nothing there is user-writable, so no reparse point can be planted (closes F33)
- [ ] The Broker serves an audit request that returns only the entries for the caller's SID
- [ ] In-process tests drive the Broker's request handling through a fake transport on every OS, covering SID mismatch, the elevation refusal, version mismatch and that no reply contains a Credential
- [ ] One test on Windows CI runs a real pipe round trip
