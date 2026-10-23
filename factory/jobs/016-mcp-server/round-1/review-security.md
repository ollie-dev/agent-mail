VERDICT: PASS

## Findings

None.

## Checked
- Read: the full diff `git diff main...factory/016-mcp-server` (CLAUDE.md, README.md, src/core.rs, src/lib.rs, src/main.rs, src/mcp.rs, tests/draft.rs, tests/support/mod.rs; tests/mcp.rs in part), then spec.md and build.md.
- src/mcp.rs `call_tool`: the token comes only from `env`. `params._meta` and every field besides `name`/`arguments` are never read. Each valid call goes through `core::execute_reporting`, and the adapter caches nothing between calls (it has no state besides the borrowed `env`/`context`). No `Command::Mandate*` is built anywhere, and `serve` never touches the Terminal.
- src/mcp.rs `build_command`/`Arguments`: unknown keys are rejected before any value is read. Types are strict (no string-to-int/bool coercion, `limit` through `as_u64` then `Limit::new`, `to` items must be strings). Every string goes through the same `pub(crate)` validators the clap parsers use (`account_login`, `reference`, `search_text`, `gmail_query`, `date`, `mailbox_name`, `address`, `subject`, `body`), so CR/LF/NUL are kept out of IMAP commands and RFC 5322 headers just as on the CLI. The `flag` name is unvalidated, as on the CLI; the core maps it to seen/flagged or `unsupported_flag`. The CLI conflicts (set/clear, subject/reply_to, account/reply_to, gmail with other criteria) are mirrored.
- Error messages: `invalid_arguments` messages echo only static validator text and property names. Unknown keys and unknown tool/method names go through `echo_name` (control and format characters escaped, capped at 64 chars), and all output is JSON-encoded. The token value is never echoed.
- `render_report` reuses the CLI's `render_outcome`/`render_error`/`render_warning`, so the redaction is the same as on the CLI. The warning carries positions only.
- `core::no_token_presented`: a pure check on env, using the same empty-field rule as `open_presented`; nothing leaks.
- src/main.rs: the `mcp` branch writes only JSON-RPC to stdout, and an I/O error goes to stderr without secrets. The `log` crate is still `max_level_off` (Cargo.toml unchanged; no new dependency, Cargo.lock untouched). `grep -rn 'println!\|print!(\|dbg!' src/` found no stray stdout writes.
- Test fixtures: tests/support helpers and tests/mcp.rs use the existing fake `CREDENTIAL` and generated tokens, with no real secrets. `ac4_no_secret_leaves_in_a_whole_session` asserts that neither token, `CREDENTIAL` nor the token hashes appear in the raw output.
- Ran: `cargo test`: all suites ok (tests/mcp.rs 31 passed). `cargo clippy -- -D warnings`: clean. `cargo fmt --check`: clean.
- Ran: the built binary `agent-mail mcp` with `AGENT_MAIL_TOKEN=SECRETTOKENVALUE`, sending an unknown argument key containing ESC, the tool name `"mandate_create\n"`, `list` with `_meta.AGENT_MAIL_TOKEN`, and a non-JSON line. The ESC and LF came back escaped. `mandate_create` gave a -32602 error. `_meta` was ignored (the env token was used, giving `invalid_token` against the empty Keychain). The bad line gave -32700 with id null, and the process exited 0. The token value did not appear in the output.
