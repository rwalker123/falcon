---
name: task-start
description: Begin work on a Falcon backlog issue — set it In Progress, and (with the user's explicit approval) cut a branch and scaffold a PR body with Closes #N. Use when the user says they want to start working on a task/issue.
---

# /task-start — start work on an issue

First read `.claude/skills/falcon-tracker-reference.md` for the IDs and gh recipes.

## Input
The issue the user wants to start (number or fuzzy reference — resolve to a number).
Read the issue and its linked `docs/plan_*.md` spec so you understand the work before touching anything.

## Do
1. Set the Project **Status = In Progress** for the item (recipe in the reference).
2. Assign it to the user: `gh issue edit <NUM> --repo rwalker123/falcon --add-assignee @me`.
3. **Branch — GATED.** This repo's git topology is human-owned. Per CLAUDE.md and the
   user's standing rule, **never create a branch without an explicit, current "yes."**
   - Propose a branch name in the repo's style (e.g. `predators-live-consumer`,
     `hud-decompose-phase2`), and ASK whether to create it, stack it, or work on the
     branch already checked out. Do not create it until they say yes.
   - Only after approval: `git switch -c <name>` from the agreed base.
4. Draft (do not open) a PR body the user can reuse, including `Closes #<NUM>` so the
   merge auto-closes the issue and moves it to Done.

## Report
Issue set In Progress + assigned; the proposed branch name (and whether it was created);
the draft PR body. A short summary of what the work entails from the spec is helpful.

## Notes
- Opening/merging PRs is the human's job — scaffold, don't submit.
- If the issue is an arc parent, the user usually starts a specific sub-issue instead — confirm which.
