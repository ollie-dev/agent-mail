# 07: Search messages

**What to build:** An Agent finds messages in a granted mailbox. Standard criteria: from, to, subject, text, since, before, unseen, flagged. At most 20 results by default, adjustable with `--limit`. Each result carries date, from, subject, flags and a message reference made of Account, mailbox, UIDVALIDITY and UID, which later operations accept.

**Blocked by:** 02, 06

**Status:** done

- [ ] Searching INBOX by `--from alice@example.com` returns only matching messages from the fake server, newest first, with date, from, subject, flags and message reference
- [ ] Each criterion (from, to, subject, text, since, before, unseen, flagged) narrows results as expected, and criteria combine with AND
- [ ] Without `--limit`, at most 20 results are returned and the result says more exist; `--limit 5` returns at most 5
- [ ] Searching a mailbox without a `search` Grant is denied with a structured error and audited
- [ ] Each search appends an audit entry with the mailbox and the number of results
- [ ] Message references include UIDVALIDITY and UID
