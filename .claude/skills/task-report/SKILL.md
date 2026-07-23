---
name: task-report
description: Read-only rollup of the Falcon Backlog project — what's in progress, what's blocked, per-arc progress, ready-to-pick-up items. Use when the user asks where things stand, what to work on next, or wants a status summary.
---

# /task-report — where do things stand

First read `.claude/skills/falcon-tracker-reference.md` for the IDs and gh recipes.
This skill is **read-only** — never create, edit, or close anything.

## Do
Pull the board with `gh project item-list 2 --owner rwalker123 --format json -L 500`
(and `gh issue list --repo rwalker123/falcon` for label/parent detail if needed), then
present a concise rollup. Tailor to what the user asked; a good default is:

- **In progress** — items with Status=In Progress (+ assignee).
- **Blocked** — items with the `blocked` label; note what each is waiting on if the body says.
- **Ready next** — `good-next` items, or Todo items grouped by Priority (P0 first).
- **Per-arc progress** — for each `type:arc` parent, its sub-issue done/total (the native
  Sub-issues progress field), so it's clear which arcs are near completion.

## Output
A short, skimmable summary — grouped lists, issue numbers as `#N`, arc rollups as `Arc name — 3/7`.
Lead with anything that needs a decision (blocked items, P0s). Link the Project URL at the end.
Do not dump every field of every item; surface what matters for "what should I do next."
