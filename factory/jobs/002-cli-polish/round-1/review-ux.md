VERDICT: PASS

## Findings
- [nit] src/lib.rs:10: a bare call prints the full help to stderr (description, usage, options). This meets AC1 and AC2 and is clap's normal behaviour. It does not use the `For more information, try '--help'` wording that `agent-mail foo` gives, but the output already lists `-h, --help`, so a first-time user is not stuck.

I ran the built binary for each flow:
- Bare call: help on stderr, empty stdout, exit 2.
- `--help`: description, then usage and options, on stdout. Exit 0, stderr empty, and no mention of "mandate".
- Unknown argument: clap's error text with a pointer to `--help`, exit 2.
