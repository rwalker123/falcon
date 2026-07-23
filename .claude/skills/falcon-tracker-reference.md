# Falcon Tracker — shared reference

Concrete IDs and `gh` recipes for the `/task-*` skills. This is not a skill itself;
the skills load it so the IDs live in exactly one place. If the Project's fields or
options change, update the tables here and every skill stays correct.

## Constants

| Thing | Value |
|---|---|
| Repo | `rwalker123/falcon` |
| Project | number `2`, owner `rwalker123` |
| Project node id (`--project-id`) | `PVT_kwHOADhqu84BeRZy` |
| Project URL | https://github.com/users/rwalker123/projects/2 |

### Single-select field + option IDs

**Status** — field `PVTSSF_lAHOADhqu84BeRZyzhYs3lk`
- Todo `f75ad846` · In Progress `47fc9ee4` · Done `98236657`

**Priority** — field `PVTSSF_lAHOADhqu84BeRZyzhYs3tU`
- P0 `a27c1a06` · P1 `4e336660` · P2 `90e71109`

**Subsystem** — field `PVTSSF_lAHOADhqu84BeRZyzhYs3tY`
- core_sim `19ee4fff` · client `3feb04e2` · schema `0d325634` · runtime `20cf5d18` · worldgen `edc677d6` · tooling `77f903a4`

## Labels

- Subsystem: `sys:core_sim` `sys:client` `sys:schema` `sys:runtime` `sys:worldgen` `sys:tooling`
- Type: `type:arc` `type:feature` `type:bug` `type:chore` `type:design`
- Workflow: `blocked` `good-next`

Convention: every issue gets exactly one `type:*`, at least one `sys:*`. Arc parents
get `type:arc`. Design docs live in `docs/plan_*.md` — link them, never copy them.

## Recipes

### Create an issue and put it on the board with fields set
```bash
URL=$(gh issue create --repo rwalker123/falcon \
  --title "TITLE" \
  --label type:feature --label sys:client \
  --body "1-3 sentence summary. Spec: docs/plan_foo.md")

ITEM=$(gh project item-add 2 --owner rwalker123 --url "$URL" --format json | jq -r .id)
PID=PVT_kwHOADhqu84BeRZy
# Status=Todo, Priority=P2, Subsystem=client  (swap option ids from tables above)
gh project item-edit --project-id $PID --id "$ITEM" --field-id PVTSSF_lAHOADhqu84BeRZyzhYs3lk --single-select-option-id f75ad846
gh project item-edit --project-id $PID --id "$ITEM" --field-id PVTSSF_lAHOADhqu84BeRZyzhYs3tU --single-select-option-id 90e71109
gh project item-edit --project-id $PID --id "$ITEM" --field-id PVTSSF_lAHOADhqu84BeRZyzhYs3tY --single-select-option-id 3feb04e2
echo "$URL"
```

### Get an issue's GraphQL node id (needed for sub-issue links)
```bash
node_id() { gh api graphql -f query='query($o:String!,$r:String!,$n:Int!){repository(owner:$o,name:$r){issue(number:$n){id}}}' -f o=rwalker123 -f r=falcon -F n=$1 -q .data.repository.issue.id; }
```

### Link a child issue under an arc parent (sub-issue)
`gh issue` has no sub-issue porcelain yet — use the GraphQL `addSubIssue` mutation.
```bash
PARENT=$(node_id <PARENT_NUM>)
CHILD=$(node_id <CHILD_NUM>)
gh api graphql -f query='mutation($p:ID!,$c:ID!){addSubIssue(input:{issueId:$p,subIssueId:$c}){issue{number}}}' -f p=$PARENT -f c=$CHILD
```

### Move an item's field (find the item id first)
```bash
# item id for a given issue number:
ITEM=$(gh project item-list 2 --owner rwalker123 --format json -L 500 \
  | jq -r '.items[] | select(.content.number==<NUM>) | .id')
gh project item-edit --project-id PVT_kwHOADhqu84BeRZy --id "$ITEM" \
  --field-id <FIELD_ID> --single-select-option-id <OPTION_ID>
```

### Read the board (for reports)
```bash
gh project item-list 2 --owner rwalker123 --format json -L 500
# each item: .content.number/.title/.url, .status, .priority?, .subsystem?, labels, parent
```
