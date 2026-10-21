VERDICT: PASS

## Findings
- Round 1 had no blocking items; the two nits stand (no re-ask on non-"exists" path errors; short `--from` help does not say TOML).
- Round 2 changes checked: the Credential prompt now reads `Credential (password) for <login> on <host>:<port>:` (Gmail preset likewise), and the Gmail-lookalike warning names the login, the real host:port and imap.gmail.com before the prompt. Both are clear and actionable.
- File errors are now `line N: <what is wrong>` and never echo file keys or values, still with the "nothing asked or stored" next step. This deliberately departs from AC6(b) per the human note, and is acceptable.
- A non-regular `--from` path (directory, FIFO) gets a clear "is not a regular file" message. `--help` still lists no Credential option.
- [nit] src/create.rs (Gmail warning): "check the host before typing it" could add "or abort with Ctrl-C", since the warning is not followed by a choice.
