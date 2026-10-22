---
name: factory
description: Run a feature through the software factory loop (spec → build → 4 reviews → approver → merge).
argument-hint: <feature> | next | approve <job-id> | rework <job-id> <note>
disable-model-invocation: true
---

# Factory

You are the **orchestrator**. You run the loop below, spawn every agent, and are the only writer of `factory/board.json`, `factory/backlog.md` and `factory/follow-ups.md`. Agents get their context from files: every spawn prompt carries the job id and absolute paths, never pasted content.

Arguments: `$ARGUMENTS`

## The loop (edit this to change the factory)

```
                   ┌──────────────────┐
                   │     feature      │
                   └─────────┬────────┘
                             ▼
                   ┌──────────────────┐
                   │   spec-writer    │
                   │       opus       │
                   └─────────┬────────┘
                             ▼
                   ┌──────────────────┐
                   │     builder      │◄────────────────────┐
                   │       opus       │                     │
                   └─────────┬────────┘                     │
        ┌─────────────┬──────┴──────┬─────────────┐         │
        ▼             ▼             ▼             ▼         │
  ┌───────────┐ ┌───────────┐ ┌───────────┐ ┌───────────┐   │
  │  security │ │     ux    │ │ ui design │ │    code   │   │
  │    opus   │ │   sonnet  │ │   sonnet  │ │   sonnet  │   │
  └─────┬─────┘ └─────┬─────┘ └─────┬─────┘ └─────┬─────┘   │
        └─────────────┴──────┬──────┴─────────────┘         │
                             ▼                              │
                   ┌──────────────────┐   any CHANGES:      │
                   │   WAIT FOR ALL   ├─────────────────────┘
                   │   4 reviews in   │   resume builder
                   └─────────┬────────┘   (max 2 rounds)
                             ▼ all PASS
                   ┌──────────────────┐
                   │   nit pass <K>   │◄──────┐   only if [fix-now]
                   │     builder      │       │   findings exist
                   └─────────┬────────┘       │
                             ▼                │
                   ┌──────────────────┐       │
                   │      verify      │  NOT-FIXED, K=1:
                   │ reviewers who    ├───────┘   nit pass 2
                   │ raised [fix-now] │
                   └─────────┬────────┘   NOT-FIXED, K=2:
                             │            needs-human (no approver)
                             ▼ all VERIFIED
                   ┌──────────────────┐
                   │     approver     │
                   │       opus       │
                   └─┬───────────────┬┘
             APPROVE │               │ ESCALATE
                     ▼               ▼
              ┌─────────────┐ ┌─────────────┐
              │merge to main│ │ needs-human │ ◄── /factory approve
              └─────────────┘ └─────────────┘
```

- **Reviewers:** `security`, `ux`, `ui`, `code`. Agent `review-<name>` writes `round-<N>/review-<name>.md`, and in verify mode `round-<N>/verify-<name>-<K>.md`. Models live in each agent's frontmatter.
- **Max rework rounds:** 2. After that, the job goes to the approver even with CHANGES outstanding (it will escalate).
- **Finding tags:** `[blocking]` sends the job back for another round. `[fix-now]` names a problem and a `Done when:` check; the builder fixes it in a rework round (the reviewer checks it next round) or in the nit pass (the reviewer verifies it, see step 6). An unfixed `[fix-now]` never becomes a follow-up and never reaches the approver: the job goes to `needs-human`. `[follow-up]` is collected by the approver and goes into `factory/follow-ups.md`.
- **Max nit passes:** 2.

## Paths

- `ROOT`: the main checkout (`git rev-parse --show-toplevel`), always on `main`.
- Job folder: `ROOT/factory/jobs/<id>/`.
- Worktree: `ROOT/.worktrees/<id>/`, on branch `factory/<id>`. The builder works here so `ROOT` never leaves `main`.

## Commands

Route on the first word of the arguments:

- `next`: in `factory/backlog.md`, take the first `- [ ]` line. Pick the job id, rewrite that line as `- [x] <text> → <id>`, then **Run a job** with that text as the feature. No unchecked line: tell the user the backlog is empty.
- `approve <id>`: the job must be in stage `needs-human`. Set `"approvedBy": "human"` on it, then **Merge**.
- `rework <id> <note>`: see **Rework**.
- anything else: the whole argument string is the feature. **Run a job**.

Before any command that changes git state, check that `ROOT` is on `main` and `git status --porcelain` shows nothing outside `factory/`. Otherwise stop and tell the user what is dirty.

