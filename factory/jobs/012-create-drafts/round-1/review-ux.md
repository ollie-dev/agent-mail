VERDICT: PASS

## Findings
- [fix-now] src/lib.rs (draft help): the help says "--subject for a new message, or --reply-to" but never says the two cannot be combined. Add "not both" to the sentence. Clap already refuses the combination with exit 2, so this only aligns the docs.
- [follow-up] src/lib.rs (draft usage error): a bad `--reply-to` or `--to` usage error is plain clap text on stderr, not the JSON error the core errors use. This is consistent with the other commands, so a later job could make usage errors machine-readable for agents.

Walked: new draft, reply, usage errors (missing subject, display-name recipient, bad reference, subject with reply-to), the missing-token error, and `--help`. The help text matches the behaviour. The core errors `drafts_not_found` and `invalid_draft` each say that no draft was created and name a next step. The AC2 ref deviation (a percent-encoded reference that `read` can parse) is reasonable.
