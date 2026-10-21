VERDICT: PASS

## Findings
- [nit] src/export.rs:131-139 (`MandateFile::load`): the `fs::metadata` check runs before `File::open`. If a FIFO is swapped in between the two, `open` still blocks, and the check after opening never runs. The path comes from the human, so this is only a hang, not a leak. Optional fix: open with `O_NONBLOCK` (`OpenOptionsExt::custom_flags(libc::O_NONBLOCK)`) and keep the check after opening.
- [nit] src/create.rs:428 (`looks_like_gmail`): an exact suffix match misses look-alike logins (e.g. a zero-width or other invisible character after `gmail.com`) and a host spelled `imap.gmail.com.` with a trailing dot. In both cases the warning is not shown. The Credential prompt now always names `host:port`, so this is defence in depth only. Optional fix: strip the trailing dot from the host before comparing, and warn on any login containing `@gmail.`/`@googlemail.`.

## Round 1 items
- No blocking items in round 1. All three nits are handled: the Credential prompt names `host:port` (escaped) and a Gmail login on a non-Gmail host gets a warning first; problems in the file no longer repeat key names; `--from` must be a regular file before it is opened.

## Checked, no issue
- File error messages (src/export.rs `Walk`): every message is built from a fixed string plus a line number from the span. The TOML syntax error reports only line and column, never `Display`/`message()`. Key names in "has no {key}" come from the constant key lists, not from the file. The serde cross-check error is swallowed. The only file values echoed are logins for AC6 (f) and (h), escaped as the spec requires. The tests assert that `password` and `hunter2-file-secret` are absent.
- Credential prompt: the host is escaped with `escape_controls`. The Gmail preset is unaffected. The warning appears before the hidden prompt, for both the generic flow and `--from` Accounts. Every kept Account still gets a hidden prompt and a live check.
- Validation parity: the walk still rejects unknown keys, bad TLS or operation values, ports outside 1-65535, empty strings, duplicate logins, Grants for unknown Accounts and draft Grants with mailboxes. The strict `deny_unknown_fields` serde types must also agree (`parsed == file`), otherwise the file is refused.
- No new dependencies this round (`toml::de::DeTable` and `Spanned` come from the existing `toml` crate). Test fixtures use only fake secrets. `cargo test`: all suites pass.
