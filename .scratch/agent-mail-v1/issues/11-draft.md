# 11: Create drafts, including replies

**What to build:** An Agent prepares mail for the human to review and send. A new draft is appended to the Account's Drafts mailbox, found by the `\Drafts` special-use flag rather than by name (Gmail localises it, e.g. `[Gmail]/Entwürfe`), with the `\Draft` flag. In reply mode (`--reply-to <reference>`) the draft gets In-Reply-To and References from the original and a reply subject, so it threads correctly. The `draft` Grant needs no mailbox list. Agents cannot edit or delete drafts.

**Blocked by:** 08

**Status:** ready-for-agent

- [ ] Creating a draft with to, subject and body appends one message with the `\Draft` flag to the mailbox marked `\Drafts` on the fake server
- [ ] With the Drafts mailbox named `[Gmail]/Entwürfe` and marked `\Drafts`, the draft lands there
- [ ] When no mailbox is marked `\Drafts`, the command fails with a structured error and creates nothing
- [ ] A reply draft carries In-Reply-To equal to the original's Message-ID, References extended with it, and the subject prefixed once with `Re:`
- [ ] Creating a reply draft requires `read` on the original's mailbox in addition to `draft`; without it the call is denied
- [ ] Without a `draft` Grant on the Account, the call is denied and audited
- [ ] No command exists to edit or delete an existing draft
- [ ] Each draft appends an audit entry with recipients and the new message's reference, and no body content
