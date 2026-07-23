---
name: task-add
description: Create a properly-tagged Falcon backlog issue on GitHub and add it to the Falcon Backlog project (labels, sub-issue link to an arc parent, linked design doc, Project fields). Use when the user wants to add a task/work item/bug/feature/arc to the tracker.
---

# /task-add — add a work item to the Falcon tracker

First read `.claude/skills/falcon-tracker-reference.md` for the IDs and gh recipes.

## Gather (ask only for what's missing; infer the rest)
1. **Title** — concise, imperative.
2. **Type** — one `type:*` label. `type:arc` if it's a multi-phase effort with a design doc; otherwise `type:feature` / `type:bug` / `type:chore` / `type:design`.
3. **Subsystem** — one or more `sys:*`. Infer from what the work touches; ask if genuinely unclear.
4. **Priority** — P0 (broken/blocking) / P1 / P2. Default P2 unless it's a bug or the user says otherwise.
5. **Parent arc** — if this is a slice of an existing arc, which parent issue #? If the user names an arc that has no parent issue yet, offer to create the arc parent first.
6. **Design doc** — if a `docs/plan_*.md` covers it, link it in the body. Don't copy the doc's contents; the doc is the source of truth.
7. **Body** — 1-3 sentences. What and why, plus `Spec: docs/plan_x.md` if applicable.

## Do
1. Create the issue with its `type:*` and `sys:*` labels (recipe: "Create an issue…").
2. Add it to Project 2 and set **Status=Todo**, **Priority**, **Subsystem** fields.
3. If it has a parent arc, link it as a sub-issue (recipe: "Link a child issue…").
4. If the user flagged it as ready-to-pick-up, add the `good-next` label; if waiting on something, add `blocked`.

## Report
Reply with the issue number, URL, and the labels/fields you set. Keep it to a couple lines.

## Notes
- Do NOT touch git (no branches, no commits) — that's `/task-start`.
- Batch a whole arc: create the parent first, then each sub-issue, linking each as you go.
- If a `gh` call fails on IDs, re-derive them with the field-list recipe (fields may have been edited).
