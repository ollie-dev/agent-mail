# 04: `mandate list` and `mandate delete`

**What to build:** The human sees which Mandates exist and revokes one instantly.

`mandate list` shows each Mandate's short ID, name, Accounts, Grants and creation date, never Credentials or tokens. Since records are sealed under token-derived keys, the listable metadata must be readable without a token while the sealed part stays sealed. `mandate delete <id>` removes a Mandate without asking for a Credential (ADR-0003 accepts that an Agent can delete its own Mandate).

**Blocked by:** 03

**Status:** ready-for-agent

- [ ] With two Mandates stored, `mandate list` shows both with ID, name, Accounts, Grants and creation date, as JSON and with `--text`
- [ ] No Credential, Agent Token or token hash appears in the listing output
- [ ] `mandate delete <id>` removes that Mandate; the other remains and still works
- [ ] After deletion, the deleted Mandate's token returns the same unknown-token error as a never-issued token
- [ ] Deleting an unknown ID returns a structured `not_found` error and exits non-zero
- [ ] Tampering with listable metadata so it no longer matches the sealed record is detected when the Mandate is next opened
