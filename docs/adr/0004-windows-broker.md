# On Windows a Broker service holds the Mandates and does the work

Supersedes ADR-0002 for Windows. Windows Credential Manager and DPAPI let any process of the same user read a stored record, so a hijacked Agent, which always holds its Agent Token, could unseal its Mandate, take the Credential and bypass the Policy. We now want Windows to protect against a hijacked Agent as macOS does, so on Windows agent-mail runs a Broker: an always-running service under the virtual account `NT SERVICE\agent-mail` that stores the sealed Mandates in that account's own Credential Manager, out of reach of the user's processes. The Broker unseals the Mandate, connects to IMAP, enforces the Policy and writes the audit log itself, and returns only results; no Credential, sealed or not, ever leaves it. The CLI and the MCP server become thin clients on Windows.

macOS stays as it is (ADR-0002): the Keychain access list and the hardened-runtime self-check already give an Agent no way around the Policy, and a launchd broker would add install complexity for no security gain. Do not unify the platforms. On both, an administrator is out of scope: they can run code as the Broker's account or as root and read everything.

## Decisions

- **Transport:** a named pipe, `\\.\pipe\agent-mail`, open to authenticated local users. The Agent Token is the only thing that authorizes an operation; the client's executable is not checked, because a hijacked Agent can start the real binary anyway. Requests carry the parsed command, the presented tokens and the client's proxy setting as length-prefixed JSON; replies are the JSON the CLI prints today. The client and the Broker are one binary, so the first message compares versions and a mismatch fails with `broker_version_mismatch`.
- **Owner:** each Mandate records the SID of the user who created it, and the Broker serves it only to a client with that SID. The Broker reads the SID, then reverts impersonation before touching the store, since Credential Manager fails with `ERROR_NO_SUCH_LOGON_SESSION` while impersonating.
- **`mandate create`:** the client must run elevated, and the Broker refuses `create` from a client token below high integrity. A same-user process at medium integrity cannot read an elevated process's memory or console, so the Credential is safe while it is typed, and the UAC prompt is a confirmation an Agent cannot click. The Broker runs the live login, generates the Agent Token and seals the Mandate; the client shows the token once and makes the export offer. `mandate list` and `mandate delete` need no elevation (ADR-0003 accepts deletion).
- **Audit log:** the Broker writes `%ProgramData%\agent-mail\audit.log`, writable only by the Broker. `agent-mail audit` shows a user the entries for their own Mandates, filtered by SID; on macOS it reads the user's own log.
- **Install:** the binary lives in `%ProgramFiles%\agent-mail`, because a Broker binary the user can write is a Broker a hijacked Agent can replace. `install.ps1` therefore asks for elevation once, installs, registers the Broker with delayed automatic start, and puts the folder on the PATH; rerunning it updates in place. A separate `uninstall.ps1` removes the Broker, the binary and the PATH entry, and deletes the stored Mandates and audit log after confirmation unless `-KeepData` is given. A client that cannot reach the Broker fails with `broker_unavailable`.

## Considered Options

- **Keep ADR-0002 and document the gap:** rejected; Windows would remain unsafe against the threat the product exists for.
- **A Broker that only holds the Credential and hands it to the client:** gains nothing, since the hijacked Agent would ask for it.
- **The Broker prompting for the Credential itself:** impossible, since services in session 0 have no UI.
