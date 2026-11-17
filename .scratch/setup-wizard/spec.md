# Setup wizard: `agent-mail setup`

Status: ready-for-agent

## Problem Statement

Getting from "I heard of agent-mail" to "my Agent reads my mail" takes seven steps in four places today: run the one-liner installer (and fix PATH), create an app password in the browser, run `agent-mail mandate create`, copy the Agent Token that is shown once, hand-edit `.claude/settings.local.json` or `claude_desktop_config.json`, run `/plugin marketplace add` and `/plugin install`, and restart Claude Code. Hand-editing the Agent App's config is where most people give up: the JSON shape, the `env` key, the comma-separated token list and the binary path are easy to get wrong, and a wrong edit fails silently until the Agent tries to use agent-mail. Nothing checks the chain end to end, and on Windows the human must also know to open an elevated terminal for `mandate create`.

## Solution

One command, `agent-mail setup`, walks the human through everything in the terminal:

1. It installs the running binary if it is not installed yet (ADR-0005).
2. It creates a Mandate with the existing guided flow, or connects an existing one.
3. It connects that Mandate to one or more destinations: a Claude Code project, Claude Desktop, or "I'll configure it myself". For a Claude Code project it writes the token into the project's `.claude/settings.local.json` and installs the Claude Code plugin for that project, pinned to the binary's release. For Claude Desktop it adds an `agent-mail` MCP server entry.
4. It checks the result as the Agent App will see it and says exactly what to do next ("Restart Claude Code in <folder>, then ask it to list your mailboxes.").

The token is written straight into the Agent App's config and is printed only when the human configures it themselves or when automatic configuration is unsafe or fails, so it is never lost. A bare `agent-mail` in a real terminal with no Mandate yet starts the wizard. The install scripts end by handing off to `agent-mail setup`.

## User Stories

### Starting the wizard

1. As a first-time user, I want `install.sh` to hand off to `agent-mail setup` once the binary is verified and installed, so that installing and setting up are one flow.
2. As a first-time Windows user, I want `install.ps1` to hand off to `agent-mail setup` in a non-elevated process once the install finishes, so that the wizard writes my own files, not an administrator's.
3. As a first-time user, I want a bare `agent-mail` to start the wizard when I have no Mandate yet, so that I don't need to know the subcommand.
4. As a returning user, I want a bare `agent-mail` to print help as today once a Mandate exists, so that the wizard doesn't get in my way.
5. As a user piping or scripting agent-mail, I want a bare `agent-mail` to print help when stdin or stdout is not a terminal, so that scripts never hang on a prompt.
6. As a human whose Agent runs `agent-mail`, I want the wizard to refuse to start inside an Agent session (the same environment indicators as `mandate create`), so that an Agent can never drive the setup.
7. As a user, I want `agent-mail setup` to be in `--help` with a one-line description, so that I can find it again later.
8. As a user, I want the wizard to be terminal-only, so that it works the same over SSH and on both platforms.

### Installing the binary

9. As a macOS user who downloaded the binary, I want `setup` to copy it to `~/.local/bin` when it is not running from there, so that it is installed where the installer would put it.
10. As a macOS user whose `~/.local/bin` is not on PATH, I want the wizard to print the line to add to my shell profile, as `install.sh` does, so that `agent-mail` works in new terminals.
11. As a Windows user who downloaded the binary, I want `setup` to install it to `%ProgramFiles%\agent-mail`, register and start the Broker and add it to the machine Path with one UAC prompt, so that I don't need an elevated terminal or a separate script.
12. As a Windows user, I want to be told to open a new terminal after the Path change, so that `agent-mail` is found.
13. As a user running the installed binary, I want `setup` to skip the install step silently, so that re-running it is quick.
14. As a user who downloaded the binary in a browser, I want the README to explain the Gatekeeper or SmartScreen warning I will see, and recommend the one-liner instead, so that I'm not surprised.

### Upgrading

