# Short help text and error messages

## Goal
Every `--help` and `-h` page of `agent-mail`, and every error and warning message the core returns, follows the CLAUDE.md convention: short paragraphs of at most about 300 characters that say each thing once. The global `--text` flag is described the same way everywhere. The ui reviewer checks the convention on every later job. This job covers follow-ups F3, F7, F9, F13 and F17 in `factory/follow-ups.md`.

## Context
- Stack: Rust, one binary crate. Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Files this touches:
  - `src/lib.rs`: the clap doc comments and the `after_help` of `Cli`, `CliCommand`, `MandateCommand` and the option structs. In clap derive, the lines of one doc-comment paragraph are joined into one rendered paragraph, and a blank `///` line starts a new one, so splitting a paragraph means adding blank doc lines and trimming text.
  - `src/core.rs`: `CoreError::body` messages and the `invalid_token` warning messages (around line 1026).
  - `.claude/agents/review-ui.md`: the "Look for" list.
  - Tests: `tests/cli.rs` for help text and messages (or a unit test in `src/core.rs` if some `CoreError` variants cannot be built from outside the crate), and `tests/project.rs` for the reviewer file.
- How long things are today: `draft --help` has a paragraph of about 1090 characters, `move` about 830, `flag` about 740, `search` about 660, `mcp` about 650, `read` about 640, `mandate create` about 470, the root `after_help` about 440, `whoami` about 390, and the `search --gmail` long help block about 370. Measured with the sample values in AC5, the `proxy_refused` message is about 320 characters and the `unsupported` message about 310.

**Paragraph rule.** The tests use this definition. Split the rendered help output into blocks at lines that are empty or hold only whitespace. Trim each line of a block and join the lines with single spaces. The result is one paragraph, measured in `chars()`. One exception: in a block whose first line is `Commands:`, each line is measured on its own, because that block is a list.

## Acceptance criteria
1. AC1: Running `agent-mail <args> --help` and `agent-mail <args> -h` for each of these `<args>` exits 0 and prints no paragraph (per the paragraph rule) longer than 300 characters: (none), `list`, `search`, `read`, `flag`, `move`, `draft`, `whoami`, `mcp`, `mandate`, `mandate create`, `mandate list`, `mandate delete`. That is 26 invocations, driven in-process through `agent_mail::run`.
2. AC2: Shortening keeps the facts an Agent depends on. Each `--help` output still contains these substrings:
   - root: `AGENT_MAIL_TOKEN`, `--account`, `agent-mail whoami`
   - `search`: `X-GM-EXT-1`, `unsupported`
   - `read`: `20,000`, `stale_reference`
   - `flag`: `unsupported_flag`, `stale_reference`
   - `move`: `move_unsupported`, `stale_reference`
   - `draft`: `drafts_not_found`, `Re: `
   - `whoami`: `invalid_tokens`
   - `mcp`: `invalid_arguments`, `agent-mail mandate`
   - `mandate create`: `--from`
   - `mandate delete`: `revocation_incomplete`
3. AC3: The global `--text` help line in `agent-mail --help` and `agent-mail -h` no longer contains `put it before the subcommand`. It reads exactly: `Print human-readable text instead of JSON; may also follow the subcommand, except for search, whose --text is a search criterion`. Behaviour is unchanged: `agent-mail --text whoami` and `agent-mail whoami --text`, run with the same valid token, print the same stdout.
4. AC4: `agent-mail search --help` gives the way to get human-readable search output once, not twice: the substring `agent-mail --text search` appears exactly once in the output. The `--text` option line of `agent-mail search -h` still starts with `Messages whose headers or body contain S`.
5. AC5: Each error message is at most 300 characters. Build every `CoreError` variant with these sample values: account `agent@example.com`; mailbox `INBOX`; host `imap.example.com`; port `993`; variable `HTTPS_PROXY`; accounts `["agent@example.com", "other@example.com"]`; mandates `["aaaa1111", "bbbb2222"]`; id `aaaa1111`; deleted `1` and unreadable `2`; operation `read`, `flag`, `move` and `draft` wherever a variant takes one; `InvalidDraft` with each of `to`, `subject` and `body`; `InvalidMandateFile` with problem `line 3: unknown operation "send"`. For each one, `body().message.chars().count()` is at most 300. The test lists the variants with an exhaustive `match` (no `_` arm), so a new variant without a sample fails to compile.
6. AC6: The `proxy_refused` message, with the AC5 sample values, is at most 300 characters and still contains `imap.example.com:993`, `agent@example.com` and the words `allowed domains`. The `unsupported` (Gmail search) message is at most 300 characters and still contains `--gmail`.
7. AC7: The `hardened_runtime_missing` message says once that the Keychain was not accessed. It reads exactly: `this agent-mail binary is not signed with the hardened runtime, so it refused to access the Keychain; ask the human to reinstall agent-mail from ` followed by `RELEASES_URL`.
8. AC8: The `invalid_token` warning is at most 300 characters, whether the list holds one position (`[2]`) or three positions (`[1, 3, 4]`).
9. AC9: In `.claude/agents/review-ui.md`, the "Look for" list has a bullet for the CLAUDE.md convention. It names help text and error messages, the limit of about 300 characters per paragraph or line, and saying each thing once. A test in `tests/project.rs` reads the file, finds a line starting with `- ` in the "Look for" section, and checks that the line contains `help text`, `error message`, `300` and `once`.
10. AC10: `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` all pass.

## Out of scope
- MCP tool descriptions and server instructions in `src/mcp.rs`. Several run 300 to 740 characters, but they are JSON strings for a host, not paragraphs a human reads. This becomes a follow-up.
- README.md and SKILL.md prose paragraphs, and a test that scans them (F22).
- Changing error codes, JSON fields, exit codes or any behaviour. Only wording changes.
- JSON-shaped usage errors and other changes to make errors readable for an Agent (F2, F4, F14, F15), and the proxy error split (F8, F10).
- The text of clap's own built-in errors (for example "unexpected argument").

## Notes for reviewers
- Security: no credential, mailbox or untrusted-input path changes. Error messages must still never echo a credential or token. Trimmed messages still interpolate only the account, mailbox, host, port, variable name, Mandate IDs and the Mandate file problem they already show.
- UX: the readers are the operator (the human reading `--help` and `mandate` errors) and the Agent (reading JSON `message` fields and `--help`). Every message must still give the next step it gives today (retry, `agent-mail search` again, ask the human). AC2 and AC6 guard the key facts. Reviewers should check that the trimming did not drop any other next step.
- UI: the rendered surfaces are every `--help` and `-h` page and the `message` of every error and warning (JSON on stderr, and the `--text` form). Check them at 80 columns. Splitting paragraphs adds blank lines in the long help, and the short `-h` help shows only each command's first line, so check that that first line still stands on its own. The AC3 wording is set exactly so it can be tested. If a reviewer prefers different words, change the AC with them.
