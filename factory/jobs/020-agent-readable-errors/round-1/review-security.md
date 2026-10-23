VERDICT: PASS

## Findings

None. Usage errors now come from clap's error context, not its rendered text, so they echo less than the old plain-text output did. The `draft_refused` error drops the server's text. The combined `missing_token` message names argument keys only (shortened by `echo_name` and escaped), and an invalid-arguments call with a token present already did the same. Nothing is read from the store before parsing finishes. No new dependencies.

## Checked
- Read: the full `git diff main...factory/020-agent-readable-errors` (src, tests, README.md, SKILL.md); then build.md.
- Read `src/lib.rs` `run`, `is_help_or_version`, `usage_message`, `subcommand_path`, and the `CliCommand::Mcp` arm. Only `ContextKind::InvalidArg`/`PriorArg`/`SuggestedSubcommand` are used. Names go through `escape_controls`. For an unknown option, the text is cut at `=`. Unexpected positionals are not echoed. Validator reasons are only `search::InvalidValue(&'static str)`, checked by grepping every `InvalidValue(` constructor: all are static strings, none holds a value.
- Read the text-mode error renderer at `src/lib.rs:968`: it escapes `body.message`, so a server-supplied Drafts mailbox name in `draft_refused` cannot inject control characters.
- Read `src/imap.rs` `append`: only a tagged `NO`/`BAD` becomes `AppendRefused`. That variant carries no server text. Everything else stays `Protocol`.
- Read `src/core.rs`: the `DraftRefused` body, the `draft` mapping that uses the Drafts mailbox, and the `connector_error` fallback. Read `src/create.rs` `failure_reason`.
- Read `src/mcp.rs`: `call_tool` (the no-token branch), `build_command`, `invalid`, `echo_name`, `tool_error`. Argument messages hold keys only, never values. Token values are never echoed (AC13 test).
- Read `src/main.rs`: `mcp` is still served only when parsing succeeds, and everything else goes through `run`.
- Read `tests/support/mod.rs` `refuse_append`, and the new tests in tests/cli.rs, tests/draft.rs, tests/mcp.rs, tests/audit.rs, tests/create.rs.
- Ran the built `target/debug/agent-mail` with 17 invalid argument lists, each holding a marker SECRETn: `-zSECRET1`, `-aSECRET2`, `--bogus SECRET3`, `read SECRET4`, `--since SECRET5`, `--cc SECRET6`, `mandate create --password=SECRET7`, unknown command `SECRET8`/`--text SECRET9`, a bad second `--to SECRET10`, `--limit SECRET12`, `-- SECRET13`, extra positionals `SECRET14 SECRET15`, a duplicate `--body=SECRET16`. Every one gave a one-line JSON `invalid_arguments` error with exit 2, and no SECRET marker appeared in any output.
- Ran `cargo test`: every suite `ok`, 0 failed. Ran `cargo clippy -- -D warnings`: clean. Ran `git diff --stat` on Cargo.toml/Cargo.lock: unchanged, so no new dependencies.
