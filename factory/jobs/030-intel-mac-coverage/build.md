## Round 1
**Changed:**
- `scripts/verify-release.sh`: after the version check passes, the host target prints `ran agent-mail --version on <target>`; every other macOS target prints `skipped agent-mail --version on <target>: not the host architecture (${host:-unknown})` and is not run. Header comment describes both lines. Check order (download, checksum, contents, extract, `sign-macos.sh --check-only`, attestation, run/skip) unchanged.
- `README.md` `## Development`: two short paragraphs after the Releasing text (`**Intel Macs.**`): no CI job covers Intel Macs; the manual flow `cargo build`, `cargo test --locked`, `scripts/verify-release.sh vX.Y.Z`; the output must show `ran agent-mail --version on x86_64-apple-darwin`.
- `tests/release.rs`: macOS-only `verify_release` module with stubbed `gh`/`codesign`/`uname`/`git` (temp dirs, `PATH` = stubs:/usr/bin:/bin, fake fixtures, `TMPDIR` redirected) for AC1–AC5; static tests for AC6 and AC7 on every OS.

**Commits:** 4b3ac8d
**Tests:** `cargo test` -> every suite `test result: ok` (release.rs: 59 passed; 0 failed); `cargo clippy -- -D warnings` and `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | `verify_release::job030_ac1_prints_ran_for_the_host_target` | pass |
| AC2 | `verify_release::job030_ac2_prints_skipped_for_the_other_target` | pass |
| AC3 | `verify_release::job030_ac3_on_intel_the_roles_swap` | pass |
| AC4 | `verify_release::job030_ac4_unknown_host_skips_both_targets` | pass |
| AC5 | `verify_release::job030_ac5_version_mismatch_fails_without_a_ran_line` | pass |
| AC6 | `job030_ac6_verify_release_prints_ran_and_skipped` | pass |
| AC7 | `job030_ac7_readme_describes_the_manual_intel_check` (+ existing `ac10_readme_install_section_and_releasing_paragraph`) | pass |
| AC8 | full `cargo test`, clippy, fmt; AC1–AC5 under `#[cfg(target_os = "macos")]` with stubbed `uname` | pass |

**Review findings:** none (round 1).
