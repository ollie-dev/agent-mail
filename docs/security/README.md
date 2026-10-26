# Security concept

agent-mail lets an Agent work with email while the human decides what it may do. The Agent never sees a Credential: it gets an Agent Token, and agent-mail does the mail work for it under the Mandate's Policy.

This page compares macOS and Windows and explains what both share. Each platform has its own document with diagrams:

- [macOS](macos.md): agent-mail runs in the Agent's own process and reads the sealed Mandate from the Keychain ([ADR-0002](../adr/0002-no-background-service.md)).
- [Windows](windows.md): the Broker, a service under its own account, holds the Mandates and does the work ([ADR-0004](../adr/0004-windows-broker.md)).

## The threat

The threat agent-mail exists for is a hijacked Agent: an Agent session that follows instructions hidden in an email, a web page or a file, and tries to do more than its human allowed.

A hijacked Agent runs as the same OS user as its human and holds its own Agent Token. On both platforms it can still do only what that Mandate's Policy grants, and it cannot get the Credential behind it.

## What both platforms share

**The Mandate.** A Mandate holds one or more Accounts with their Credentials, a Policy and the hash of one Agent Token. It is sealed as one record and never changed: to change anything, the human creates a new Mandate ([ADR-0003](../adr/0003-immutable-mandates.md)).

**The sealed record.** The Credentials, the Policy and the token hash are encrypted together with ChaCha20-Poly1305 under a key derived from the Agent Token with HKDF-SHA256. A forged Policy cannot be paired with a real Credential.

The record also carries a listing in clear (ID, name, Accounts without Credentials, Grants), so `mandate list` works without a token. The listing is bound to the sealed part as associated data: changing one byte of it makes the record fail to open.

**The Agent Token.** An Agent Token is `amt_...`: 256 random bits, shown to the human once at `mandate create`. It is the only thing that authorizes a call. Whoever presents it acts under its Mandate's Policy; the program presenting it is not checked.

A second HKDF key from the token is the record's lookup key in the store, so the store never sees the token. A token that is malformed, unknown or does not open its record is ignored with an `invalid_token` warning.

**The Policy.** A Policy is the set of Grants in one Mandate, each naming an operation (`list`, `search`, `read`, `flag`, `move`, `draft`), an Account and its mailboxes. Anything not granted is denied, and Grants never combine across Mandates.

**Creating a Mandate.** `mandate create` runs only in an interactive terminal, refuses known Agent sessions and sandboxes, and asks for every Credential with hidden input, checked by a live login. Knowing the Credential is the admin check.

**Deleting a Mandate.** Any program running as the human, a hijacked Agent included, can delete a Mandate. That only removes access, and ADR-0003 accepts it.

**The audit log.** Every mail operation an Agent asks for appends one line with the Mandate, the Account, the operation and whether the Policy allowed it. No line holds a Credential, an Agent Token or a message body.

**Administrators are out of scope.** Someone with administrator or root rights can run code as the Broker's account or as root, read memory and the stores, and so read everything. Neither platform protects against them.

## Side by side

| | macOS | Windows |
|---|---|---|
| Who runs a call | agent-mail itself, in the Agent's process | the Broker, as `NT SERVICE\agent-mail` |
| Where Mandates live | the user's Keychain | the Broker account's Credential Manager |
| What keeps the Agent out | the Keychain access list and the hardened-runtime self-check | the OS account boundary between the user and the Broker |
| Where the Credential is in memory | in each agent-mail process, including those the Agent starts | in the Broker, and in the elevated `mandate create` terminal while it is typed |
| How the client reaches the work | none needed, in-process | the named pipe `\\.\pipe\agent-mail` |
| Who may use a Mandate | anyone presenting its Agent Token | anyone presenting its Agent Token, from its owner's Windows account (SID) |
| Extra step for `mandate create` | none | an elevated terminal (UAC prompt) |
| Where the binary lives | `~/.local/bin`, writable by the user | `%ProgramFiles%\agent-mail`, writable by administrators only |
| The audit log | per user, writable by the user | `%ProgramData%\agent-mail\audit.log`, writable by the Broker only |
| A changed binary | changes its signature, so the Keychain prompts; deny it | cannot be installed without administrator rights |

## Not protected against on either platform

- An administrator or root (see above).
- Anyone holding an Agent Token: they can do what its Mandate grants, and no more. A token in a host config file, such as `.claude/settings.local.json`, is plain text readable by every program of that user, other Agents included.
- Deletion of a Mandate by a program of the same user.
- Anything the mail provider sees or allows: a Grant to `read` lets the Agent read those messages, whatever they say.

So keep Grants narrow, give each project its own Mandate, and delete a Mandate whose token may have leaked.
