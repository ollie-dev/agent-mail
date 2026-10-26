VERDICT: PASS

## Findings
- [follow-up] install.ps1, uninstall.ps1: the elevated window and the calling shell each print a closing line, so the "open a new terminal" hint appears in two windows. A later job could keep only the caller's line.

## Checked
- Read: `git diff main...factory/027-broker-installers --stat`, the Write-Host/throw/Read-Host lines added to `install.ps1` and `uninstall.ps1`, my round-1 review (no blocking or fix-now items), and the round 2 section of `build.md`.
- Ran: no scripts (a real install needs elevation and changes the machine). I judged the text from source. Messages are one short line each, say whether anything changed, and are plain text, so piped output reads the same. The destructive prompt names the consequence and defaults to N.
