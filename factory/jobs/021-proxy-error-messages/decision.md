APPROVE
All four round-1 reviews pass, every criterion has a passing test I ran, and the diff stays in scope: it names only the proxy variable, never its value.

## Acceptance criteria
- AC1: pass (tests/proxy.rs `ac1_allowlist_refusals_stay_proxy_refused`, existing `ac4_proxy_refusal_names_the_host`)
- AC2: pass (tests/proxy.rs `ac2_connect_407_is_proxy_auth_failed`)
- AC3: pass (tests/proxy.rs `ac3_wrong_connect_credentials_are_proxy_auth_failed_and_never_shown`)
- AC4: pass (tests/proxy.rs `ac4_socks5_no_acceptable_method_is_proxy_auth_failed`)
- AC5: pass (tests/proxy.rs `ac5_socks5_rejected_credentials_are_proxy_auth_failed_and_never_shown`)
- AC6: pass (tests/proxy.rs `ac6_other_connect_4xx_answers_are_connection_failed`; src/proxy.rs `connect_statuses_map_to_tunnel_errors`)
- AC7: pass (tests/proxy.rs `ac7_text_output_of_proxy_auth_failed`)
- AC8: pass (tests/mcp.rs `ac8_proxy_auth_failed_matches_the_cli`)
- AC9: pass (tests/proxy.rs `ac9_mandate_create_through_a_proxy_rejecting_credentials`)
- AC10: pass (tests/tokens.rs `ac10_whoami_reports_a_usable_proxy_without_connecting`)
- AC11: pass (tests/tokens.rs `ac11_whoami_reports_an_invalid_proxy_variable`)
- AC12: pass (tests/tokens.rs `ac12_whoami_without_a_proxy_has_no_proxy_key`, existing exact-output whoami tests unchanged)
- AC13: pass (tests/mcp.rs `ac13_whoami_tool_reports_the_proxy_like_the_cli`)
- AC14: pass (tests/plugin.rs `job021_ac14_readme_and_skill_document_proxy_auth_failed_and_whoami_proxy`, tests/cli.rs error samples 300-character check)
- AC15: pass (I ran `cargo test` in the worktree: every suite ok, 0 failed. `cargo clippy -- -D warnings` and `cargo fmt --check` are clean)

## For the human
Nothing needs your decision. The new `proxy_auth_failed` error, the `whoami` `proxy` field and its `--text` line carry only the variable name (a `&'static str` from the fixed list), never the URL, host, port or credentials. The proxy's reply is reduced to a status code. `whoami` stays connection-free. The `whoami` JSON change only adds a key, and it is left out when no proxy is set, so nothing breaks. No new dependencies. This closes F8 and F10.

## Follow-ups
- ux · src/create.rs:479: the `mandate create` line for `proxy_refused` still says "allow that host in the proxy, or unset the proxy variable", which differs from the Agent-facing `proxy_refused` wording ("allowed domains") → align the human-facing `mandate create` proxy failure lines with the Agent error wording.
