VERDICT: PASS

## Findings
- [nit] src/lib.rs:10: a bare call prints clap's full help (description, `Usage: agent-mail`, options) to stderr with exit 2. Stdout is empty, lines are under 80 columns, and there is no colour or symbol dependence. It meets AC1/AC2, and `--help` stdout is identical with a non-empty description and no "mandate". The `--help` pointer shows only as the `-h, --help` option row, which is acceptable.
