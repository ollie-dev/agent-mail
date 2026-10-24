# Small code cleanups: token cap, parse once, rejected `--to` audit, audit `mandate` per command, drop guarded `keys()`

## Goal
This job clears five small follow-ups from `factory/follow-ups.md`: F1, F5, F6, F12 and F16.
- An Agent that puts too many tokens in `AGENT_MAIL_TOKEN` gets a clear `too_many_tokens` error before any store lookup, so it never sets off a long run of Keychain prompts.
- The MCP `draft` tool's handling of a rejected `--to` is pinned down by a test.
- The binary parses its arguments once.
- The tests check which Mandate each audit line names.
- The `GuardedStore<KeychainStore>::keys` method has no caller and goes.

## Context
- Stack: Rust, one binary crate `agent-mail`. Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- F5 (token cap):
  - `src/core.rs`: add `pub const MAX_TOKENS: usize = 8`. `open_presented` counts the distinct non-empty fields of `AGENT_MAIL_TOKEN` before it opens any token. Fields are split on `,` and trimmed of ASCII whitespace, as today. If there are more than `MAX_TOKENS`, it fails with a new `CoreError::TooManyTokens`.
  - The new error has code `too_many_tokens`, no `operation` and no `account`. Its message is short and is added to `CoreError::body`.
  - `open_presented` already serves every Agent command and `whoami`, over both the CLI and MCP, so the cap applies to all of them.
  - `tests/cli.rs` `error_samples` gets the new variant.
  - Docs: `README.md` (the paragraph on several tokens) and `plugin/skills/agent-mail/SKILL.md` (the error list).
  - Tests: `tests/tokens.rs`, plus one MCP parity case in `tests/mcp.rs`.
- F1 (rejected `--to` from the MCP `draft` tool):
  - `src/mcp.rs` `draft_command` already rejects a bad `to` item through `crate::address`. The rejection is `invalid_arguments` and comes before the core runs, so no audit line is written, like the CLI's clap `value_parser = address`.
  - The core's `"recipients":[]` entry (`src/core.rs`, the `Command::Draft` arm, "a rejected value is never logged") can only be reached by in-process core callers.
  - This job confirms that with a test in `tests/mcp.rs`. It also rewrites the comment at that spot in `src/core.rs` to say that both adapters reject a bad address before the core runs. Core behaviour does not change.
- F6: `tests/tokens.rs` `ac5_every_agent_command_warns_and_still_succeeds` also checks the audit line of each command.
- F12 (parse once):
  - `src/lib.rs` splits `run` into `pub fn parse(args) -> Result<Cli, RunOutput>` and `pub fn run_cli(cli: Cli, env, context) -> RunOutput`. `parse` returns the help, version and `invalid_arguments` output exactly as `run` does today.
  - `run` stays, as `parse` followed by `run_cli`, so the existing in-process tests keep working.
  - `src/main.rs` calls `agent_mail::parse` once. On `Err` it writes that output and exits with its code. On `CliCommand::Mcp` it serves MCP. Otherwise it calls `run_cli`.
  - Tests: `tests/cli.rs`.
- F16: `src/store.rs` loses the `#[cfg(target_os = "macos")] impl GuardedStore<KeychainStore> { pub fn keys ... }` block. `KeychainStore::keys` stays, since `tests/keychain.rs` and `entries` use it. Test: `tests/selfcheck.rs` or `tests/project.rs`.
- The builder ticks F1, F5, F6, F12 and F16 in `factory/follow-ups.md` with `→ 022-small-code-cleanups`.

## Acceptance criteria
In the criteria below, "the env" is the `tests/tokens.rs` `Env`, with `LOGIN` on the fake IMAP server. "A valid token" means a token from `env.add(...)` for a Mandate with a `list` Grant on `INBOX` for `LOGIN`.

1. AC1: At most `MAX_TOKENS` (8) distinct tokens are accepted.
   - Setup: `AGENT_MAIL_TOKEN` holds 1 valid token followed by 7 distinct malformed tokens (`garbage1` to `garbage7`).
   - Expected: `agent-mail list` exits 0. stderr has one `invalid_token` warning with `positions` `[2,3,4,5,6,7,8]`.
   - With the same valid token repeated 20 times (`t,t,...,t`), `list` exits 0 with empty stderr, since repeats count once.
2. AC2: More than 8 distinct tokens is `too_many_tokens`, for Agent commands.
   - Setup: `AGENT_MAIL_TOKEN` holds 9 distinct valid tokens, each for a different Mandate.
   - Expected: `agent-mail list` exits 1 with empty stdout. stderr is one JSON line whose `error` object has `code` `too_many_tokens` and a `message` that names `AGENT_MAIL_TOKEN` and the limit `8`. The message is at most 300 characters. The error has no `account` or `operation` key.
   - The fake IMAP server sees 0 connections.
   - The last audit line is `{"time":T,"operation":"list","outcome":"denied","reason":"too_many_tokens"}`, with no `mandate` key. This matches the shape of the existing `invalid_token` entry in `tests/audit.rs`.
   - No token appears in stdout, stderr or the audit log.
   - With `--text`, stderr starts with `error: too_many_tokens: `.
