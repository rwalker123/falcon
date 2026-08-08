# Agent return contract — what both review agents hand back

Agent A (comment triage) and Agent B (local review) return the **same two
things**, so the orchestrator can concatenate them without special-casing either
source. Read this alongside your own brief
(`agent-a-comment-triage.md` or `agent-b-local-review.md`).

## 1. The report (markdown)

Agent A groups by assessment:

```
## PR Review Comment Analysis — PR #{NUMBER}

### Fixes Needed (X items)
| # | Source | Severity | File | Finding | Assessment |
|---|--------|----------|------|---------|------------|
| 1 | Copilot | High | core_sim/src/systems.rs:444 | unwrap on missing tile | Valid — panics on edge hex |

### Already Fixed (X)
### Out of Scope (X)
### Style Nits (X)
### Disagree (X)
```

Agent B has only one group — every finding it reports is a fix-needed — so it
returns a single table with a `Failure` column in place of `Assessment`:

```
### Found by Local Review (X items)
| # | Severity | File | Finding | Failure |
|---|----------|------|---------|---------|
| 1 | Critical | core_sim/src/graze.rs:88 | capacity divides by herd size | panics when a herd hits 0 animals |
```

## 2. The JSON manifest

One object per finding, so the orchestrator can drive fixes and GitHub
side-effects without re-fetching anything:

```json
[
  {
    "n": 1,
    "source": "copilot|claude|human|local",
    "severity": "...",
    "assessment": "fix-needed|already-fixed|out-of-scope|style-nit|disagree",
    "file_path": "core_sim/src/systems.rs",
    "line": 444,
    "title": "...",
    "description": "...",
    "comment_id": 123,               // database id of the source comment
    "comment_type": "pulls|issues",  // which reactions/reply path applies; null for local
    "thread_id": "PRRT_...",         // GraphQL node id, or null if no thread yet
    "in_reply_to_id": 123,           // for threaded replies
    "needs_inline_comment": true     // true for Claude findings with file+line and no thread yet
  }
]
```

**Agent B's field values are fixed**: `"source": "local"`, `"severity"` of
`Critical|Important|Nice-to-have`, `"assessment": "fix-needed"`, and `null` for
`comment_id`, `comment_type`, `thread_id`, and `in_reply_to_id`, with
`needs_inline_comment: false`. `fix-needed` is its only assessment: it was told
to drop anything it can't state as a failure, so a local `style-nit` would be a
finding it should not have reported at all.

## 3. Numbering

Both agents number their findings from `n: 1` **independently**. The
orchestrator renumbers on merge. Do not try to guess an offset — the two agents
run in parallel and neither knows the other's count.

## 4. Read-only

Neither agent creates comments, adds reactions, resolves threads, or touches the
working tree. All GitHub mutations belong to the orchestrator, after the user
has approved the report.

Return **only** the report and the manifest. No diff summary, no narration of
what the PR does well.
