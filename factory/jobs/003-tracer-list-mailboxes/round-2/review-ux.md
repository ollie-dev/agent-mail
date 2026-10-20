VERDICT: PASS

## Findings
- Previous blocking item (error messages lacking next steps) is resolved: every code in src/core.rs now ends with an action, codes unchanged, invalid_token stays one fixed string. Verified by running the binary (missing_token, invalid_token in JSON and --text).
- list --help now mentions AGENT_MAIL_TOKEN and JSON default, and has no "mandate".
- [nit] --text errors omit account details; acceptable.
