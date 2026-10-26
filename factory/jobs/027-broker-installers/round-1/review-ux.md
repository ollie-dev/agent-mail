VERDICT: PASS

## Findings
- [follow-up] uninstall.ps1:~178 and ~215: when the operator answers "no" the script prints `Keeping the stored Mandates and the audit log.` and later `Kept the stored Mandates and the audit log.`, two lines saying the same thing → a later pass can drop one.
- [follow-up] install.ps1 relaunch path: when the elevated window fails, the parent only prints `the installation failed; its window showed why`, and the reason is lost once the window closes → a later job could pass the error back (for example through a file in the install folder) so the parent can print it.

## Checked
- Read: spec.md, the full diff (install.ps1, uninstall.ps1, src/broker.rs, src/broker/windows.rs, src/store.rs, src/audit.rs, the two workflows, scripts/verify-release.sh), and build.md after the diff.
- Ran: `cargo test --locked --test release job027`, 10 passed (static elevation, checksum-before-extract, SDDL and quoted path, parse under each shell, relaunch length, publish assets, verify-release, CI step). `cargo test --locked --test broker job027`, 2 passed (purge deletes every Mandate and the audit folder; only the single argument `purge` purges).
- Not run: the elevated install, update and uninstall flows (this shell is not elevated, and running them would change the machine). I checked them by reading the code against the spec.
  - Error and refusal texts match the spec: checksum mismatch, malformed checksum file, download failure, wrong zip contents, non-x64, UAC declined with "nothing was installed" / "nothing was removed", no confirmation possible, purge failure, previous version restored.
  - Success lines match too: `Installed agent-mail <version>`, `agent-mail was removed.`, `agent-mail is not installed.`
  - The prompt text and the y/yes handling match.
  - The install and uninstall scripts relaunch once, only behind the Administrators check. The URL is passed only as an argument. Staging happens inside the install folder, and paths come from `GetFolderPath`.
