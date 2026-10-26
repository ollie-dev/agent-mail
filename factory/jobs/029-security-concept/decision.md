APPROVE
Documentation-only security concept for macOS and Windows; all five criteria are tested and pass, all four reviews pass, and the scope is kept.

## Acceptance criteria
- AC1: pass (job029_ac1_overview_compares_both_platforms)
- AC2: pass (job029_ac2_platform_docs_have_the_six_sections_and_terms)
- AC3: pass (job029_ac3_platform_docs_have_component_and_sequence_diagrams)
- AC4: pass (job029_ac4_docs_follow_the_text_convention_and_link_the_adrs, job029_ac4_helpers_split_tables_and_tell_placeholders_from_tokens; matching the code and ADRs checked by the security and code reviews, and by a spot check of `handle_create`'s order: integrity check, then live login, then store)
- AC5: pass (job029_ac5_readme_links_the_security_docs; job028_ac1_* and the tests/plugin.rs README scans still pass)

Full `cargo test` run in the worktree: every suite `test result: ok` (release 46, plugin 26). `cargo fmt --check` is clean.

## For the human
Nothing needs your call. The diff touches only `docs/security/` (three new files), one README paragraph and the new `job029_*` tests in `tests/release.rs`. There are no Rust source, installer, workflow or dependency changes. One reading of AC4 to be aware of: the test splits each Markdown table row into its own paragraph before applying the 300-character limit, so tables are allowed but each row is still held to the limit. The documents describe existing behaviour and the reviewers checked them against the code. They state the known gaps openly: tokens in host configs are plain text, deletion by an Agent is accepted, the macOS audit log is writable by the user, and administrators are out of scope.

## Follow-ups
- ui · docs/security/macos.md, docs/security/windows.md (Mermaid blocks): the diagrams were never rendered (no Mermaid toolchain), so labels with backslashes such as `\\.\pipe\agent-mail` and `NT SERVICE\agent-mail` are unconfirmed → open both documents on GitHub after merge, confirm every diagram renders, and fix any label that breaks.
