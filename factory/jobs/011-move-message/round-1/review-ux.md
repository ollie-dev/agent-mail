VERDICT: PASS

## Findings
- [nit] src/lib.rs (move --help): the usage error for a missing --to lists both args but is clap-default; fine. Help, README and behaviour agree (keys, null new_ref, stale_reference, move_unsupported, both-mailbox Grants).
- [nit] mailbox_not_found for a refused target says "nothing is moved" only in README; confirm the runtime message also names the target and next step (list).
