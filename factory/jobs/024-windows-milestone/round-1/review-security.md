VERDICT: PASS

## Findings
- [follow-up] src/audit.rs:158-200: on Windows the audit log is opened with plain create+append. There is no equivalent of the Unix `O_NOFOLLOW` / regular-file refusal, so a symlink or junction (reparse point) at `%LOCALAPPDATA%\agent-mail` or `audit.log` is followed. Under ADR-0002 any same-user process can already rewrite the log, and the spec puts Windows ACLs out of scope, so this does not block the job. → A later job should open with `FILE_FLAG_OPEN_REPARSE_POINT` and refuse a reparse point or a non-regular file, matching the Unix behaviour the README describes.

## Checked
- Read: the full diff `git diff main...factory/024-windows-milestone` (all 20 files), then spec.md and build.md (no human-notes.md, no earlier round).
- src/store.rs `credential_layout`: target names hold only `agent-mail:` plus base64url of the HKDF-derived store key (`seal.rs` `store_key`, not the token). split/join/part_count reject a zero count, a count that does not match the parts, and parts over 2,560 bytes.
- src/store.rs `CredentialManagerStore`:
  - `put` reads every target name before any `CredWriteW`, writes later parts first and the first part last, and rolls back only the parts it wrote. Nothing existing is overwritten. The read-then-write race is accepted under ADR-0002.
  - `get`, `delete` and `entries`: an undecodable or foreign name is counted with an empty value and never targeted, since delete re-encodes the key.
  - Persist is `CRED_PERSIST_LOCAL_MACHINE`, so nothing roams. `StoreError::Unavailable` carries no data, and no value is logged or printed.
  - Unsafe FFI: CredFree is called once on each buffer, and pointer lengths come from the API.
- `RuntimeStore` is fixed by cfg at compile time. src/main.rs still wraps it in `GuardedStore::new` on every OS. `InMemoryStore` stays behind `cfg(any(test, feature = "test-support"))`.
- src/audit.rs: Windows `platform_path` uses `SHGetKnownFolderPath(FOLDERID_LocalAppData)`, not an environment variable. `CoTaskMemFree` is always called, and the path must be absolute.
- src/terminal.rs: the Windows `write_tty` opens `CONOUT$` and uses `WriteConsoleW` on that handle only, never stdout or stderr. `is_interactive` still needs stdin, stdout and stderr to be terminals and `CONOUT$` to open, so an Agent with piped stdio cannot run `mandate create`.
- install.ps1:
  - Checks the arch, and the `.sha256` format with an anchored, case-sensitive regex naming the asset, before use.
  - Compares the hash with `-cne` before anything is extracted. The zip must hold exactly one entry, `agent-mail.exe`, extracted to a fixed path in a fresh temp dir.
  - Staging and rename happen in the bin dir. No `RunAs`, nothing executed, PATH only hinted. Runs in its own `& {}` scope and calls `exit` only under `-File`.
  - The download URL override matches `install.sh`.
- .github/workflows/release.yml and ci.yml:
  - `build-windows` and `test` (matrix) have `contents: read` only, and checkout uses `persist-credentials: false`. Only `publish` holds the write and OIDC scopes.
  - New actions (upload-artifact, download-artifact) are pinned to full SHAs with version comments. The artifact comes from the same run. The zip is in the attestation `subject-path`.
- scripts/verify-release.sh: checks the format, then `shasum -c`, then that `unzip -Z1` lists exactly `agent-mail.exe`, then `gh attestation verify`. It never runs the binary.
- Cargo.toml / Cargo.lock: `windows-sys` 0.61.2 was already in the lock tree (transitive), so the job adds no new crate. Features are limited to Foundation, Credentials, Com, Console and Shell.
- Tests:
  - tests/credential_manager.rs: opt-in, fake values, a Drop cleanup.
  - tests/create.rs and tests/selfcheck.rs: the source checks.
  - tests/audit.rs and tests/release.rs: the Windows installer tests set `USERPROFILE` and `TEMP` to temp dirs.
- README.md: the Windows paragraph states the honest-Agents-only model without hedging. release-notes.md says the same.
- Ran:
  - `cargo build --locked` and `cargo check --release --locked` (no test-support) on Windows: clean.
  - `cargo test --locked` on Windows: every suite ok, 0 failed. Only the opt-in tests are ignored (2 in credential_manager, 1 in gmail_live).
  - A grep of the added diff lines for println, eprintln, dbg!, log and tracing, password, secret and token assignments: no Credential or store value is printed, and no real secret is in a fixture or commit.
  - A grep of `git log -p main..factory/024-windows-milestone` for password literals: none.
