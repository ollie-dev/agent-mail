VERDICT: PASS

## Findings
- [nit] README.md:6-24: long single-line paragraphs, consistent with the existing README; fine.
- [nit] src/core.rs:457: audit_unavailable message is one long sentence with a semicolon; it names the next step and leaks no path, so it is acceptable. It reads in both JSON and --text through the existing renderers.
