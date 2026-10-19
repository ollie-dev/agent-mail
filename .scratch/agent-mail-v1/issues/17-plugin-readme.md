# 17: Claude Code plugin and README

**What to build:** Users and Agents can set agent-mail up without help. The repo contains a Claude Code plugin with the agent-mail skill and an MCP entry (no binary); the skill teaches Agents the commands and tells the human how to install the binary if it is missing. The README covers installation, `mandate create`, where tokens go (`env` in a project's local, git-ignored Claude Code settings; `env` of the Claude Desktop MCP entry), the Claude Code sandbox settings (allowed domains for the mail hosts), one Gmail app password per Mandate, the Keychain prompt after updates, and exactly what each OS protects against.

**Blocked by:** 14, 15, 16

**Status:** ready-for-agent

- [ ] Installing the plugin in Claude Code makes the agent-mail skill and the `agent-mail mcp` server available
- [ ] The skill documents every Agent command, the JSON output, the denial error, and that Mandates are created only by the human
- [ ] When the binary is missing, the skill tells the human to run the installer and does not try to install it itself
- [ ] The README shows where to put the Agent Token for Claude Code and for Claude Desktop, using placeholder values only
- [ ] The README lists the sandbox allowed-domain settings for Gmail's IMAP server and how to add other hosts
- [ ] The README states that on macOS a hijacked Agent cannot extract Credentials, that a Keychain prompt outside an update is a red flag, and that tokens in host config files are readable by other Agents of the same user
- [ ] The README recommends one Gmail app password per Mandate and explains revoking on Google's side
- [ ] No example in the plugin or README contains a real Credential or Agent Token
