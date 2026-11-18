VERDICT: PASS

## Findings
- [follow-up] src/setup.rs: the wizard messages ("... so the token was not written there.") repeat the same tail in five variants; a later job could share one helper. Not needed now.

## Checked
- Read: spec.md, `git diff main...factory/036-claude-code-token --stat`, and the src/setup.rs diff parts that render text: the destination labels and constants, the git-risk warnings, the settings-problem messages, the folder-refusal message, the old-Mandate delete confirm and result messages, the final-check failure message and the next-step line.
- Ran: nothing (no interactive surface run). Judged from the texts: each is one line under about 200 characters, names the file or folder and never file content or tokens, and uses the same style as the existing wizard messages. The default is No on the delete confirm. The token is revealed only on the fallback paths, through the existing SHOWN_ABOVE logic, so it reads the same with colour off.