15. As a user with an older version installed, I want running a newer `setup` (or the installer again) to replace the installed binary, so that upgrading is the same command as installing.
16. As a Windows user upgrading, I want the Broker stopped, the binary replaced and the Broker started again within the one UAC prompt, so that the upgrade can't leave a stopped Broker.
17. As a macOS user upgrading, I want to be told beforehand that the first Keychain access will ask once and that I should allow it, so that I trust that prompt and still distrust any other.
18. As a user upgrading, I want every Claude Code marketplace registration the wizard made re-pinned to the new release tag, so that the skill matches the binary.
19. As a user upgrading, I want my existing Mandates and destinations left as they are, so that nothing needs reconnecting.

### Creating or choosing the Mandate

20. As a user, I want `setup` to run the same guided Mandate creation as `mandate create` (name, Provider picker, Provider note before the Credential prompt, live login check, mailboxes and operations, export offer), so that I learn one flow.
21. As a Gmail user, I want the Provider note telling me how to create an app password before the Credential prompt, so that I have it ready.
22. As a user who already has a Mandate connected somewhere, I want "Connect an existing Mandate", so that a second Agent App can use it without a new Mandate.
23. As that user, I want to take the token from a Claude Code project folder or the Claude Desktop config that already holds it, so that I don't need to find and paste it.
24. As that user, I want to paste the token hidden as an alternative, so that I can use a token I kept elsewhere.
25. As that user, I want the wizard to check the token against the stored Mandates and show the Mandate's name, so that I know which one I'm connecting.
26. As that user, I want a clear error and nothing written when the token matches no stored Mandate, so that I don't connect a dead token.
27. As a user sharing one Mandate between Agent Apps, I want the wizard to tell me once that every Agent App holding the token gets the same Grants and that deleting the Mandate disconnects all of them, so that I share knowingly.

### Windows elevation

28. As a Windows user, I want the wizard itself to run without elevation and ask for elevation exactly once, for installing and storing the Mandate, so that I see a single UAC prompt.
29. As a Windows user, I want the Credential typed only in the elevated window, so that my other programs can't read it (ADR-0004).
30. As a Windows user who elevates with a different administrator account, I want the wizard to stop before any Credential is asked and say why, so that I don't create a Mandate owned by another user that my Agents can't open.
31. As a Windows user, I want the Agent Token handed from the elevated window back to the wizard over a channel only my own user can open, so that the wizard can write my config files without me copying the token.

### Choosing destinations

32. As a user, I want to choose where to connect the Mandate: a Claude Code project, Claude Desktop, or "I'll configure it myself", so that I set up the Agent App I use.
33. As a Claude Code user, I want the project folder to default to the current folder, so that running `setup` inside my project needs no typing.
34. As a Claude Code user, I want to type another folder, so that I can set up a project I'm not in.
35. As a user, I want "Connect it somewhere else too?" after each destination, so that one run can connect several Agent Apps or projects.
36. As a user who chooses "I'll configure it myself", I want the token printed once with the exact snippet to paste for Claude Code and for Claude Desktop, so that I can do it by hand without the README.

### Claude Code destination

37. As a Claude Code user, I want the token written to `env.AGENT_MAIL_TOKEN` in the project's `.claude/settings.local.json`, with every other key kept, so that only this project's sessions get it.
38. As a Claude Code user, I want the file and the `.claude` folder created when missing, so that a fresh project works.
39. As a Claude Code user, I want the wizard to stop automatic configuration for a project, print a warning and fall back to "configure it myself" when the token file could end up in git, so that my token is never committed.
40. As a Claude Code user, I want "could end up in git" to mean: the folder is in a git repository and `git check-ignore` does not report `.claude/settings.local.json` as ignored, or the file is already tracked, or there is a `.git` but git can't be run, so that the check errs on the safe side.
41. As a Claude Code user, I want the wizard never to edit any `.gitignore` or other repository file, so that it doesn't change repositories I share with others.
42. As a Claude Code user in a folder outside any git repository, I want the file written normally, so that non-git projects work.
43. As a Claude Code user, I want the agent-mail marketplace registered once for my user, pinned to the binary's release tag, so that the plugin matches the binary.
44. As a Claude Code user, I want the plugin installed for this project only (local scope), so that it appears only where the token is.
45. As a Claude Code user whose `claude` command is not on PATH, I want the wizard to look in the usual install places, so that the plugin still gets installed.
46. As a Claude Code user without any `claude` found, I want the token still written and the two plugin commands printed for later, so that I can finish by hand.
47. As a Claude Code user, I want the sandbox's allowed domains left unchanged, so that the wizard touches only what the MCP server needs.