3. AC3: The cap is checked before any store lookup.
   - Setup: a test `MandateStore` wrapper around `InMemoryStore` that counts `get` calls, and `AGENT_MAIL_TOKEN` holding 9 distinct malformed tokens.
   - Expected: both `list` and `whoami` fail with `too_many_tokens`, and the wrapper counts 0 `get` calls.
   - With 8 distinct, well-formed but unknown tokens, the count is 8 and the error is `invalid_token`.
4. AC4: `whoami` and MCP apply the same cap.
   - Expected: with the 9 tokens of AC2, `agent-mail whoami` exits 1 with `too_many_tokens` and writes no audit line.
   - In `tests/mcp.rs`, the `list` tool with the same 9 tokens returns `isError` `true`, and its `structuredContent.error.code` is `too_many_tokens`. Its error object equals the CLI's.
5. AC5: The MCP `draft` tool rejects an invalid recipient with no audit line and no echo.
   - Call: in `tests/mcp.rs`, the `draft` tool with `{"to": ["carol@example.com", "not-an-address-zq7"], "subject": "S", "body": "B"}` and a valid token holding a `draft` Grant.
   - Expected: the result has `isError` `true` and `structuredContent.error.code` `invalid_arguments`. The message names `` `to` `` and does not contain `not-an-address-zq7`.
   - The audit log has no line, and the fake IMAP server sees 0 connections.
   - The same input through the CLI (`draft --to carol@example.com --to not-an-address-zq7 --subject S --body B`) also exits 2 with `invalid_arguments` and writes no audit line.
6. AC6: Every Agent command's audit line names the Mandate that ran it.
   - Test: `ac5_every_agent_command_warns_and_still_succeeds` in `tests/tokens.rs`, with tokens `garbage,<token of Mandate aaaa1111>`.
   - Expected: for each of `search`, `read`, `flag`, `move` and `draft`, the audit log grows by exactly one line. That line's `mandate` is `"aaaa1111"`, its `operation` is the command's operation name, and its `outcome` is `allowed`.
7. AC7: `run_cli` on a parsed `Cli` matches `run`.
   - Test: in `tests/cli.rs`, for each of `["agent-mail", "whoami"]`, `["agent-mail", "--text", "whoami"]` and `["agent-mail", "mandate", "list"]` with an empty in-memory store, compare `run_cli(Cli::try_parse_from(args).unwrap(), &env, &ctx)` with `run(args, &env, &ctx)`.
   - Expected: the two give the same `stdout`, `stderr` and `code`.
   - `parse(["agent-mail", "--bogus"])` returns `Err` with code 2 and an `invalid_arguments` JSON line on stderr. `parse(["agent-mail", "--help"])` returns `Err` with code 0 and help text on stdout.
8. AC8: `main.rs` parses once.
   - Test: a check in `tests/cli.rs` reads `src/main.rs` and requires exactly one call to `agent_mail::parse` and no `try_parse_from` or `Cli::parse`.
   - The existing binary tests keep passing, including the `mcp` subcommand test in `tests/mcp.rs` and the `--help`/usage-error tests in `tests/cli.rs`.
9. AC9: The guarded `keys()` is gone.
   - Test: a check reads `src/store.rs`. It requires that `impl GuardedStore<KeychainStore>` does not appear and that `pub fn keys` appears exactly once, on `KeychainStore`.
   - `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass.

## Out of scope
- Making the cap configurable, or changing how tokens are split, trimmed or deduplicated.
- Changing what the core logs for an invalid `to` when it is called in-process (`"recipients":[]` stays).
- Any other follow-up in `factory/follow-ups.md`, including F11, F18 and F19, which are a separate backlog item.
- Other refactors of `run`, the MCP loop or the store port.

## Notes for reviewers
- Security:
  - The token cap limits how many Keychain lookups, and so how many access prompts, one untrusted `AGENT_MAIL_TOKEN` value can cause. The cap is checked before any store call (AC3).
  - The new error and its audit line never hold a token or a token's position list (AC2).
  - The F1 test pins down that a rejected recipient from either adapter is never logged or echoed (AC5).
  - Removing the guarded `keys()` leaves every remaining `GuardedStore` method behind the self-check, and adds no unguarded path. `KeychainStore::keys` is only reachable from the opt-in `tests/keychain.rs`.
  - Parsing once must keep the test-support audit redirect in `main.rs` in place before any command runs.
- UX:
  - Agent: holding more than 8 tokens is unusual. The error tells the Agent to cut the list down; it is not a Grant failure. The `too_many_tokens` message should give that next step, for example "set AGENT_MAIL_TOKEN to at most 8 tokens".
  - Operator: nothing changes for the operator.
  - The limit of 8 is a judgement call: one prompt per token on macOS, and several Accounts usually fit in one Mandate. Reviewers may push back on the number.
- UI:
  - The new JSON error line and its `--text` form (`error: too_many_tokens: <message>`).
  - README and SKILL.md mention the code.
  - There is no other UI change.
