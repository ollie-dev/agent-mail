# 01: Provider list and searchable picker in `mandate create`

**What to build:** when a human adds an Account in `mandate create`, they pick their mail service from a searchable list of Providers instead of the "Account type" choice. A chosen Provider fills in the IMAP settings; only the address and the Credential are asked. "Custom server…" keeps the generic path. See `../spec.md`, decisions 1–3 and 5–9.

**Blocked by:** None (can start immediately)

**Status:** done

- [ ] Prefactor first: the Terminal port offers a single choice with search, and the scripted test terminal can answer it
- [ ] The 30 supported Providers from `.scratch/provider-list/providers.json` are built into the binary, not read at runtime; a code comment says why
- [ ] A test checks the built-in list: the schema of spec decision 3, unique ids, no domain in two entries, port not 0, IMAP settings and Credential kind present on every supported entry, notes at most 300 characters
- [ ] `mandate create` asks "Mail provider:" with search on: "Custom server…" first, then the Providers alphabetically as `Name (d1, d2, d3, …)`; typing part of the name or of any domain, shown or not, finds the entry
- [ ] Choosing a Provider asks only "Email address at <Name>:" and the Credential ("Password" or "App password" per the entry), showing the entry's note first; the stored Account has the entry's host, port and TLS mode
- [ ] Gmail is an ordinary entry; the separate Gmail preset choice is gone (the Gmail-only host warning stays until ticket 03)
- [ ] "Custom server…" asks host, port, TLS mode and login as the generic path does today; `mandate create --from` asks no Provider
- [ ] Mandates and the Mandate file are unchanged, with no Provider id; existing create and export scenarios pass, adjusted for the new prompts
- [ ] CLAUDE.md's layout description names the new module and data file
- [ ] `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass

## Comments
