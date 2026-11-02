# Unsupported Providers in the "Mail provider:" picker

## Goal
A human running `mandate create` who searches the "Mail provider:" list for Outlook.com, Microsoft 365, Proton Mail, Tuta or HEY finds them, marked "– not supported". Choosing one shows the one-sentence reason and asks "Mail provider:" again, so they learn why it cannot work instead of guessing at server settings. No Credential is asked and nothing is stored for these entries (ticket `.scratch/provider-list/issues/02-unsupported-providers.md`, feature spec `.scratch/provider-list/spec.md` decisions 3 and 4).

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Files this touches:
  - `data/providers.json`: append the 5 entries with an `unsupported` reason (`outlook`, `microsoft365`, `proton`, `tuta`, `hey`) from `.scratch/provider-list/providers.json`. Copy them unchanged and in the same order, so the built-in list equals the starting list (35 entries).
  - `src/provider.rs`: `Provider` gains `unsupported: Option<String>`. `imap` and `credential` become optional, or move into a supported/unsupported split, as the builder prefers. `parse` enforces the new rules. `label()` / `search_option()` add the suffix. Add a `REASON_MAX` (300) constant next to `NOTE_MAX`, or reuse a shared limit.
  - `src/create.rs`: `ask_provider` / `add_account` loop back to "Mail provider:" after showing the reason. The code that reads a chosen Provider's `imap` and `credential` must still work with the new type.
  - `tests/provider.rs` and `tests/create.rs`: new tests. Update the existing tests that hard-code 30 Providers or 31 picker options (`ac2_all_returns_the_30_supported_providers_in_file_order`, `ac4_sorted_is_case_insensitive_by_name`, `ac4_search_finds_by_name_and_every_domain`'s `found("").len()`, `job031_ac4_mail_provider_lists_custom_server_then_providers_by_name`) so they pass with the 35-entry list. Keep what they check, for example "the 30 supported entries", by filtering on `unsupported`.
- Glossary terms: Provider, Account, Credential, Mandate. "Unsupported Provider" means a Provider whose `unsupported` reason is set. "Reason" means that field's text.

## Acceptance criteria
1. AC1: The 5 unsupported entries from `.scratch/provider-list/providers.json` are in the built-in list, with a reason and without IMAP settings or Credential kind.
   - Test: `provider::all()` returns 35 Providers. Their ids, in order, equal the ids of `.scratch/provider-list/providers.json` in file order: the 30 supported ones, then `outlook`, `microsoft365`, `proton`, `tuta`, `hey`.
   - Test: for each of those 5 ids, the reason equals the starting list's `unsupported` string exactly, for example `hey` gives "HEY provides no IMAP access.", and the Provider has no IMAP settings and no Credential kind.
   - Test: the other 30 Providers have no reason.
2. AC2: The list test also checks that IMAP settings and Credential kind are present exactly when no reason is set, and that reasons are at most 300 characters.
   - Test: the built-in list test asserts this rule for every entry of `provider::all()`.
   - Test: `provider::parse` returns `Err` for each of these second entries:
     - an entry with `unsupported` set that also has `imap`;
     - an entry with `unsupported` set that also has `credential`;
     - an entry with `unsupported` of 301 characters and no `imap`/`credential`;
     - an entry with neither `unsupported` nor `imap` (already covered);
     - an entry with neither `unsupported` nor `credential` (already covered).
   - Test: `provider::parse` returns `Ok` for an entry with `unsupported` of exactly 300 characters, `domains: []` and no `imap`/`credential`.
3. AC3: The picker shows the unsupported Providers alphabetically among the others, suffixed "– not supported", and the search finds them by name and domain.
   - Test: in a `mandate create` session, the "Mail provider:" entry has 36 options. Option 0 is "Custom server…". Options 1 to 35 are all built-in Providers sorted case-insensitively by name.
   - Test: the unsupported entries' labels are exactly the following. The suffix is a space, U+2013 EN DASH, a space, then `not supported`.
     - `Outlook.com / Hotmail (outlook.com, hotmail.com, live.com, …) – not supported`
     - `Microsoft 365 / Exchange Online (work) – not supported`
     - `Proton Mail (proton.me, protonmail.com, protonmail.ch, …) – not supported`
     - `Tuta (Tutanota) (tuta.com, tutanota.com, tutanota.de, …) – not supported`
     - `HEY (hey.com) – not supported`
   - Test: supported labels carry no suffix, for example `Gmail (gmail.com, googlemail.com)` is unchanged.
   - Test: the search finds unsupported entries by name and by any domain, including domains the label does not show:
     - `hotmail.de` finds `outlook`;
     - `microsoft` finds `microsoft365`;
     - `pm.me` finds `proton`;
     - `keemail.me` finds `tuta`;
     - `HEY` finds `hey`.
   - Test: an empty query matches all 35.
4. AC4: Choosing an unsupported Provider shows its reason and asks "Mail provider:" again. No Credential is asked and nothing is stored.
   - Test (cancel): the script answers a Mandate name, then `Answer::search("pm.me", <Proton label>)`, then `Answer::Cancel` at the next prompt, with no fake IMAP server. The transcript shows, in order:
     - the "Mail provider:" choice answered with the Proton label;
     - a message whose text is exactly Proton's reason;
     - a second "Mail provider:" choice with the same 36 options and default "Custom server…".
   - Between the two choices there is no `Text` prompt (no "Email address at Proton Mail:") and no `Hidden` prompt. The script is used up, and `store.entries()` is empty.
   - Test (continue): the script picks `HEY (hey.com) – not supported` and then the Gmail entry, and completes the session as `gmail_session` does. The stored Mandate has exactly one Account, on `imap.gmail.com:993`. HEY's reason appears in the transcript once, before the second "Mail provider:".
5. AC5: `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass.

## Out of scope
- The generalised host warning (ticket 03, decision 10). `by_domain` may return unsupported Providers. How the warning treats them, for example `x@outlook.com` on a Custom server, is for ticket 03. The Gmail-only warning stays as it is.
- OAuth2 or any other way to make the unsupported Providers work.
- Changes to the Mandate, the Mandate file, `mandate create --from` (it asks no Provider), the CLI, the MCP server or the Broker protocol.
- Changing the reasons' wording or the list's contents beyond copying the 5 entries.

## Notes for reviewers
- Security: there is no new credential path. The point is that choosing an unsupported Provider never reaches the Credential prompt, the IMAP connector or the store; AC4 checks this. The data stays compiled in with `include_str!`, never read at runtime (decision 1). Reasons are fixed text from the binary, not untrusted input.
- UX: the user is the operator (the human running `mandate create` at a terminal), never an Agent. In the flow, they type part of a name or domain at "Mail provider:", see the entry marked "– not supported", choose it, read the one-sentence reason, and are back at "Mail provider:" to pick another entry or "Custom server…", or press Esc/Ctrl-C to cancel as before. Marking unsupported entries in the label means most users never need to choose them.
- UI: terminal output only. The suffix is added to the existing labels of the inquire-based "Mail provider:" picker, and the reason is a `terminal.message` line. The reasons are at most 300 characters, in line with the CLAUDE.md message length convention.
- Ticket wording: the ticket says "reason"; the data field is `unsupported` (feature spec decision 3). Both mean the same text. No criterion conflicts with an ADR. ADR-0003 is unaffected because nothing about a Provider enters the Mandate.
