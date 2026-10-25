# Windows Broker service

## Goal
On Windows, `agent-mail` can run as the Broker (ADR-0004): a Windows service under `NT SERVICE\agent-mail` that listens on `\\.\pipe\agent-mail`, holds the sealed Mandates in its own Credential Manager and carries out each request itself. It opens the presented Agent Tokens, selects the Mandate, connects to IMAP through the client's proxy setting, enforces the Policy, writes the audit log and replies with the JSON the CLI prints today. No Credential, sealed or not, ever goes back to a client. This job builds the Broker and its request handling. The clients that call it (Ticket 20) are not part of this job.

## Context
- Stack: Rust, one binary crate `agent-mail`. Windows APIs come from `windows-sys` (0.61). Add only the features the job needs, for example `Win32_System_Services`, `Win32_System_Pipes`, `Win32_Storage_FileSystem`, `Win32_Security_Authorization`, `Win32_System_Threading` and `Win32_System_IO`. Do not add a new crate unless no `windows-sys` route exists.
- Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`. CI runs `cargo test --locked` on macOS and Windows.
- Sources: ticket `.scratch/windows-broker/issues/19-broker.md`, ADR `docs/adr/0004-windows-broker.md` (it supersedes ADR-0002 on Windows), ADR-0003 (immutable Mandates, deletion is accepted), Follow-ups F31 and F33 in `factory/follow-ups.md`, and `GLOSSARY.md` (Broker, Mandate, Agent Token, Credential, Policy, Grant, Account, Agent).
- New files:
  - `src/broker.rs` holds the platform-neutral part, compiled on every OS: the protocol types (`Request`, `Reply`), the length-prefixed framing, the version handshake, the `Caller` (SID string plus integrity RID), request handling over any `Read + Write` transport, the serialized-writes store wrapper, the SID filter for audit entries and the pure SDDL builders for the pipe and the audit folder.
  - `src/broker/windows.rs` (or `src/service.rs`), `cfg(windows)`, holds the service control manager entry point, the named-pipe server, impersonation (reading the client SID and integrity level, then `RevertToSelf`), the `%ProgramData%` audit path and applying the access lists.
  - `tests/broker.rs` holds the fake-transport tests on every OS and the Windows-only real pipe round trip.
- Files that change:
  - `src/lib.rs`: a hidden `broker` subcommand that runs the service entry point.
  - `src/main.rs`: hands off to the service entry point for `broker`.
  - `src/core.rs`: make `Command` and its parts serde-serializable; add an owner check when a Mandate is opened, owner filtering for `mandate list` and `mandate delete`, and new error codes.
  - `src/seal.rs`: `Listing` gets an optional `owner` SID.
  - `src/store.rs`: `StoreError::NoLogonSession`, the Win32 error mapping, and the doc comment, which must no longer cite ADR-0002 for the Broker.
  - `src/audit.rs`: entries get an optional `sid`, plus the Broker log location.
  - `CLAUDE.md`: update the layout list with the new modules and `tests/broker.rs`. `tests/release.rs` and `tests/project.rs` check this file.
  - `factory/follow-ups.md`: tick F31 and F33.
- Behaviour that must not change: on macOS and in the current in-process Windows path, every CLI and MCP output stays byte for byte the same. `owner` and `sid` are left out of the serialized output when they are `None`, so existing records still open and existing audit lines do not change.

## Acceptance criteria
1. AC1: The same binary has a service entry point that registers with the Windows service control manager and stops cleanly on a stop request.
   - `agent-mail broker` calls `StartServiceCtrlDispatcherW` for the service name `agent-mail`. The name is a constant `broker::SERVICE_NAME == "agent-mail"`.
   - When the binary is not started by the service control manager (`ERROR_FAILED_SERVICE_CONTROLLER_CONNECT`), it exits with code 1 and prints one JSON error with code `service_required` on stderr. A test runs the built binary with `broker` on Windows and checks this.
   - On macOS, `agent-mail broker` exits with code 1 and prints the error code `unsupported`.
   - `broker` does not appear in `agent-mail --help`.
   - Stopping is driven by a stop signal that the control handler sets on `SERVICE_CONTROL_STOP`. An in-process Windows test starts the pipe server loop on a test pipe name and sends the stop signal. The loop must return `Ok(())` within 5 seconds, and opening the pipe afterwards must fail with "file not found".
   - The handler reports `SERVICE_STOPPED` with exit code 0. That path is covered by an opt-in test, `#[ignore]`, run with `cargo test --test broker -- --ignored`. It needs an elevated shell, registers the built binary with `sc.exe create`, starts and stops it, and checks that the state is `STOPPED` with exit code 0.