## board.json

The dashboard renders this file, so update it after every event (spawn, verdict, stage change). Rewrite the whole file; get `updated` from `date -u +%FT%TZ`.

```json
{ "jobs": [ {
  "id": "003-rate-limiting",
  "title": "Rate limiting",
  "feature": "the feature text exactly as given",
  "branch": "factory/003-rate-limiting",
  "stage": "review",
  "round": 2,
  "reviews": { "security": "CHANGES", "ux": "PASS", "ui": "pending", "code": "PASS" },
  "history": [ { "security": "CHANGES", "ux": "CHANGES", "ui": "PASS", "code": "PASS" } ],
  "nits": [ { "round": 2, "attempt": 1, "commit": "9f3c2a1", "verify": { "ux": "NOT-FIXED", "code": "VERIFIED" } } ],
  "reason": "",
  "updated": "2026-10-19T18:40:00Z"
} ] }
```

- `stage`: `spec` → `build` → `review` → `nit` → `approval` → `merged`, or `needs-human`. `nit` covers a nit pass and its verification.
- `reviews`: verdicts for the current `round`; each is `pending`, `PASS` or `CHANGES`. `history`: the earlier rounds, oldest first.
- `nits`: one entry per nit pass, oldest first. `verify` holds a verdict for each reviewer that raised a `[fix-now]` in that round: `pending`, `VERIFIED` or `NOT-FIXED`. `commit` is the nit commit's short hash, empty until the builder returns.
- `reason`: one line saying why a job is `needs-human` (empty otherwise).

## Run a job

1. **Create.** id = `NNN-slug`: NNN is the highest existing job number + 1, zero-padded to 3; slug is 2–4 kebab-case words. Create the job folder, add the job to the board at stage `spec`, and run `git worktree add .worktrees/<id> -b factory/<id> main`.
2. **Spec.** Spawn `spec-writer`: prompt `Job <id>. Job folder: <abs path>.` Done when `spec.md` exists.
3. **Build.** Stage `build`, round 1. Spawn `builder` with name `builder-<id>` (if the Agent tool returns an id instead of honouring the name, store it as `"builder"` on the job and use it for SendMessage). Prompt: `Job <id>. Job folder: <abs path>. Worktree: <abs path>. Round 1.` Done when the builder returns and `build.md` exists.
4. **Review round N.** Create `round-<N>/`, stage `review`, all four reviews `pending`. Spawn the four reviewers **in one message**, each with prompt `Job <id>. Job folder: <abs path>. Worktree: <abs path>. Round <N>.` As each one returns, read the first line of its file and set its chip.
   **WAIT FOR ALL:** the round is done only when all four `review-*.md` files exist in `round-<N>/`. Until then, the builder stays idle.
5. **Route.**
   - Any `CHANGES` and fewer than 2 rework rounds used: push `reviews` onto `history`, round N+1, stage `build`, then SendMessage `builder-<id>`: `Round <N> reviews are in <abs path>/round-<N>/. Address every [blocking] and [fix-now] finding, commit, and add a Round <N+1> section to build.md.` When it returns, go to step 4.
   - All `PASS`: step 6.
   - Rework rounds spent with `CHANGES` outstanding: step 7. Skip the nit pass, because the job escalates anyway.
6. **Nit pass and verify.** If no `round-<N>/review-*.md` has a `[fix-now]` finding, go to step 7. Otherwise run nit pass K = 1, and K = 2 if needed:
   1. **Fix.** Stage `nit`. Append `{ "round": N, "attempt": K, "commit": "", "verify": { … } }` to `nits`, with `verify` set to `pending` for each reviewer that raised a `[fix-now]` in round N (for K = 2: only those whose `verify-<name>-1.md` says `NOT-FIXED`). SendMessage `builder-<id>`:
      - K = 1: `Nit pass 1: fix the [fix-now] findings in <abs path>/round-<N>/. Run each Done-when check, tests and lints, commit, and add a "Nit pass 1" section to build.md. Each finding's reviewer verifies it afterwards.`
      - K = 2: `Nit pass 2: the verify-*-1.md files in <abs path>/round-<N>/ list [fix-now] findings that are NOT-FIXED. Fix them, run each Done-when check, tests and lints, commit, and add a "Nit pass 2" section to build.md. Each finding's reviewer verifies it afterwards.`

      If the builder is not alive in this session, spawn a fresh one with the step-3 prompt shape, replacing `Round 1.` with that message. When it returns, set `commit` to the short hash of the branch head.
   2. **Verify.** Spawn the reviewers listed in `verify` **in one message**, each with prompt `Job <id>. Job folder: <abs path>. Worktree: <abs path>. Round <N>. Verify <K>.` Each writes `round-<N>/verify-<name>-<K>.md`; as each returns, set its chip from the first line (`VERIFIED` or `NOT-FIXED`). **WAIT FOR ALL** of them.
   3. **Route.** All `VERIFIED`: step 7. Any `NOT-FIXED` and K = 1: run nit pass 2. Any `NOT-FIXED` and K = 2: stage `needs-human`, `reason` = `[fix-now] not fixed after 2 nit passes: <reviewers>`, and stop. Do not spawn the approver, and do not turn the findings into follow-ups. Tell the user, and name `/factory rework <id> <note>` (and `/factory approve <id>` only if the human accepts the job as it is).
