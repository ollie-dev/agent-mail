# 13: Several Mandates per Agent and `whoami`

**What to build:** An Agent can hold several Mandates. `AGENT_MAIL_TOKEN` accepts several tokens, comma-separated. Each command targets one Account (`--account`, optional when only one Account is reachable). agent-mail uses the Mandate covering that Account; if two presented Mandates cover it, that is an error. Grants never combine across Mandates. `whoami` shows the Policies of all presented tokens.

**Blocked by:** 02

**Status:** ready-for-agent

- [ ] With two tokens for Mandates covering different Accounts, `list --account work` uses the Mandate covering `work` and `list --account private` the other
- [ ] With one reachable Account, `--account` can be omitted; with several, omitting it returns a structured error listing the choices
- [ ] When two presented Mandates cover the same Account, any command on it returns a structured `ambiguous_mandate` error
- [ ] A Grant from one Mandate never allows an operation on an Account covered only by another Mandate
- [ ] An invalid token in the list is reported without blocking the valid ones
- [ ] `whoami` returns, per presented Mandate, its short ID, name, Accounts and Grants, and never Credentials
