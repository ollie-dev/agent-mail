VERIFIED

- FIXED · unknown-command message had two semicolons · ran `agent-mail lsit` and `agent-mail mandate delet`: each prints "unknown command (did you mean ...?); run `agent-mail --help` for usage" with one `;`; tests/cli.rs:776 uses the new wording.
