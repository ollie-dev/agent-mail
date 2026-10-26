VERDICT: PASS

## Findings

None.

## Checked
- Read: the full diff `git diff main...factory/030-intel-mac-coverage` (one commit, 4b3ac8d): `README.md`, `scripts/verify-release.sh`, `tests/release.rs`.
- Read: all of `scripts/verify-release.sh` after the change. The two new `echo` lines print only `$target` (from the fixed `targets` list) and `${host:-unknown}` (from a fixed `case` mapping of `uname -m`; nothing from `uname` reaches output unmapped). Neither prints the tag, a path or a gh token. The checks still run in the same order: download, the sha256 format check and `shasum -c`, the exactly-one-member check, extract, `sign-macos.sh --check-only`, `gh attestation verify`, then the run or the skip. The `ran` line comes only after the version comparison passes, so a mismatch still exits 1 before it. The skip branch runs nothing. The Windows and installer sections are unchanged.
- Read: the `verify_release` test module and the `temp_dir` helper. The stubs (`gh`, `codesign`, `uname`, `git`) go first on `PATH`, ahead of `/usr/bin:/bin`. The `gh` stub copies only from `$FAKE_FIXTURES` and exits 1 on any unexpected subcommand, so nothing reaches the network or the real GitHub. The `git` stub reads only fixture files and never touches the repo's tags. `TMPDIR` points to a fresh temp dir. All fixture values are fake: fake version strings, `fake exe` bytes and `# fake install.ps1`. The tests add no new dependencies.
- Read: the README addition. It holds no credentials, tokens or account data.
- Read: `build.md`, after the diff. Its claims match the code.
- Ran: `cargo test --locked --test release job030`: 7 passed, 0 failed.
- Ran: grep of the diff for `password|token|secret|ghp_|github_pat`. The only hit is an unchanged README context line about the app password in the live-test paragraph. The diff adds no credential.
