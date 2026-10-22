VERDICT: PASS

## Findings
- [fix-now] src/lib.rs:293: the `--gmail` doc comment is one ~290-character line, so in `search -h` the option line runs far past 80 columns while every other option line there is under 60 (`--help` wraps it, but the wall of text buries the other options). → Make the first doc paragraph short (`Search with Gmail's own search syntax, e.g. "from:alice has:attachment"`) and put the rest (needs X-GM-EXT-1, `unsupported` elsewhere, cannot be combined with the standard criteria, works with --mailbox and --limit) after a blank `///` line, so `-h` shows one short line and `--help` shows the detail. Done when: `agent-mail search -h` shows the `--gmail` line under 100 characters, and `agent-mail search --help` still names X-GM-EXT-1, `unsupported`, the excluded criteria and that --mailbox/--limit work.

Seen and fine: `search --help`/`-h` text, the `unsupported` error message (names the next step, does not echo the query), README Search and Development sections (readable, commands in code blocks).
