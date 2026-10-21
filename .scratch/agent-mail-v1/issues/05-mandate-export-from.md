# 05: Export a Mandate's choices and re-create from file

**What to build:** The human never edits config by hand, but can save and reuse choices. At the end of `mandate create` the flow offers to export the Accounts (server settings and login, never Credentials) and Grants as a TOML file. `mandate create --from <file>` pre-fills the guided flow from such a file; every Credential is still typed and live-checked. This is how a Mandate is effectively changed: export, create a new one from the file with adjustments, delete the old one.

**Blocked by:** 03

**Status:** done

- [ ] Accepting the export offer writes a TOML file containing the name, Accounts (server, port, TLS mode, login) and Grants
- [ ] The exported file never contains a Credential or Agent Token
- [ ] `mandate create --from <file>` shows the guided flow with the file's Accounts, mailboxes and operations pre-selected, and the human can change any of them before confirming
- [ ] Credentials are asked and live-checked for every Account even when starting from a file
- [ ] A file referencing a mailbox that no longer exists on the server shows it as unavailable instead of failing
- [ ] A malformed file fails with an error naming the problem, before any Credential is asked
- [ ] Declining the export offer writes nothing
