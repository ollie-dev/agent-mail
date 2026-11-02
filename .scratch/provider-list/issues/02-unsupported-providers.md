# 02: Unsupported Providers in the picker

**What to build:** a human searching for Outlook.com, Microsoft 365, Proton Mail, Tuta or HEY finds them in the "Mail provider:" list, marked "– not supported". Choosing one shows the one-sentence reason and returns to the list, so they learn why instead of guessing. See `../spec.md`, decisions 3 and 4.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] The 5 unsupported entries from `.scratch/provider-list/providers.json` are in the built-in list, with a reason and without IMAP settings or Credential kind
- [ ] The list test also checks: IMAP settings and Credential kind present exactly when no reason is set, reasons at most 300 characters
- [ ] The picker shows them alphabetically among the others, suffixed "– not supported", and the search finds them by name and domain
- [ ] Choosing one shows its reason and asks "Mail provider:" again; no Credential is asked and nothing is stored
- [ ] `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass

## Comments
