VERDICT: PASS

## Findings
(none)

## Checked
- Read: spec.md, full `git diff main...factory/032-unsupported-providers` for src/provider.rs and src/create.rs, build.md (after the diff).
- Ran: `diff data/providers.json .scratch/provider-list/providers.json` showed identical files. `cargo test --test provider --test create` in the worktree: all pass (30 and 12). Flow read in code: the picker label gets the " – not supported" suffix; choosing an unsupported entry prints its reason once through `terminal.message` and re-asks "Mail provider:" with the same options and default 0; it never reaches the login or Credential prompt, so nothing is stored. Esc/Ctrl-C still cancels. Parse errors name the provider id and the rule broken.
