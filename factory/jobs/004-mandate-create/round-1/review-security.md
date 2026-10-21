VERDICT: PASS

## Findings
- [nit] src/terminal.rs:138: `InquireTerminal::message` writes the one-time Agent Token to stdout. The TTY check (stdin, stdout, stderr) makes this safe against plain pipes, but a caller that wraps the process in a pty (`script`, `expect`, tmux capture) still reads it from there. Writing token-bearing messages to `/dev/tty` (opened directly) would keep the token off fd 1 completely, which matches the spec's "never to stdout" more literally. This is not blocking: it only happens after a human typed a Credential that passed the live login.
- [nit] src/terminal.rs:100-107, src/create.rs:84-91: `inquire` keeps its own input buffer, and the `format!` string that carries `token.expose()` is a plain `String`. Neither is zeroized. `Credential::new` takes ownership of the returned `String`, so the main copy is wiped. If it can be done cheaply, build the token message in a `Zeroizing<String>`.
- [nit] src/create.rs:27-40: the indicator list covers Claude Code, Codex, Gemini CLI and Cursor. You could also add other common agent harnesses that set a marker variable (for example opencode's `OPENCODE`). Treat this as best effort. ADR-0003's "every Credential is typed" stays the real barrier.
- [nit] src/store.rs:85-88: `set_generic_password` adds the item, or updates it if one exists. If an item with the same name already exists, its existing ACL is kept. Store keys come from fresh 256-bit tokens, so this cannot happen in practice. Once ticket 04 adds `mandate delete`, consider refusing `put` when the key already exists (SecItemAdd only, treating errSecDuplicateItem as an error).

Checked with no blocking issues:
- Credentials come only from `Terminal::hidden`. `mandate create` has no options, the env is used only for sandbox detection, and `PasswordDisplayMode::Hidden` gives no mask, toggle or echo. Login failure text comes from a fixed `&'static str` per `ImapError` variant and names only the escaped login.
- The TTY and sandbox refusals run before any prompt and any store access (src/create.rs:63-68). The live login sends the Credential only over the existing TLS-only connector. When `with_route` redirects TCP, TLS still verifies the Account host as `ServerName`.
- `ScriptedTerminal`, `InMemoryStore`, `with_route` and `trusting_only` all sit behind `cfg(any(test, feature = "test-support"))`. The release-marker test now covers all three markers and has a positive control.
- The Keychain item is created in-process through the Security framework (security-framework 3.7.0, service `agent-mail`, account = base64url store key). The store sees only the token-derived key and the sealed bytes. On other OSes `put` fails with `store_unavailable`.
- Access policy: `list` refuses multi-Account Mandates with `account_required` and never merges them. The deny-by-default check still runs before connecting. Grants are keyed per login, and the same login cannot be added twice.
- Server mailbox names, the Mandate name and logins are escaped for control characters in checklists, the summary and messages.
- New dependencies (`inquire`/crossterm and the macOS-only security-framework) are mainstream, have no build-time network access and add no credential paths.
- Test fixtures use only fake values (`fake-app-password-123`, `wrong-password`, `env-supplied-secret`). `cargo test` passes on all suites, and the Keychain test is ignored by default.
