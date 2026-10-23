VERDICT: PASS

## Findings
None. Round 1 had no `[blocking]` or `[fix-now]` items. Its one `[follow-up]` was that `cargo test` ran inside the `publish` job, which holds write scopes and the OIDC token. The human note of 2026-10-23 asked for that to be fixed, and commit 256130f does it. `cargo test --locked` and the tag check now run in a separate `test` job with `contents: read` only. `publish` has `needs: test` and no longer runs the tests. The workflow has no cache step, so the `test` job has no way to plant a poisoned cache for `publish` to pick up. The release build in `publish` still runs the build scripts of normal (non-dev) dependencies while the write scopes are held. That cannot be avoided when you build and attest in the same job, and it is not new.

## Checked
- Read: `spec.md`, `human-notes.md`, `round-1/review-security.md`, `decision.md`, then `git diff main...factory/017-self-check-release --stat` and the full `git show 256130f` (the only commit since round 1; it touches only `.github/workflows/release.yml`, `README.md` and `tests/release.rs`). After that I read the round-2 part of `build.md`. Its claims match the code.
- Read: the whole of `.github/workflows/release.yml` as it stands now.
  - Top-level permissions are `contents: read`. `test` has `contents: read`. `publish` has exactly `contents: write`, `id-token: write` and `attestations: write`. `verify` has `contents: read` and `attestations: read`.
  - Every checkout uses `persist-credentials: false`. The actions are still pinned to the SHAs I checked in round 1.
  - `GH_TOKEN` is set only on the Publish step and the Verify step.
  - `github.ref_name` reaches the scripts only through `env`. It is never placed inline in a `run` block.
  - Grepped the workflow for `cache`, `secrets.` and `${{`. There is no cache and no secrets, and the only `${{` uses are `github.ref_name` and `github.token` in `env`.
- Read: the new test `ac3_tests_run_in_a_read_only_job_that_publish_depends_on` in `tests/release.rs`, and its `job()` helper. It asserts that the `test` job has no write scope and that `publish` has `needs: test` and no `cargo test`.
- Ran: `cargo test` in the worktree. Every binary reported `test result: ok`, with 0 failed (the opt-in tests are ignored).
