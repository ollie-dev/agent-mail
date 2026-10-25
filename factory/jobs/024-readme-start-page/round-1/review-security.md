VERDICT: PASS

## Findings

None. The diff only touches `README.md` (new header, Why, Quick start, Contents) and tests in `tests/plugin.rs`. No code path, dependency, or secret handling changes. The new text holds no Agent Token, Credential, or email address. The Quick start does not put a token in a file or show a JSON config; it links to the Agent Tokens section, which carries the never-commit warning. I checked the Why bullets against the code and the existing reference sections:
- "never sends" matches the code: there is no send operation, and `draft` only appends to Drafts.
- "which mailboxes, which operations" matches the Mandate model: `Grant.mailboxes` / `MailboxPattern` and `Policy::allows_mailbox` in `src/mandate.rs`.
- "Every mail call ... audit log" matches the Audit log section and the six `audit::Entry` sites in `src/core.rs` (list, search, read, flag, move, draft).
- The Keychain claim is scoped to macOS, and the platform badge links to "What each OS protects against".

## Checked
- Read: full `git diff main...factory/024-readme-start-page` (README.md top part, tests/plugin.rs job024 tests and helpers); spec.md; build.md (after the diff); README `## Install` section (same installer URL, checksum and attestation text unchanged) and `## Audit log` section; `src/mandate.rs` Grant/MailboxPattern/allows_mailbox declarations; audit entry sites in `src/core.rs`.
- Ran: `git log main..factory/024-readme-start-page`: one commit, two files changed (README.md, tests/plugin.rs), no fixtures or other files added.
- Ran: grep of the diff for `@`, `token`, `TOKEN`, `password`, `am_`: only section names, anchor links and test code; no email address and no token value.
- Ran: `cargo test --test plugin --test release`: 24 passed and 21 passed, including the README placeholder-only scan `ac8_no_real_credential_or_agent_token_in_plugin_or_readme` and the new `job024_ac9_token_values_in_the_top_part_are_placeholders`.