### Claude Desktop destination

48. As a Claude Desktop user, I want one `agent-mail` MCP server entry with `command` set to the binary's absolute path, `args` `["mcp"]` and the token in its `env`, so that Claude Desktop starts it without relying on PATH.
49. As a Claude Desktop user, I want a `.bak` copy of `claude_desktop_config.json` made before any change, so that I can undo it.
50. As a Claude Desktop user, I want every other key and server in the file kept, so that my other MCP servers keep working.
51. As a Claude Desktop user whose config file is not valid JSON, I want nothing changed and the fallback to "configure it myself", so that the wizard never breaks my config.
52. As a Claude Desktop user without a config file yet, I want it created, so that a fresh install works.
53. As a Windows Claude Desktop user, I want the wizard to find the config in `%APPDATA%\Claude\` and, where the Microsoft Store version keeps it elsewhere, there too, so that the right file is changed.
54. As a Claude Desktop user, I want to be told to quit and restart Claude Desktop, so that it picks up the new token.

### Existing tokens at a destination

55. As a user whose destination already holds `AGENT_MAIL_TOKEN`, I want to choose between adding the new token (comma-separated) and replacing the old one, so that I decide whether the Agent keeps its old access.
56. As a user adding a token that is already in the list, I want it not added twice, so that the list stays clean.
57. As a user replacing a token, I want to be offered to delete the old Mandate, found by the old token's hash, with a warning that other Agent Apps holding that token stop working and with No as the default, so that I don't leave unused access around by accident or break another Agent App by accident.
58. As a user, I want the existing tokens never shown on screen, so that the wizard doesn't leak them.

### Final check

59. As a user, I want the wizard to read back the token it wrote at each destination and run the `whoami` check with it, so that I know the written config opens the Mandate.
60. As a Claude Code user, I want the wizard to confirm the plugin with `claude plugin list`, and report "plugin: not installed" as a warning, not a failure, so that a missing CLI doesn't fail the whole setup.
61. As a user, I want one closing line per destination saying what to do next ("Restart Claude Code in <folder>, then ask it to list your mailboxes."), so that I know how to try it.

### Failures

62. As a user, I want any failure after the Mandate is stored (read-only file, invalid JSON, a `claude` error, Ctrl-C) to fall back to printing the token once with the snippet to paste, so that a stored Mandate is never left with a token nobody has seen.
63. As a user who cancels at that point, I want to be offered to delete the Mandate, so that I don't leave an orphan behind.
64. As a user, I want every error message short, in at most a few lines, and never containing a Credential or token unless it is the deliberate one-time token display, so that the output stays safe (project conventions).

### Documentation

65. As a new user, I want the README's Quick start to be the one-liner followed by `agent-mail setup`, with the manual steps kept below as the "configure it myself" reference, so that the easy path comes first.
66. As a user, I want the README's advice "give each project its own Mandate" turned into a recommendation that explains sharing, so that it matches what the wizard allows.
67. As an Agent, I want the plugin skill to tell me to send the human to `agent-mail setup` when no token is present, so that I point them to the right command and never try to run it myself.

## Implementation Decisions

- **Vocabulary.** "Agent App" (now in the glossary) is the application an Agent runs in. A **destination** is one place the wizard writes a token: one Claude Code project or Claude Desktop. The command name `setup` names the onboarding, not a Mandate (the glossary keeps "setup" as an _Avoid_ word for Mandate).
- **New subcommand `setup`** in the CLI definition, and a bare `agent-mail` starts it when stdin and stdout are terminals, no Agent-session indicator from the `mandate create` list is set, and the store lists no Mandate. Otherwise a bare `agent-mail` prints help as today.
- **New module for the wizard flow** (install step, Mandate step, destination loop, final check). It reuses the `mandate create` flow for the Mandate step, including the Provider notes and the export offer; `mandate create` stays the low-level primitive and does not change behaviour.
- **New `Machine` port on `Context`**: the wizard's view of the operating system, injected like the Terminal and the store. It provides the home, application-data and current folders, the running binary's path and the install location, the folders searched for `claude`, running `claude` and `git` with arguments and capturing exit status and output, and on Windows launching the elevated child and installing or upgrading with the Broker. The real implementation is the platform; a test implementation (behind `cfg(any(test, feature = "test-support"))`) is rooted in a temporary folder, answers `claude` and `git` from a script and records every command. `Context::new` uses the platform `Machine`.
- **Self-install (ADR-0005).** macOS: copy the running binary to `~/.local/bin` (same location and PATH hint as `install.sh`), keeping the hardened-runtime signature; skip when already running from there; replace when the installed version is older. Windows: the elevated child does what `install.ps1` does (install folder, Broker registration with delayed automatic start, machine Path), stopping and restarting the Broker on upgrade. `install.sh` and `install.ps1` end by running the installed `agent-mail setup` when attached to a terminal; `install.ps1` runs it from its non-elevated parent.
- **Windows elevated child.** A hidden subcommand runs elevated. **Corrected from the grilling's Q14:** the parent cannot pass only the token hash, because a Mandate is sealed under a key derived from its token (ADR-0003) and the Broker generates the token (ADR-0004). So:
  - The parent passes its user SID, its process id and the name of a named pipe it created, with a random name and an access list granting only its own user SID.
  - The child first checks that its own user SID equals the parent's, and stops with nothing stored otherwise (an administrator account elevating for a standard user would own the Mandate, which the user's Agents could not open).
  - The child installs or upgrades, runs the Mandate step against the Broker, connects to the pipe, checks that the pipe's server process is the parent's id, sends the Agent Token and the Mandate id, and exits.
  - The parent writes every destination file without elevation.
  - The threat model of ADR-0004 does not change: the token ends up in a plain-text config file of the same user anyway, and the Credential is still typed only in the elevated window.
- **Connecting an existing Mandate.** The wizard reads candidate tokens from a chosen project's `.claude/settings.local.json` or from the Claude Desktop config, or takes a hidden paste, opens each against the store (as `whoami` does) and lists the Mandates they open by name. Tokens are never echoed.
- **Claude Code destination.**
  - The git-risk check runs before anything is written. Any risk stops automatic configuration for that destination and falls back to "configure it myself".
  - The token goes in `env.AGENT_MAIL_TOKEN` of `.claude/settings.local.json`, merged into the existing JSON with unknown keys and their order kept.
  - The marketplace is added at user scope as `ollie-dev/agent-mail#v<binary version>`, and on an upgrade it is re-pinned by adding it again, never by removing it (removing uninstalls its plugins everywhere).
  - The plugin is installed with `claude plugin install agent-mail@agent-mail --scope local`, run from the project folder.
  - `claude` is looked up on PATH, then in the usual install places: on macOS `~/.local/bin/claude`, `~/.claude/local/claude`, `/opt/homebrew/bin/claude`, `/usr/local/bin/claude` and npm's global bin; on Windows `%USERPROFILE%\.local\bin\claude.exe` and `%APPDATA%\npm\claude.cmd`. The first that runs `claude --version` is used.
