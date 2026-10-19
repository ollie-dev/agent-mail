# Agent Mail

A command-line tool and MCP server that lets AI agents work with email while the human decides exactly what each agent may do and keeps the mail credentials out of the agent's hands.

## Language

**Account**:
A mail account agent-mail connects to: its IMAP/SMTP servers and login name.
_Avoid_: Mailbox (that is a folder inside an Account), user, profile

**Credential**:
The secret that unlocks an Account, such as a password or app password.
_Avoid_: Login, secret, key

**Agent**:
A caller of agent-mail, such as a Claude Code, Codex or Copilot session; it is known only by the Agent Tokens it presents.
_Avoid_: Client, user, bot

**Mandate**:
A sealed, unchangeable authority over one or more Accounts: their Credentials, a Policy and the hash of one Agent Token. Changing anything means creating a new Mandate; an Agent may hold several.
_Avoid_: Profile, config, vault, setup

**Agent Token**:
The token that unlocks exactly one Mandate; whoever presents it acts under that Mandate's Policy.
_Avoid_: API key, password

**Policy**:
The set of Grants inside one Mandate; anything not granted is denied.
_Avoid_: Access rights, permissions, profile

**Grant**:
One permission inside a Policy, naming an operation and the Account and mailboxes it applies to.
_Avoid_: Right, permission, rule
