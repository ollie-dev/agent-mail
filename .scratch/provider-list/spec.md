# Provider list in `mandate create`

## Problem

When a human adds an Account in `mandate create`, the only shortcut is the Gmail preset. For every other mail service they have to know the IMAP host, port and TLS mode, and whether the service wants an app password or IMAP switched on first.

## Solution

agent-mail ships a list of **Providers** (see `GLOSSARY.md`): the most used mail services in Germany, the rest of Europe and the US, with their IMAP settings. `mandate create` shows the list with a search instead of the "Account type" choice. "Custom server…" stays for everything else.

The regions only decided which Providers are in the first version of the list (10 German, 10 other European, 10 US, all usable with a password or app password). They are not part of the data and not shown.

## Decisions

1. **Built in, not read at runtime.** The list is `data/providers.json` in the repo, embedded with `include_str!`. A file read at runtime could send the Credential the human is about to type to another server. Changing the list means a new release. The reason sits as a comment in the new module `src/provider.rs` (loading, validating, domain lookup). No ADR.
2. **IMAP only.** agent-mail sends nothing; drafts use APPEND. No SMTP data.
3. **Schema** per entry:
   - `id`: lowercase slug, unique
   - `name`: display name
   - `domains`: the service's own mail domains; may be empty (IONOS, STRATO: customer domains only)
   - `imap`: `{ host, port, tls }`, `tls` is `implicit` or `starttls`; required unless `unsupported` is set
   - `credential`: `password` or `app_password`; picks the Credential prompt ("Password" / "App password"); required unless `unsupported` is set
   - `note`: optional, at most 300 characters, shown before the Credential prompt (enable IMAP first, where to create the app password)
   - `unsupported`: optional reason, at most 300 characters

   No `region`, no `login` field (the login is always the full address), no paid-only flag (where it surprises, `note` says it).
4. **Unsupported Providers** (Outlook.com, Microsoft 365, Proton, Tuta, HEY) are in the list, marked "not supported". Choosing one shows the reason and returns to the list. They do not count towards the 30.
5. **Picker.** The "Account type:" choice becomes "Mail provider:" with filtering on. First option "Custom server…", then all Providers alphabetically as `Name (domain1, domain2, domain3, …)`, at most three domains shown, unsupported ones suffixed "– not supported". The search matches the name and *all* domains (custom scorer for `inquire::Select`), so "web.de" or "gmx" finds the entry.
6. **Fixed settings.** A chosen Provider fixes host, port and TLS mode; only the login ("Email address at <Name>:") and the Credential are asked. Anything else goes through "Custom server…".
7. **`mandate create --from <file>`** asks no Provider, as today.
8. **Nothing new in the Mandate.** It keeps `host`, `port`, `tls` and `login`; no Provider id. The Mandate file format does not change (ADR-0003: Mandates are immutable, the list changes with releases).
9. **Gmail is an ordinary entry.** The `GMAIL*` constants and `looks_like_gmail` go; Gmail search (`X-GM-RAW`) depends on server capabilities, not on the preset.
10. **Host warning, generalised.** Before the Credential prompt: if the login's domain belongs to a Provider in the list whose IMAP host differs from the chosen host (Custom server or another Provider), warn as the Gmail warning does today, naming the expected host. Same host, no warning (`x@cox.net` on Yahoo is fine). Domain not in the list, no warning.
11. **Data source.** The list is curated by hand. Mozilla's Thunderbird ISPDB was a research source only: it is small (about 120 IMAP providers), barely maintained, and lacks app-password and unsupported information. No import, no runtime lookup.

## Data

The starting list is `.scratch/provider-list/providers.json` (35 entries: 30 supported, 5 unsupported). Copy it to `data/providers.json`.

| Group | Providers |
|---|---|
| Germany | GMX, WEB.DE, Telekom (T-Online), freenet, 1&1, IONOS, Posteo, mailbox.org, Vodafone, STRATO |
| Europe | Orange, Free, SFR, Libero, Virgilio, Seznam, WP, Onet, Interia, Swisscom Bluewin |
| US | Gmail, Yahoo, AOL, iCloud, Xfinity, AT&T, Verizon, Spectrum, Cox, GMX (international) |
| Not supported | Outlook.com / Hotmail, Microsoft 365, Proton Mail, Tuta, HEY |

Sources: the providers' help pages where reachable, otherwise the ISPDB or third-party guides. Not verified against an official page: freenet, SFR, Libero, Virgilio, WP, Interia, Spectrum, Verizon.

### Live check before release

No test accounts exist, so these need a manual login check:

- WEB.DE and WP: full address or local part as login (ISPDB says local part)
- iCloud: full address as login (Apple says "usually" the local part)
- Free.fr: full address or local part
- Verizon: `imap.aol.com` with an AOL app password
- Spectrum: `mobile.charter.net`
- Seznam: whether IMAP must be switched on first

## Out of scope

SMTP, OAuth2, runtime or online lookup, auto-detecting the Provider from the address before the picker, Provider ids in Mandates.
