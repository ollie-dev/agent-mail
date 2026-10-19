# 14: Work inside the Claude Code sandbox via its proxy

**What to build:** With the Claude Code sandbox on, there is no direct network; traffic must go through the sandbox proxy, and the system certificate verifier is unavailable. When `HTTPS_PROXY` or `ALL_PROXY` is set, agent-mail connects through it (HTTP CONNECT or SOCKS5, honouring credentials in the proxy URL) and verifies TLS against built-in root certificates. Without a proxy it connects directly with system verification. Tested with a minimal in-process proxy.

**Blocked by:** 02

**Status:** ready-for-agent

- [ ] With `HTTPS_PROXY` pointing at an in-process CONNECT proxy, `list` reaches the fake IMAP server through the proxy (the proxy observes the tunnel)
- [ ] With `ALL_PROXY` pointing at an in-process SOCKS5 proxy, the same works
- [ ] Proxy credentials in the URL are sent to the proxy and never appear in output or the audit log
- [ ] A proxy refusal (e.g. 403 for a host not on the allowlist) produces a structured error naming the host
- [ ] Without proxy variables, connections are direct
- [ ] In proxy mode, TLS verification uses the built-in roots, and a certificate not chained to a trusted root is rejected
