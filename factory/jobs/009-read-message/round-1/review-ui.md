VERDICT: PASS

## Findings
- [nit] src/lib.rs:352 (render_message): an attachment with no name (`name` is `""`) renders as `   (application/pdf, 12 bytes)` with a dangling leading gap. Show a placeholder such as `(unnamed)` for an empty name.
- [nit] src/lib.rs:352 (render_message): a missing `From` or `Date` prints a bare `From: ` or `Date: ` line. This is acceptable and stays consistent with the JSON `""`, so it needs no change.
- Checked: `--text` output is a plain header block (From, To, Cc only when present, Date, Subject, Ref), a blank line, the body, then an `Attachments:` list. It has no colour or symbols, so it reads the same piped or with NO_COLOR. Lines are short apart from the body. Controls are escaped via `escape_controls`. The truncation marker sits at the end of the body, where the Agent sees it. The `--help` text names REF, `--full`, the 20,000 limit and `stale_reference`, and matches the style of the existing subcommands.
