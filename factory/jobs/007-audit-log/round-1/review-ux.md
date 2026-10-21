VERDICT: PASS

## Findings
- [nit] src/core.rs (AuditUnavailable message): the agent is told to "ask the human to check that the agent-mail data directory is writable", which is the right next step, but the human only learns the directory from the README. This is deliberate (AC6 forbids exposing the path to agents), so no change is needed.
- [nit] src/core.rs (execute, Command::List): when the audit append fails, `audit_unavailable` replaces any earlier error such as `denied` or `login_failed`. This is documented in build.md and is fail-closed by design. Consider one sentence in the README's fail-closed line saying the original result is withheld too.
- [nit] README.md "Audit log": the section says only `list` calls are logged. It could also say that `mandate create`, `mandate list` and `mandate delete` are not yet logged, so the human does not assume they are.
