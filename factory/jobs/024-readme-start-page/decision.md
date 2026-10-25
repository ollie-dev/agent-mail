APPROVE
README-only change with a test for every criterion, the full suite green, all four round-1 reviews PASS and no fix-now findings.

## Acceptance criteria
- AC1: pass (job024_ac1_header_is_centred_with_icon_name_and_tagline)
- AC2: pass (job024_ac2_old_title_is_gone_and_first_line_is_not_a_markdown_heading)
- AC3: pass (job024_ac3_header_has_release_platform_and_license_badges_in_links)
- AC4: pass (job024_ac4_why_section_has_short_bullets_covering_the_promises)
- AC5: pass (job024_ac5_quick_start_lists_install_create_and_token_link_in_order)
- AC6: pass (job024_ac6_contents_links_every_later_heading_in_order)
- AC7: pass (job024_ac7_sections_after_contents_are_kept_in_place_and_order)
- AC8: pass (job024_ac8_prose_paragraphs_before_install_are_short)
- AC9: pass (job024_ac9_token_values_in_the_top_part_are_placeholders, plus the unchanged tests in tests/plugin.rs (24 passed) and tests/release.rs (21 passed), including ac8_no_real_credential_or_agent_token_in_plugin_or_readme; I ran `cargo test` (all suites ok, 0 failed), `cargo clippy -- -D warnings` and `cargo fmt --check`, all clean)

## For the human
Nothing needs a human decision. The diff (one commit, 937d10a) touches only README.md and tests/plugin.rs. In README.md only the top part changed: the old title line and intro were replaced, and nothing from `## Install` on was edited. No code, dependency or credential handling changed. Security confirmed that the Why claims ("never sends", Keychain on macOS only, audit log for every mail call) match the code. GitHub rendering was not checked live, since reviewers had no network. A quick look at the repo page after merge will confirm the centred header.

## Follow-ups
- code/ui/ux · README.md:11: the platform badge links to `#what-each-os-protects-against`, a target the spec left open; reviewers find it sensible → revisit if a dedicated platform or release page appears, and consider the CI badge and the Cargo.toml `license` field that this spec left out of scope.
