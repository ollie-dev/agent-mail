# Setup wizard: Claude Code destination writes the Agent Token

## Goal
"Connect it where?" in `agent-mail setup` gains "A Claude Code project". The human names the project folder (the default is the current folder). The wizard checks that the token cannot end up in git, then writes the new Agent Token into that project's `.claude/settings.local.json` under `env.AGENT_MAIL_TOKEN`, so Claude Code sessions in that folder present it. The token is never printed on this path. It is printed only when the wizard falls back to "I'll configure it myself" because of a git risk, a file it cannot use, or a cancel. (Ticket `.scratch/setup-wizard/issues/03-claude-code-token.md`, feature spec `.scratch/setup-wizard/spec.md`, user stories 33–34, 37–42, 55–58, 62.)

## Context
- Stack: Rust, one binary crate `agent-mail` (ADR-0001). Test: `cargo test`. Lint: `cargo clippy -- -D warnings` and `cargo fmt --check`.
- `src/setup.rs` (the main change):
  - add `Destination::ClaudeCode`, labelled `A Claude Code project` and offered first in "Connect it where?", before `I'll configure it myself`;
  - add the folder prompt, the git-risk check, the JSON merge and write, add/replace, the old-Mandate delete offer and the per-destination final check;
  - export new prompt and message texts as `pub const`, as the module does today (`CLAUDE_CODE_PROJECT`, `PROJECT_FOLDER`, `EXISTING_TOKENS`, `ADD_TOKEN`, `REPLACE_TOKENS`, …), so tests use the constants.
  - The fallback reuses `Wizard::reveal` / `SHOWN_ABOVE` and `NEXT_CONFIGURE_MYSELF`.
- `src/machine.rs`: the wizard uses `Machine::current_dir()` for the default folder and `Machine::run(Path::new("git"), args, project_folder)` for git. No new `Machine` method is needed. `ScriptedMachine` already answers `git` from its script and records each call.
- `src/core.rs`: `execute(.., Command::Whoami, ..)` for the read-back check. `Command::MandateDelete { id }` deletes the old Mandate the same way `mandate delete` does, audit entry included.
- `src/seal.rs`: `AgentToken::parse`, `AgentToken::store_key` and `seal::open` find the Mandate an old token opens. The old token's store key is derived from the token, as the token hash is, so the old token is never shown or stored anywhere.
- `Cargo.toml`: keeping key order on rewrite needs `serde_json`'s `preserve_order` feature or an equivalent. If the feature is turned on, check that no existing test depends on sorted map output.
- `tests/setup.rs`: add the new tests here, using the existing helpers (`script`, `run_on`, `assert_token_once`, `store_with_one`, `support::store_mandate`). `run_on` must accept scripted `git` replies and let a test pre-create files under the machine's project folder (`ScriptedMachine::root()/project`). The existing AC5 test, which asserts "Connect it where?" has exactly one option, is updated to the two options in the order given above.
- `CLAUDE.md`: update the `src/setup.rs` and `tests/setup.rs` descriptions to name the Claude Code destination (`tests/release.rs` checks this file).
- Glossary terms: Agent App, Agent Token, Mandate, destination (one place the wizard writes a token).

## Acceptance criteria
These are carried over from the ticket in the same order. Concrete inputs and outputs are added. In every test the Mandate step is the existing `script(...)` Gmail flow against the fake IMAP server, with an empty `InMemoryStore` and a `ScriptedMachine`, unless the criterion says otherwise. "Project" means `<machine root>/project`, the machine's current folder. "The file" means `<folder>/.claude/settings.local.json`.

1. AC1: the folder prompt defaults to the current folder and accepts another path. A folder that does not exist is refused with a short message and asked again.
   - (a) Answers: pick `A Claude Code project`, then accept the default at the text prompt `PROJECT_FOLDER` (for example "Claude Code project folder:"), then "Connect it somewhere else too?" No. The prompt's offered default equals the machine's current folder, and the file is written in the project.
   - (b) Typing the absolute path of another existing folder `<root>/other` writes `<root>/other/.claude/settings.local.json`, and nothing is created under the project. A relative path such as `other-rel` is resolved against the current folder.
   - (c) Typing `<root>/missing` first shows one short message naming the path (for example "No such folder: <path>"), then asks the folder prompt again. Typing an existing folder then writes there. A path that names a file, not a folder, is refused the same way. `<root>/missing` is not created.
   - (d) Cancelling at the folder prompt follows the existing cancel path: the token is revealed once and the delete offer follows. Nothing is written.
