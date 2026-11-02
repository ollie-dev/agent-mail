# Provider list and searchable picker in `mandate create`

## Goal
When a human adds an Account in `mandate create`, the old "Account type:" choice is replaced by a searchable "Mail provider:" list. The list holds the 30 supported Providers, built into the binary. If they choose a Provider, the IMAP host, port and TLS mode are filled in, so only the email address and the Credential are asked. "Custom server…" keeps today's generic path, and Mandates do not change.

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Prompts use `inquire` 0.9 through the Terminal port.
- Test command: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- Source ticket: `.scratch/provider-list/issues/01-provider-list-and-picker.md`. Feature spec: `.scratch/provider-list/spec.md`, decisions 1–3 and 5–9. Glossary terms used here: Provider, Account, Credential, Mandate, Mandate file, Terminal.
- Files touched:
  - `data/providers.json` (new). It holds the 30 supported entries of `.scratch/provider-list/providers.json`, copied unchanged, in the same `{ "providers": [...] }` shape. The 5 entries with `unsupported` are left out until ticket 02.
  - `src/provider.rs` (new; add `pub mod provider;` in `src/lib.rs`). It loads `data/providers.json` with `include_str!`, parses and validates it, builds the picker labels and does the domain lookup. A code comment says why the list is built in: a list read at runtime could send the Credential the human is about to type to another server, and changing the list means a new release.
  - `src/terminal.rs`. The Terminal port gets a single choice with search. `InquireTerminal` implements it with `inquire::Select` and filtering turned on, using a custom scorer. `ScriptedTerminal` can answer it with a search query.
  - `src/create.rs`. "Mail provider:" replaces "Account type:". The `GMAIL` and `GENERIC` choice constants go, and a public `CUSTOM_SERVER` (`"Custom server…"`) is added. `GMAIL_HOST`, `GMAIL_PORT` and `looks_like_gmail` stay for now: the Gmail-only host warning stays until ticket 03.
  - Tests:
    - `tests/provider.rs` (new): the built-in list, labels and search.
    - `tests/create.rs`: the picker flow.
    - Adjust the scripts that use `GENERIC` or "Account type:" to `CUSTOM_SERVER` and "Mail provider:": `tests/export.rs`, `tests/client.rs`, `tests/proxy.rs` and `tests/credential_manager.rs`.
    - `tests/release.rs`: the CLAUDE.md check.
  - `CLAUDE.md`: the layout description.

## Acceptance criteria
1. AC1: The prefactor comes first. The Terminal port offers a single choice with search, and the scripted test terminal can answer it.
   - Each searchable option has a display label and a list of search keys.
   - An option matches a query when the trimmed query, compared case-insensitively, is a substring of the label or of any key. An empty query matches every option.
   - `ScriptedTerminal` accepts a new answer `Answer::search(query, label)`. It picks `label` only if that option matches `query` under the rule above, and panics otherwise. `Answer::choose(label)`, `Answer::Accept` and `Answer::Cancel` work as they do for `choose`.
   - The choice is recorded in the transcript as `Kind::Choice` with all options, the default label and the chosen label.
   - A unit test in `src/terminal.rs` checks the following:
     - Options `[("Alpha", ["a.example"]), ("Beta", ["hidden.example"])]` with query `"HIDDEN"` let `Beta` be picked.
     - The same options with query `"hidden"` and label `Alpha` panic.
     - `Answer::Cancel` returns `Err(Cancelled)`.
2. AC2: The 30 supported Providers are built into the binary, not read at runtime, and a code comment says why.
   - `src/provider.rs` contains `include_str!("../data/providers.json")` and no runtime file read of the list.
   - A comment next to it says that a list read at runtime could send the Credential to another server.
   - `provider::all()` returns exactly 30 entries, and their ids equal the 30 supported ids in `.scratch/provider-list/providers.json`, in file order.
   - A test checks the source text and the ids.
3. AC3: A test checks the built-in list against the schema of spec decision 3. `provider::all()` parses and validates without error, and in particular:
   - every `id` is a lowercase slug (`^[a-z0-9]+(-[a-z0-9]+)*$`) and unique;
   - no domain (compared case-insensitively) appears in two entries;
   - every entry has `imap` with a non-empty `host`, a `port` that is not 0 and `tls` of `implicit` or `starttls`;
   - every entry has `credential` of `password` or `app_password`;
   - every `note` is at most 300 characters.

   The same test feeds the validating parser (`provider::parse(&str)`) one bad list per rule and expects an error each time:
   - a duplicate id;
   - `gmx.de` in two entries;
   - `port` 0;
   - a missing `imap`;
   - a missing `credential`;
   - a `note` of 301 characters;
   - `tls: "plain"`;
   - the id `"GMX"`.
4. AC4: `mandate create` asks "Mail provider:" with search on.
   - The options are "Custom server…" first, then the 30 Providers sorted case-insensitively by name. The default is "Custom server…".
   - Each Provider's label is `Name (d1, d2, d3, …)`. It shows at most the first three domains in data order, then `, …` only when there are more than three. An entry with no domains shows just `Name`. Examples:
     - `Gmail (gmail.com, googlemail.com)`
     - `GMX (gmx.de, gmx.net, gmx.at, …)`
     - `WEB.DE (web.de)`
     - `IONOS Mail`
   - The search keys are the name and all domains, shown or not. Searching finds these entries, checked by test:
     - `web.de` finds WEB.DE;
     - `TELEKOM` finds Telekom (T-Online);
     - `posteo.us` finds Posteo, although that domain is not shown;
     - `ameritech.net` finds AT&T Mail, although that domain is not shown;
     - `gmx` finds exactly GMX and GMX (international) among the Providers.
