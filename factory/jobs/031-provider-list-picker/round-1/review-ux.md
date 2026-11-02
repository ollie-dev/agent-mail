VERDICT: PASS

## Findings
- [follow-up] src/terminal.rs:289: the picker's help message does not say that "Custom server…" is the first/default entry or that it drops out when a search matches nothing else (accepted in the spec). Ticket 02 or 03 could mention the Custom server escape in the help line.

## Checked
- Read: spec.md, the diff of src/create.rs, src/provider.rs (head), src/terminal.rs (SearchOption, trait, InquireTerminal::search), CLAUDE.md diff stat.
- Ran in the worktree: `cargo test --test provider --test create --test export --lib` (all pass, 89/27/21/7), `cargo clippy -- -D warnings` (clean), `cargo fmt --check` (clean).
- Flow check by code: "Mail provider:" with Custom server… first and default; Provider path asks "Email address at <Name>:", shows the note, then "Password"/"App password" hidden prompt; custom path keeps the old prompts; --from skips the picker. Credential comes only from include_str! data with the explaining comment. I did not drive the real inquire UI on a tty.
