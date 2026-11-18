VERDICT: PASS

## Findings
- None.

## Checked
- Read: `git diff main...factory/035-setup-tracer` for `src/setup.rs`, `src/lib.rs` and `src/main.rs`, plus the spec. I did not read the test file line by line.
- Ran: `agent-mail --help`. The `setup` line is on one line, not hidden, and fits within 120 columns.
- Ran: `agent-mail setup -h` and `agent-mail setup --help`. Both give the same short page, with `--text`.
- Ran: `agent-mail setup` and `agent-mail setup --text` with no tty. Each gives a single error line of `interactive_terminal_required`, one as JSON and one as `error: …`, and each message is under 300 characters.
- Ran: bare `agent-mail` with no tty. It prints the usual help.
- Read: the wizard prompts and messages in `src/setup.rs`. Each is a short line, and the token appears only in the single `reveal` text.
- Not run: the interactive wizard transcript, because it needs a tty.
