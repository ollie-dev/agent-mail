# 09: Set and clear read and starred flags

**What to build:** An Agent triages by marking messages read or unread and starred or unstarred. Only `\Seen` and `\Flagged` can be changed in v1.

**Blocked by:** 07

**Status:** ready-for-agent

- [ ] Marking a message read sets `\Seen` on the fake server; marking it unread clears it
- [ ] Starring sets `\Flagged`; unstarring clears it
- [ ] Any other flag is rejected with a structured error
- [ ] Flagging in a mailbox without a `flag` Grant is denied and audited
- [ ] A stale message reference fails without changing any flag
- [ ] Each change appends an audit entry with the message reference and the flag change
