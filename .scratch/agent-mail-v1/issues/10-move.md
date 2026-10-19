# 10: Move a message between granted mailboxes

**What to build:** An Agent files or archives a message. Moving needs a `move` Grant on both the source and the target mailbox. Use IMAP MOVE where the server advertises it; otherwise copy, mark deleted and expunge only that UID (UIDPLUS).

**Blocked by:** 07

**Status:** ready-for-agent

- [ ] With `move` granted on INBOX and Archive, moving a message leaves it in Archive and removes it from INBOX on the fake server
- [ ] With `move` granted only on the source, the move is denied, nothing changes, and the denial is audited
- [ ] With `move` granted only on the target, the move is denied and nothing changes
- [ ] On a server without MOVE but with UIDPLUS, the fallback moves the message and leaves every other message in the source untouched
- [ ] On a server with neither MOVE nor UIDPLUS, the move fails with a structured error rather than expunging other messages
- [ ] A stale message reference fails without moving anything
- [ ] The result returns the message's new reference in the target mailbox where the server reports it
