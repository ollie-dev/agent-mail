# 08: Windows: the elevated child and the token hand-back

**What to build:** on Windows the wizard runs without elevation. It starts one elevated child that runs the Mandate step against the Broker (the Credential typed only in the elevated window), and the child hands the Agent Token back over a pipe only the same user can open. The non-elevated wizard then writes every destination. See `../spec.md`, user stories 28–31 and Implementation Decisions ("Windows elevated child", corrected from the grilling's Q14).

**Blocked by:** 02

**Status:** ready-for-agent

- [ ] The wizard creates a named pipe with a random name and an access list granting only its own user SID, and starts the child elevated through the `Machine` port, passing the pipe name, its user SID and its process id
- [ ] A hidden subcommand runs the child; it first checks that its user SID equals the parent's and otherwise stops before any prompt with a short message saying to elevate as the same user; nothing is stored
- [ ] The child runs the Mandate step through the Broker (`mandate create` path, export offer included), then connects to the pipe, checks that the pipe's server process id is the parent's, and sends the token and the Mandate id
- [ ] The parent never runs elevated, writes all destination files, and falls back to printing the token once if anything after storing fails
- [ ] A refused UAC prompt or a failed child ends the wizard with a short message and nothing stored
- [ ] Tests on every OS: the hidden subcommand in-process with `Context::with_elevation` and the fake Broker, the SID-mismatch stop, the token reaching the parent, and the parent writing destinations
- [ ] Opt-in Windows tests: the real pipe's access list refuses another user, and the server-process check refuses a pipe not served by the parent
- [ ] `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass

## Comments