2. AC2: the git-risk check runs before anything is written. There is a risk when:
   - the folder is in a git repository and `git check-ignore` does not report `.claude/settings.local.json` as ignored, or
   - the file is tracked, or
   - there is a `.git` but `git` cannot be run.

   Any risk prints a warning naming the reason and falls back to "configure it myself" for this destination. Nothing is written there.
   - The folder is in a git repository when it, or any of its ancestors, holds an entry named `.git` (a folder or a file, for worktrees and submodules).
   - The commands run in the project folder, in this order:
     - `git check-ignore -q -- .claude/settings.local.json`. Exit 0 means ignored. Exit 1 means not ignored, which is a risk. Any other exit, or `NotStarted`, means git cannot be run, which is a risk.
     - Only when the file is ignored: `git ls-files -- .claude/settings.local.json`. Exit 0 with empty stdout means untracked. Exit 0 with non-empty stdout means tracked, which is a risk. A non-zero exit or `NotStarted` means git cannot be run, which is a risk.
   - Tests create `<project>/.git/` and script the replies. Each test asserts the exact `RecordedCommand`s: program stem `git`, args, and dir equal to the project folder.
   - (a) check-ignore exits 1: one warning names "not ignored by git", then the token is revealed once with both snippets (or `SHOWN_ABOVE` if it was already revealed). No `.claude` folder exists afterwards. Only the check-ignore command was run.
   - (b) check-ignore exits 0 and ls-files prints `.claude/settings.local.json\n`: the warning names "tracked by git", then the same fallback.
   - (c) the first reply is `Reply::not_started()`: the warning says git could not be run to check the folder, then the same fallback. The same happens when check-ignore exits 128.
   - (d) In each case the final check shows `NEXT_CONFIGURE_MYSELF` for this destination, not the Claude Code next step. `assert_token_once` holds.
   - (e) check-ignore exits 0 and ls-files exits 0 with empty stdout: the file is written.
3. AC3: the wizard never creates or changes a `.gitignore` or any other repository file. The test asserts the repository's files are untouched.
   - Set up `<project>/.git/` with a placeholder `config` file and `<project>/.gitignore` containing `target/\n`. Snapshot every path and byte content under the project before the run.
   - (a) In the risk case (AC2a), the tree after the run equals the snapshot exactly.
   - (b) In the written case (AC2e), the tree after the run equals the snapshot plus only `.claude/` and `.claude/settings.local.json`.
   - (c) No `.gitignore` exists anywhere under the machine root except the one the test wrote, and its content is unchanged.
4. AC4: a folder outside any git repository is written normally. With no `.git` under the project or its ancestors, the file is written and `machine.commands()` is empty: no `git` command is run.
5. AC5: the token goes in `env.AGENT_MAIL_TOKEN`. The file and the `.claude` folder are created when missing. Every other key, and their order, is kept.
   - (a) With no `.claude` folder, the run creates it and the file. The file parses as `{"env":{"AGENT_MAIL_TOKEN":"<new token>"}}`, and that token opens the new Mandate in the store.
   - (b) The existing file is `{"permissions":{"allow":["Bash(ls)"]},"env":{"FOO":"bar"},"zeta":1,"alpha":true}`. After the run, the top-level keys are `permissions, env, zeta, alpha` in that order, with values unchanged. `env` is `{"FOO":"bar","AGENT_MAIL_TOKEN":"<new token>"}` in that order.
   - (c) An existing file without `env` gets `env` appended as the last top-level key.
   - (d) On the written path, no transcript entry, stdout or stderr contains the new token, and there is no `Reveal` entry.
6. AC6: when `AGENT_MAIL_TOKEN` already holds tokens, "Add" appends with a comma (no duplicate when the token is already listed) and "Replace" swaps them. Existing tokens are never shown.
   - The existing file is `{"env":{"AGENT_MAIL_TOKEN":"<old>"}}`, where `<old>` is a placeholder token stored with `support::store_mandate`. A choice prompt `EXISTING_TOKENS` (for example "This project already has an Agent Token.") offers `ADD_TOKEN` (for example "Add the new token") and `REPLACE_TOKENS` (for example "Replace the old token"). The default is Add.
   - (a) Add: the value becomes `<old>,<new>`.
   - (b) Replace: the value becomes `<new>`.
   - (c) The same project is connected twice in one run: Claude Code, then "somewhere else too?" Yes, then Claude Code with the same folder, then Add. The value holds `<new>` once. An empty existing value counts as no tokens, and no prompt is shown.
   - (d) In every case, `<old>` appears in no transcript entry (prompt, options, answer, message or reveal), nor in stdout or stderr.
   - (e) A cancel at `EXISTING_TOKENS` follows the cancel path, and the file is byte-for-byte unchanged.
