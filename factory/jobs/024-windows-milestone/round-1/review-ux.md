VERDICT: PASS

## Findings
- [follow-up] install.ps1:109: when `%USERPROFILE%\.local\bin` is not on PATH, the hint only says "add it to your user Path environment variable". A later job could print the exact one-line PowerShell command to do it, as the human will otherwise have to look it up.

## Checked
- Read: `spec.md`, the diff stat, `install.ps1` in full, the diff of `README.md`, `.github/release-notes.md` and `src/terminal.rs`. I did not read `build.md` or the rest of the diff (store, audit, workflows, tests); my verdict rests on the suite results and the manual installer runs below.
- Ran: `cargo test --locked` in the worktree on Windows 11: every test binary ok, 0 failed, and the opt-in tests (Credential Manager, live Gmail) reported as ignored. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` were clean.
- Ran: `install.ps1` through `Get-Content install.ps1 -Raw | iex` (the README one-liner path) with `USERPROFILE` in a temp dir and a `file:///` download dir holding a zip and a correct `.sha256`. It printed "Downloading ...", "Checksum verified.", "Installed agent-mail to <path>\.local\bin\agent-mail.exe" and a PATH hint, and returned rc 0.
- Ran: `powershell.exe -NoProfile -File install.ps1` against a download dir with no zip. It printed `agent-mail installer: could not download <url>; nothing was installed` and returned rc 1. The message says what failed and that nothing was installed.
- Not run: PowerShell 7 (`pwsh` is not installed here), and the checksum mismatch, malformed checksum and wrong-zip-content paths by hand. The `tests/release.rs` Windows tests cover them and passed.
- Not run: the real Credential Manager flow (`mandate create` then `list`), which is opt-in.
- Compared with the spec: the README Install, Windows, Audit log and macOS-only hardened-runtime text and the release notes match the acceptance criteria. The Windows terminal path uses `CONOUT$` with `WriteConsoleW`.
