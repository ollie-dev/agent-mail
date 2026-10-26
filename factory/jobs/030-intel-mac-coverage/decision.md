APPROVE
All eight criteria have passing tests, all four round-1 reviews pass with no fix-now findings, and the diff only adds two informational echo lines, a README paragraph and tests.

## Acceptance criteria
- AC1: pass (`verify_release::job030_ac1_prints_ran_for_the_host_target`)
- AC2: pass (`verify_release::job030_ac2_prints_skipped_for_the_other_target`)
- AC3: pass (`verify_release::job030_ac3_on_intel_the_roles_swap`)
- AC4: pass (`verify_release::job030_ac4_unknown_host_skips_both_targets`)
- AC5: pass (`verify_release::job030_ac5_version_mismatch_fails_without_a_ran_line`)
- AC6: pass (`job030_ac6_verify_release_prints_ran_and_skipped`)
- AC7: pass (`job030_ac7_readme_describes_the_manual_intel_check`, plus the existing `ac10_readme_install_section_and_releasing_paragraph`)
- AC8: pass (I ran `cargo test --locked` in the worktree on an x86_64 host: exit 0, 29 suites ok, 0 failed; `cargo clippy -- -D warnings` clean; `cargo fmt --check` clean. The behaviour tests are under `#[cfg(target_os = "macos")]` and stub `uname`)

## For the human
Nothing needs a decision. The script change is two `echo` lines. The `ran` line comes after the version comparison and the `skipped` branch runs nothing. The download, checksum, contents, extract, signature, attestation and version-check order is unchanged. The tests stub `gh`, `codesign`, `uname` and `git` with fake fixtures in temporary directories, so they make no network calls and never touch the repo's tags. CI and the release workflows are untouched, as the spec requires.

## Follow-ups
- ui · scripts/verify-release.sh:69: the `skipped` line is about 99 characters wide on a typical host → shorten it if the script's output is ever reformatted.
