VERDICT: PASS

## Findings
- [follow-up] src/lib.rs (draft --help): the long description is one unwrapped paragraph (about 980 chars) in a single line when not on a terminal. Other subcommands follow the same style, so a later job could wrap or split long help text across all commands.
