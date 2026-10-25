## Round 1
**Changed:**
- `README.md`: replaced the old title line and intro paragraph with a centred HTML header (`assets/icon.png` at width 128, `<h1 align="center">Agent Mail</h1>`, the tagline, and linked badges for latest release, platform macOS and license Apache 2.0). After it come `## Why Agent Mail` (six bullets), `## Quick start` (installer, `agent-mail mandate create`, a link to the Agent Tokens section, two code blocks, no token or JSON) and `## Contents` (links to all 17 later `##` sections). Nothing from `## Install` on changed.
- The platform badge links to `#what-each-os-protects-against`, which explains why only macOS has Keychain protection.
- `tests/plugin.rs`: added one `job024_acN_*` test per AC, plus the helpers `readme_header`, `readme_h2s`, `readme_top`, `github_anchor` and `assert_in_order`. They reuse `readme_section`, `paragraph_with`, `token_values` and `is_placeholder`. No existing test was edited.

**Commits:** 937d10a
**Tests:** `cargo test`: every suite `test result: ok.`, 421 passed, 0 failed, 6 ignored. `cargo clippy -- -D warnings` (also with `--all-targets`) is clean, and `cargo fmt --check` is clean.

| AC | Test | Status |
|----|------|--------|
| AC1 | job024_ac1_header_is_centred_with_icon_name_and_tagline | pass |
| AC2 | job024_ac2_old_title_is_gone_and_first_line_is_not_a_markdown_heading | pass |
| AC3 | job024_ac3_header_has_release_platform_and_license_badges_in_links | pass |
| AC4 | job024_ac4_why_section_has_short_bullets_covering_the_promises | pass |
| AC5 | job024_ac5_quick_start_lists_install_create_and_token_link_in_order | pass |
| AC6 | job024_ac6_contents_links_every_later_heading_in_order | pass |
| AC7 | job024_ac7_sections_after_contents_are_kept_in_place_and_order | pass |
| AC8 | job024_ac8_prose_paragraphs_before_install_are_short | pass |
| AC9 | job024_ac9_token_values_in_the_top_part_are_placeholders, plus the existing tests in tests/plugin.rs and tests/release.rs, unchanged | pass |

**Review findings:** none, this is the first round.
