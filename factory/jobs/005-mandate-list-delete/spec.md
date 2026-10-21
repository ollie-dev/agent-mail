# `mandate list` and `mandate delete`: see and revoke Mandates

## Goal
The human runs `agent-mail mandate list` to see every stored Mandate with its short ID, name, Accounts, Grants and creation date. Credentials, Agent Tokens and token hashes never show up there. `agent-mail mandate delete <id>` removes one Mandate straight away, with no Credential asked, so the Agent holding its token is revoked at once. Listing works without any token: each record carries listable metadata in clear, bound to the sealed part, so tampering with it is caught the next time the Mandate is opened.

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`. Opt-in Keychain test: `cargo test -- --ignored keychain` on macOS.
- Source ticket: `.scratch/agent-mail-v1/issues/04-mandate-list-delete.md`. Feature spec `.scratch/agent-mail-v1/spec.md`, sections "Mandate model and sealing" and "Other commands". Also `GLOSSARY.md`, ADR-0002 and ADR-0003 (deleting is allowed without authentication).
- Files this feature touches:
  - `src/store.rs`: the Mandate store port gains a way to enumerate what it holds, e.g. `entries() -> Result<Vec<(Vec<u8>, Vec<u8>)>, StoreError>` returning every key with its value.
    - `InMemoryStore`: returns its map.
    - `EmptyStore`: returns an empty list.
    - `KeychainStore`: finds every generic-password item with service `agent-mail`, e.g. `SecItemCopyMatching` with `kSecMatchLimitAll` returning attributes, then reads each value. Fetching data together with `kSecMatchLimitAll` may be refused on macOS, so reading values one by one is fine.
  - `src/seal.rs`: the record format moves to a new `RECORD_VERSION`. The stored value carries:
    - the listable metadata in clear (name, ID, creation time, each Account's login, host, port and TLS mode, and the Grants; never a Credential, token or token hash);
    - the AEAD-sealed part as today.

    The metadata is authenticated with the record, e.g. as part of the AEAD associated data together with the store key, so changing a single byte of it makes `open` fail. There is a token-free function that reads the listable metadata from a stored value. Records that still use the old version have no listable metadata.
  - `src/core.rs`: new commands `MandateList` and `MandateDelete { id }`. They read no token and need no Terminal. There are new result structures and a new `not_found` error, whose message follows the existing "; <next step>" rule (the unit test listing every error is extended).
  - `src/lib.rs`: the `mandate list` and `mandate delete <ID>` subcommands with JSON and `--text` rendering. Text output escapes control characters with `escape_controls`, as `--text list` does.
  - Tests: new `tests/mandates.rs` (list and delete, in-process with `InMemoryStore` and the fake IMAP server). `tests/sealing.rs` gets the tamper tests and an updated clear-text check (see Notes). `tests/keychain.rs` additionally enumerates the dummy item. `tests/cli.rs` checks that `mandate --help` lists `list` and `delete`. Update the test-layout line in `CLAUDE.md` for the new test file.
- Fixtures used below. All values are fake; helpers live in `tests/support/`.
  - Mandate A: name `work-inbox`, ID `aaaa1111`, created `1792483200`, which renders as `2026-10-20T08:00:00Z`. One Account: `agent@example.test`, `localhost`, fake server port, implicit TLS. Grants `[Grant{list, agent@example.test, [INBOX, Archive]}, Grant{search, agent@example.test, [INBOX]}]`.
  - Mandate B: name `other-archive`, ID `bbbb2222`, created `1792575015`, which renders as `2026-10-21T09:30:15Z`. One Account: `other@example.test` on the same fake server. Grants `[Grant{list, other@example.test, [Archive]}]`.
  - Both are sealed under fresh tokens and stored in one `InMemoryStore`. The Credential is `fake-app-password-123`.

## Acceptance criteria
1. AC1: With two Mandates stored, `mandate list` shows both with ID, name, Accounts, Grants and creation date, as JSON and with `--text`. With A and B stored (B first, to prove the ordering), `agent-mail mandate list` with no `AGENT_MAIL_TOKEN` exits 0 with empty stderr. Stdout is exactly one JSON object followed by a newline: `{"mandates":[{"id":"aaaa1111","name":"work-inbox","created":"2026-10-20T08:00:00Z","accounts":[{"login":"agent@example.test","host":"localhost","port":<port>,"tls":"implicit"}],"grants":[{"operation":"list","account":"agent@example.test","mailboxes":["INBOX","Archive"]},{"operation":"search","account":"agent@example.test","mailboxes":["INBOX"]}]},{"id":"bbbb2222",...}]}`. Mandates are ordered by `created` ascending, then by ID. `agent-mail --text mandate list` exits 0, and for each Mandate it prints a header line with ID, name and created date, then one indented line per Account and one per Grant, for example:
   ```
   aaaa1111  work-inbox  created 2026-10-20T08:00:00Z
     account agent@example.test (localhost:<port>, implicit)
     grant list on agent@example.test: INBOX, Archive
     grant search on agent@example.test: INBOX
   bbbb2222  other-archive  created 2026-10-21T09:30:15Z
     account other@example.test (localhost:<port>, implicit)
     grant list on other@example.test: Archive
   ```
   An empty store gives `{"mandates":[]}` and, with `--text`, the line `No Mandates stored.`. A name or mailbox holding a control character (e.g. `evil\nname`) appears escaped in `--text` output and never adds a line.
2. AC2: No Credential, Agent Token or token hash appears in the listing output. In both modes of AC1, stdout and stderr do not contain:
   - `fake-app-password-123`;
   - either Mandate's token string;
   - either token's hash, i.e. base64url without padding of SHA-256 over the 32 secret bytes, computed in the test from the token string;
   - any substring `amt_`.

   The JSON keys of each Mandate object are exactly `id`, `name`, `created`, `accounts`, `grants`. Each Account object has exactly `login`, `host`, `port`, `tls`.
3. AC3: `mandate delete <id>` removes that Mandate; the other remains and still works. With A and B stored, `agent-mail mandate delete aaaa1111` with no token set exits 0. Stdout is `{"deleted":{"id":"aaaa1111","name":"work-inbox"}}` plus a newline. With `--text`, stdout is `Deleted Mandate "work-inbox" (ID aaaa1111).`. No prompt is shown and no Credential is asked: the run uses a context with no Terminal. Afterwards:
   - the store holds exactly one entry;
   - `mandate list` lists only `bbbb2222`;
   - `agent-mail list` with B's token returns `{"account":"other@example.test","mailboxes":[{"name":"Archive"}]}` and exits 0.
4. AC4: After deletion, the deleted Mandate's token returns the same unknown-token error as a never-issued token. After AC3, `agent-mail list` with A's token, and `agent-mail list` with a freshly minted token that was never stored, both exit 1. Their stdout is empty and their stderr is byte-for-byte identical: the `invalid_token` JSON error. The same holds with `--text`. Neither run connects to the fake server (its connection count does not change).
5. AC5: Deleting an unknown ID returns a structured `not_found` error and exits non-zero. With A and B stored, `agent-mail mandate delete ffffffff` exits 1 with empty stdout. Stderr is `{"error":{"code":"not_found","id":"ffffffff","message":"no Mandate has ID ffffffff; run `agent-mail mandate list` to see the IDs"}}` plus a newline. With `--text`, stderr is `error: not_found: <same message>`. The store still holds both entries unchanged. A deleted ID, such as `aaaa1111` deleted a second time, gives the same `not_found` error.
6. AC6: Tampering with listable metadata so it no longer matches the sealed record is detected when the Mandate is next opened. Each case starts from A stored on its own:
   - (a) In A's raw stored value, the same-length clear-text name `work-inbox` is replaced by `evil-inbox`.
   - (b) The clear-text login `agent@example.test` is replaced by `agent@example.evil`.
   - (c) The clear-text mailbox `Archive` in the `list` Grant is replaced by `Spamxyz`.

   In every case, `seal::open` with A's token fails. `agent-mail list` with A's token exits 1 with the `invalid_token` error and makes no connection to the fake server. The unmodified record still opens and lists. Also (d): record B's listable metadata spliced in front of A's sealed part, via the seal module's token-free metadata reader and its format, fails to open with A's token.
7. AC7: A record whose listable metadata cannot be read is not dropped silently. With A stored plus a raw entry whose value is a version-1 record (or arbitrary bytes `b"garbage"`) under another key, `mandate list` exits 0. It lists A, and the JSON object carries `"unreadable":1` next to `"mandates"`. `--text` adds the line `1 stored record could not be read and is not listed.` When every record is readable, `unreadable` is `0` and the text line is absent.

## Out of scope
- Deleting records that have no readable metadata (old-format or corrupt items). They can only be counted (AC7); the human removes them with Keychain Access.
- Migrating records from ticket 03's format. No release has shipped, so there is no migration path.
- Guaranteeing unique short IDs at create time (8 random hex digits). If two records share an ID, `mandate delete` may delete either; re-rolling on collision during `mandate create` is a follow-up.
- A confirmation prompt on delete, delete by name, and bulk delete.
- Refusing `mandate list` or `mandate delete` inside an agent sandbox. ADR-0003 accepts that an Agent can delete its own Mandate.
- `mandate create` export and `--from` (ticket 05), audit log entries for list and delete (ticket 06), `whoami` and several tokens (ticket 13), the hardened-runtime self-check (ticket 16), Windows (ticket 18).

## Notes for reviewers
- Security:
  - Listable metadata (names, logins, hosts, mailboxes, Grants) now sits in clear inside the Keychain item's value, which only the agent-mail binary can read. It must not go into item attributes such as label, comment or account, because any process can read those without a prompt. The item account stays the base64url store key.
  - The existing test `ac8_store_holds_no_clear_text_secrets_or_names` in `tests/sealing.rs` asserts that `LOGIN` and `Archive` are absent from the raw bytes. That no longer holds by design, so it has to be narrowed to the Credential, the token and the token hash. This is a deliberate change to a ticket-03 guarantee and should be called out in the build notes.
  - The listing is unauthenticated: it shows whatever metadata the store holds. Trust comes only from `open` (AC6). Someone who can write Keychain items under service `agent-mail` could make a fake listing entry or change one, but could never widen what a token unlocks. A review should confirm that the authenticated metadata covers every listed field, including `created` and each Account's host, port and TLS mode.
  - `mandate delete` needs no token or Credential (ADR-0003). Any Agent can revoke any Mandate, not only its own. This is accepted as "only removes access". It also lets any Agent run `mandate list` and see every Mandate's Grants. The feature spec calls listing "always safe", so reviewers should confirm that is acceptable.
  - The store key never appears in output either (a test may assert this alongside AC2). Delete-by-ID resolves the key internally by reading each record's metadata.
  - The ID argument is untrusted input. It is matched exactly and echoed only through JSON serialisation or `escape_controls`.
  - AC6 (d) depends on the record format. If the builder's design binds the metadata differently (e.g. by comparing it with a copy inside the sealed plaintext), the test should still splice one record's metadata onto another record's sealed part.
- UX: the operator (the human mail owner) runs `mandate list` to see who may do what, then `mandate delete <id>` to revoke access, with no prompt, typically to "change" a Mandate by deleting it and creating a new one. Agents may also run both. Agents get the same JSON structures, and the `not_found` message tells them how to find valid IDs. Revocation takes effect at once because there is no background service or cache (ADR-0002).
- UI: terminal output only.
  - `mandate list`: one JSON object on stdout, or the `--text` block layout in AC1 with created dates as RFC 3339 UTC.
  - `mandate delete`: a one-line confirmation.
  - Errors: JSON on stderr, or `error: <code>: <message>` with `--text`.
  - `mandate --help` lists `create`, `list` and `delete`.
