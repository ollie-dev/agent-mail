# 03: `mandate create`: guided setup stored in the Keychain

**What to build:** The human creates a Mandate in their own terminal through a guided prompt flow, and it is stored in the macOS Keychain, so agent-mail becomes usable with a real mailbox.

Flow: name → add one or more Accounts (Gmail preset with server and port filled in, or generic host, port, TLS mode, login) → type each Credential with hidden input → live login check → per Account, tick mailboxes from the server's list → per Account, tick operations (list, search, read, flag, move, draft) → confirm a summary → seal and store → show the Agent Token exactly once.

`mandate create` is how the human proves admin rights: it requires every Credential (ADR-0003). It must refuse to run when it cannot talk to a human: no TTY on stdin and stdout, or an agent sandbox detected. Credentials are never accepted via arguments, environment variables or piped input. Tested through the scripted terminal port and the test store; the Keychain store gets an opt-in, ignored-by-default test.

**Blocked by:** 02

**Status:** done

- [ ] A scripted session that adds a generic IMAP Account, ticks two mailboxes and the operations list and search, and confirms, results in a stored Mandate whose Policy has exactly those Grants, and prints one Agent Token
- [ ] The printed token opens the stored Mandate, and `agent-mail list` with that token returns exactly the two ticked mailboxes
- [ ] Choosing the Gmail preset pre-fills Gmail's IMAP server and port, and only the address and app password are asked
- [ ] A wrong Credential fails the live login, shows an error and lets the human retry or abort; nothing is stored on abort
- [ ] A session with two Accounts produces one Mandate covering both, with separate Grants per Account
- [ ] Declining the summary stores nothing
- [ ] Run without a TTY (e.g. stdin piped), the command exits non-zero with a message saying it must be run by a human in a terminal, and stores nothing
- [ ] Run with sandbox indicators present, the command refuses with an explanatory message
- [ ] No command-line flag or environment variable can supply a Credential
- [ ] The Credential is never echoed and never appears in output, errors or the stored record in clear text
- [ ] The Agent Token is printed once and cannot be retrieved again by any command
- [ ] On macOS the Mandate is stored as one Keychain item created by agent-mail itself (opt-in test, ignored by default, round-trips a dummy Mandate and deletes it)
