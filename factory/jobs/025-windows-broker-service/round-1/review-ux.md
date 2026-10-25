VERDICT: PASS

## Findings
- [fix-now] src/store.rs:313: the doc comment holds a BEL control character (0x07) where a backslash and `a` were meant, so it reads `NT SERVICE<BEL>gent-mail` (an unescaped `\a` went through a shell or editor) → write `NT SERVICE\agent-mail`. Done when: `grep -nP '\x07' -r src tests` prints nothing and `grep -n 'NT SERVICE' src/store.rs` shows `NT SERVICE\agent-mail`.

## Checked
- Read: spec.md, the diff stat, and the diff of src/core.rs, src/store.rs, src/lib.rs, src/main.rs (new error codes, messages, hidden subcommand wiring). Did not read build.md or the full broker/windows.rs and tests (UX scope: operator-visible messages and codes).
- Ran: `cargo build` in the worktree, then `target/debug/agent-mail broker` on Windows: exit 1, one JSON line on stderr with code `service_required`, message says what is wrong and gives `sc.exe start agent-mail` in an elevated terminal. `agent-mail --help | grep -i broker`: no output (hidden, as specified).
- Messages for `store_no_logon_session`, `invalid_request`, `broker_version_mismatch`, `elevation_required`, `service_required`, `unsupported` are each short (under 300 characters), say what happened and name a next step. The `unsupported` text on macOS was read in the diff, not run.
- `grep` for `NT SERVICE` found the BEL character in src/store.rs:313 (cat -A shows `^G`).
