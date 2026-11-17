VERDICT: PASS

## Findings
- No UI surface in this change.

## Checked
- Read: spec.md, `git diff main...factory/034-machine-port --stat` and the non-machine.rs src diff.
- Ran: grep of added/removed lines in the diff for print, help, about, message and clap (none), and grep of src/machine.rs for println, eprintln and clap (none). No rendered output, help text or error message changes.
