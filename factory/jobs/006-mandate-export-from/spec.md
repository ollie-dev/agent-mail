# Mandate export and `mandate create --from`

## Goal
At the end of `mandate create` the human can save the new Mandate's Accounts (server, port, TLS mode, login) and Grants as a TOML file. That file never holds a Credential or Agent Token. `agent-mail mandate create --from <file>` later starts the same guided flow with the file's choices pre-filled. The human can still change any of them, and every Credential is still typed and live-checked. A Mandate is "changed" this way: export it, create a new one from the file with adjustments, then delete the old one (ADR-0003).

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Ticket: `.scratch/agent-mail-v1/issues/05-mandate-export-from.md`. Feature spec: `.scratch/agent-mail-v1/spec.md`, section "`mandate create` guided flow", steps 8 and `--from`.
- Files touched:
  - `src/create.rs`: export offer after the Agent Token is revealed; loading and validating the `--from` file; pre-filling the name, Account, mailbox and operation prompts.
  - `src/terminal.rs`: the Terminal port needs pre-selection. `choose` gets a default option and `checklist` gets initially ticked options, implemented in `InquireTerminal` (`Select::with_starting_cursor`, `MultiSelect::with_default`). `ScriptedTerminal` records the default or pre-ticked labels in each transcript `Entry`, plus the `default` of text and confirm prompts. It also gets an answer that accepts the pre-selection unchanged, so tests can assert both what was pre-filled and what was finally chosen.
  - `src/core.rs`: `Command::MandateCreate` carries an optional `from: Option<PathBuf>`. New `CoreError::InvalidMandateFile { problem }` with stable code `invalid_mandate_file`.
  - `src/lib.rs`: `MandateCommand::Create { #[arg(long, value_name = "FILE")] from: Option<PathBuf> }`, plus help text.
  - New module for the export file format (e.g. `src/export.rs`): serde types with `deny_unknown_fields`, `to_toml` / `from_toml` and validation. These types have no Credential field.
  - `Cargo.toml` / `Cargo.lock`: add the `toml` crate (serde-based).
  - `tests/create.rs`: existing scripts gain the answer to the new export offer. New tests can live there or in a new `tests/export.rs` (update the test layout line in `CLAUDE.md` if a new file is added).
- Export file format (TOML). Mailbox names are the decoded UTF-8 names, as in Grants:
  ```toml
  name = "triage"

  [[accounts]]
  login = "agent@example.test"
  host = "localhost"
  port = 993
  tls = "implicit"        # or "starttls"

  [[grants]]
  operation = "list"      # list | search | read | flag | move | draft
  account = "agent@example.test"
  mailboxes = ["INBOX", "Archive"]   # empty for draft; "*" allowed
  ```
- Pre-fill mapping for `--from`:
  - The name prompt defaults to the file's `name`.
  - For each file Account, in file order, the flow first asks a yes/no question to keep it (default yes). For a kept Account it asks host, port and login as text prompts defaulting to the file values, and the TLS mode as a choice defaulting to the file value. It then asks the hidden Credential and runs the live login check, with retry or abort as today.
  - The mailbox checklist pre-ticks every server mailbox named in a non-`draft` Grant for that Account. `*` pre-ticks all of them.
  - The operation checklist pre-ticks every operation granted on that Account.
  - After the file's Accounts, "Add another Account?" (default no) continues with the normal empty flow.
- Export offer: after the Agent Token reveal succeeds, the flow asks "Export this Mandate's Accounts and Grants to a TOML file?" (default no). If yes, it asks for a path (text prompt, default `agent-mail-<id>.toml` in the current directory). The file is created exclusively (never overwrites an existing path or follows a symlink there). An existing path gives a message and the path is asked again. A write error shows a message naming the path and reason, and the run still exits 0 because the Mandate is already stored.

## Acceptance criteria
1. AC1: Accepting the export offer writes a TOML file containing the name, Accounts (server, port, TLS mode, login) and Grants.
   - Input: a `mandate create` session against the fake server. Name `test-mandate`, one generic Account (`localhost`, fake server port, implicit TLS, login `agent@example.test`), ticked mailboxes `INBOX` and `Archive`, ticked operations `list`, `search` and `draft`. The summary is confirmed, the export offer is answered yes, and the path prompt gets a path in a fresh temp directory.
   - Expected: exit 0. The file exists and parses as TOML with `name = "test-mandate"` and exactly one `[[accounts]]` with `login`, `host = "localhost"`, the port and `tls = "implicit"`. There are three `[[grants]]`: `list` and `search` on `["INBOX", "Archive"]`, and `draft` with `mailboxes = []`. The Grants equal the stored Mandate's Policy.
2. AC2: The exported file never contains a Credential or Agent Token.
   - Input: the AC1 session with Credential `fake-app-password-123`.
   - Expected: the exported file's bytes do not contain the Credential, the Agent Token shown in the transcript, the token's hash or the store key. The file has no key named `credential`, `password`, `token` or `secret` at any level.
