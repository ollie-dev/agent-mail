VERDICT: PASS

## Findings
- [follow-up] src/terminal.rs:285: the picker help line is "type to search, enter to choose" and the list stays at inquire's default page size with 31 options. Once ticket 02 adds the unsupported entries, check how the list reads in a real 80-column terminal, and whether a "no match" line is clear.

## Checked
- Read: spec.md, the diff of src/create.rs and src/terminal.rs, src/provider.rs (label, search_option, credential prompt) and data/providers.json (every note and credential kind).
- Ran: a script over data/providers.json listing each note length (all at most 197 characters, under the 300 limit) and each credential kind. This gives the prompt openers "Password" and "App password". Labels follow the `Name (d1, d2, d3, …)` format in the code.
- Did not drive the real inquire prompt on a TTY. The scripted-terminal path and the scorer code were read only.
- Prompt texts match the spec ("Mail provider:", "Email address at <Name>:", note as a message before the hidden prompt, "Custom server…" first and default).
- The old "Account type:" wording is gone from create.rs.
