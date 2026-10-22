VERDICT: PASS

## Findings

None.

Checked through the security lens:
- Mandate selection (`src/core.rs` `Presented::select`): a Mandate is picked by exact login match. Two or more covering Mandates give `ambiguous_mandate` before any Grant check. An Account no Mandate covers gives `None`, which every caller (list, search, read, flag, move, both draft paths) turns into `denied` before connecting. Grant checks, the `list` filter, the Credential and the audit `mandate` all come from the selected `(mandate, account)` pair only, so Grants never combine across Mandates (AC4 tests cover this).
- In read, flag, move and draft, the selection result is held until after the input checks. It is still applied (`selection?`) before any Grant check or connection, and `trail.mandate` is set only when exactly one Mandate covers the Account.
- Invalid tokens: only 1-based positions are recorded. The warning, the errors, the audit line and `whoami` hold no token text, and malformed, unknown and tampered tokens look the same. Tests check that `garbage`, the tokens, the token hashes and the Credential are absent from stdout, stderr and the audit log.
- `whoami` builds its output from `Listing::of` on the opened (authenticated) records: id, name, accounts as `ListedAccount` (no Credential) and Grants. It makes no connection.
- `--account` is refused when empty or when it holds CR, LF or NUL, both by the clap parser and again in the core (`is_valid_account`). The value is only compared as a string and echoed in errors and the audit log, which are JSON or escaped text.
- A store read failure still fails the whole call with `store_unavailable`, so it fails closed.
- No new dependencies (Cargo.toml and Cargo.lock unchanged).
