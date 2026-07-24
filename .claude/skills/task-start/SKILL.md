---
name: task-start
description: Begin work on a Falcon backlog issue — set it In Progress, create a worktree, and scaffold a PR body with Closes #N. Use when the user says they want to start working on a task/issue.
---

# /task-start — start work on an issue

First read `.claude/skills/falcon-tracker-reference.md` for the IDs and gh recipes.

## Input
The issue the user wants to start (number or fuzzy reference — resolve to a number).
Read the issue and its linked `docs/plan_*.md` spec so you understand the work before touching anything.

## Do
1. Set the Project **Status = In Progress** for the item (recipe in the reference).
2. Assign it to the user: `gh issue edit <NUM> --repo rwalker123/falcon --add-assignee @me`.
3. **Create a worktree for the work.** Propose a branch/worktree name in the repo's
   style (e.g. `predators-live-consumer`, `hud-decompose-phase2`, `options-pan-speed`),
   then create it with `EnterWorktree` (branches fresh off `origin/main`). Run every
   build/edit/commit from the worktree root — confirm with `git rev-parse --show-toplevel`
   before builds or commits (see CLAUDE.md → Working from a Git Worktree).
4. Draft a PR body the user can reuse, including `Closes #<NUM>` so the merge auto-closes
   the issue and moves it to Done.

## Git ownership — READ THIS
- **The agent owns commit, push, and opening the PR.** Once the work is implemented and
  self-verified, commit it to the worktree branch, push, and open the PR with `gh`.
- **The human owns the MERGE.** Never merge the PR — that is the human's call through their
  own review flow.
- Stage only the specific files you changed, by explicit path (never `git add -A`/`.`/`<dir>`).

## Report
Issue set In Progress + assigned; the worktree/branch name created; the draft PR body (and
the PR URL once opened). A short summary of what the work entails from the spec is helpful.

## Notes
- Merging PRs is the human's job — commit, push, and open, but never merge.
- If the issue is an arc parent, the user usually starts a specific sub-issue instead — confirm which.
