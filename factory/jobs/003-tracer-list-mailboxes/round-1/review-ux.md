VERDICT: CHANGES

## Findings
- [blocking] src/core.rs:153-186: most error messages say what went wrong but not what to do next, which the review bar requires for an Agent consumer. `invalid_token` ("the agent token is not valid"), `tls_required` ("...refusing to send the credential"), `tls_certificate`, `connection_failed`, `login_failed`, `server_error` and `store_unavailable` give no action. Only `missing_token` and `denied` do. → Add a next step to each, with no secrets and no new detail for `invalid_token` (it must stay identical for malformed and unknown tokens). For example: `invalid_token`: "...; check AGENT_MAIL_TOKEN or ask the human for a new token"; `tls_required`: "ask the human to fix the account's server or TLS settings"; `login_failed`: "ask the human to check the account credential"; `connection_failed`: "check the network or ask the human to check the account host and port". Keep the codes unchanged.

## Walked flows (all OK)
- `agent-mail --help` and `list --help` match the behaviour, have no "mandate", and `--text` is global and works after `list` too.
- A bare call prints usage and exits 2.
- Missing, empty and malformed token give a JSON error on stderr, empty stdout and exit 1. `--text` gives `error: <code>: <message>`.
- Results, `denied` and the TLS flows are covered by the tests, which are green per build.md.

## Nits
- [nit] src/core.rs:148: the `denied` message says "ask the human to grant it", which is good. Consider including the account as a field in the `missing_token` and `invalid_token` cases only if that does not leak anything. Currently it correctly does not, so leave it.
- [nit] `list --help` could mention that the token is read from AGENT_MAIL_TOKEN and that output is JSON by default. This is useful for first-time Agents.