5. AC5: Choosing a Provider asks only "Email address at <Name>:" and the Credential, showing the entry's note first. The stored Account has the entry's host, port and TLS mode.
   - Gmail example: after `Answer::search("gmail", "Gmail (gmail.com, googlemail.com)")`, the only prompts before the mailbox checklist are:
     - the text prompt `Email address at Gmail:`;
     - the hidden prompt `App password for <login> on imap.gmail.com:993:`.
   - Gmail's note is shown as a message after the address and before the hidden prompt.
   - No prompt mentions host, port or TLS. The stored Account is `imap.gmail.com`, port 993, `TlsMode::Implicit`.
   - For a `password` entry such as Posteo, the hidden prompt starts with `Password for `. For an entry without a note, such as Posteo, no note message is shown.
   - The fake server is reached through the existing connect override.
6. AC6: Gmail is an ordinary entry, and the separate Gmail preset choice is gone.
   - No transcript of `mandate create` contains `Account type:` or `Gmail (imap.gmail.com, app password)`.
   - `create::GMAIL` and `create::GENERIC` no longer exist.
   - The Gmail-only host warning stays until ticket 03. With "Custom server…", host `localhost` and login `someone@Gmail.com`, the "looks like a Gmail address" warning is still shown before the Credential prompt. The existing `tests/export.rs` warning scenarios still pass.
7. AC7: "Custom server…" asks host, port, TLS mode and login as the generic path does today, and `mandate create --from` asks no Provider.
   - Choosing `Custom server…` asks, in order:
     - `IMAP server host:`
     - `IMAP server port:` (default 993)
     - `TLS mode:`
     - `Login name:`
     - the hidden `Credential (password) for <login> on <host>:<port>:`

     The stored Account carries exactly the typed values.
   - With `--from` and one kept file Account, no `Mail provider:` prompt appears before that Account's checklists.
   - An Account added afterwards with "Add another Account?" goes through the picker, as the Account type choice does today.
8. AC8: Mandates and the Mandate file are unchanged and carry no Provider id. Existing create and export scenarios pass, adjusted only for the new prompts.
   - Take an Account added via the Gmail Provider with login `someone@gmail.com`, and a second Mandate with the same login added via "Custom server…" with `imap.gmail.com`, 993 and implicit TLS.
   - The two export to identical Account tables in the Mandate file (`MandateFile::of(..).to_toml()`, ignoring the Mandate name and id lines).
   - Neither the TOML nor the stored Mandate's serialized JSON contains `gmail"` as a provider id or the key `provider`.
   - All of the following pass after their scripts are adjusted: `tests/create.rs`, `tests/export.rs`, `tests/client.rs` and `tests/proxy.rs`. `tests/credential_manager.rs` must compile with the adjusted script; it is ignored by default.
9. AC9: CLAUDE.md's layout description names the new module and data file. A test in `tests/release.rs` checks that `CLAUDE.md` contains `src/provider.rs`, `data/providers.json` and `tests/provider.rs`.
10. AC10: `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass.

## Out of scope
- The 5 unsupported Providers, the "– not supported" suffix and showing their reason (ticket 02). This slice's loader may reject or ignore an `unsupported` field.
- The generalised host warning and removing `GMAIL_HOST`, `GMAIL_PORT` and `looks_like_gmail` (ticket 03).
- Changing any Provider's data. Copy the list unchanged.
- The manual live login checks listed in the feature spec.
- SMTP, OAuth2, runtime or online lookup, and detecting the Provider from the address before the picker.
- Provider ids in Mandates or in the Mandate file.
- Keeping "Custom server…" visible when a search matches nothing else. It matches by its label like any option.

## Notes for reviewers
- Security:
  - The point of building the list in is that the host the Credential goes to comes only from data compiled into the signed binary. Check that there is no runtime file, environment or network read of the list (AC2).
  - A chosen Provider fixes host, port and TLS mode, so the human cannot edit them on that path. Every entry in this slice uses implicit TLS on 993.
  - Credentials are still typed only at the hidden prompt and never echoed. Logins and hosts in prompts and messages stay escaped with `escape_controls`.
  - Notes and labels are trusted built-in text. The search query never leaves the process.
  - The Windows Broker route runs the same guided flow client-side, so it needs no Broker protocol change.
- UX:
  - This is for the operator (the human) running `agent-mail mandate create` in their own terminal. Agents never see this flow.
  - The flow is: Mandate name, then "Mail provider:" (type to filter, e.g. `web.de`), then the email address. The note follows, for example "enable IMAP first" or where to create the app password, and then the Credential, live login, checklists and so on as today.
  - Prompt wording is "Email address at <Name>:" and "Password"/"App password". The Custom server path keeps today's wording, "Credential (password)".
  - Spec decision 3 shows the note "before the Credential prompt", while the ticket says "first". This spec places it after the address and before the Credential prompt, which satisfies both.
- UI: Only terminal output changes. There is a new `inquire` select with filtering and 31 options, labels as in AC4, and one note message for each Provider that has a note. No pages and no change to CLI JSON output.