7. AC7: on "Replace" the wizard offers to delete the old Mandate(s), found by the old token's hash. It warns that other Agent Apps holding that token stop working. The default is No.
   - After the file is written with Replace, the wizard asks one confirm for each stored Mandate that an old token opens (found by `store_key` and opened with `seal::open`). The confirm names that Mandate's name and ID and says other Agent Apps holding its token stop working. Its offered default is `false`.
   - (a) Answer No (or accept the default): the old Mandate stays in the store.
   - (b) Answer Yes: the old Mandate is gone from the store, and a message says it was deleted with its ID. The new Mandate stays.
   - (c) An old value `<oldA>,not-a-token,<oldB>` with two stored Mandates gives two offers and no offer for the malformed entry.
   - (d) An old token that opens no stored Mandate gives no offer.
   - (e) Add never offers a delete. A write that fails (AC8) never offers a delete.
8. AC8: a file that is not valid JSON, or cannot be written, falls back to "configure it myself". Nothing is written.
   - (a) The file holds `{"env": ` (truncated): a short message says the file is not valid JSON, then the fallback reveal. The file is byte-for-byte unchanged.
   - (b) The same applies to valid JSON that cannot hold the token: a top-level array `[]`, an `env` that is not an object (`{"env":"x"}`), or an `AGENT_MAIL_TOKEN` that is not a string.
   - (c) `<project>/.claude/settings.local.json` is a folder, so the write fails on every OS: a short message says the file could not be written, then the fallback reveal. The folder is unchanged.
   - (d) In each case the next step for this destination is `NEXT_CONFIGURE_MYSELF`, `assert_token_once` holds, and no message contains file content.
9. AC9: the final check reads the token back from the file and opens the Mandate with it.
   - For each written Claude Code destination, the wizard reads `env.AGENT_MAIL_TOKEN` from the file as written. It runs `whoami` with that value as the MCP server would, and shows the `whoami --text` lines. The check passes when the new Mandate's ID is among the opened Mandates.
   - (a) After Add with an old stored token, the lines list both Mandates.
   - (b) When the value read back does not open the new Mandate, the existing failure path runs: one "Final check failed" message, the token revealed once, exit 1. Nothing in a normal run can change the file between the write and the check. So test this through a narrow test-only hook behind `cfg(any(test, feature = "test-support"))` that runs the check on a given file. For example, the file holds only a placeholder token that opens nothing.
10. AC10: the next step is "Restart Claude Code in <folder>, then ask it to list your mailboxes." For a written destination, the last transcript messages include exactly that line, with `<folder>` being the resolved absolute folder as shown by `Path::display`, not canonicalized. There is one line per written destination, in the order chosen.
11. AC11: `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass, including every existing test in `tests/setup.rs` and `tests/create.rs`, the AC5 option-list test updated as noted in Context.

## Out of scope
- The Claude Code plugin: the `claude` lookup, the marketplace pin, `plugin install --scope local` and `claude plugin list` in the final check (ticket 04). This job runs no `claude` command.
- Claude Desktop as a destination (ticket 05), connecting an existing Mandate (ticket 06), the install and upgrade steps (07, 09), the Windows elevated child (08), and docs (10). On the Broker route `setup` still refuses as today.
- Editing any `.gitignore` or other repository file, and changing the sandbox's allowed domains.
- Expanding `~` in the folder prompt.

## Notes for reviewers
- Security:
  - This job writes an Agent Token into a plain-text file in a user-chosen folder. The git-risk check is the guard. It errs on the side of risk: a `.git` anywhere up the tree combined with any git failure means nothing is written. It relies on `git check-ignore` with the user's own git config (the global excludes file that Claude Code fills).
  - `git` is run without a shell, with fixed arguments and `--` before the path. The folder string is never passed as an argument; it is only the working folder.
  - Existing tokens in the file are untrusted input. They are parsed only to compute store keys and are never echoed, logged or put in error text. Malformed entries are skipped silently.
  - Delete offers need an explicit Yes.
  - Suggestion, not an AC: create a new file with mode 0600 on Unix, and write via a temporary file in `.claude/` plus a rename, so a crash cannot leave half a file.
  - `preserve_order` changes `serde_json::Map` crate-wide; check that no output that agents see changes order unexpectedly.
- UX: the operator is a human in their own terminal. The flow is: Mandate step, then "Connect it where?", then `A Claude Code project`, then the folder (Enter for the current one), then the git check (silent when it is safe), then, if tokens exist, Add or Replace (and on Replace the delete offers, default No), then "Connect it somewhere else too?", then the final check, then "Restart Claude Code in <folder>, then ask it to list your mailboxes." Every fallback leaves the human with the token and the snippets, so a stored Mandate always has a token someone has seen. Message wording in AC examples is a suggestion. Keep each message to one short line.
- UI: terminal output only, through the `Terminal` port: one new choice option, a text prompt with a default, a choice (Add/Replace), confirms, short messages, and the existing reveal on fallback. No pages.
