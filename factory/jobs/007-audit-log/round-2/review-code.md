VERDICT: PASS

## Findings
- [nit] src/audit.rs (append): `create(dir)` then `restrict_dir` leaves a small window where the directory is checked by path and then opened; O_NOFOLLOW on the open covers the symlink case, so this is acceptable.
- [nit] src/audit.rs (append): the 0600 tightening runs before the write, and the mode is not re-checked if another process changes it later. Fine for the threat model.

Round 1 had no blocking items. The human note (tighten existing modes to 0700/0600, open audit.log with O_NOFOLLOW, a test for each) is done. The directory is opened with O_NOFOLLOW|O_DIRECTORY and fchmod-ed. The file is opened with O_NOFOLLOW|O_NONBLOCK, refused unless it is a regular file, and fchmod-ed. Unit tests cover tightening, a symlinked file, a symlinked directory and a FIFO. Integration tests cover the loose-mode and symlink cases end to end (fail closed with audit_unavailable). The README is updated. `cargo test` is green on every target, `cargo clippy --all-targets -- -D warnings` is clean and `cargo fmt --check` is clean.
