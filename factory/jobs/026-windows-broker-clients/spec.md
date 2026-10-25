# Windows Broker clients

## Goal
On Windows, every `agent-mail` CLI command and every `agent-mail mcp` tool call is carried out by the Broker (ADR-0004) over `\\.\pipe\agent-mail`. The client only parses arguments, does terminal I/O and renders the reply, so stdout, stderr, exit codes, `--text` lines and MCP results stay exactly as they are today. `mandate create` runs in an elevated terminal and hands the Credential to the Broker. A new `agent-mail audit` command shows the caller's own audit entries on macOS (from the user's own log) and on Windows (from the Broker). macOS keeps running everything in-process.

## Context
- Stack: Rust, one binary crate `agent-mail`; Windows APIs from `windows-sys` (add only the features needed, for example `Win32_Security` for the elevation check). No new crate unless no `windows-sys` route exists.
- Test: `cargo test`. Lint: `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`. CI runs `cargo test --locked` on macOS and Windows.
- Sources: ticket `.scratch/windows-broker/issues/20-windows-clients.md`, ADR `docs/adr/0004-windows-broker.md` (supersedes ADR-0002 on Windows), ADR-0003, `GLOSSARY.md` (Broker, Agent, Agent Token, Mandate, Credential, Policy, Grant, Account), and job 025's spec and decision (`factory/jobs/025-windows-broker-service/`), whose follow-ups mention a `create` audit entry "once Ticket 20 defines the operator view" (not required here).
- What exists (job 025): `src/broker.rs` has the protocol (`Request::{Hello, Run, Create, Audit}`, `RunRequest`, `ProxyVars`, `CreateRequest`, `CreateAccount`, `Reply`), `write_frame`/`read_frame`, `serve_session` over any `Read + Write`, and `VERSION`; `src/broker/windows.rs` has `PIPE_NAME`, `connect(name)`, `serve_on` and `create_test_pipe` (test-support); `tests/support/broker.rs` has the in-memory `duplex()` and `session()` used to drive `serve_session` on every OS. `core::CoreError` already has `broker_version_mismatch` and `elevation_required`.
- Suggested shape (the builder may choose another that meets the criteria):
  - New `src/client.rs`, compiled on every OS: the Broker client. It runs over any `Read + Write` transport, sends `hello` then one request, and turns the reply back into the CLI's `RunOutput` or the MCP tool result. A `Context` gets an optional Broker route (a connector returning a transport). When set, `run_cli`, `mcp::call_tool`, `mandate create` and `audit` use it instead of `core::execute_reporting`. `src/main.rs` sets the route only under `cfg(windows)`, using `broker::windows::connect(PIPE_NAME)`.
  - `--text` rendering needs typed values on the client: either derive `Deserialize` on the result types and reuse `render_outcome`/`render_error`/`render_warning`, or extend `RunRequest` with the output mode. Client and Broker are the same binary, and the version handshake guards protocol changes.
  - Test-support only: a pipe-name override for the built binary (for example the variable `AGENT_MAIL_TEST_PIPE`, read only under `cfg(feature = "test-support")`) and a fixed elevation status for the client, like `GuardedStore::with_status`.
