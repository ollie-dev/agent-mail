# 07: macOS: self-install, upgrade, `install.sh` hands off to `setup`

**What to build:** on macOS, `agent-mail setup` run from anywhere but the install location installs itself to `~/.local/bin`, and a newer binary upgrades an older one, re-pinning the plugin marketplace. `install.sh` ends by running `agent-mail setup`. See `../spec.md`, user stories 1, 9–10, 13, 15, 17–19, and ADR-0005.

**Blocked by:** 02, 04

**Status:** ready-for-agent

- [ ] Run from outside `~/.local/bin`, `setup` copies the binary there (keeping its hardened-runtime signature) and prints the PATH line when `~/.local/bin` is not on PATH, as `install.sh` does
- [ ] Run from the install location, the install step is skipped silently
- [ ] An older installed version is replaced; the same or a newer installed version is left alone and the wizard says so
- [ ] Before replacing, the wizard warns once: the first Keychain access will ask once, allow it; a prompt at any other time is a red flag
- [ ] On upgrade, the marketplace is re-pinned to the new tag by adding it again (never removed); existing Mandates and destination files are untouched
- [ ] `install.sh` runs the installed `agent-mail setup` at the end when attached to a terminal, and skips it otherwise; the existing `install.sh` tests are updated
- [ ] Opt-in, ignored-by-default tests: the platform `Machine` copying the binary into a temporary home's `~/.local/bin`; with a real `claude` and two published tags, re-pinning keeps the installed local-scope plugin. If it does not, implement the spec's `release` branch fallback instead and record that here
- [ ] `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass

## Comments
