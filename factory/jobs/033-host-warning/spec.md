# Host warning for every Provider

## Goal
When a human adds an Account in `mandate create`, agent-mail warns before the Credential prompt whenever the login's mail domain belongs to a built-in Provider whose IMAP host is not the host the Credential is about to go to. This covers "Custom server…", a different Provider and a `mandate create --from` file. It replaces today's Gmail-only warning, so a WEB.DE, GMX or Yahoo password is not sent to the wrong server by mistake.

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Ticket: `.scratch/provider-list/issues/03-host-warning.md`. Feature spec: `.scratch/provider-list/spec.md`, decisions 9 and 10. Terms come from `GLOSSARY.md` (Provider, Account, Credential, Mandate).
- `src/create.rs`: `add_account` builds the Gmail-only warning after the login is known and before the hidden Credential prompt. It uses `GMAIL_HOST`, `GMAIL_PORT` (already `#[expect(dead_code)]`) and `looks_like_gmail`, plus the unit test `gmail_addresses_are_recognised`. All of these go away. The new check runs at the same point. It applies to every path into `add_account`: a chosen Provider, "Custom server…", and a saved Account from `--from`.
- `src/provider.rs`: `by_domain(domain)` already does a case-insensitive lookup of a built-in Provider by one of its `domains`. `Provider::connection()` returns `Some((&ImapSettings, CredentialKind))` only for supported Providers. A small helper may be added here, such as one that returns the Provider expected for a login together with its host.
- `src/terminal.rs`: `escape_controls` escapes untrusted text shown in messages.
- `data/providers.json`: relevant entries are WEB.DE (`web.de` → `imap.web.de`), GMX (`gmx.de`, … → `imap.gmx.net`), Gmail (`gmail.com`, `googlemail.com` → `imap.gmail.com`), Yahoo (`yahoo.com`, … → `imap.mail.yahoo.com`) and Cox (`cox.net` → `imap.mail.yahoo.com`).
- Tests: `tests/create.rs` (the interactive flow with the scripted terminal and `FakeImap`) and `tests/export.rs` (the `--from` path). The existing Gmail warning tests are `job031_ac6_gmail_login_on_a_custom_server_is_still_warned` in `tests/create.rs`, and `gmail_login_on_another_host_is_warned_before_the_credential` and `credential_prompt_names_the_server_host_and_port` in `tests/export.rs`. Their assertions must keep passing unchanged.

Warning text. `{login}` is `escape_controls(login)`, `{server}` is `escape_controls(host):port`, `{Name}` is the Provider's `name` and `{expected}` is the Provider's IMAP host:

```
Warning: {login} looks like a {Name} address, but the server is {server}, not {expected}. The credential you type next goes to {server}; check the host before typing it.
```

For Gmail this gives the same text as today. The login's domain is the text after its last `@`. A login without `@` has no domain and gets no warning. Hosts are compared ASCII case-insensitively. The port is not compared.

## Acceptance criteria
1. AC1: With "Custom server…" and a login like `max@web.de` on a host other than WEB.DE's, the warning names the chosen server and the expected host, before the Credential prompt. Test: choose "Custom server…", then type host `localhost`, port `<fake port>`, "Implicit TLS" and login `max@web.de`, then cancel at the hidden prompt. The transcript has exactly one Message containing `looks like a WEB.DE address`. That message contains `max@web.de`, `localhost:<fake port>` and `imap.web.de`, and it comes before the first Hidden entry.
2. AC2: Choosing one Provider and typing an address of another Provider with a different host warns the same way. Test: search `web.de` and pick "WEB.DE (web.de)", type `max@gmx.de` at "Email address at WEB.DE:", then cancel at the hidden prompt. One Message before the Hidden entry contains `looks like a GMX address`, `max@gmx.de`, `imap.web.de:993` and `imap.gmx.net`.
3. AC3: No warning when the hosts match, including two Providers sharing a host (`x@cox.net` with Yahoo), and none for domains not in the list. Tests, one per case, each cancelled at the hidden prompt. In every case the transcript has no Message starting with `Warning:`.
   - (a) Pick Yahoo and type `x@cox.net`. Yahoo and Cox share `imap.mail.yahoo.com`.
   - (b) Pick "Custom server…" with host `IMAP.WEB.DE` (different case) and login `max@web.de`.
   - (c) Pick "Custom server…" with host `localhost` and login `agent@example.test` (domain not in the list).
   - (d) Pick "Custom server…" with host `localhost` and login `max` (no `@`).
4. AC4: The Gmail-only check and the Gmail constants are gone; the existing Gmail warning scenarios pass through the general rule. Test: `src/create.rs` does not contain `GMAIL_HOST`, `GMAIL_PORT` or `looks_like_gmail`. `job031_ac6_gmail_login_on_a_custom_server_is_still_warned` (`someone@Gmail.com` on a custom server) still passes unchanged. So do `gmail_login_on_another_host_is_warned_before_the_credential` (a `--from` file with `someone@Gmail.com` on `localhost`, where the warning contains the login, `localhost:<port>` and `imap.gmail.com`) and `credential_prompt_names_the_server_host_and_port` (no warning for `agent@example.test`).
5. AC5: Host names and logins in the warning are escaped as today. Test: choose "Custom server…", then type host `evil\u{1b}[2Jhost` and login `max\u{7}@web.de`, and cancel at the hidden prompt. The warning message contains `evil\u{1b}[2Jhost` and `max\u{7}@web.de` as the literal escaped text that `escape_controls` produces. It contains no raw `\x1b` or `\x07` character. The domain is still recognised as `web.de`, so the warning appears.
6. AC6: `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass.

## Out of scope
- Unsupported Providers (Outlook.com, Microsoft 365, Proton, Tuta, HEY) have domains but no IMAP host. A login at one of their domains gets no host warning, because there is no expected host to name.
- Comparing ports or TLS modes. Only the host is compared, per decision 10.
- Blocking or re-asking the host. The warning is informational, as today.
- Auto-detecting the Provider from the address before the picker, and storing a Provider id in the Mandate (feature spec, Out of scope and decision 8).
- Changes to `data/providers.json`, to the picker, or to unsupported-Provider handling (tickets 01 and 02).

## Notes for reviewers
- Security: this is a guard right before the Credential is typed, against sending a password to a host the human did not mean. The login and host are untrusted input: they are typed by the human or come from a `--from` TOML file. Both must pass through `escape_controls` before they are shown. The Provider name and expected host come from the compiled-in `data/providers.json` and are trusted. No Credential is read, logged or shown by this change. Domain matching must be exact on the text after the last `@`, never a suffix match: `someone@gmail.com.evil.test` must not count as Gmail. That case is covered by AC3(c) in spirit, and the builder may add it as an extra test. The ticket's criteria contain nothing untestable and nothing that conflicts with an ADR (ADR-0003 is unaffected because nothing new goes into the Mandate).
- UX: the human operator running `mandate create` or `mandate create --from` sees one warning line after the login and before the hidden "Password"/"App password"/"Credential (password)" prompt, then continues as usual. Agents never see this flow. The wording keeps today's Gmail text, so existing users see no change for Gmail.
- UI: terminal output only. One message line is written through the Terminal port (`/dev/tty` or `CONOUT$`). There are no other UI changes.
