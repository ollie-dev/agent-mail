VERDICT: PASS

## Findings
No findings. Selection runs before any Grant check or connection, Grants are evaluated only against the selected Mandate, invalid tokens are reported by position only, and --account is validated in both the CLI and the core. cargo test, clippy --all-targets -D warnings and fmt --check are all green.
