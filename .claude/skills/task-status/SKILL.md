---
name: task-status
description: Update a Falcon backlog item's status on GitHub — move its Project Status/Priority/Subsystem field, toggle blocked/good-next labels, or set a milestone — without hand-writing gh commands. Use when the user wants to change where a task stands.
---

# /task-status — update a work item's state

First read `.claude/skills/falcon-tracker-reference.md` for the IDs and gh recipes.

## Input
The user names an issue (number, title, or "the predators arc") and what to change:
- **Status** → Todo / In Progress / Done
- **Priority** → P0 / P1 / P2
- **Subsystem** → one of the six
- **blocked / good-next** labels → add or remove
- **Milestone** → set or clear

Resolve a fuzzy reference to an issue number with `gh issue list --repo rwalker123/falcon --search "…"` and confirm if ambiguous.

## Do
- For Project fields (Status/Priority/Subsystem): find the item id, then `gh project item-edit` with the field id + option id (recipe: "Move an item's field").
- For labels: `gh issue edit <NUM> --repo rwalker123/falcon --add-label blocked` / `--remove-label good-next`.
- For milestone: `gh issue edit <NUM> --repo rwalker123/falcon --milestone "NAME"`.
- **Done**: set Status=Done. Only `gh issue close` the issue if the user says the work is actually finished (usually a merged PR closes it via `Closes #N` automatically — prefer that).

## Report
State the before→after for each field/label you changed, plus the issue URL.

## Notes
- Setting Status=Done ≠ closing the issue. Keep them distinct unless the user wants both.
- If moving an arc parent, remind the user its sub-issues have their own status.
