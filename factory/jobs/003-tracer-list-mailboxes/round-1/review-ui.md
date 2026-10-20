VERDICT: PASS

## Findings
- [nit] src/lib.rs:113: under --text an error is one line "error: <code>: <message>" with no account or operation; the Denied/TlsRequired details are only in the message for denied. Acceptable per spec; consider surfacing account on tls errors.
- [nit] src/lib.rs:96: --text with zero granted mailboxes prints only the account line; a "(none)" marker would make an empty result unambiguous.
- Verified by running the binary: bare call exits 2 with usage; --help and list --help are 80-column clean, contain no "mandate"; JSON errors are single-line, no colour or symbols, so NO_COLOR and piped output are identical; --text output is plain two-level indent.
