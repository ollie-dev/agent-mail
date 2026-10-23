VERDICT: PASS

## Findings
- [fix-now] src/lib.rs (usage_message, InvalidSubcommand arm): the unknown-command message reads "unknown command; did you mean `list`?; run `agent-mail --help` for usage", with two semicolons in a row, because the pointer is appended with "; " after a sentence that already has a "; " → join so it reads once, e.g. "unknown command, did you mean `list`? Run `agent-mail --help` for usage" or "unknown command (did you mean `list`?); run ...". Done when: `target/debug/agent-mail lsit` and `agent-mail mandate delet` print a message with exactly one `;`, and the tests in tests/cli.rs that match the old wording are updated and pass.

## Checked
- Read: spec.md, full `git diff main...factory/020-agent-readable-errors` (src/lib.rs, core.rs, imap.rs, mcp.rs, create.rs, README.md, SKILL.md; tests skimmed for the AC10 and usage-error cases).
- Ran (debug binary from the worktree): `list --account=`, `search --limit 0`, `read not-a-ref`, `flag <ref>`, `move <ref>`, `whoami --full`, `mandate delete`, `mandate create --bogus`, `mcp --bogus`, `lsit`, `list --bogus=hunter2`, `read <ref> extra`, `draft --to not-an-address ...`, `draft ... --subject --reply-to`, `flag --set --clear`: each exits 2, empty stdout, one JSON `invalid_arguments` line with the right argument and the right `--help` pointer; `hunter2` and `extra` not echoed. `--help`, `-h`, `draft --help`, `help draft`, `--version` print plain text with exit 0; bare `agent-mail` and `mandate` print plain help on stderr with exit 2. `--text lsit` still gives JSON.
- Ran `cargo test` (all green), `cargo clippy -- -D warnings`, `cargo fmt --check` (clean).
- Read README and SKILL.md wording: `draft_refused` and the usage-error paragraph are present and short; the draft_refused message gives a next step.
