VERDICT: PASS

## Findings
- [nit] src/audit.rs:150-165: `DirBuilder::mode(0o700)` and `OpenOptions::mode(0o600)` only apply on creation. A pre-existing `agent-mail/` directory or `audit.log` with looser modes (e.g. 0755/0644 from a manual `touch` or an older tool) is left as is, so other OS users could read the log. Consider checking the mode after open (`file.metadata()`) and tightening it with `set_permissions` when it is looser than 0600, and likewise for the directory. The builder already notes this; not blocking because entries hold only metadata.
- [nit] src/audit.rs:160-163: `open` follows a symlink at `audit.log`. A same-user process could point it at another file and have agent-mail append JSON lines there. That process could write the file itself, so there is no privilege gain; `O_NOFOLLOW` (`custom_flags(libc::O_NOFOLLOW)`) would still be cheap hardening for later tickets that log Agent-supplied text.
- [nit] src/audit.rs:110-125 / tests: `AuditLog::temporary()` directories are never removed by most integration tests, so each `cargo test` run leaves `agent-mail-test-*` dirs in the system temp dir. They hold only metadata (AC4 proves no secrets), so this is hygiene only.

## Checked
- Entry type (src/audit.rs) has only `time`, `mandate`, `operation`, `account`, `mailbox`, `messages`, `outcome`, `reason`; `reason` is `&'static str` from `CoreError::code`, never the error message. `mandate` is the random 8-hex short ID (`seal::short_id`), unrelated to the token or its hash. `Trail` in src/core.rs holds only short ID, login and a bool. No Credential, Agent Token, token hash or store key can reach an entry; AC3/AC4 tests assert this on the bytes, including the `amt_` prefix and hex store key.
- Unknown and malformed tokens: the entry omits `mandate` and `account`, and the presented text is not logged (AC3, plus tests/cli.rs binary test).
- Injection: entries are serialised with `serde_json`, which escapes control characters, so CR/LF in logins or mailbox names cannot forge a second line (unit test and AC5).
- Path: on macOS the home directory comes from `getpwuid_r(getuid())`, not `$HOME`, with a bounded ERANGE retry and an absolute-path check; the `unsafe` blocks are sound. No agent input influences the path. XDG on other Unix is environment-based, as the spec accepts.
- Test-only redirects (`AuditLog::at`, `temporary`, `for_test_binary`, `AGENT_MAIL_TEST_AUDIT_LOG`, `Context::with_audit_log`) are all behind `cfg(any(test, feature = "test-support"))`; the new `agent-mail-audit-redirect` marker is included in the release-binary check in tests/project.rs. A release build cannot be pointed at another log by an Agent.
- Fail closed: if the append fails, `list` returns `audit_unavailable` and no mailboxes; the message names no path and no `audit.log` (tested). Policy denial still happens before any connection, unchanged.
- AC6: no subcommand reads the log; `audit` and `log` are unknown subcommands.
- Ran `cargo test` in the worktree: every target passes, and `~/Library/Application Support/agent-mail` did not exist before or after, so tests do not touch the developer's real log.
- New dependency: `libc 0.2` (unix only), already in the lock graph via other crates; used only for `getpwuid_r`/`getuid`. No new network or filesystem reach.
- Commits 6e287f0 and 0df4fa3: no real credentials; only the existing fake fixtures (`fake-app-password-123`, new `wrong-app-password-456`), which follow the established test convention.
