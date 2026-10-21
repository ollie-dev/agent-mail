VERDICT: PASS

## Findings
- [nit] src/core.rs:422-454 (`mandate_delete`): a real record whose value cannot be read is still possible. `entries()` turns an access-denied read into an empty value, for example after a rebuild changes the code signature that the item ACL trusts. That record has no listing, so it is never matched. If a readable decoy carries the same ID, delete removes the decoy, prints success and exits 0, and the real record and its token survive. This is narrower than the round-1 issue, because the attacker can't make the real record unreadable without a prompt. Consider having delete report (or refuse) when the store holds unreadable records, so the human knows revocation may be incomplete.
- [nit] src/core.rs:439: the reported `name` comes from the first record deleted, which may be the decoy's forged listing. It is escaped on output, so this is cosmetic only. Preferring the name from a record that `open`s, or omitting the name when the matching listings disagree, would be more accurate.

## Round-1 blocking item
- Decoy record defeats `mandate delete`: **resolved.** Delete now removes every record whose listing has the ID. It reports success only if at least one was removed, and it returns `store_unavailable` on any store error, so no false success is printed. `tests/mandates.rs::delete_removes_every_record_with_the_id_so_a_decoy_cannot_block_revocation` covers a decoy filed both before and after A in enumeration order. It asserts that A's token then gets `invalid_token` and that B still works.

## Round-1 nits
- Fallback for a non-base64url account: resolved. `keys()` skips such items. `entries()` counts them as unreadable with an empty value, so they never produce a listing and are never targeted by delete.
- Escaping Cf and bidi characters: resolved. `escape_controls` now escapes the Unicode Cf set as well, and a test covers U+202E and U+200B in `--text mandate list`.

Checked again and fine: there are no new dependencies. Changed files contain no credentials, tokens or store keys in output or fixtures. The about-text change is cosmetic. `cargo test` is green.
