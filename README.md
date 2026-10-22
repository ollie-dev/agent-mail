# Agent Mail - Connects you agents securely to your E-Mails
Agent Mail is a simple and secure way to configure access to your email for your agents. It securely stores your credentials hidden from your agents and allows you to configure what agents can access and do.

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
- `reason`: the error code when the call failed, e.g. `denied`, `invalid_token`, `missing_token`, `account_required`, `ambiguous_mandate`, `login_failed`, `stale_reference`, `message_not_found`, `unsupported_flag`, `same_mailbox`, `move_unsupported`, `drafts_not_found`, `invalid_draft` or `unsupported`; left out on success. An `allowed` call can still carry a reason, such as `login_failed`, when it failed after the Policy granted it.

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