3. AC3: `mandate create --from <file>` shows the guided flow with the file's Accounts, mailboxes and operations pre-selected, and the human can change any of them before confirming.
   - Input: a file like the AC1 export (Grants `list` and `search` on `INBOX` and `Archive`). Run `mandate create --from <file>`, accept every pre-filled default, type the Credential, then confirm.
   - Expected: the transcript shows these pre-fills: the name prompt defaults to `test-mandate`; the keep-Account question names `agent@example.test`; the host, port and login prompts default to the file values; the TLS choice defaults to implicit; the mailbox checklist pre-ticks exactly `INBOX` and `Archive`; the operation checklist pre-ticks exactly `list` and `search`. The stored Mandate has those Grants.
   - Second run from the same file: untick `Archive`, tick `Sent`, untick `search` and tick `read`. The stored Mandate then has `list` and `read` on `["INBOX", "Sent"]` only.
   - Third run with a two-Account file: answer no to keeping the second Account. The stored Mandate covers only the first Account, and no Grant names the second Account.
4. AC4: Credentials are asked and live-checked for every Account even when starting from a file.
   - Input: a two-Account `--from` file (`agent@example.test` and `other@example.test` on two fake servers), both kept.
   - Expected: the transcript has one hidden Credential prompt per Account, and each fake server records a LOGIN. In a second run, the first Credential is wrong (`wrong-password`) and the human chooses `Retry` and types the right one. The Mandate is stored only after the retried login succeeds. Choosing `Abort` instead stores nothing, and the store stays empty.
5. AC5: A file referencing a mailbox that no longer exists on the server shows it as unavailable instead of failing.
   - Input: a `--from` file whose `list` Grant names `INBOX` and `Gone`. The fake server has no `Gone`.
   - Expected: the flow continues. A message names `Gone` as unavailable on `agent@example.test` (escaped with `escape_controls`). The mailbox checklist does not offer `Gone` and pre-ticks only `INBOX`. After confirming, the stored Mandate has no Grant naming `Gone`, and exit is 0.
6. AC6: A malformed file fails with an error naming the problem, before any Credential is asked.
   - Each of these inputs fails with exit 1 and stderr JSON `error.code == "invalid_mandate_file"`, with a message naming the problem:
     - (a) invalid TOML syntax: the message names the line;
     - (b) an unknown key `password = "hunter2-file-secret"` in an `[[accounts]]` entry: the message names `password` and does not contain `hunter2-file-secret`;
     - (c) `tls = "none"`;
     - (d) `port = 0`;
     - (e) `operation = "send"`;
     - (f) a Grant whose `account` is not among the file's Accounts: the message names that login;
     - (g) no `[[accounts]]`;
     - (h) two Accounts with the same login;
     - (i) a path that does not exist.
   - In every case the transcript has no hidden prompt and no prompt of any kind, the fake server records no connection and the store is untouched. The refusal checks (non-interactive terminal, sandbox indicator) still take precedence over file errors.
7. AC7: Declining the export offer writes nothing.
   - Input: the AC1 session, but the export offer is answered no, with the current directory set to a fresh temp directory (or the default path checked).
   - Expected: exit 0, the Mandate is stored, no path prompt is shown, and the temp directory is still empty. When the store write fails or the token reveal fails, no export offer is shown at all.

## Out of scope
- Any edit or update command for a stored Mandate (ADR-0003). Exporting an already stored Mandate later (e.g. `mandate export <id>`): export is offered only at the end of `mandate create`.
- Holding Credentials, Agent Tokens or any secret in the file, or reading a Credential from it.
- Per-operation mailbox lists in the guided flow. Mailboxes stay ticked once per Account, as in ticket 03. A hand-written file whose Grants differ per operation is pre-ticked with the union (see Notes).
- A file format version field, migration, or a schema for third-party tools.
- `mandate list` / `mandate delete` changes, MCP, audit log.

## Notes for reviewers
- Security:
  - The export types must be separate from `Account` and have no Credential field. AC2 checks the bytes.
  - The `--from` file is untrusted input. It is parsed with `deny_unknown_fields`, so a `credential` or `password` key is rejected rather than silently ignored. Error messages must not echo values from the file: the `toml` crate's default error `Display` quotes the source line, which could contain a pasted secret, so the builder must build the message from the error's key or span and line only (AC6 b).
  - Names, logins, hosts and mailbox names from the file are displayed only through `escape_controls`.
  - The file only pre-fills the flow. Every Credential is still typed and live-checked (AC4), so an Agent that writes a file cannot widen a Mandate (ADR-0003). `--from` runs only after the TTY and sandbox refusal checks.
  - The export is written with exclusive create, so it cannot clobber or follow a symlink to an existing file. Its contents are not secret.
  - Union pre-tick: a hand-crafted file with different mailboxes per operation pre-ticks the union for every ticked operation. That could widen access if the human accepts the defaults blindly. The summary before confirmation shows the result. Reviewers may prefer rejecting such files as malformed instead.
- UX: the operator (the human, in their own terminal) is the only user, never an Agent. Flow:
  1. `mandate create` → confirm summary → token shown once → "Export…?" → path.
  2. Later, `mandate create --from file` → pre-filled name → keep each Account? → pre-filled settings → Credential → live check → pre-ticked mailboxes and operations → summary → token → export offer again.
  3. Then `mandate delete <old id>`.

  Existing `tests/create.rs` scripts must answer the new export offer, which comes after the reveal.
- UI: terminal prompts and messages only, through the Terminal port (`/dev/tty`). New are the export confirm and path prompts, the keep-Account confirm, defaults and pre-ticks on existing prompts, and the "unavailable" message. Stdout stays empty on success, as today. `--help` for `mandate create` documents `--from` and still lists no Credential option.