- **Claude Desktop destination.**
  - Config location: `~/Library/Application Support/Claude/claude_desktop_config.json` on macOS, `%APPDATA%\Claude\claude_desktop_config.json` on Windows, plus the Microsoft Store package's redirected location if it is present (to be confirmed on Windows during the build).
  - A `.bak` copy is written first, every other key is kept, and the one server entry named `agent-mail` is set with `command` (the absolute install path), `args` `["mcp"]` and `env.AGENT_MAIL_TOKEN`.
  - If the file is not valid JSON, nothing is written and the wizard falls back to "configure it myself".
- **Existing tokens.** Add or replace, as in the user stories. On replace, the old Mandate is found by the old token's hash, and deleting it is offered with No as the default.
- **Final check.** For each written destination, the wizard reads the token back from the file, opens the Mandates as the MCP server would, shows `whoami --text` lines, runs `claude plugin list` for Claude Code destinations, and prints the next step.
- **The token is printed only** on "configure it myself", after the git-risk stop, and after a failure once the Mandate is stored. Each time it is printed once, with the snippet.
- **Nothing new in the Mandate or its file format** (ADR-0003). The wizard keeps no state of its own: destinations are discovered from the files, and upgrades re-pin the user-scope marketplace.
- **ADR-0005** records the self-installing binary. The README, the plugin skill and `CLAUDE.md`'s module and test layout are updated to match.

