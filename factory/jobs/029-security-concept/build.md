## Round 1
**Changed:**
- Added `docs/security/README.md`: the shared concept (sealed Mandate, Agent Token, Policy, `mandate create`, deletion, audit log, administrators out of scope), a macOS/Windows comparison table and what neither platform protects against.
- Added `docs/security/macos.md` and `docs/security/windows.md`, each with the six required `##` sections, a flowchart with `subgraph` trust boundaries and sequence diagrams for one Agent request and `mandate create`, checked against `seal.rs`, `store.rs`, `selfcheck.rs`, `core.rs`, `create.rs`, `audit.rs`, `broker.rs`, `broker/windows.rs` and `client.rs`.
- README "What each OS protects against": a new **Security concept.** paragraph linking all three documents.
- `tests/release.rs`: `job029_*` tests plus helpers `doc_section`, `mermaid_blocks`, `doc_prose` and `real_token_shapes`.
- One deliberate reading of AC4: `doc_prose` feeds the document to `prose_paragraphs` with each Markdown table row split off as its own paragraph, the way each list item already is. Without that, a whole table joins into one "paragraph" and any comparison table goes over 300 characters. Every table row and every paragraph is still held to 300 characters.

**Commits:** 9ce11ba
**Tests:** `cargo test`: every suite `test result: ok` (release: 46 passed, plugin: 26 passed). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | job029_ac1_overview_compares_both_platforms | pass |
| AC2 | job029_ac2_platform_docs_have_the_six_sections_and_terms | pass |
| AC3 | job029_ac3_platform_docs_have_component_and_sequence_diagrams | pass |
| AC4 | job029_ac4_docs_follow_the_text_convention_and_link_the_adrs (+ job029_ac4_helpers_split_tables_and_tell_placeholders_from_tokens) | pass |
| AC5 | job029_ac5_readme_links_the_security_docs (job028_ac1_* and the plugin.rs README scans still pass) | pass |

**Review findings:** none (round 1).