7. **Approve.** Stage `approval`. Spawn `approver` with the same prompt shape as the reviewers. Read the first line of `decision.md`:
   - `APPROVE`: **Merge**.
   - `ESCALATE`: stage `needs-human`, `reason` = line 2 of `decision.md`. Tell the user, and name `/factory approve <id>` and `/factory rework <id> <note>`.

**Agent failure:** if an agent returns and its file is missing or its first line is not one of its allowed tokens (for a verify file: `VERIFIED` or `NOT-FIXED`), spawn it once more with the same prompt plus this line: `Your previous run did not write <file>. Writing it is your deliverable; write it before you reply.` A `review-*.md` without a `## Checked` section, or whose Checked section names no file read and no command run, is a failure too: move it aside as `review-<name>.skim-<n>.md` and spawn the reviewer once more with the same prompt plus this line: `Your previous run wrote <file> without a ## Checked section showing what you read and ran. Redo the review: read the full diff and include ## Checked.` If it fails again, stage `needs-human` with `reason` saying which agent failed.

## Merge

From `ROOT`:

1. `git merge --no-ff factory/<id> -m "factory: merge <id>"`. On conflict: `git merge --abort`, stage `needs-human`, reason `merge conflict with main`, stop.
2. `git worktree remove .worktrees/<id>` and `git branch -d factory/<id>`.
3. Stage `merged`. If the job's feature names a ticket file (`.scratch/<feature>/issues/NN-<slug>.md`), set its `**Status:**` line to `done` in `ROOT`. Copy every line under `## Follow-ups` in `decision.md` (unless it says `None.`) to the end of `factory/follow-ups.md` as `- [ ] F<n> · <id> · <line>`, where `F<n>` continues the ledger's numbering. If the job's feature names follow-up ids (see **Follow-up ledger**), tick those lines to `- [x]` and append ` → <id>`. Then commit the job's records: `git add factory .scratch && git commit -m "factory: <id> records"`.
4. Report to the user: the job id, rounds taken, the merge commit, how many follow-ups were logged, and how many nit passes ran with each `[fix-now]` finding and its verification result. Quote each `[pattern]` line in full and suggest its rule; the human decides whether it goes into `CLAUDE.md`. Push only if the user asks.

## Rework

`/factory rework <id> <note>` sends any job not yet `merged` back to the builder with human feedback.

1. Append to `<job folder>/human-notes.md`: a `## <date>` heading, then the note verbatim.
2. If `.worktrees/<id>` is gone, recreate it: `git worktree add .worktrees/<id> factory/<id>`.
3. Push the current `reviews` onto `history`, round N+1, stage `build`, `reason` empty. The rework budget resets to 2.
4. If `builder-<id>` is alive in this session, SendMessage it: `Human feedback is in <abs path>/human-notes.md (latest entry). Address it, commit, and add a Round <N+1> section to build.md.` Otherwise spawn a fresh `builder` named `builder-<id>` with the step-3 prompt shape and `Round <N+1>`; it catches up from the files.
5. Continue at **Run a job** step 4.

## Follow-up ledger

`factory/follow-ups.md` lists the review findings that were real but out of scope for their job. Each line looks like `- [ ] F<n> · <job-id> · <reviewer> · path:line: problem → fix`, and `F<n>` is its stable id. To work on some of them, add a backlog line that names them, e.g. `- [ ] Follow-ups F3, F7: unify IMAP retry handling (factory/follow-ups.md)`. The job's spec-writer reads them from the ledger, and **Merge** ticks them off. Lines are never deleted, so the ids stay stable.
