# 03: Host warning for every Provider

**What to build:** before the Credential prompt, `mandate create` warns whenever the login's domain belongs to a Provider whose IMAP host is not the chosen host, so a password is not sent to the wrong server by mistake. This replaces the Gmail-only warning. See `../spec.md`, decision 10.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] With "Custom server…" and a login like `max@web.de` on a host other than WEB.DE's, the warning names the chosen server and the expected host, before the Credential prompt
- [ ] Choosing one Provider and typing an address of another Provider with a different host warns the same way
- [ ] No warning when the hosts match, including two Providers sharing a host (`x@cox.net` with Yahoo), and none for domains not in the list
- [ ] The Gmail-only check and the Gmail constants are gone; the existing Gmail warning scenarios pass through the general rule
- [ ] Host names and logins in the warning are escaped as today
- [ ] `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass

## Comments
