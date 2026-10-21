VERDICT: PASS

## Findings
- [nit] src/lib.rs (mandate subcommand about): "Create and manage Mandates (run by the human, not by agents)" now sits above `list` and `delete`, which the spec says agents may also run, and `list` JSON is meant for agents. Reword, e.g. "run by the human; agents may list and delete".
- [nit] AC1 shows `{"mandates":[...]}` but output is always `{"mandates":[...],"unreadable":N}`. The builder documented the reading (AC7 needs the key), and it is predictable for agents. Fine.
- [nit] `mandate list --help` / `delete --help` do not mention the `not_found` error or the JSON shape. Optional.

Walked: `mandate --help`, `list --help`, `delete --help`, missing-ID usage error (clear, exit 2). List, delete, not_found and unreadable flows are covered by passing tests in tests/mandates.rs. The not_found message names the next step (`mandate list`). I did not run against the real Keychain.
