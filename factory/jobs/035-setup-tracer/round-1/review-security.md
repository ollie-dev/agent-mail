VERDICT: PASS

## Findings
None.

## Checked
- Read: the full diff `git diff main...factory/035-setup-tracer` (CLAUDE.md, src/create.rs, src/lib.rs, src/main.rs, src/setup.rs, tests/draft.rs, tests/setup.rs), then spec.md and build.md (no human-notes.md, no earlier round).
- src/create.rs `create_and_store`: the refusal order is unchanged. Interactive check, then `sandbox_indicator`, then `store.check()`, all before any prompt or store write. `create::run` builds the same reveal text, built from `stored_line` plus the token, so there is still a single reveal for `mandate create`. `take_back` keeps the old delete-on-failed-reveal behaviour. `Stored` derives no `Debug`, so the token and Credentials cannot be printed through it.
- src/setup.rs `starts_on_bare_call`: the cheap checks (no Broker, interactive, no sandbox indicator) run before the store is touched. It falls back to help on any store error or unreadable record, so the default is no wizard.
- src/setup.rs `Wizard::reveal` and `reveal_text`: the token goes only to `Terminal::reveal`, at most once, guarded by `revealed`. Both snippets are serialised by `serde_json` from structs that borrow the token, so the `command` path is JSON-escaped and not formatted by hand. The buffers are `Zeroizing`. If the reveal fails, the Mandate is deleted.
- src/setup.rs `cancelled`: the delete offer defaults to No, and a cancel there counts as No. The delete-failure message carries only the Mandate ID.
- src/setup.rs `final_check`: whoami gets a private env holding only the token, and that env string is zeroized afterwards. The message shows `render_whoami`: ID, name, accounts and grants, with no token or Credential, and `escape_controls` on the ID and name. On failure only the error body goes to stderr, with no token.
- src/lib.rs `setup_command`, `render_body`, `error_message`: on the Broker route the wizard refuses before `setup::run`, so the in-process store is never used there. Error messages are static strings, and `--text` still escapes controls.
- src/main.rs: a bare call goes through `agent_mail::run` with the same context, so the Broker route still prints help.
- tests/setup.rs: only placeholder values (`support::CREDENTIAL`, `wrong-password`). `assert_token_once` covers one Reveal entry and no token in stdout or stderr on paths a to e plus the final-check failure, and path f checks that no token appears anywhere. AC9 asserts that `CREDENTIAL` is absent from the transcript, stdout and stderr.
- No change to Cargo.toml or Cargo.lock, so no new dependencies.
- Ran: `cargo test --test setup --test create --test cli`: 30, 39 and 28 passed, 0 failed. `cargo clippy -- -D warnings`: clean. Grepped the diff for added `println`, `eprintln`, `dbg!` and `log::` lines: none.