2. AC2: The pipe's access list allows authenticated local users and the Broker, and nothing else.
   - `broker::pipe_sddl(broker_sid)` returns exactly `D:P(A;;0x12019b;;;AU)(A;;GA;;;<broker_sid>)`. A test on every OS checks this with `S-1-5-80-1-2-3-4-5`. `0x12019b` is `FILE_GENERIC_READ | FILE_GENERIC_WRITE` without `FILE_CREATE_PIPE_INSTANCE` (0x4). `GRGW` would let any authenticated user add a server instance of the pipe (changed in round 2 after the security review).
   - The pipe is created with that security descriptor, with `FILE_FLAG_FIRST_PIPE_INSTANCE` (a second Broker, or a squatter, cannot create it) and with `PIPE_REJECT_REMOTE_CLIENTS`.
   - A Windows test creates a test pipe and reads its DACL back with `GetSecurityInfo`. The DACL must be protected and hold exactly two allow entries: Authenticated Users with `0x12019b` (no 0x4 bit), and the Broker SID (the process's own user SID in the test) with full access.
   - A second server-side create of the same pipe name fails. A Windows test creates the pipe with a foreign Broker SID, so the test process is only an authenticated user, and checks that a later, non-first `CreateNamedPipeW` fails with access denied.
3. AC3: Requests and replies are length-prefixed JSON. A request carries the parsed command, the presented Agent Tokens and `HTTPS_PROXY`/`ALL_PROXY`. The first exchange compares versions, and a mismatch fails with `broker_version_mismatch`.
   - Framing: a 4-byte big-endian length, then that many bytes of UTF-8 JSON. Frames over 1 MiB, or whose body is not valid JSON, get the reply error `invalid_request` and the connection is closed.
   - Requests:
     - `{"hello":{"version":"<v>"}}`
     - `{"run":{"command":<core::Command>,"tokens":<AGENT_MAIL_TOKEN value or null>,"proxy":{"HTTPS_PROXY":..,"https_proxy":..,"ALL_PROXY":..,"all_proxy":..}}}`. `run` covers the Agent commands plus `MandateList` and `MandateDelete`. A `MandateCreate` command inside `run` is refused with `invalid_request`.
     - `{"create":{...}}` (see AC5)
     - `{"audit":{}}`
   - Replies: `{"ok":{"version":"<broker v>"}}` for `hello`; `{"result":<JSON value>}` or `{"error":<ErrorBody>}` for the other requests. An invalid-token warning is added as `"warning"`, with the same object the CLI prints.
   - When the version is `env!("CARGO_PKG_VERSION")`, the reply is `ok`.
   - When the version is `0.0.0-other`, the reply is `{"error":{"code":"broker_version_mismatch",...}}` and the connection closes.
   - Any request sent before `hello` gets `invalid_request`.
   - For `run` with `List` against a store with one Mandate the caller owns, the `result` value equals the JSON `agent_mail::run` prints on stdout for `list` with the same token and store. This is checked with the fake IMAP server.
4. AC4: Each Mandate records the SID of the user who created it. The Broker serves a Mandate only to a pipe client with that SID, and reverts impersonation before any store call.
   - `Listing.owner = Some(sid)` is part of the authenticated listing (AAD).
   - Test 1: a Mandate owned by `S-1-5-21-1-2-3-1001` is presented by a caller with SID `S-1-5-21-1-2-3-1002`. `run`/`List` replies exactly as for an unknown token (`invalid_token`), and the audit entry is `denied`.
   - Test 2: a Mandate with no `owner`, written by an in-process build, is served to no caller.
   - Revert check: in the Windows pipe test, the store is a test wrapper that calls `OpenThreadToken` in every method. Every store call must see `ERROR_NO_TOKEN`, which means no impersonation is active.
5. AC5: `mandate create` is refused unless the pipe client's token is at high integrity (elevated). The Broker runs the live login, generates the Agent Token, seals and stores the Mandate, and returns the token once.
   - Request: `{"create":{"name":..,"accounts":[{"login","host","port","tls","credential"}],"grants":[...]}}`.
   - With caller integrity RID `0x2000` (medium), the reply is `{"error":{"code":"elevation_required",...}}` and nothing is stored or logged in.
   - With RID `0x3000` (high), the Broker logs in to each Account (the fake IMAP server in tests). On a bad Credential it replies with `login_failed` naming the Account, and stores nothing.
   - On success the Broker stores a record whose listing `owner` is the caller SID and replies `{"result":{"id":..,"name":..,"token":"<Agent Token>"}}`. That token opens the stored record. The token appears in no other reply, and no other reply can recover it.
6. AC6: `mandate list` and `mandate delete` need no elevation and see only the caller's own Mandates.
   - Store setup: Mandates A and B owned by SID X, and C owned by SID Y. A medium-integrity caller with SID X runs `MandateList` and gets exactly A and B. Its `unreadable` count does not count C.
   - Caller X runs `MandateDelete` on C's ID and gets `not_found`. C is still stored.
   - Caller X runs `MandateDelete` on A's ID, which succeeds and removes A.
7. AC7: Records are stored with `CredentialManagerStore` under the Broker's account. Writes are serialized inside the Broker, so add-only cannot race (closes F31). `ERROR_NO_SUCH_LOGON_SESSION` is reported under its own name instead of a generic unavailable store.
   - A `SerializedStore` wrapper holds one mutex around `put` and `delete`.
   - Test: 8 threads call `put` and `delete` concurrently through the wrapper on a test store that sleeps 10 ms in each write and records how many writes are in flight. The maximum in flight must be 1.
   - `store::from_win32(1312)` returns `StoreError::NoLogonSession`, and `from_win32(5)` returns `StoreError::Unavailable`. Both are pure and tested on every OS.
   - `CoreError` from `NoLogonSession` has code `store_no_logon_session` and a short message.
8. AC8: The Broker writes the audit log to `%ProgramData%\agent-mail\audit.log` and creates the folder with an access list that lets only the Broker write and Administrators read. Nothing there is user-writable, so no reparse point can be planted (closes F33).
   - `broker::audit_dir_sddl(broker_sid)` returns exactly `O:<broker_sid>D:P(A;OICI;FA;;;<broker_sid>)(A;OICI;FR;;;BA)`. A test on every OS checks this.
   - `%ProgramData%` comes from `FOLDERID_ProgramData` (`SHGetKnownFolderPath`), not from an environment variable.
   - Before each append, the Broker refuses the log, and the operation fails with `audit_unavailable`, in either of these cases:
     - the folder is a reparse point, or its owner is neither the Broker SID nor Administrators. Users can create folders under `%ProgramData%`.
     - `audit.log`, opened with `FILE_FLAG_OPEN_REPARSE_POINT`, is a reparse point, is not a regular file, or has more than one hard link.
   - Windows tests run in a temp directory standing in for `%ProgramData%`:
     - the folder the Broker creates reads back with the protected DACL above.
     - a pre-created junction `agent-mail` makes the append fail.
     - a pre-created symlink or hard link `audit.log` makes the append fail.
   - Every Broker entry carries `"sid":"<caller SID>"`.
9. AC9: The Broker serves an audit request that returns only the entries for the caller's SID.
   - The log holds three entries: `sid` X, `sid` Y, and one with no `sid`. Caller X sends `{"audit":{}}` and gets `{"result":{"entries":[<the X entry>]}}`, in file order.
   - When the log file does not exist, the result is `{"entries":[]}`.
10. AC10: In-process tests drive the Broker's request handling through a fake transport on every OS. They cover SID mismatch, the elevation refusal, version mismatch, and that no reply contains a Credential.
    - The fake transport is an in-memory duplex in `tests/support/` with a `Caller` set by the test. It runs AC3's version-mismatch case, AC4's SID mismatch and AC5's medium-integrity refusal.
    - Credential check: one session sends `hello`, `create` (Credential `Cred-7f3a-secret`), then `run` for `Whoami`, `List`, `MandateList` and `MandateDelete`, then `audit`. No reply frame may contain:
      - the Credential;
      - the stored record value, as raw bytes or in base64 or hex;
      - the sealed part.
    - Only the `create` reply may contain the Agent Token.
11. AC11: One test on Windows CI runs a real pipe round trip.
    - A `cfg(windows)` test in `tests/broker.rs` starts the pipe server in-process on a unique test pipe name. Test-support builds only may override the pipe name.
    - The server uses an `InMemoryStore` holding one Mandate owned by the current user's SID and one owned by `S-1-5-21-1-2-3-1001`.
    - The test connects as a client and sends `hello`, then `run`/`MandateList`. It gets `ok`, then a listing with exactly the first Mandate.
    - The test then stops the server.
    - It runs in plain `cargo test --locked` on `windows-latest`, without being ignored.

## Out of scope
- Ticket 20: the CLI and MCP server forwarding to the Broker, `broker_unavailable`, the client side of `mandate create`, the `agent-mail audit` command and the sandbox pipe check. Until then, the Windows CLI keeps running in-process with its current per-user store and audit log.
- Ticket 21: `install.ps1`/`uninstall.ps1`, registering the service with delayed automatic start, `%ProgramFiles%` and the PATH.
- Tickets 22 and 23: README and security documents. The only doc change here is the CLAUDE.md layout line.
- Moving existing per-user Windows Mandates into the Broker.
- Checking the identity of the pipe server from the client side, beyond `FILE_FLAG_FIRST_PIPE_INSTANCE` on the server. This belongs to Ticket 20.
- macOS: no behaviour change.
- F32, F34, F35 and the other follow-ups.

## Notes for reviewers
- Security:
  - Untrusted input: every pipe request is untrusted, from any authenticated local user. Check the frame-size cap, JSON parsing, the existing argument checks running again inside the Broker, and that `MandateCreate` cannot be reached through `run`.
  - Credentials arrive only in `create`. The frame buffer and the parsed request should be zeroized (`Zeroizing`, `Credential`). Nothing may log or echo them. AC10 is the guard.
  - Owner SID: it is taken from the impersonated client token. It is never taken from the request.
  - Impersonation must be reverted on every path, including errors, before any store or IMAP call.
  - The owner field rides in the AAD-protected listing. `mandate list`/`delete` read it without opening the record, which is safe only because the store is Broker-only.
  - A Mandate owned by someone else must look exactly like an unknown token or ID, so no reply or timing tells the caller that it exists.
  - AC8 adds owner, reparse-point and hard-link checks beyond the ticket's wording. Users can create subfolders of `%ProgramData%`, so a pre-planted folder is a real threat. Confirm that this matches F33's intent.
  - Administrators are out of scope (ADR-0004).
- UX: Agents and operators do not use this surface directly. Only the Broker's own entry point and clients (Ticket 20) use it. The operator sees new stable error codes, `service_required`, `elevation_required`, `broker_version_mismatch`, `store_no_logon_session` and `invalid_request`, whose messages follow the CLAUDE.md convention (short, each thing said once). Sizing: the ticket's eleven criteria are carried over in full, as the factory rules require, so the branch will likely go past a few hundred lines. If the builder must cut, AC1's real SCM test is already opt-in.
- UI: no UI surface beyond one JSON error line on stderr from `agent-mail broker` when it is not run as a service. The protocol replies reuse the existing CLI JSON shapes unchanged.
