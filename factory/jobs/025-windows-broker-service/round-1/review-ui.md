VERDICT: PASS

## Findings
- No blocking UI issues. The only rendered surfaces are the hidden `broker` subcommand's error output and the new error messages. They are short, single-sentence and state the next step.

## Checked
- Read: the diff stat, the diff of src/core.rs, src/lib.rs, src/main.rs and src/store.rs (error codes, messages, `hide = true`), and the new error messages in src/core.rs (store_no_logon_session, invalid_request, elevation_required).
- Ran: `agent-mail --help` (a debug build in the worktree). `broker` is not listed and the rest of the help is unchanged. `agent-mail broker` printed one JSON error with code `service_required` and exited 1. The message is a single line with the next step (`sc.exe start agent-mail` in an elevated terminal).
