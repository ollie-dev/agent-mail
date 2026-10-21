VERDICT: PASS

## Findings
- [nit] src/core.rs:335: `invalid_reference` is an extra error code only reachable from non-CLI adapters; fine, but document it in the README error list when MCP lands.
- [nit] README Read section does not mention that `denied` is returned for an Account the Mandate does not cover (only "denied before any connection"); a short clause would help.

Walked: read --help (names REF, --full, 20,000), malformed ref (exit 2 with usage and next step), missing token error, JSON/--text layout, stale_reference and message_not_found messages (next step present), README matches behaviour.
