# Agent Mail - Connects you agents securely to your E-Mails
Agent Mail is a simple and secure way to configure access to your email for your agents. It securely stores your credentials hidden from your agents and allows you to configure what agents can access and do.

## Install

agent-mail releases are built for macOS (Apple silicon and Intel). Install the latest release with:

```sh
curl -fsSL https://github.com/ollie-dev/agent-mail/releases/latest/download/install.sh | sh
```

The installer picks the binary for your Mac's architecture, downloads it with its SHA-256 checksum file, verifies the checksum and only then installs the binary to `~/.local/bin/agent-mail`. If the checksum does not match, it stops and installs nothing. It never uses `sudo` and needs no Rust toolchain. If `~/.local/bin` is not on your `PATH`, it prints a hint.

The checksum comes from the same release as the binary, so it catches a corrupted or tampered download, not a compromised release. To check that a release was built from this repository by its release workflow, download the tarball from the [releases page](https://github.com/ollie-dev/agent-mail/releases) and verify its build attestation with the GitHub CLI:

```sh
gh attestation verify agent-mail-aarch64-apple-darwin.tar.gz --repo ollie-dev/agent-mail
```

On macOS the first Keychain access after an update prompts once, because each release has a new code signature; allow it. A Keychain prompt at any other time is a red flag: deny it.

Release binaries are signed ad hoc with the hardened runtime, which blocks code injection into agent-mail. A release binary checks its own signature before it touches the Keychain; if the hardened runtime is missing, every command that needs the Keychain fails with `hardened_runtime_missing` and the Keychain is not accessed. Reinstall from the releases page if you see it.

## Creating a Mandate

A Mandate names the email accounts an Agent may use and grants it operations (`list`, `search`, `read`, `flag`, `move`, `draft`) on chosen mailboxes. Only you create Mandates; an Agent cannot. Run this in your own terminal, not inside an Agent session or sandbox (it refuses to run there):

```sh
agent-mail mandate create
```

It asks for a name, then for each account its server, login and Credential (typed hidden; for Gmail an app password, see [Gmail app passwords](#gmail-app-passwords)). It checks a live login, then lets you tick mailboxes and operations.

At the end it stores the Mandate in the Keychain and prints its Agent Token. The token is shown once and never stored in readable form: copy it into the Agent's host now (see [Agent Tokens in Claude Code and Claude Desktop](#agent-tokens-in-claude-code-and-claude-desktop)). Lost it? Create a new Mandate.

It then offers to save the Mandate's accounts and Grants (never its Credentials or token) as a TOML file. `agent-mail mandate create --from <FILE>` pre-fills a new setup from that file; every Credential is still typed and checked.

A Mandate cannot be changed. To give an Agent different access, create a new Mandate, swap in its token, and delete the old one:

```sh
agent-mail mandate list                 # IDs, names, accounts, Grants, creation dates
agent-mail mandate delete <ID>          # revokes that Mandate's Agent Token at once
```

## Agent Tokens in Claude Code and Claude Desktop

The Agent gets its Agent Token from the `AGENT_MAIL_TOKEN` environment variable. Give each project its own Mandate, and never put a token in a file you commit.

**Claude Code.** Put the token in the `env` of the project's `.claude/settings.local.json`. This file is git-ignored: Claude Code adds it to your global git excludes when it creates the file; if you create it by hand, add it to `.gitignore` yourself.

```json
{"env":{"AGENT_MAIL_TOKEN":"<your agent token>"}}
```

Claude Code applies it once you trust the project folder, to commands the Agent runs and to the [plugin's](#claude-code-plugin) MCP server. Restart Claude Code after changing it. Several tokens go in one value, comma-separated.

**Claude Desktop.** Add agent-mail as an MCP server in `claude_desktop_config.json` (Settings, Developer, Edit Config), with the token in the server's `env`, then restart Claude Desktop:

```json
{
  "mcpServers": {
    "agent-mail": {
      "command": "agent-mail",
      "args": ["mcp"],
      "env": { "AGENT_MAIL_TOKEN": "<your agent token>" }
    }
  }
}
```

If Claude Desktop cannot find `agent-mail`, give the full path, such as `/Users/<you>/.local/bin/agent-mail`, as `command`.

## Claude Code plugin

The plugin gives every Claude Code session the agent-mail skill, which teaches the Agent the commands, and the `agent-mail mcp` server. It carries no binary: [install](#install) agent-mail first. In Claude Code, run:

```
/plugin marketplace add ollie-dev/agent-mail
/plugin install agent-mail@agent-mail
```

The plugin's MCP server (`plugin/.mcp.json`) holds no token. It passes on `AGENT_MAIL_TOKEN` from Claude Code's environment, which the project's `.claude/settings.local.json` sets (see [above](#agent-tokens-in-claude-code-and-claude-desktop)).

If agent-mail is not installed, the skill tells the Agent to ask you to run the installer; it never installs anything itself.

## Gmail app passwords

Gmail needs an app password for IMAP (2-Step Verification must be on). Use one app password per Mandate, and name it after the Mandate. Create them at https://myaccount.google.com/apppasswords.

With one app password per Mandate you can see in Google's list which Agent is which, and revoke one Agent on Google's side without touching the others. Revoke means deleting the app password there; that Mandate's logins then fail with `login_failed`.

To retire a Mandate, do both: run `agent-mail mandate delete <ID>` to revoke its Agent Token, and revoke its app password at Google.

## What each OS protects against

**macOS.** Credentials live in the Keychain, readable only by the signed agent-mail binary. A hijacked Agent cannot extract a Credential: it can only call agent-mail, which enforces the Mandate's Policy and logs each call.

The Keychain prompts once after each agent-mail update; allow it. A Keychain prompt at any other time is a red flag: deny it. Approving it could hand a Credential to another program.

**Agent Tokens.** A token in a host config file, such as `.claude/settings.local.json` or `claude_desktop_config.json`, is plain text. Any program running as the same user can read it, including other Agents. Whoever holds an Agent Token can do what its Mandate grants, and no more.

So keep Grants narrow, give each project its own Mandate, and delete a Mandate whose token may have leaked. The audit log can also be read or changed by any program running as you.

**Windows.** There is no Windows release yet. The Windows milestone, when it ships, protects only against honest Agents: any program running as the same user can read the Credentials, so a hijacked Agent could extract them.

## Several Mandates and whoami

An agent can hold several Mandates at once, for example one for a work account and one for a private account. `AGENT_MAIL_TOKEN` takes one or more agent tokens, comma-separated; spaces around each token and empty fields are ignored, and a token given twice counts once:

```
AGENT_MAIL_TOKEN="amt_...work...,amt_...private..."
```

**Choosing the account.** Every command works on one account. On `list`, `search` and `draft`, name it with `--account <LOGIN>`, the account's login exactly as it appears in Grants and output (for example `agent-mail list --account agent@example.test`). It may be left out when the presented tokens reach only one account. `read`, `flag`, `move` and `draft --reply-to` take the account from the message reference, so a reference from `search` can be followed without naming the account again; they do not accept `--account`, and `draft` refuses `--account` together with `--reply-to`. An empty `--account`, or one holding a line break or NUL, is a usage error (exit code 2).

**One Mandate per call.** agent-mail uses the one presented Mandate that covers the account. Every Grant check, the mailboxes `list` shows, the credential used and the audit line's `mandate` come from that Mandate only: Grants never combine across Mandates, so a Grant in one Mandate never allows anything on an account covered only by another. An account no presented Mandate covers is denied (`denied`) before any connection; since no Grant can fix that, its message says no presented Mandate covers the account and points to `agent-mail whoami` or a new token from the human, rather than to a missing Grant.

**`account_required`.** When the tokens reach several accounts and the command names none, the call fails before any connection with `account_required`; its `accounts` list holds the reachable logins to choose from. Run the command again with `--account` and one of them:

```json
{"error":{"accounts":["agent@example.test","other@example.test"],"code":"account_required","message":"..."}}
```

**`ambiguous_mandate`.** When two presented Mandates cover the same account, agent-mail never picks one: any command on that account fails before any Grant check or connection with `ambiguous_mandate`, naming the `account` and the short IDs of the Mandates in `mandates`, in token order. This happens even when only one of them grants the operation. Present only one of those tokens in `AGENT_MAIL_TOKEN`, or ask the human to delete one of the Mandates with `agent-mail mandate delete <ID>`. Commands on other accounts are unaffected.

**Invalid tokens.** A token that is malformed, unknown, or whose stored record does not open is ignored, and the valid tokens still work. The call's result is unchanged, and stderr gets one warning line before any error, giving the 1-based position of each ignored token in `AGENT_MAIL_TOKEN` (empty fields count), never the token itself:

```json
{"warning":{"code":"invalid_token","positions":[1],"message":"the token at position 1 of AGENT_MAIL_TOKEN is not valid and was ignored; check it, or ask the human for a new token"}}
```

With `--text` it is `warning: invalid_token: <message>`. If no token is valid, the call fails with `invalid_token`; if no token is given at all, with `missing_token`.

**`whoami`.** `agent-mail whoami` shows what the presented tokens allow, so an agent can check before it tries. It needs no Grant, makes no connection and writes no audit line. It prints one JSON object: `mandates`, in token order, each with `id`, `name`, `accounts` (`login`, `host`, `port`, `tls`) and `grants` (`operation`, `account`, `mailboxes`), taken from the opened Mandate, and `invalid_tokens`, the positions of the tokens that are not valid. Two Mandates covering one account are both shown. It never shows a credential, a token or a token hash.

```json
{"mandates":[{"id":"aaaa1111","name":"work-inbox","accounts":[{"login":"agent@example.test","host":"imap.example.test","port":993,"tls":"implicit"}],"grants":[{"operation":"list","account":"agent@example.test","mailboxes":["INBOX","Archive"]}]}],"invalid_tokens":[]}
```

With `--text` (`agent-mail --text whoami` or `agent-mail whoami --text`) each Mandate is a line `<id>  <name>`, followed by `  account <login> (<host>:<port>, <tls>)` and `  grant <operation> on <account>: <mailboxes>` lines, and each invalid token a line `token <position> of AGENT_MAIL_TOKEN is not valid and was ignored`.

## Search

`agent-mail search` finds messages in one mailbox the agent's Mandate grants `search` on, in the account given with `--account` (see [Several Mandates and whoami](#several-mandates-and-whoami)). It only reads: the mailbox is opened read-only, and no flag (not even `\Seen`) changes.

```
agent-mail search [--account LOGIN] [--mailbox NAME] [--from S] [--to S] [--subject S] [--text S]
                  [--since YYYY-MM-DD] [--before YYYY-MM-DD] [--unseen] [--flagged] [--limit N]
agent-mail search [--account LOGIN] [--mailbox NAME] --gmail QUERY [--limit N]
```

- `--mailbox`: the mailbox name as `agent-mail list` prints it. Default `INBOX`.
- `--from`, `--to`, `--subject`, `--text`: the header (or, for `--text`, headers and body) contains the string, ignoring case.
- `--since`: received on or after the date. `--before`: received before the date. Both compare the day the server received the message.
- `--unseen`, `--flagged`: only unread, or only flagged, messages.
- `--limit N`: return at most N messages, 1 to 1000. Default 20.
- `--gmail QUERY`: search with Gmail's own search syntax instead, for example `agent-mail search --gmail "has:attachment newer_than:7d"`. See below.

Criteria combine with AND; with none, every message matches. Results are newest first (highest UID first). Because `--text` after `search` is the text criterion, ask for human-readable output by putting `--text` before the subcommand: `agent-mail --text search --from alice@example.com`.

The JSON result has `account`, `mailbox`, `total` (how many messages matched), `more` (`true` when more matched than are shown; narrow the search or raise `--limit`) and `messages`. Each message has:

- `ref`: the message reference, which later commands such as `read` accept.
- `date`: when the server received it, RFC 3339 UTC, e.g. `2026-10-10T09:00:00Z`.
- `from`: the first sender, `Name <addr>` or `addr`.
- `subject`: the decoded subject.
- `flags`: the server's flags, e.g. `\Seen`, `\Flagged`.

**Gmail search.** With `--gmail QUERY`, the query is passed to the server unchanged as Gmail's raw search (`X-GM-RAW`), so every Gmail operator such as `from:`, `has:attachment`, `newer_than:7d` or `label:` works as it does in Gmail's search box. It works only on servers that advertise Gmail's IMAP extensions (`X-GM-EXT-1`); elsewhere the call fails with `unsupported` before any mailbox is opened, and the standard criteria are the way to search. `--gmail` cannot be combined with the standard criteria (`--from`, `--to`, `--subject`, `--text`, `--since`, `--before`, `--unseen`, `--flagged`): that is a usage error (exit code 2). It works with `--mailbox` and `--limit`. The query must not be empty or hold a line break or NUL. Grants, the limit, the result shape and the audit line are the same as for a standard search, and, as with the standard criteria, the query is never written to the audit log or echoed in an error.

A message reference has the form `<account>/<mailbox>/<uidvalidity>/<uid>`. The account and mailbox are percent-encoded like URL path segments: letters, digits and `-._~!$&'()*+,;=:@` stay as they are, every other byte of their UTF-8 (such as `%`, `/`, `[`, `]` or a space) becomes `%XX` with upper-case hex. For example, message 7 in `[Gmail]/Entwürfe` with UIDVALIDITY 1700000003 is `agent@example.test/%5BGmail%5D%2FEntw%C3%BCrfe/1700000003/7`. The UIDVALIDITY ties the reference to the mailbox's current numbering, so a reference can be recognised as stale if the server ever renumbers the mailbox.

## Read

`agent-mail read <REF>` returns one message, given the `ref` that `agent-mail search` printed for it. The Mandate needs a `read` Grant on the reference's account and mailbox; otherwise the call is denied before any connection is made. Reading never marks the message as seen: the mailbox is opened read-only and the message is fetched with `BODY.PEEK[]`.

```
agent-mail read <REF> [--full]
```

The JSON result has these keys, in this order:

- `ref`, `account`, `mailbox`: the message reference, and the account and mailbox it names.
- `date`: the message's `Date:` header in RFC 3339 UTC; `""` when it is missing or cannot be parsed.
- `from`: the first sender, `Name <addr>` or `addr`. `to`, `cc`: every recipient in the header, same format.
- `subject`: the decoded subject. `message_id`: the `Message-ID` header, with its angle brackets.
- `body`: the text of the first `text/plain` part, or else the first `text/html` part converted to plain text (no markup, styles or scripts; nothing is fetched or run). Line endings are `\n` and trailing line breaks are removed.
- `truncated`: whether `body` was cut. A body longer than 20,000 characters is cut at 20,000 and ends with `[truncated: showing 20000 of <N> characters; read again with --full for the complete body]`. `--full` returns the whole body.
- `attachments`: each attachment's `name`, `type` (e.g. `application/pdf`) and `size` in bytes. Attachment content is never returned.

With `--text` (`agent-mail --text read <REF>` or `agent-mail read <REF> --text`) the output is `From:`, `To:`, `Cc:` (when there is one), `Date:`, `Subject:` and `Ref:` lines, a blank line, the body, and then the attachment list. Control characters from the message are shown escaped.

If the mailbox was renumbered since the search (its UIDVALIDITY changed), the read fails with `stale_reference` and nothing is fetched: search again for a fresh reference. A UID that no longer exists fails with `message_not_found`. A reference that is not in the form `search` prints is a usage error (exit code 2).

## Flag

`agent-mail flag <REF> --set <FLAG>` or `agent-mail flag <REF> --clear <FLAG>` marks one message read or unread, or starred or unstarred, given the `ref` that `agent-mail search` printed for it. The Mandate needs a `flag` Grant on the reference's account and mailbox; otherwise the call is denied before any connection is made.

```
agent-mail flag <REF> (--set <FLAG> | --clear <FLAG>)
```

Exactly one of `--set` or `--clear` is required. Only two flags can be changed, named without regard to case and with or without the backslash:

- `seen` or `\Seen`: the message is read. `--set seen` marks it read, `--clear seen` unread.
- `flagged` or `\Flagged`: the message is starred. `--set flagged` stars it, `--clear flagged` unstars it.

Any other flag, such as `\Deleted`, `\Answered`, `\Draft` or a keyword like `$Junk`, fails with `unsupported_flag` before any connection is made; the error does not repeat the rejected name. Only the canonical `\Seen` or `\Flagged` is ever sent to the server.

The JSON result has these keys, in this order: `ref`, `account`, `mailbox`, and `flags`, every flag of the message after the change, as the server reports them, in server order. For example:

```json
{"ref":"agent@example.test/Triage/1700000007/1","account":"agent@example.test","mailbox":"Triage","flags":["\\Seen"]}
```

Setting a flag that is already set, or clearing one that is not, succeeds and reports the unchanged flags, so repeating a call is harmless. With `--text` (`agent-mail --text flag ...` or after the subcommand) the output is one line, `<ref>: <flags>`, or `<ref>: no flags`.

The mailbox is opened read-write with `SELECT`, and the session ends with `LOGOUT` only: agent-mail never sends `CLOSE` (which would expunge messages marked `\Deleted`) or `EXPUNGE`, and never fetches a message body. If the mailbox was renumbered since the search (its UIDVALIDITY changed), nothing is changed and the call fails with `stale_reference`: search again for a fresh reference. A UID that no longer exists fails with `message_not_found`, also without any change. A reference that is not in the form `search` prints, or a call with neither or both of `--set` and `--clear`, is a usage error (exit code 2).

## Move

`agent-mail move <REF> --to <MAILBOX>` moves one message, given the `ref` that `agent-mail search` printed for it, to another mailbox of the same account, for example to file or archive it. The Mandate needs a `move` Grant on both mailboxes of the reference's account: the source mailbox the reference names and the target mailbox. If either is missing, the call is denied before any connection is made, and the error's `mailbox` names the mailbox that lacks the Grant (the source is checked first).

```
agent-mail move <REF> --to <MAILBOX>
```

`--to` is required and takes the mailbox name as `agent-mail list` prints it, e.g. `Archive` or `[Gmail]/Entwürfe`. An empty name, or one containing a line break or NUL, is a usage error (exit code 2), as is a reference that is not in the form `search` prints. Naming the reference's own mailbox fails with `same_mailbox` before any connection is made.

The JSON result has these keys, in this order: `ref` (as given), `account`, `mailbox` (the source), `target` (as given), and `new_ref`, the message's reference in the target mailbox, built from the target's UIDVALIDITY and the new UID the server reported in a `COPYUID` response (RFC 4315). Servers without UIDPLUS report none, and then `new_ref` is `null`: search the target mailbox to find the message. For example:

```json
{"ref":"agent@example.test/INBOX/1700000001/2","account":"agent@example.test","mailbox":"INBOX","target":"Archive","new_ref":"agent@example.test/Archive/1700000002/26"}
```

With `--text` (`agent-mail --text move ...` or after the subcommand) the output is one line, `<ref> -> <new_ref>`, or `<ref> -> <target> (new reference not reported)`.

How the message is moved depends on what the server advertises after login:

- With `MOVE` (RFC 6851), agent-mail sends `UID MOVE`.
- Without `MOVE` but with `UIDPLUS`, it sends `UID COPY`, marks that one message `\Deleted` with `UID STORE`, and removes it with `UID EXPUNGE` of that one UID only, so messages you marked `\Deleted` yourself are never expunged. If a step after the copy fails, the error is reported and you may find a copy in both mailboxes, or a `\Deleted` original in the source.
- With neither, the call fails with `move_unsupported` and nothing is selected, copied or changed, because the only remaining way, a plain `EXPUNGE`, would also remove other messages marked `\Deleted`.

agent-mail never sends `CLOSE` or a plain `EXPUNGE` and never fetches a message body. If the mailbox was renumbered since the search (its UIDVALIDITY changed), nothing is moved and the call fails with `stale_reference`: search again for a fresh reference. A UID that no longer exists fails with `message_not_found`, and a target mailbox the server refuses (for example one that does not exist) fails with `mailbox_not_found` naming the target; in both cases nothing is moved. agent-mail does not create mailboxes.

## Draft

`agent-mail draft` prepares a message for you to review and send: it is appended, with the `\Draft` flag, to your account's Drafts mailbox, and you send it (or not) from your own mail client. agent-mail never sends mail.

```
agent-mail draft [--account <LOGIN>] --to <ADDR> [--to <ADDR>...] --subject <TEXT> --body <TEXT>
agent-mail draft --to <ADDR> [--to <ADDR>...] --reply-to <REF> --body <TEXT>
```

The Mandate needs a `draft` Grant on the account; `draft` Grants take no mailbox list. Without `--reply-to` the account is the one given with `--account`, which may be left out when the presented tokens reach only one account; with several and no `--account`, the call fails with `account_required` and lists them (see [Several Mandates and whoami](#several-mandates-and-whoami)). With `--reply-to` the account is the reference's, and `--account` is not allowed. If the Grant is missing, the call is denied before any connection is made.

- `--to` is required and repeatable; recipients keep their order. Each value is a bare address `local@domain`: exactly one `@` with something on each side, printable ASCII only, no spaces and none of `<>()[]\,;:"`, at most 254 characters. Display names (`Carol <carol@example.com>`) are not accepted.
- `--subject` is required without `--reply-to`, and cannot be combined with it. It may hold any text except line breaks and NUL, and must not be empty.
- `--body` is required. It may hold any text except NUL, including line breaks.

Anything else is a usage error (exit code 2): nothing is connected to and nothing is logged.

**How the Drafts mailbox is found.** agent-mail lists your mailboxes and uses the first one, in server order, that the server marks with the `\Drafts` special-use attribute (RFC 6154), whatever its name, so localised names such as `[Gmail]/Entwürfe` work. The name never comes from the agent, so an agent cannot write into another mailbox. If no mailbox is marked, the call fails with `drafts_not_found` and nothing is created: create a Drafts folder, or mark one as the Drafts folder, in your mail client.

**Reply mode.** With `--reply-to <REF>`, given the `ref` that `agent-mail search` printed for the original message, the draft threads with it: `In-Reply-To` is the original's Message-ID, `References` is the original's References followed by its Message-ID, and the subject is the original's with `Re: ` in front, unless it already starts with `Re:` (in any case). The account is the Mandate's account whose login matches the reference, and the Mandate also needs a `read` Grant on the original's mailbox; without it the call is denied (with `operation` `read` and the mailbox) before any connection. The original is opened read-only and its headers are fetched with a peek, so it is not marked as read. Message ids from the original that hold anything but printable ASCII without spaces or angle brackets are left out; an original without a Message-ID gets a reply subject but neither header. If the mailbox was renumbered since the search, the call fails with `stale_reference`, and a UID that no longer exists fails with `message_not_found`; in both cases no draft is created.

The draft is a plain-text UTF-8 message with `From` (your login), `To`, `Subject` (RFC 2047-encoded when not ASCII), `Date` and a random `Message-ID`; its body is quoted-printable encoded, so no line is too long.

The JSON result has these keys, in this order: `ref` (the new draft's reference, built from the `APPENDUID` response (RFC 4315); `null` when the server reports none), `account`, `mailbox` (the Drafts mailbox), `to`, `subject` (as sent, which in reply mode is the reply subject), and `original` (the `--reply-to` reference, or `null`). For example:

```json
{"ref":"agent@example.test/Drafts/1700000010/1","account":"agent@example.test","mailbox":"Drafts","to":["carol@example.com"],"subject":"Status","original":null}
```

With `--text` (`agent-mail --text draft ...` or after the subcommand) the output is one line, `draft created: <ref>`, or `draft created in <mailbox> (new reference not reported)`.

Errors besides those above: `denied`, `drafts_not_found`, `invalid_draft` (a value the command line would have refused, reached through another adapter; the message names the field but never echoes the value), `stale_reference`, `message_not_found`, `invalid_reference`, and the connection errors every command shares.

Drafts cannot be edited, replaced or deleted with agent-mail: there is no command for it, and `draft` never selects a mailbox read-write or sends `STORE`, `COPY`, `MOVE` or `EXPUNGE`. Change or discard a draft in your own mail client.

## MCP server

`agent-mail mcp` runs agent-mail as an MCP server over stdio, so a host app
(Claude Desktop, Cowork or Claude Code) can start it and the agent calls its
commands as MCP tools. The agent token comes from the server's environment.

For Claude Desktop, put it in the `env` of the `claude_desktop_config.json` entry, with `"command": "agent-mail"` and `"args": ["mcp"]`; see [Agent Tokens in Claude Code and Claude Desktop](#agent-tokens-in-claude-code-and-claude-desktop).

In Claude Code the [plugin](#claude-code-plugin) adds the server (its `plugin/.mcp.json` holds no token). Without the plugin, add it for yourself only with:

```sh
claude mcp add agent-mail --env AGENT_MAIL_TOKEN=<your agent token> -- agent-mail mcp
```

Never put a token in a project's checked-in `.mcp.json`.

The server offers seven tools: `whoami`, `list`, `search`, `read`, `flag`,
`move` and `draft`. Their arguments mirror the command-line options
(`reply_to` for `--reply-to`, `ref` for the message reference, `to` of
`draft` as a list of addresses), and each returns the same JSON as the CLI
command, with the same audit log entry. An agent starts with `whoami`, then
`list` and `search`, and passes the `ref` values `search` returns to `read`,
`flag`, `move` and `draft`.

- A successful call returns the CLI's JSON line as text content and as
  `structuredContent`. With an invalid token among several, the
  `invalid_token` warning follows as a second text item.
- A failed call is a tool error (`isError` true) whose content is the CLI's
  error object `{"error":{"code":...}}`, with the CLI's codes (`denied`,
  `missing_token`, `stale_reference` and so on). Arguments the CLI would
  refuse (a malformed reference, both `set` and `clear`, an unknown
  property, a wrong type) fail with `invalid_arguments`, a code only the MCP
  server uses, before anything is connected or logged.
- The token comes only from the server's environment (`AGENT_MAIL_TOKEN`,
  read once when the host starts the server; restart it for a new token).
  No tool argument or request field can supply or override it.
- Mandate management is CLI-only: no tool creates, lists or deletes
  Mandates. Run `agent-mail mandate` in your own terminal for that.

stdout carries only JSON-RPC messages, one per line.

## Sandbox and proxies

**Allowed domains.** The Claude Code sandbox lets commands reach only the hosts in its allowed domains, which start empty. Allow Gmail's IMAP server in `sandbox.network.allowedDomains` of the project's `.claude/settings.local.json` (or of `.claude/settings.json` to share it with your team):

```json
{
  "sandbox": {
    "network": {
      "allowedDomains": ["imap.gmail.com"]
    }
  }
}
```

An entry without a port allows every port, so this covers port 993. For another account, run `agent-mail whoami` and add each account's `host` it shows to the same list. A host missing from the list makes calls fail with `proxy_refused`, naming the `host` to add.

The sandbox applies to commands the Agent runs in the shell. The `agent-mail mcp` server is started by Claude Code itself and connects directly. Under an organisation's managed settings, ask your administrator to allow the host.

Inside the Claude Code sandbox there is no direct network: agent-mail reaches
the mail server through the proxy named in the environment, and nothing
changes in how an Agent calls it. The first non-empty one of `HTTPS_PROXY`,
`https_proxy`, `ALL_PROXY` and `all_proxy` is used (`HTTP_PROXY` and
`NO_PROXY` are ignored):

- `http://host:port` (or no scheme; default port 80) tunnels with HTTP
  CONNECT; `socks5://host:port` or `socks5h://host:port` (default port 1080)
  uses SOCKS5. Credentials in the URL (`user:password@`, percent-encoded) are
  sent as `Proxy-Authorization: Basic` or SOCKS5 username/password, and never
  appear in output or the audit log.
- The mail server's host name is sent to the proxy unresolved.
- TLS still runs end to end to the mail server (implicit TLS or mandatory
  STARTTLS), so the proxy sees only ciphertext. Because the system
  certificate verifier is unavailable in the sandbox, proxy mode verifies
  against built-in root certificates (webpki-roots). Without a proxy,
  agent-mail connects directly and uses the system verifier.
- The live login check of `mandate create` also goes through a proxy set in
  the human's shell.

Two error codes are specific to proxies:

- `proxy_refused`: the proxy refused the tunnel (an HTTP 4xx answer such as
  `403`, a SOCKS5 "not allowed by ruleset" reply or rejected proxy
  credentials). The error carries `account` and `host`; ask the human to
  allow that host in the sandbox's allowed domains.
- `invalid_proxy`: the proxy variable is not a usable `http://` or
  `socks5://` URL. The message names the variable but never echoes its
  value; nothing is connected to.

A proxy that cannot be reached or answers malformed data gives
`connection_failed`, as a direct connection would.

## Audit log

Every `agent-mail list`, `agent-mail search`, `agent-mail read`, `agent-mail flag`, `agent-mail move` and `agent-mail draft` call an agent makes appends one line to an audit log, so you can see which Mandate did what, on which account, and whether its Policy allowed it. The log lives in your per-user data directory:

- macOS: `~/Library/Application Support/agent-mail/audit.log` (the home directory comes from the user database, not from `$HOME`)
- Other Unix: `$XDG_DATA_HOME/agent-mail/audit.log`, or `~/.local/share/agent-mail/audit.log` when `XDG_DATA_HOME` is unset

The directory is created on the first call with mode `0700` and the file with mode `0600`; an existing directory or file with looser modes is tightened to these on the next call. If `audit.log` or its `agent-mail` directory is a symbolic link, or `audit.log` is not a regular file, nothing is written. Entries are only ever appended; no agent-mail command reads the log, so open it yourself with a pager or `jq`. If an entry cannot be written, the call fails with `audit_unavailable` and returns no result.

Each line is one JSON object with these keys, in this order, absent keys left out:

- `time`: when the call was made, RFC 3339 UTC to the second, e.g. `2026-10-21T09:30:15Z`.
- `mandate`: the short ID of the Mandate selected for the call; left out when no presented token opened a Mandate, when no account was chosen, when no presented Mandate covers the account, or when several do (`ambiguous_mandate`).
- `operation`: the operation: `list`, `search`, `read`, `flag`, `move` or `draft`.
- `account`: the account's login; left out when no single account was chosen (`account_required`).
- `mailbox`: the mailbox involved: for `search`, the searched mailbox as the agent named it; for `read`, `flag` and `move`, the mailbox the message reference names, which for `move` is the source; for `draft`, the Drafts mailbox, once it was found (left out when the call was denied or failed before that) (not used by `list`).
- `target`: for `move`, the target mailbox as the agent named it, also when the call was denied or failed. Not used by `list`, `search`, `read`, `flag` or `draft`.
- `messages`: the message references involved: for `search`, the references of the messages returned, newest first (left out when the search failed); for `read` and `flag`, the one reference the agent asked for, also when the call was denied or failed; for `move`, the reference the agent asked for, followed on success by the message's new reference in the target when the server reported one; for `draft`, in reply mode the original's reference (also when the call was denied or failed), followed on success by the new draft's reference when the server reported one, and left out when that leaves none (not used by `list`).
- `recipients`: for `draft`, the recipients the agent gave, in order, on every `draft` entry including denials; an empty list when one of them was not a valid address, so a rejected value is never logged. Not used by `list`, `search`, `read`, `flag` or `move`.
- `change`: for `flag`, the change asked for: `+\Seen` or `+\Flagged` for `--set`, `-\Seen` or `-\Flagged` for `--clear`; also when the call was denied or failed. Left out when the flag name was not supported (`unsupported_flag`), so an unsupported name is never logged. Not used by `list`, `search`, `read`, `move` or `draft`.
- `results`: for `search`, the number of messages returned, which is the length of `messages` (not used by `list`, `read`, `flag`, `move` or `draft`, left out when the search failed). How many matched in total is not logged.
- `outcome`: `allowed` if the Mandate's Policy granted the operation, otherwise `denied`.
- `reason`: the error code when the call failed, e.g. `denied`, `invalid_token`, `missing_token`, `account_required`, `ambiguous_mandate`, `login_failed`, `stale_reference`, `message_not_found`, `unsupported_flag`, `same_mailbox`, `move_unsupported`, `drafts_not_found`, `invalid_draft`, `store_unavailable`, `hardened_runtime_missing` or `unsupported`; left out on success. An `allowed` call can still carry a reason, such as `login_failed`, when it failed after the Policy granted it.

The log never contains a credential, an agent token, a token hash, a message body, a subject, an attachment name or an address from a message. For `draft` it holds the recipients the agent gave and references only, never the draft's subject or body or the original's headers. Any process running as you, including an agent, can read or delete the file directly; the file mode only keeps out other users.

## Development

`cargo test` runs the test suite against an in-process fake IMAP server; it never talks to a real mail server.

**Live Gmail test.** `tests/gmail_live.rs` holds an opt-in test against a real Gmail account, ignored by default. It checks `list`, a standard search, a `--gmail` search, that the Drafts mailbox is found, and that `read` leaves a message unread. It only reads: it creates no draft, sets no flag and moves no message. Use a dedicated test account, never your own mail, with IMAP enabled and an app password. Its INBOX must not be empty and must hold at least one unread message. Export the account's address and app password, then run the test:

```
export AGENT_MAIL_LIVE_GMAIL_ADDRESS=you.test@gmail.com
read -rs AGENT_MAIL_LIVE_GMAIL_APP_PASSWORD     # type the app password; it stays out of your shell history
export AGENT_MAIL_LIVE_GMAIL_APP_PASSWORD
cargo test --test gmail_live -- --ignored
```

The app password is held in memory only: the test keeps it in an in-memory Mandate store, writes its audit lines to a temporary file, and checks that neither the output nor that file contains it or the agent token. Without the two variables the test prints a one-line skip message and passes.

**Release-binary tests.** `tests/selfcheck.rs` holds opt-in macOS tests, ignored by default, that build the release binary, sign a copy with `scripts/sign-macos.sh` and another without the hardened runtime, and check that only the first reaches the Keychain, that it links only system libraries and that it runs without a Rust toolchain. They present a freshly minted token that no Keychain item matches, so they raise no prompt; the refused `list` run adds one `denied` line to your audit log. Run them with `cargo test --test selfcheck -- --ignored`.

**Releasing.** Bump `version` in `Cargo.toml`, commit, then tag the commit `vX.Y.Z` with the same version and push the tag (`git tag vX.Y.Z && git push origin vX.Y.Z`). The `release` workflow (`.github/workflows/release.yml`) checks that the tag matches `Cargo.toml` and runs the tests in a `test` job that only has read access to the repository; once it passes, the `publish` job builds both macOS targets, signs them with `scripts/sign-macos.sh`, attests them and publishes the release with `.github/release-notes.md` as its notes. A final `verify` job then runs `scripts/verify-release.sh vX.Y.Z` on a fresh runner, checking the checksums, signatures and attestations of the published assets; you can run the same script locally against any published tag.
