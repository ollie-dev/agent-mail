VERDICT: PASS

## Findings
- [follow-up] install.ps1:128: when run through `irm ... | iex` a failure only prints the error to stderr and the shell keeps going with no non-zero status. This is deliberate (it keeps the human's shell open) but a later job could add a short "nothing was installed" line on stdout in that mode, or a `$LASTEXITCODE` hint.

## Checked
- Read: the full diff stat, `install.ps1`, the README, `.github/release-notes.md`, `src/terminal.rs` and `scripts/verify-release.sh` diffs, and spec.md.
- Ran: nothing; I did not run `install.ps1` or the PowerShell tests, and I judged the rendered text from the source.
- Rendered text reviewed: the installer's progress lines (`Downloading ...`, `Checksum verified.`, `Installed agent-mail to <path>`), its PATH hint and its error messages. Each is one short line, says its thing once and uses no colour, so it reads the same piped or with NO_COLOR. The README and release-notes paragraphs are under the 300-character convention per sentence group. The Windows `CONOUT$` writer writes UTF-16 to the console only.