- Files that change: `src/lib.rs` (new `Audit` subcommand, Broker route in `run_cli`, help text), `src/main.rs`, `src/mcp.rs`, `src/create.rs` (client side of create on the Broker route), `src/core.rs` (`CoreError::BrokerUnavailable` with code `broker_unavailable`, `broker_version_mismatch` message), `src/audit.rs` (reading the user's own log for `audit`), `src/broker.rs` and `src/broker/windows.rs` (client connect, server-identity check, elevation check), `README.md` ("Sandbox and proxies" paragraph, plus one clause in "Audit log"), `CLAUDE.md` layout list (new module and test file; `tests/project.rs` and `tests/release.rs` read this file).
- New tests: `tests/client.rs` (in-process versus Broker comparisons through the fake transport on every OS, plus the Windows binary and pipe tests). Add the `audit` command tests to `tests/audit.rs`, the help checks to `tests/cli.rs`, and the README check to `tests/plugin.rs`.
- Existing binary tests that run an Agent command on Windows (for example `tests/cli.rs` `ac6_binary_reports_missing_token`) now reach the Broker route. On Windows they must point the binary at an in-process test pipe served with `serve_on`, so their expected output stays the same. They must not be weakened to accept `broker_unavailable`.

## Acceptance criteria
1. AC1: On Windows, all Agent commands and `mandate list`/`delete` go through the pipe, with results and errors identical to the in-process ones (shared tests compare them).
   - Comparison tests run on every OS. One `RunOutput` comes from `agent_mail::run` in-process against a store holding Mandate M without an owner. The other comes from the Broker route over the fake transport, against a `serve_session` whose store holds the same Mandate M (same ID, name, Accounts and Grants, same Agent Token) owned by the caller SID `S-1-5-21-1-2-3-1001`. Both use the fake IMAP server. `stdout`, `stderr` and `code` must be byte-for-byte equal for each of these:
     - `list`, `search`, `read <ref>`, `flag <ref> --set seen`, `move <ref> --to Archive`, `draft --to a@example.com --subject Hi --body Hello`, `whoami`, `mandate list` and `mandate delete <M's ID>`, each once with JSON and once with `--text`;
     - a denied call (`read` on a mailbox without a `read` Grant): exit 1 with the same `error` line;
     - `AGENT_MAIL_TOKEN` set to `<valid token>,garbage`: the same invalid-token warning on stderr, in the same position;
     - no token: exit 1 with `missing_token`.
   - On the Broker route the client touches no local store: a counting store passed as the client's `Context` store records 0 accesses across all the cases above.
   - Windows only: the built binary run as `agent-mail list` with a valid token and the pipe override pointing at an in-process `serve_on` test pipe prints the same stdout as the in-process run. With the override pointing at a pipe name nobody serves, it fails with `broker_unavailable`. This shows the Windows binary forwards instead of running in-process.
   - macOS: the binary still runs in-process, and every existing test in `tests/list.rs`, `tests/search.rs`, `tests/read.rs`, `tests/flag.rs`, `tests/move_message.rs`, `tests/draft.rs`, `tests/tokens.rs` and `tests/mandates.rs` passes unchanged.
2. AC2: The MCP server forwards each tool call to the Broker and renders results as today.
   - For each tool (`whoami`, `list`, `search`, `read`, `flag`, `move`, `draft`), with valid arguments, and for one denied call, the `tools/call` response from `mcp::serve` on the Broker route (fake transport, owned Mandate as in AC1) equals the response on the in-process route: the same `content`, `structuredContent` and `isError`.
   - Bad tool arguments still fail with `invalid_arguments` without connecting: a counting connector records 0 connections.
   - Each tool call opens its own connection, which starts with `hello`. Two tool calls in one session give 2 connections, so a Broker restarted between calls serves the next call. `initialize` and `tools/list` open none.
3. AC3: `mandate create` and `mandate create --from` on Windows refuse to start when not elevated, with a short error saying to rerun from an elevated terminal. When elevated, the client runs the guided flow, sends the Credential to the Broker, shows the returned Agent Token via `CONOUT$` and makes the export offer.
   - Not elevated (fixed status "not elevated"; on Windows the real check reads `TokenElevation` of the process token): exit 1 with code `elevation_required`. The message says to rerun `agent-mail mandate create` in an elevated (Run as administrator) terminal and is at most 300 characters. The scripted terminal shows no prompt, the connector records 0 connections, and nothing is stored. The same holds with `--from <file>`, and the file is not read (a path to a missing file still gives `elevation_required`).
   - The existing refusals still come first and are unchanged: no interactive terminal gives the current "must be run by a human in a terminal" error, and a sandbox indicator gives `sandbox_detected`.
   - Elevated, fake transport to `serve_session` with caller integrity `0x3000` and the fake IMAP server: the scripted guided flow (one Account, Credential `Cred-7f3a-secret`, a `read` Grant on `INBOX`) sends exactly one `create` request whose Account, Grants and name match the answers. The Broker stores one Mandate owned by the caller SID. The terminal's `reveal` receives text holding the token from the `create` reply, and that token opens the stored Mandate. The export offer follows, and accepting it writes the TOML file with no Credential and no token.
   - With `--from <file>` the guided flow is pre-filled from the file, exactly as in-process, and the Credential is still typed.
   - The Broker replies `login_failed`: the client prints that error, says "Nothing was stored." and stores nothing.
   - If `reveal` fails after a successful `create`, the client deletes the new Mandate through the Broker (`run`/`MandateDelete` with its ID), prints the existing "could not be shown on the terminal. Nothing was stored." message and exits with `aborted`. Afterwards the Broker's store is empty.
   - The Credential and the token appear in no stdout or stderr output and in no request other than `create`.
   - macOS: `mandate create` is unchanged; no elevation check runs.
4. AC4: A client that cannot reach the pipe fails with `broker_unavailable`, and a version mismatch with `broker_version_mismatch`. Both messages name the fix (start the service, rerun `install.ps1`).
   - The connector fails with "file not found" (no pipe): exit 1, `{"error":{"code":"broker_unavailable","message":...}}` on stderr, nothing on stdout. The message contains `sc.exe start agent-mail` and `install.ps1` and is at most 300 characters. With `--text` the line is `error: broker_unavailable: <message>`.
   - The Broker closes the connection before replying (an EOF instead of a reply frame, after `hello` or after the request): `broker_unavailable`.
   - The Broker replies to `hello` with `{"error":{"code":"broker_version_mismatch",...}}`: exit 1 with that code. A fake Broker that replies `{"ok":{"version":"0.0.0-other"}}` also gives `broker_version_mismatch`, and the client sends no further request. The message contains `install.ps1` and is at most 300 characters.
   - The same codes come back as MCP tool errors (`isError: true`, `structuredContent.error.code`) for a tool call.
   - Before it sends any request, the client checks that the pipe server is the Broker. On Windows it reads the server's process with `GetNamedPipeServerProcessId`, and that process's user SID must be the service SID of `NT SERVICE\agent-mail`. Otherwise the client fails with `broker_unavailable` and sends nothing. A Windows test serves a test pipe from the test process: a client expecting the service SID gets `broker_unavailable` and the server receives no frame, and a client set (test-support) to expect the current user's SID connects. See Notes for reviewers.
5. AC5: `agent-mail audit` prints the caller's audit entries, as JSON by default and with `--text`, on macOS from the user's own log and on Windows from the Broker.
   - It needs no Agent Token and writes no audit entry.
   - In-process route (macOS, and every OS in tests with a redirected log): the log holds, in this order, a valid `list` entry, a line `not json`, and a valid `read` entry. `agent-mail audit` prints `{"entries":[<list entry>,<read entry>]}` plus a newline on stdout, with the entries in file order and each object equal to its log line. The non-JSON line is skipped. Exit 0.
   - With no log file it prints `{"entries":[]}`.
   - If the log or its directory is a symbolic link (Unix), it fails with `audit_unavailable` and prints nothing on stdout.
   - `--text` prints one line per entry: time, outcome and operation, then `key=value` for each present key among `mandate`, `account`, `mailbox`, `target`, `messages`, `recipients`, `change`, `results` and `reason`, in that order. Lists are joined with `,`, every value goes through `escape_controls`, and `sid` is not shown. Example: `2026-10-21T09:30:15Z allowed list mandate=ab12cd34 account=a@example.com`. An empty log prints `No audit entries.`. `--text` works both before and after `audit`.
   - Broker route (fake transport): the client sends `{"audit":{}}`. The Broker log holds entries for SID X, SID Y and one without `sid`, and caller X prints `{"entries":[<the X entry>]}`. The same data with `--text` prints the X entry's line only.
   - On the Broker route the user's local log is not read: with the local log redirected to a file holding one entry, that entry is not printed.
6. AC6: Check whether an Agent in the Claude Code sandbox on Windows can open the named pipe, and record the result in the README's "Sandbox and proxies" section.
   - The section gains one paragraph, of at most about 300 characters, that names `\\.\pipe\agent-mail` and states the observed result. If the pipe cannot be opened, the paragraph says what the Agent sees (`broker_unavailable`) and the way around it, for example using the `agent-mail mcp` server, which Claude Code starts outside the sandbox.
   - A test in `tests/plugin.rs` checks that the section contains `\\.\pipe\agent-mail` and that no paragraph in it is longer than 300 characters.
   - The build notes (`build.md`) say how the check was made: Claude Code version, sandbox setting and command run. If it could not be run, the README says the result is not yet verified instead of guessing.
   - The README "Audit log" clause "no agent-mail command reads the log" is changed to point to `agent-mail audit`. Nothing else in the README changes; the rest belongs to Ticket 22.
7. AC7: Help text follows the CLAUDE.md convention.
   - `agent-mail --help` lists `audit`. `agent-mail audit --help` and `-h` are added to `HELP_PATHS` in `tests/cli.rs`, the invocation count rises to match, and every paragraph stays at most 300 characters.
   - `agent-mail audit --help` names what it shows: your own Mandates' entries, JSON by default, with `--text` available. `agent-mail mandate create --help` says that on Windows it needs an elevated terminal. Both are added to `ac2_help_keeps_the_facts_an_agent_depends_on` (needles `--text` and `elevated`).
   - `broker_unavailable` and the changed `broker_version_mismatch` are added to `error_samples()`, so `ac5_every_error_message_is_at_most_300_characters` covers them.
   - Each thing is said once: the `audit` help repeats no sentence from the top-level help.

## Out of scope
- Ticket 21: `install.ps1`/`uninstall.ps1`, registering the service, `%ProgramFiles%`, the PATH. The error messages only name `install.ps1` and `sc.exe start agent-mail`.
- Ticket 22: the README install, Mandate and audit sections, release notes, the skill and the CLAUDE.md lists beyond the layout line for this job's new module and test file.
- Ticket 23: security documents.
- An MCP tool for `audit`, and `mandate` commands over MCP.
- Moving existing per-user Windows Mandates or the per-user Windows audit log into the Broker. After this job the Windows client neither reads nor writes the per-user store.
- The 025 follow-ups: connection caps and read deadlines, a NULL DACL, a `create` audit entry, merging the `run_service` match arms.
- Any macOS behaviour change other than the new `audit` command.

## Notes for reviewers
- Security:
  - Credentials: on the Broker route the Credential is typed in the elevated client, live-checked by the client's guided flow, sent once in the `create` frame and wiped (`Credential`, `Zeroizing`). Check that it reaches no stdout or stderr, no other request, no error message and no panic message. AC3 tests this with `Cred-7f3a-secret`.
  - Agent Tokens and proxy variables (which may hold proxy credentials) travel in every `run` request. They must stay in `Secret` and be wiped, and they must never appear in a `broker_unavailable` or mismatch message.
  - Untrusted input: Broker replies are parsed by the client and rendered as today, with `escape_controls` on `--text`. A reply frame over 1 MiB or with malformed JSON must give `broker_unavailable` (or a clear error), not a panic.
  - AC4's server-identity check goes beyond the ticket's wording. Job 025 deferred it to this ticket, and it closes a real hole. While the Broker is stopped, any local user can create `\\.\pipe\agent-mail` first, because `FILE_FLAG_FIRST_PIPE_INSTANCE` only guards an existing Broker. An elevated `mandate create` would then send the Credential to that process. Confirm that checking the server process's user SID against the `NT SERVICE\agent-mail` service SID is the intended guard, and that it holds for every request, not only `create`.
  - Elevation: the client check is convenience and UX. The Broker's integrity check from job 025 is the real control, so a forged "elevated" client still gets `elevation_required`.
  - `audit` on macOS reads only the user's own log and shows all of it, which contains no Credentials or tokens by design. On Windows the Broker filters by SID.
  - Sizing: the ticket's seven criteria are carried over in full, as the factory rules require. This will exceed a few hundred lines, mostly in tests.
- UX:
  - Operator (the human): runs `agent-mail mandate create` in an elevated terminal on Windows (UAC prompt), types the Credential, sees the token once and is offered the export. They run `agent-mail audit` (or `--text`) to see what their Agents did. When the Broker is down or out of date, they get one error naming `sc.exe start agent-mail` or `install.ps1`.
  - Agent: calls the same commands and MCP tools as before and sees identical output. The only new outcomes are `broker_unavailable` and `broker_version_mismatch`, whose messages say to ask the human.
  - AC6's sandbox result may change Agent guidance (shell versus MCP). Ticket 22 carries that into the skill.
- UI: terminal output only.
  - New `agent-mail audit` output: one JSON line, or one text line per entry, or `No audit entries.`.
  - New error lines: `broker_unavailable`, and the reworded `broker_version_mismatch`.
  - New `audit` help page and one added sentence in `mandate create --help`.
  - One README paragraph in "Sandbox and proxies" and one changed clause in "Audit log".
  - Every other output is unchanged byte for byte, which AC1 and AC2 check.