## Testing Decisions

- Good tests here check what the human sees and what ends up on disk or in the store: the terminal transcript, the destination files after the run, the commands sent to `claude` and `git`, and the stored Mandates. They don't check how the wizard is put together internally.
- **Main seam: `agent_mail::run` in-process** with a `ScriptedTerminal`, the in-memory store, the fake IMAP server and the test `Machine`, the same way the `mandate create` integration tests drive it today. A new integration test file for `setup` covers:
  - when a bare `agent-mail` starts the wizard (terminal, Agent indicators, existing Mandate) and the Agent-session refusal;
  - the install step being skipped or done, and upgrading;
  - the Mandate step, including Provider notes, and connecting an existing Mandate from a file and from a paste;
  - the Claude Code destination: the merge, the git-risk stop in each case, no `.gitignore` written, the marketplace pin and re-pin commands, the local-scope install, the `claude` lookup order and its fallback;
  - the Claude Desktop destination: `.bak`, unknown keys kept, the absolute `command`, invalid JSON, a missing file;
  - add versus replace, no duplicates, the delete offer and its default;
  - the final check output;
  - every failure path printing the token exactly once, and the token never appearing anywhere else in the transcript or output.
- **Windows client paths** run through the in-memory fake Broker that the Broker client tests already use, on every OS: the hidden elevated-child subcommand with `Context::with_elevation`, the SID-mismatch stop, the token handed back, and the parent writing destinations. The Broker client tests are the prior art.
- **Opt-in, ignored-by-default tests**, like the existing elevated installer tests:
  - on macOS, the platform `Machine` copying the binary to `~/.local/bin` in a temporary home;
  - on Windows, run elevated: the real elevated child, the pipe access list and server-process check, install, upgrade and the Broker restart;
  - with a real `claude` and two published tags: re-pinning the marketplace keeps the installed local-scope plugin. If that fails, switch to the fallback below.
- **Static checks**, added to the existing release and plugin tests: the installers hand off to `setup`, the README's Quick start, the skill's pointer to `setup`, and `CLAUDE.md` updated.
- Every test uses placeholder Credentials and tokens only (project convention). The plugin and README placeholder scan stays green.

## Out of Scope

- Codex, Copilot and other Agent Apps. Pinning carries over to them (Copilot `owner/repo#ref`, Codex `--ref`), but neither documents passing the token per project through `${AGENT_MAIL_TOKEN:-}`, and neither has per-project installs. A follow-up spec covers them.
- A non-interactive mode, and `setup --from <file>`: `mandate create --from` stays the scripted route.
- A GUI.
- Paid code signing (Developer ID with notarization, Authenticode) and the change to what the Keychain binds a Mandate to.
- Editing `.gitignore` or any other repository file.
- Changing the Claude Code sandbox's allowed domains.
- Changing `mandate create`, the Mandate model or the Mandate file format.

## Further Notes

- **Q5b fallback:** if re-adding the marketplace with a new tag doesn't keep the installed plugins, the release workflow fast-forwards a `release` branch on each `v*` tag. The wizard then pins to `#release`, and upgrades run `claude plugin marketplace update agent-mail` instead of re-pinning. Commit SHAs can't be pinned: Claude Code only accepts branch and tag names.
- Facts checked during the grilling: Claude Code adds `**/.claude/settings.local.json` to the user's global git ignore file when it creates the file, so the git-risk check usually passes without any change. The `owner/repo#ref` form was tested in an isolated Claude config. `claude plugin install --scope local` writes `enabledPlugins` to the project's `.claude/settings.local.json`.
- Remaining fact to confirm during the build: whether the Microsoft Store version of Claude Desktop redirects `%APPDATA%\Claude\claude_desktop_config.json`.
