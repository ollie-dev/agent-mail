# 03: Claude Code destination: write the token

**What to build:** "Connect it where?" gains "A Claude Code project". The wizard asks for the project folder (default: the current folder), makes sure the token cannot end up in git, and writes it into that project's `.claude/settings.local.json`, so Claude Code sessions in that folder get it. See `../spec.md`, user stories 33–34, 37–42, 55–58, 62.

**Blocked by:** 02

**Status:** done

- [ ] The folder prompt defaults to the current folder and accepts another path; a folder that does not exist is refused with a short message and asked again
- [ ] Git-risk check before anything is written: risk when the folder is in a git repository and `git check-ignore` does not report `.claude/settings.local.json` as ignored, or the file is tracked, or there is a `.git` but `git` cannot be run. Any risk prints a warning naming the reason and falls back to "configure it myself" for this destination; nothing is written there
- [ ] The wizard never creates or changes a `.gitignore` or any other repository file (test asserts the repository's files are untouched)
- [ ] A folder outside any git repository is written normally
- [ ] The token goes in `env.AGENT_MAIL_TOKEN`; the file and `.claude` folder are created when missing; every other key, and their order, is kept
- [ ] When `AGENT_MAIL_TOKEN` already holds tokens: "Add" appends with a comma (no duplicate when the token is already listed), "Replace" swaps them; existing tokens are never shown
- [ ] On "Replace" the wizard offers to delete the old Mandate(s), found by the old token's hash, warning that other Agent Apps holding that token stop working; the default is No
- [ ] A file that is not valid JSON, or cannot be written, falls back to "configure it myself"; nothing is written
- [ ] The final check reads the token back from the file and opens the Mandate with it
- [ ] Next step: "Restart Claude Code in <folder>, then ask it to list your mailboxes."
- [ ] `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass

## Comments
