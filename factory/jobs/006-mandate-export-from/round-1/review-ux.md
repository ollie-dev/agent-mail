VERDICT: PASS

## Findings
- [nit] src/create.rs (export path prompt): a path error other than "exists" ends the offer with no retry; acceptable since the Mandate is stored and the message names path and reason, but a re-ask would be friendlier for a typo in a directory name.
- [nit] src/lib.rs (`--from` help): short help says "saved by an earlier `mandate create`"; fine, but could mention that the file is TOML and holds no secrets (long help does).

Walked: `mandate create --help` documents `--from <FILE>` and still lists no Credential option. `--from` on a non-TTY returns the refusal first (as spec'd). Error message for `invalid_mandate_file` states the problem by key path or line, that nothing was asked or stored, and the next step. Prompts (keep Account default yes, pre-filled host/port/TLS/login, unavailable-mailbox message, export offer default no, default path `agent-mail-<id>.toml`, already-exists re-ask, write-error message with path and reason) are clear and match the spec. Tests cover AC1-AC7 and pass per build.md.
