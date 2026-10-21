# Agent Mail - Connects you agents securely to your E-Mails
Agent Mail is a simple and secure way to configure access to your email for your agents. It securely stores your credentials hidden from your agents and allows you to configure what agents can access and do.

## Search

`agent-mail search` finds messages in one mailbox the agent's Mandate grants `search` on. It only reads: the mailbox is opened read-only, and no flag (not even `\Seen`) changes.

```
agent-mail search [--mailbox NAME] [--from S] [--to S] [--subject S] [--text S]
                  [--since YYYY-MM-DD] [--before YYYY-MM-DD] [--unseen] [--flagged] [--limit N]
```

- `--mailbox`: the mailbox name as `agent-mail list` prints it. Default `INBOX`.
- `--from`, `--to`, `--subject`, `--text`: the header (or, for `--text`, headers and body) contains the string, ignoring case.
- `--since`: received on or after the date. `--before`: received before the date. Both compare the day the server received the message.
- `--unseen`, `--flagged`: only unread, or only flagged, messages.
- `--limit N`: return at most N messages, 1 to 1000. Default 20.

Criteria combine with AND; with none, every message matches. Results are newest first (highest UID first). Because `--text` after `search` is the text criterion, ask for human-readable output by putting `--text` before the subcommand: `agent-mail --text search --from alice@example.com`.

The JSON result has `account`, `mailbox`, `total` (how many messages matched), `more` (`true` when more matched than are shown; narrow the search or raise `--limit`) and `messages`. Each message has:

- `ref`: the message reference, which later commands such as `read` accept.
- `date`: when the server received it, RFC 3339 UTC, e.g. `2026-10-10T09:00:00Z`.
- `from`: the first sender, `Name <addr>` or `addr`.
- `subject`: the decoded subject.
- `flags`: the server's flags, e.g. `\Seen`, `\Flagged`.

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

## Audit log

Every `agent-mail list`, `agent-mail search` and `agent-mail read` call an agent makes appends one line to an audit log, so you can see which Mandate did what, on which account, and whether its Policy allowed it. The log lives in your per-user data directory:

- macOS: `~/Library/Application Support/agent-mail/audit.log` (the home directory comes from the user database, not from `$HOME`)
- Other Unix: `$XDG_DATA_HOME/agent-mail/audit.log`, or `~/.local/share/agent-mail/audit.log` when `XDG_DATA_HOME` is unset

The directory is created on the first call with mode `0700` and the file with mode `0600`; an existing directory or file with looser modes is tightened to these on the next call. If `audit.log` or its `agent-mail` directory is a symbolic link, or `audit.log` is not a regular file, nothing is written. Entries are only ever appended; no agent-mail command reads the log, so open it yourself with a pager or `jq`. If an entry cannot be written, the call fails with `audit_unavailable` and returns no result.

Each line is one JSON object with these keys, in this order, absent keys left out:

- `time`: when the call was made, RFC 3339 UTC to the second, e.g. `2026-10-21T09:30:15Z`.
- `mandate`: the Mandate's short ID; left out when the agent's token opened no Mandate.
- `operation`: the operation: `list`, `search` or `read`.
- `account`: the account's login; left out when no single account was chosen.
- `mailbox`: the mailbox involved: for `search`, the searched mailbox as the agent named it; for `read`, the mailbox the message reference names (not used by `list`).
- `messages`: the message references involved: for `search`, the references of the messages returned, newest first (left out when the search failed); for `read`, the one reference the agent asked for, also when the read was denied or failed (not used by `list`).
- `results`: for `search`, the number of messages returned, which is the length of `messages` (not used by `list` or `read`, left out when the search failed). How many matched in total is not logged.
- `outcome`: `allowed` if the Mandate's Policy granted the operation, otherwise `denied`.
- `reason`: the error code when the call failed, e.g. `denied`, `invalid_token`, `missing_token`, `login_failed`, `stale_reference` or `message_not_found`; left out on success. An `allowed` call can still carry a reason, such as `login_failed`, when it failed after the Policy granted it.

The log never contains a credential, an agent token, a token hash, a message body, a subject, an attachment name or an address from a message. Any process running as you, including an agent, can read or delete the file directly; the file mode only keeps out other users.
