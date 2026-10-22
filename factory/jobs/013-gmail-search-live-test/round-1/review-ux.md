VERDICT: PASS

## Findings
- [fix-now] README.md:165 (audit `reason` list): the list of error codes a log line can carry omits the new `unsupported`, though the AC2 failure writes `reason` `unsupported` → add `unsupported` to that list. Done when: `grep -n 'reason' README.md` shows the audit `reason` bullet containing `unsupported`.

Walked: `search --help`/`-h` list `--gmail` with the conflicts and `unsupported`; empty value and combination with `--from` exit 2 with clear clap messages; `unsupported` error names the next step and does not repeat the query; README Search section and Development section match behaviour; skip message for the live test is a clear single line.
