---
name: agent-mail
description: Work with the human's email through agent-mail. Use it to read, search and triage email (mark read or starred, move to another mailbox) and to draft replies or new messages for the human to send. Access is limited to what the human's Mandate grants.
---

# agent-mail

agent-mail gives you access to the human's email accounts within the limits of a Mandate the human created. A Mandate names the accounts and grants operations (`list`, `search`, `read`, `flag`, `move`, `draft`) on chosen mailboxes. You never see the mail password.

You can use agent-mail in two ways, with the same results and errors:

- The MCP tools of the `agent-mail` server: `whoami`, `list`, `search`, `read`, `flag`, `move` and `draft`. Their arguments mirror the command-line options (`ref` for the message reference, `reply_to` for `--reply-to`, `to` as a list).
- The `agent-mail` command in the shell, described below.

## Agent Token

Every command reads one or more Agent Tokens, comma-separated, from the `AGENT_MAIL_TOKEN` environment variable. The human puts them there. Never print, echo, log or ask for a token, and never pass one as an argument.

If no token is set the call fails with `missing_token`; if none is valid, with `invalid_token`. Ask the human to set a valid token in `AGENT_MAIL_TOKEN` themselves and restart the host. Never have them paste a token into the chat.

## Workflow

1. Run `agent-mail whoami` to see which accounts, mailboxes and operations you have.
2. Run `agent-mail list` to see the mailboxes, then `agent-mail search` to find messages.
3. Pass the `ref` values that `search` returns to `read`, `flag`, `move` and `draft --reply-to`.

Never send mail: agent-mail cannot. `draft` saves a draft that the human reviews and sends.

## Commands

- `agent-mail whoami`: the presented Mandates, each with `id`, `name`, `accounts` (`login`, `host`, `port`, `tls`) and `grants` (`operation`, `account`, `mailboxes`), plus `invalid_tokens`. Needs no Grant and makes no connection.
- `agent-mail list [--account LOGIN]`: the mailboxes you may use, as `{"account":...,"mailboxes":[{"name":...}]}`.
- `agent-mail search [--account LOGIN] [--mailbox NAME] [--from S] [--to S] [--subject S] [--text S] [--since YYYY-MM-DD] [--before YYYY-MM-DD] [--unseen] [--flagged] [--limit N]`: newest first. Mailbox defaults to `INBOX`, limit to 20 (at most 1000).
- `agent-mail search [--account LOGIN] [--mailbox NAME] --gmail QUERY [--limit N]`: Gmail's own search syntax, such as `--gmail "has:attachment newer_than:7d"`. Only on Gmail; elsewhere it fails with `unsupported`. It cannot be combined with the other criteria.
- `agent-mail read REF [--full]`: one message, without marking it read. A body over 20,000 characters is cut, with `truncated` true; `--full` returns all of it. Attachments are listed by name, type and size only.
- `agent-mail flag REF --set FLAG` or `--clear FLAG`: FLAG is `seen` (read) or `flagged` (starred). Other flags fail with `unsupported_flag`.
- `agent-mail move REF --to MAILBOX`: moves one message to another mailbox of the same account. The result's `new_ref` is its new reference, or `null` if the server does not report it.
- `agent-mail draft [--account LOGIN] --to ADDR [--to ADDR...] --subject TEXT --body TEXT`: a new draft. `--to` takes bare addresses such as `carol@example.com`.
- `agent-mail draft --to ADDR --reply-to REF --body TEXT`: a reply draft that threads with the message REF; the subject becomes `Re: ` plus the original's. Needs a `read` Grant on the original's mailbox too.

`--account` names the account by its login. Leave it out when your tokens reach only one account. `read`, `flag`, `move` and `draft --reply-to` take the account from the `ref`.

A `ref` looks like `agent@example.test/INBOX/1700000001/2`. Use it exactly as `search` printed it.

## Output

Every command prints one JSON object on stdout and exits with code 0. Add `--text` before the subcommand for human-readable lines instead, as in `agent-mail --text whoami`. After `search`, `--text` is the body-text criterion, so put it before `search` for text output.

Errors are one JSON object on stderr with exit code 1, such as `{"error":{"code":"stale_reference","message":"..."}}`. Act on `code`; `message` explains it. A usage error, such as a malformed `ref`, exits with code 2.

With an invalid token among valid ones, stderr first gets a warning with `"code":"invalid_token"` and the token positions; the call still runs.

## Errors and what to do

- `denied`: your Mandate does not grant this operation. Example:

  ```json
  {"error":{"code":"denied","operation":"move","account":"agent@example.test","mailbox":"Archive","message":"..."}}
  ```

  Tell the human which `operation` you need on which `account` and `mailbox`. Do not retry or look for another way in.
- `account_required`: your tokens reach several accounts. Run again with `--account` and one login from `accounts`.
- `ambiguous_mandate`: two presented Mandates cover the same account. Tell the human; they keep one token for that account.
- `stale_reference`: the mailbox was renumbered. Search again for a fresh `ref`. `message_not_found`: the message is gone.
- `proxy_refused`: the sandbox does not allow the mail host. Tell the human to add the error's `host` to the sandbox's allowed domains.
- `missing_token`, `invalid_token`: ask the human to set a valid token in `AGENT_MAIL_TOKEN` and restart the host, as above.
- `login_failed`, `connection_failed`, `store_unavailable`, `hardened_runtime_missing`: tell the human; you cannot fix these.

## Mandates

Only the human creates Mandates. A new Mandate, more Grants or another account all need the human to run `agent-mail mandate create` in their own terminal (a human-only command); it refuses to run in an agent sandbox.

Never create, change or delete a Mandate yourself, and never try to reach the stored passwords. When you lack access, say what you need and let the human decide.

## If agent-mail is missing

If the shell says `agent-mail: command not found`, or the `agent-mail` MCP server fails to start, the binary is not installed. Ask the human to install it in their own terminal with:

```sh
curl -fsSL https://github.com/ollie-dev/agent-mail/releases/latest/download/install.sh | sh
```

Do not run the installer yourself, and do not download or build agent-mail another way. Wait until the human says it is installed.
