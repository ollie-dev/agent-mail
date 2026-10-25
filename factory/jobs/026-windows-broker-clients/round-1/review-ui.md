VERDICT: PASS

## Findings
- [follow-up] README.md:448-470: the proxy bullets in "Sandbox and proxies" were re-split into more bullets and paragraphs, and the new pipe paragraph sits between "Allowed domains" and the proxy text. This is harmless (it looks done to meet the 300-character paragraph test), but the spec says the rest of the README belongs to Ticket 22 → fold the section into the Ticket 22 rewrite, and move the pipe paragraph after the proxy list.

## Checked
- Read: the diff of README.md, src/lib.rs, src/core.rs, src/create.rs and src/main.rs; the spec; the new error messages in core.rs (broker_unavailable, broker_version_mismatch and elevation_required are each under 300 characters and name the fix).
- Ran: `agent-mail audit --help`, `agent-mail --help` and `agent-mail mandate create --help` from the worktree debug build. Lines are short, the audit command is listed consistently with the other commands, and the elevation note is on the create help.
- Read: the `audit --text` rendering code (time, outcome, operation, then key=value in the spec order, `escape_controls`, "No audit entries."). I did not run it against real log data.
