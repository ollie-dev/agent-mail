# Agent Mail

## Stack

No code yet. The first factory job chooses the language and test runner and replaces this section with: language, test command, lint command, source and test layout.

## Conventions

- Credentials never appear in logs, CLI output, agent-visible responses, test fixtures, or commits.

## Software factory

Features are built by `/factory` (`.claude/skills/factory/SKILL.md`). State and job records live in `factory/`; each job is built on branch `factory/<job-id>` in the worktree `.worktrees/<job-id>/`. Dashboard: `cd factory && python3 -m http.server`, then open http://localhost:8000/dashboard.html.
