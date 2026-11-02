VERDICT: PASS

## Findings
None.

The Credential's destination on the Provider path comes only from `include_str!("../data/providers.json")`, and nothing reads the list from a file, the environment or the network at runtime. The chosen Provider fixes host, port and TLS mode, and the human cannot edit them on that path. `TlsMode` still has no plaintext variant, so `tls: "plain"` fails to parse. The Credential is still asked only at the hidden prompt. Login and host in prompts and messages still go through `escape_controls`. Provider names and notes are printed as they are, which is fine because they are built-in trusted text. The search query stays in the process. No dependency was added, and no Broker protocol changed.

## Checked
- Read: the full `git diff main...factory/031-provider-list-picker`, all 14 files, with the source files line by line:
  - `src/provider.rs`: the `include_str!` and its comment; `parse` with `deny_unknown_fields`, the slug, unique id, unique case-insensitive domain, empty host and port 0 checks, and the note length; `all`, `sorted`, `by_domain`, `label`, `search_option`.
  - `src/terminal.rs`: `SearchOption::matches`; `InquireTerminal::search` and its scorer; `NoTerminal::search`, which cancels; `ScriptedTerminal::search`, which is behind `cfg(any(test, feature = "test-support"))` like the rest of the scripted module.
  - `src/create.rs`: `add_account` (the Provider and Custom-server branches, the Gmail warning still applied to both, the hidden Credential prompt and the escaping), `ask_provider`, and how the index maps to a Provider (`checked_sub(1)`, so index 0 means "Custom server…").
  - `src/lib.rs` and `CLAUDE.md`.
  - The test diffs: `tests/create.rs`, `tests/export.rs`, `tests/provider.rs`, `tests/client.rs`, `tests/proxy.rs`, `tests/credential_manager.rs`, `tests/release.rs` and `tests/support/mod.rs`. The new fixtures use the fake Credential and `someone@gmail.com` against the in-process fake server, with no real secrets.
  - `src/mandate.rs` `TlsMode` (lowercase serde, Implicit and Starttls only).
  - `build.md`, after the diff. Its claims match the code.
- Ran:
  - A Python comparison of `data/providers.json` against the supported entries of `.scratch/provider-list/providers.json`: identical. All 30 entries use implicit TLS on 993.
  - A grep of `src/provider.rs` for `std::fs`, `env::`, `read_to_string`, `File::` and `var(`: no matches.
  - `git diff --stat` for `Cargo.toml` and `Cargo.lock`: unchanged, so no new dependencies.
  - A grep of the added test lines for password, secret, token and credential: only prompt strings and the fake `CREDENTIAL`.
  - `cargo test`: every suite `test result: ok`, 0 failed.
