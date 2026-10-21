VERDICT: PASS

## Findings
- [nit] src/lib.rs (search --text): the spec's global `--text` cannot coexist with the `--text S` criterion, and the builder's workaround (`agent-mail --text search ...`) is documented in `search --help`, the top-level help and the README. An agent that appends a bare `--text` to `search` gets clap's "a value is required for '--text <S>'" with no hint about the human-readable flag. Consider a custom error or help line saying "for text output put --text before `search`".
- [nit] README.md: the spec deviation on ref encoding (RFC 3986 pchar, so `[`/`]` encoded, `!$&'()*+,;=:@` kept) is documented clearly; the search --help summary still says "bytes outside printable ASCII" only, which understates it. Point it at the README or list `[`/`]`.

Walked: search help, usage errors (--limit 0, bad date: exit 2, clear messages naming the valid range/format), missing token error, denial/audit/ref format via README and the builder's tests. Flow matches the spec; defaults (INBOX, 20) are guessable and shown in help.
