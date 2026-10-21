# Agent Mail - Connects you agents securely to your E-Mails
Agent Mail is a simple and secure way to configure access to your email for your agents. It securely stores your credentials hidden from your agents and allows you to configure what agents can access and do.

## Audit log

Every `agent-mail list` call an agent makes appends one line to an audit log, so you can see which Mandate did what, on which account, and whether its Policy allowed it. The log lives in your per-user data directory:

- macOS: `~/Library/Application Support/agent-mail/audit.log` (the home directory comes from the user database, not from `$HOME`)
- Other Unix: `$XDG_DATA_HOME/agent-mail/audit.log`, or `~/.local/share/agent-mail/audit.log` when `XDG_DATA_HOME` is unset

The directory is created on the first call (mode `0700`, file mode `0600`). Entries are only ever appended; no agent-mail command reads the log, so open it yourself with a pager or `jq`. If an entry cannot be written, the call fails with `audit_unavailable` and returns no result.

Each line is one JSON object with these keys, in this order, absent keys left out:

- `time`: when the call was made, RFC 3339 UTC to the second, e.g. `2026-10-21T09:30:15Z`.
- `mandate`: the Mandate's short ID; left out when the agent's token opened no Mandate.
- `operation`: the operation, e.g. `list`.
- `account`: the account's login; left out when no single account was chosen.
- `mailbox`: the mailbox involved (not used by `list`).
- `messages`: the message references involved (not used by `list`).
- `outcome`: `allowed` if the Mandate's Policy granted the operation, otherwise `denied`.
- `reason`: the error code when the call failed, e.g. `denied`, `invalid_token`, `missing_token` or `login_failed`; left out on success. An `allowed` call can still carry a reason, such as `login_failed`, when it failed after the Policy granted it.

The log never contains a credential, an agent token, a token hash or a message body. Any process running as you, including an agent, can read or delete the file directly; the file mode only keeps out other users.
