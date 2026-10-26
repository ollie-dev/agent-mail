## 2026-10-26

The elevated installer tests fail with tls_certificate (AC2, AC6). The fake IMAP server's test CA and leaf both use rcgen's default DN "CN=rcgen self signed cert", so Windows treats the leaf as self-signed and never chains to the CA in LocalMachine\Root. In tls_config in tests/support/mod.rs, give the CA its own CN ("agent-mail test CA") and the leaf CN=localhost.

## 2026-10-26

fix the psmodulepath risk
