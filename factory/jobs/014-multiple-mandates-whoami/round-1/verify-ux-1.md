VERIFIED

- FIXED · uncovered-Account denial pointed to a Grant · ran `cargo test --test tokens ac4_uncovered`: passes; it covers list, search, draft --account, read, flag, move and draft --reply-to with only <tW>, asserting code `denied`, "no presented Mandate covers account ..." plus `agent-mail whoami`, no "grant it", and that Grant-missing denials on a covered Account keep "ask the human to grant it". Grepped src/core.rs: every empty-selection site (list, search, read, flag, move, draft both paths) uses `uncovered()`.
