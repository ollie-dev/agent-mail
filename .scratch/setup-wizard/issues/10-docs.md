# 10: Docs for `agent-mail setup`

**What to build:** the README, the plugin skill and CLAUDE.md lead with the wizard: install with the one-liner, then `agent-mail setup`. The manual steps stay as the "configure it myself" reference. See `../spec.md`, user stories 14, 65–67.

**Blocked by:** 06, 07, 09

**Status:** ready-for-agent

- [ ] README Quick start: the one-liner, then `agent-mail setup`; the manual token, Claude Desktop and plugin steps move below as the reference for "configure it myself"
- [ ] README explains the Gatekeeper or SmartScreen warning for a browser-downloaded binary and recommends the one-liner
- [ ] README turns "give each project its own Mandate" into a recommendation and explains sharing one Mandate between Agent Apps
- [ ] The plugin skill tells the Agent to send the human to `agent-mail setup` when no token is present, and never to run it itself
- [ ] CLAUDE.md names the wizard module, the `Machine` port, the hidden elevated-child subcommand and the new tests
- [ ] The README and plugin static checks and the placeholder-only scan are updated and pass; text follows the CLAUDE.md convention
- [ ] `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass

## Comments
