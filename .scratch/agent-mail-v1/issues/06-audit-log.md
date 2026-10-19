# 06: Audit log

**What to build:** Every call to agent-mail leaves a metadata-only trail the human can inspect. One entry per call: time, Mandate short ID, operation, Account, mailbox, message references, outcome (allowed or denied) and reason. Never Credentials, tokens or message bodies. Stored in the platform's per-user data directory; no agent-mail command exposes it to Agents. Later operation tickets add their own entries through the same mechanism.

**Blocked by:** 02

**Status:** ready-for-agent

- [ ] A successful `list` call appends one entry with time, Mandate short ID, operation, Account and outcome `allowed`
- [ ] A denied call appends an entry with outcome `denied` and the reason
- [ ] A call with an unknown token appends an entry without any Mandate ID and without the token
- [ ] No entry ever contains a Credential, Agent Token, token hash or message body (asserted across allowed, denied and error cases)
- [ ] Entries are appended, never rewritten; earlier entries survive later calls
- [ ] No agent-facing command reads or returns the audit log
- [ ] The log location follows the platform's per-user data directory convention and is documented
