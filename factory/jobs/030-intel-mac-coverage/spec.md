# Intel Mac manual check

## Goal
No CI job covers Intel Macs: there is no Rosetta step and no Intel runner, and `macos-latest` is Apple silicon. The maintainer checks Intel Macs (x86_64-apple-darwin) by hand from time to time. After this ships, the README's Releasing text describes that manual check. `scripts/verify-release.sh <tag>` prints which macOS binary it ran `--version` on and which one it skipped, so a run on an Intel Mac shows that the x86_64 binary was really run.

## Context
- Stack: Rust, one binary crate. The release tooling is POSIX `sh`. Test: `cargo test` (CI: `cargo test --locked`). Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`. A native x86_64-apple-darwin toolchain is installed on the build Mac, so `cargo build` / `cargo test --locked` build and test natively there.
- Files touched:
  - `scripts/verify-release.sh`: the macOS loop (the `if [ "$target" = "$host" ]` block) and the header comment.
  - `README.md`: the `## Development` section, in or right after the `**Releasing.**` paragraph.
  - `tests/release.rs`: new `job030_*` tests. A macOS-only behaviour test reuses the existing `temp_dir` helper and the tarball-building approach of the `installer` module.
- Current behaviour. `host` comes from `uname -m`: `arm64`/`aarch64` maps to `aarch64-apple-darwin`, `x86_64` maps to `x86_64-apple-darwin`, and anything else maps to empty. Only the target equal to `host` runs `"$dir/agent-mail" --version` and compares the result with `agent-mail ${tag#v}`. The other target is skipped silently.
- Behaviour test harness (macOS only, `#[cfg(target_os = "macos")]`). It runs `sh scripts/verify-release.sh v9.9.9` with a `PATH` that puts a temporary stub directory first, followed by the real `/usr/bin:/bin`. The stubs are:
  - `gh`. `release download <tag> --repo R --dir D --pattern P...` copies each fixture file named by a `--pattern` into `D`, if that fixture exists. `attestation verify ...` exits 0.
  - `codesign`. It prints `CodeDirectory v=20500 size=1 flags=0x10002(adhoc,runtime) hashes=1+0 location=embedded`.
  - `uname`. It prints the architecture given by an env var: `arm64` or `x86_64`.
  - `git`. `-C <root> show <tag>:<name>` prints the fixture copy of `<name>`.

  The fixtures are:
  - `agent-mail-<target>.tar.gz` for each macOS target, each holding exactly one executable script `agent-mail` that prints `agent-mail 9.9.9`, plus a matching `.sha256` file in `shasum -a 256` format.
  - `agent-mail-x86_64-pc-windows-msvc.zip`, made with `/usr/bin/zip` and holding exactly `agent-mail.exe` (any bytes), plus its `.sha256`.
  - `install.ps1` and `uninstall.ps1` with fake content.

  The real `shasum`, `tar`, `unzip` and `cmp` are used, as is the repo's real `scripts/sign-macos.sh`, which calls the stubbed `codesign`. All values are fake.

## Acceptance criteria
1. AC1: For the target that matches the host, after the version check passes, `verify-release.sh` prints exactly one stdout line `ran agent-mail --version on <target>`. Test (macOS behaviour harness, `uname` stub prints `arm64`): the exit status is 0 and stdout contains the line `ran agent-mail --version on aarch64-apple-darwin`.
2. AC2: For each macOS target that does not match the host, `verify-release.sh` prints exactly one stdout line `skipped agent-mail --version on <target>: not the host architecture (<host>)` and does not run that binary. Test (harness, `uname` prints `arm64`): stdout contains `skipped agent-mail --version on x86_64-apple-darwin: not the host architecture (aarch64-apple-darwin)` and does not contain `ran agent-mail --version on x86_64-apple-darwin`.
3. AC3: On an Intel host the roles swap. Test (harness, `uname` prints `x86_64`): the exit status is 0. Stdout contains `ran agent-mail --version on x86_64-apple-darwin` and `skipped agent-mail --version on aarch64-apple-darwin: not the host architecture (x86_64-apple-darwin)`, and it ends with the line `release v9.9.9 verified`.
4. AC4: On a host whose architecture is neither `arm64`/`aarch64` nor `x86_64`, both macOS targets are skipped with `(unknown)` as the host, and the script still succeeds as it does today. Test (harness, `uname` prints `riscv64`): the exit status is 0, stdout contains `skipped agent-mail --version on aarch64-apple-darwin: not the host architecture (unknown)` and the same line for `x86_64-apple-darwin`, and stdout contains no `ran agent-mail --version` line.
5. AC5: A version mismatch still fails, and no `ran` line is printed for it. Test (harness, `uname` prints `x86_64`, the x86_64 fixture binary prints `agent-mail 0.0.1`): the exit status is non-zero, stderr contains `expected agent-mail 9.9.9, got: agent-mail 0.0.1`, and stdout does not contain `ran agent-mail --version on x86_64-apple-darwin`.
6. AC6 (static, every OS): The text of `scripts/verify-release.sh` contains `echo "ran agent-mail --version on $target"` and `skipped agent-mail --version on $target: not the host architecture`. The header comment (the leading `#` lines) mentions both `ran` and `skipped` for the `--version` step. On unix, `sh -n scripts/verify-release.sh` succeeds and the file stays executable.
7. AC7 (static, every OS): In `readme_section("## Development")`, the text after `Releasing` describes the manual Intel check. It contains `Intel`, `x86_64-apple-darwin`, `cargo test --locked`, `scripts/verify-release.sh vX.Y.Z`, `ran agent-mail --version on x86_64-apple-darwin`, and a statement that no CI job covers Intel Macs, with the substring `no CI job`. Each prose paragraph it adds is at most 300 characters. The existing `ac10_readme_install_section_and_releasing_paragraph` test still passes.
8. AC8: `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass. The behaviour tests from AC1–AC5 run on macOS, on both Apple silicon and Intel, because they stub `uname`. They are compiled out on Windows.

## Out of scope
- Any CI or release-workflow change: no Rosetta step, no Intel runner, no new job in `ci.yml` or `release.yml`.
- Running the x86_64 binary under Rosetta on Apple silicon.
- Changes to `install.sh`, `sign-macos.sh`, the Windows checks in `verify-release.sh`, or the installer scripts.
- Updating `CLAUDE.md` or `.github/release-notes.md`.
- A script that automates the manual Intel check.

## Notes for reviewers
- Security: nothing touches credentials, Mandate stores or mailbox access. `verify-release.sh` only gains two informational `echo` lines. The checksum, signature, attestation and version checks must stay in place and keep their order (download, checksum, contents, extract, `sign-macos.sh --check-only`, attestation, then the run or skip). The behaviour tests use stubbed `gh`/`codesign`/`git`/`uname` with fake fixtures in temporary directories. They must not call the network or the real GitHub, and must not touch the user's repo tags.
- UX: this is for the maintainer (operator). The flow on an Intel Mac is: `cargo build`, then `cargo test --locked`, then `scripts/verify-release.sh vX.Y.Z` after a release. The output then shows which binary ran and which was skipped. The README text should say each thing once and stay short.
- UI: terminal output only (the two new stdout lines of `verify-release.sh`) and one README paragraph. There are no pages.
