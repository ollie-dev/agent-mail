VERDICT: PASS

## Findings
- [fix-now] src/core.rs (the `denied` error built when `Presented::select` returns `None`): for an Account no presented Mandate covers, the message reads "no grant allows `list` on account nobody@x; ask the human to grant it". That sends the Agent to ask for a Grant, but the real gap is that none of its tokens covers the Account at all, and no Grant can fix that. Keep code `denied` and the fields (the spec fixes them), but change the message for this case to say no presented Mandate covers the Account, and name the next step: run `agent-mail whoami` to see the covered Accounts, or ask the human for a token for that Account. Done when: with `<tW>` only, `agent-mail list --account other@example.test` (also `search --account ...`, and `read`/`flag`/`move`/`draft --reply-to` with a reference on that Account) prints a `denied` message that mentions no covering Mandate and `whoami`, while a Grant-missing denial on a covered Account keeps its current "ask the human to grant it" wording.

## What I ran (worktree built with `cargo build`; mail flows driven through a throwaway test using the fake IMAP server and in-memory store, removed afterwards; Mandates W aaaa1111 / P bbbb2222 / Y cccc3333, token text masked)
Help screens
- `agent-mail --help`: lists `whoami`; the after-help paragraph explains comma-separated tokens, `--account`, reference commands and `whoami`. OK.
- `whoami --help` / `whoami -h`: long and short forms both describe output, no grant, no connection; `--text` present. OK.
- `list --help` / `list -h`, `search -h`, `draft -h`: each shows `--account <LOGIN>`; draft says "not allowed with --reply-to". OK.
CLI-only paths (no store needed)
- `whoami` with the variable unset: exit 1, `missing_token`. With `" , ,"`: exit 1, `missing_token`. OK.
- `AGENT_MAIL_TOKEN=garbage,x whoami`: exit 0, `{"mandates":[],"invalid_tokens":[1,2]}`. `--text whoami` with `garbage`: `token 1 of AGENT_MAIL_TOKEN is not valid and was ignored`. OK.
- `AGENT_MAIL_TOKEN=garbage list`: exit 1, `invalid_token` with a next step, no separate warning line. OK.
- `list --account ""` and `--account $'a\nb'`: exit 2, clap message says "must not be empty or hold a line break or NUL". OK.
- `draft --account a@b --reply-to x ...`: exit 2 (the `--reply-to` value error came first because my ref was malformed; the conflict itself is covered by tests/tokens.rs and the help text). `whoami --account x`: exit 2, unexpected argument. OK.
Mail flows
- `whoami` with `<W>,<P>,garbage`: exit 0, one JSON line, keys in the spec order, `invalid_tokens":[3]`, P with `"grants":[]`, an ESC in a name stays JSON-escaped (`\u001b`), no Credential or token text. OK.
- `--text whoami` and `whoami --text`: `<id>  <name>` lines, indented `account`/`grant` lines, ESC escaped as `\u{1b}`, trailing `token 3 ...` line. OK. Two Mandates on one login both listed, exit 0. OK.
- `list` with only `<W>`: succeeds without `--account`. With `garbage,<W>`: exit 0, stdout unchanged, stderr one JSON warning line with `positions":[1]` and a next step; `--text` gives `warning: invalid_token: ...`. No token text echoed. OK.
- `list`, `search`, `draft --to ... --subject ... --body ...` with `<W>,<P>` and no account: exit 1 `account_required` with `accounts` list and a message naming `--account <LOGIN>`; text form is a single `error:` line. Audit lines: denied, reason `account_required`, no mandate/account. OK.
- `list --account agent@example.test` with `<W>,<P>`: exit 0, W's mailboxes, audit `mandate` aaaa1111. `list --account other@example.test`: selects P, denied because P has no `list` Grant, audit `mandate` bbbb2222. OK.
- `list --account other@example.test` with `<W>,garbage`: exit 1, warning line first then `denied`. OK. `search --account other@example.test --mailbox INBOX` with `<W>,<P>`: `denied` with operation, account and mailbox, audit names bbbb2222. OK.
- `list --account nobody@x` and uncovered Account: `denied`, no mandate in the audit line, no connection. Works, but the message wording is the finding above.
- `list --account agent@example.test` with `<W>,<Y>` (both cover it): exit 1 `ambiguous_mandate`, `mandates` in token order, message says agent-mail never picks and gives both next steps (single token or `mandate delete <ID>`); text form is one line; `read <ref>` gives the same error. Audit: denied, reason `ambiguous_mandate`, account present, no mandate. OK.
- `garbage,,x` for `list`: `invalid_token`. `" , ,"`: `missing_token`. OK.

README: the new section matches the observed behaviour (error shapes, warning line, `whoami` output and text format, audit key rules). Draft and Audit sections updated as the spec asks.

## Notes (no action)
- `ambiguous_mandate` keys are sorted (`account, code, mandates, message`) rather than the order in the spec example; the build notes explain this follows the existing error convention. Agents parse JSON, so I accept it.
