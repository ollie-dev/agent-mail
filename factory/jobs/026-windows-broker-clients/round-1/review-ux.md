VERDICT: PASS

## Findings
- [follow-up] src/broker/windows.rs (`served_by`): the identity check relies on the pipe owner being the service SID (job 025's SDDL sets no `O:`). If the real service does not own its pipe that way, every client would get `broker_unavailable` against a healthy Broker. build.md says this is unconfirmed; run the opt-in elevated service test (`cargo test --test broker -- --ignored`) with a real client before release, and add a client call to it.
- [follow-up] README "Sandbox and proxies": the sandbox-to-pipe result is "not yet verified" (honest, allowed by AC6). A later job should run the check and record the result.

## Checked
- Read: spec.md, build.md, the README diff, the diff stat, and the help and error text in the diff.
- Ran (Windows, debug build from the worktree):
  - `agent-mail --help`: lists `audit`.
  - `agent-mail audit --help` and `audit -h`: both name "your own Mandates", JSON and `--text`. No sentence repeats the top-level help.
  - `agent-mail mandate create --help`: includes "On Windows it needs an elevated (Run as administrator) terminal".
  - `agent-mail list`, `--text list`, `audit` and `audit --text` with no service: JSON `broker_unavailable` on stderr (exit 1) and `error: broker_unavailable: ...` with `--text`. The message names `sc.exe start agent-mail` and `install.ps1` and is under 300 characters.
  - `mandate create` and `mandate create --from nofile` without a terminal: the existing `interactive_terminal_required` error, unchanged.
  - `audit extra`: `invalid_arguments` pointing to `audit --help`, exit 2.
  - `cargo test --test client --test audit --test cli --test plugin`: all pass (client 20, audit 17, cli 30, plugin 25).
- Not run: the real service (not installed), the sandbox check, and the macOS paths.
