VERDICT: PASS

## Findings
- [follow-up] install.ps1:312, uninstall.ps1:166: the elevated window and the calling shell each print a closing line ("Open a new terminal..." then "agent-mail was installed. Open a new terminal to use it."), so the hint appears in two windows. A later job could keep only the caller's line.

## Checked
- Read: `git diff main...factory/027-broker-installers --stat`, the Write-Host/throw/Read-Host lines of `install.ps1` and `uninstall.ps1`, and their elevation, confirmation and error paths (install.ps1 310-365, uninstall.ps1 170-235).
- Ran: no scripts (the real install needs elevation and changes the machine). I judged the text from source only. Messages are one short line each, say what happened and whether anything changed ("nothing was installed", "nothing was removed"), and go to stderr with a prefix. The destructive prompt names the consequence and defaults to N. Output is plain text with no colour, so piped output reads the same.
