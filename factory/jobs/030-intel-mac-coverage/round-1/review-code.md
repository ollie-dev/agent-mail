VERDICT: PASS

## Findings
- None.

## Checked
- Read: full `git diff main...factory/030-intel-mac-coverage` (`scripts/verify-release.sh` header and macOS loop, the README Intel paragraphs, `tests/release.rs` `verify_release` harness and `job030_ac6`/`ac7`), and `spec.md`. The `ran` echo sits after the version comparison and the `skipped` echo is in the else branch. The check order is unchanged, and `${host:-unknown}` covers an unknown host.
- Ran: `cargo test --locked --test release`: 59 passed, 0 failed. `cargo test --locked` (all targets): every result ok, 0 failed. `cargo clippy --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.
- Criteria: AC1 `job030_ac1` (exit 0, ran line once, after the version output). AC2 `job030_ac2` (skipped line, no ran line, binary not run). AC3 `job030_ac3` (roles swap, last line is `release v9.9.9 verified`). AC4 `job030_ac4` (riscv64, both targets skipped with `(unknown)`, no ran line). AC5 `job030_ac5` (non-zero exit, stderr text, no ran line). AC6 `job030_ac6` (strings, header, `sh -n`, executable bit). AC7 `job030_ac7` (needles, 300-character limit). AC8 is met by the clean runs above. The behaviour tests fail without the script change, since the new lines are absent. The harness stubs `gh`, `codesign`, `uname` and `git`, so there is no network.
