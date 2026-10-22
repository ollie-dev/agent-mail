VERDICT: PASS

## Findings
- [nit] src/core.rs:592: unsupported_flag message says "nothing was changed" and shows example commands; good. Consider also naming the accepted `\Seen`/`\Flagged` spellings, optional.
- [nit] Malformed-reference usage error (clap) does not say which part is wrong; acceptable and consistent with `read`.

Walked: help text, usage errors (exit 2: bad ref, neither/both of --set/--clear), README Flag and Audit sections, error wording for unsupported_flag/stale_reference/message_not_found, full test suite passes. Help, README and behaviour agree.
