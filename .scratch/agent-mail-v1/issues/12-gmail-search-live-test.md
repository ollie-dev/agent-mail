# 12: Gmail search syntax and opt-in live Gmail test

**What to build:** When the server advertises Gmail's extensions (`X-GM-EXT-1`), `search --gmail "<query>"` passes the query through as Gmail's raw search, so Agents can use Gmail syntax like `has:attachment newer_than:7d`. On other servers the option fails clearly. An opt-in live test confirms standard and Gmail search, `\Drafts` detection and peek-reading against a dedicated real Gmail account.

**Blocked by:** 07

**Status:** ready-for-agent

- [ ] Against a fake server advertising `X-GM-EXT-1`, `search --gmail "from:alice has:attachment"` sends the query as the server's raw Gmail search and returns its results
- [ ] Against a fake server without `X-GM-EXT-1`, `--gmail` fails with a structured `unsupported` error
- [ ] `--gmail` respects the same Grants, limit and audit behaviour as standard search
- [ ] A live test, ignored by default and run only explicitly, reads the Gmail test account's address and app password from the developer's environment, and checks LIST, standard search, Gmail search, `\Drafts` detection and that reading leaves messages unread
- [ ] The live test never writes the Credential to disk, output or the repo, and skips with a clear message when its environment is missing
