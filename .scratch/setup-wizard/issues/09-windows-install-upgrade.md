# 09: Windows: self-install, upgrade, `install.ps1` hands off to `setup`

**What to build:** on Windows, the elevated child of ticket 08 also installs or upgrades agent-mail as `install.ps1` does, so a downloaded binary needs only `agent-mail setup` and one UAC prompt. `install.ps1` ends by running `setup` without elevation. See `../spec.md`, user stories 2, 11–13, 15–16, 18–19, and ADR-0005.

**Blocked by:** 08, 04

**Status:** ready-for-agent

- [ ] Run from outside `%ProgramFiles%\agent-mail`, the elevated child copies the binary there, registers the Broker with delayed automatic start, starts it and adds the folder to the machine Path, matching `install.ps1`
- [ ] Run from the install location with the same or a newer version, the install step is skipped; the elevated child is then needed only for the Mandate step
- [ ] Upgrading an older version stops the Broker, replaces the binary and starts the Broker again, all in the same UAC prompt; a failure restarts the old Broker and says so
- [ ] After the install, the wizard tells the human to open a new terminal for the Path change
- [ ] On upgrade, the marketplace is re-pinned to the new tag as in ticket 07
- [ ] `install.ps1` runs `agent-mail setup` from its non-elevated parent at the end when interactive; the static checks of `install.ps1` are updated
- [ ] Opt-in, elevated Windows tests: fresh install, upgrade with Broker restart, and skip when current
- [ ] `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass

## Comments
