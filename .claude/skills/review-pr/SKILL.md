---
name: review-pr
description: Review a pull request end to end — read the GitHub review comments from Copilot and Claude, run an independent local review of the diff, then report both and fix what's valid. Use when the user wants a PR reviewed or wants to process code review feedback on a pull request.
disable-model-invocation: true
user-invocable: true
argument-hint: [pr-number]
---

# PR Reviewer

Review a pull request from **two independent directions** and merge the results:

1. **The posted review comments** — Copilot and Claude (and human reviewers) on
   GitHub, triaged for validity.
2. **Your own review of the diff** — read the changes yourself and find what the
   bots missed. The posted comments are input, not the ceiling: a reviewer that
   only triages someone else's findings inherits their blind spots.

Both feed one report, one approval gate, and one round of fixes.

This repo is a Rust (Bevy ECS) workspace plus a Godot thin client with
FlatBuffers contracts. The review rules live in `.github/copilot-instructions.md`
and the subsystem `CLAUDE.md` files — consult them when assessing a comment.

## Delegation model — keep review out of the orchestrator's context

Processing a PR means reading many source files (analysis) and running builds
and tests (fixes). Both would flood the main session's context, so this skill
**delegates the heavy work to agents** and keeps only the report, the approval
gate, and lightweight GitHub API calls in the orchestrator:

- **Comment triage** (fetch comments, read code, assess each finding) → a
  read-only **`general-purpose`** agent that returns *only* a structured report
  plus a JSON finding-manifest. It never mutates GitHub or the working tree.
- **Local review** (read the diff cold, find defects nobody filed) → a second
  read-only **`general-purpose`** agent, run *in parallel* with the first and
  deliberately not told what the bots said, so its findings are independent.
- **Fixes** (edit code, then fmt/clippy/test or godot-build) → **`server-dev`**
  (Rust) and **`client-dev`** (Godot/GDScript) agents, which self-verify and
  return terse summaries.
- **The orchestrator** does everything in between: presents the report, gets the
  user's approval, and performs the GitHub side-effects (inline comments,
  replies, thread resolution, `eyes` reactions). These are small JSON calls,
  cheap on context.

The orchestrator must **not** read source files for analysis itself, and must
**not** run the fmt/clippy/test/build loop itself — that is what the agents are
for. Its own tool use should be limited to `gh` API calls and dispatching agents.

**The two agent briefs live in `references/`, not here**, for the same reason:
they are written for the agents, and the orchestrator only needs to hand over a
path. Do not read them yourself.

| File | Read by |
|---|---|
| `references/agent-a-comment-triage.md` | Agent A |
| `references/agent-b-local-review.md` | Agent B |
| `references/agent-return-contract.md` | both agents — the report + manifest shape they return |

## Step 1: Identify the PR (orchestrator)

If a PR number was provided in `$ARGUMENTS`, use it. Otherwise detect from the
current branch:

```bash
gh pr view --json number,headRefName,url --jq '{number, headRefName, url}'
```

Also capture repo slug and HEAD SHA — the review agents and the side-effect
calls both need them:

```bash
gh repo view --json owner,name --jq '.owner.login + "/" + .name'
gh pr view {PR_NUMBER} --json headRefOid --jq '.headRefOid'
```

**Then confirm the working tree is at that SHA** — both agents Read local files
to assess findings, and `gh pr diff` will happily hand back a diff for a PR that
isn't checked out:

```bash
git rev-parse HEAD
```

If it doesn't match `headRefOid`, **stop and ask the user** before going further:
reviewing PR #N's diff against a different branch's file contents produces
findings about code the PR doesn't contain and misses the code it does. Offer
`gh pr checkout {PR_NUMBER}` or a worktree — but never switch branches yourself;
this repo's branch topology is the user's call (see the root `CLAUDE.md`).

## Step 2: Delegate the review to two `general-purpose` agents, in parallel

Spawn **both agents in a single message** so they run concurrently (each needs
Bash for `gh` plus Read/Grep). Hand each one:

- the **PR number**, **repo slug**, and **HEAD SHA** from Step 1;
- the repo-relative path to its brief, with an instruction to read that file
  first and follow it:
  - **Agent A — comment triage** → `.claude/skills/review-pr/references/agent-a-comment-triage.md`
  - **Agent B — local review** → `.claude/skills/review-pr/references/agent-b-local-review.md`

Each brief points at `agent-return-contract.md` for the return format, so you do
not need to restate it.

**Do not pass Agent B the posted comments, or any part of Agent A's brief.** Its
entire value is that it looks at the diff without knowing what the bots already
flagged; handing it A's material defeats the independence that is the reason
there are two agents.

Both agents number their findings from `n: 1` independently; **the orchestrator
renumbers on merge** (Step 3). Do not ask either agent to guess an offset — they
run in parallel and neither knows the other's count.

## Step 3: Present the merged report and get approval (orchestrator)

**Concatenate the two manifests into one and renumber `n` sequentially across the
merged list.** Both agents number from 1, so the raw manifests collide — two #1s
in the report make "fix 1,3,5" unanswerable and dispatch the wrong findings in
Step 4. Everything downstream (Steps 4 and 5) reads this single merged manifest.

Then **de-duplicate** where a local finding and a posted comment describe the same
defect. Keep the posted entry's `comment_id` / `comment_type` / `thread_id` — the
side-effects in Step 5 need them — but take the **more severe of the two
assessments**, and say in the report that the local review found it
independently. Two passes converging on the same defect is the strongest evidence
available that it's real; a rule that let Agent A's `disagree` or `style-nit`
silently outrank Agent B's `fix-needed` would discard exactly the signal the
second pass exists to produce.

Add a section for the local-only findings (numbering continues from the merged
list — the example below assumes six posted findings came first):

```
### Found by Local Review (X items — not filed by any reviewer)
| # | Severity | File | Finding | Failure |
|---|----------|------|---------|---------|
| 7 | Critical | core_sim/src/graze.rs:88 | capacity divides by herd size | panics when a herd hits 0 animals |
```

Relay the merged report to the user. Then ask:
**"Which items should I fix? (e.g., 'all fixes needed', '1,3,5', 'skip')"**

Do not proceed to fixes or any thread resolution without an explicit answer.

## Step 4: Delegate fixes to the coder agents (orchestrator dispatches)

Partition the approved findings by area — from **both** sources, posted comments
and local review alike — and dispatch in parallel:
- **Rust** (`core_sim`, `sim_runtime`, `sim_schema`, `xtask`, generated
  FlatBuffers) → **`server-dev`**
- **Godot / GDScript / native extension** (`clients/godot_thin_client`) →
  **`client-dev`**
- **Docs, rules, skills, agent definitions, workflows** (`docs/**`,
  `.claude/rules/**`, `.claude/skills/**`, `CLAUDE.md`, `.github/**`) → **the
  orchestrator, inline**. There is no coder agent for prose and no build to run,
  and the edits need the judgment that produced the finding.

Give each agent the specific findings (file, line, description, the fix intent)
for its area. Both agents self-verify before returning:
- `server-dev`: `cargo fmt --all` + `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` + `cargo test --workspace --locked`. If a
  `.fbs` schema changed, regenerate first (`cargo build -p
  shadow_scale_flatbuffers`, then `rustfmt` the generated file).
- `client-dev`: `cargo xtask godot-build` + the ui_preview PNG harness.

Never silence clippy with `#[allow(...)]` just to pass — fix the underlying
issue. Each agent returns a terse summary (files touched, fix per finding,
verification result). The orchestrator does **not** re-run these checks itself.

If a fix is architectural / cross-cutting and doesn't fit a scoped agent spec,
handle it inline — but that should be the exception.

## Step 5: GitHub side-effects (orchestrator)

These are small `gh` calls; keep them in the orchestrator so mutations stay under
the user's eye. Drive them from the merged JSON manifest built in Step 3.

**Local findings (`"source": "local"`) have no GitHub side-effects** — no
thread to reply to, nothing to resolve, no reaction to add. They live in the
report and (if approved) in the fix; skip 5a–5d for them entirely. Do not open
inline comments on your own findings — you're fixing them in the same push.

**5a — Convert Claude findings into inline threads** (for any manifest entry with
`needs_inline_comment: true`). First confirm the file is in the diff
(`gh pr diff {PR_NUMBER} --name-only`), then:

```bash
gh api repos/:owner/:repo/pulls/{PR_NUMBER}/comments \
  -f body="**[Claude Review — {SEVERITY}]** {TITLE}

{DESCRIPTION}

_Converted from Claude's flat review comment for tracking._" \
  -f path="{FILE_PATH}" \
  -F line={LINE} \
  -f commit_id="{HEAD_SHA}"
```

After converting all findings from a given Claude issue comment, mark that
comment processed (Step 5d). Claude findings with no clear file/line
(architectural, ECS ordering, schema-contract questions) stay report-only — no
inline comment.

**5b — Reply on each processed thread:**
- Fixed: `-f body="Fixed in latest push. {BRIEF_DESCRIPTION}"`
- Not fixing (out of scope / disagree / style nit): `-f body="Not fixing — {REASON}"`
- Already fixed: `-f body="Already addressed — {NOTE}"`

```bash
gh api repos/:owner/:repo/pulls/{PR_NUMBER}/comments \
  -f body="..." -F in_reply_to={ORIGINAL_COMMENT_ID}
```

**5c — Resolve every processed thread** (fixed or not) once its reply is posted:

```bash
gh api graphql -f query='mutation { resolveReviewThread(input: {threadId: "{THREAD_ID}"}) { thread { id isResolved } } }'
```

**5d — Mark source comments processed** with an `eyes` reaction so the next run
skips them (path depends on `comment_type`):

```bash
# Copilot inline:
gh api repos/:owner/:repo/pulls/comments/{COMMENT_ID}/reactions -f content=eyes
# Claude flat issue comment:
gh api repos/:owner/:repo/issues/comments/{COMMENT_ID}/reactions -f content=eyes
```

## Important notes

- NEVER resolve threads or reply to comments without explicit user approval —
  present the full report FIRST and wait for direction.
- The orchestrator does not read source for analysis or run the build/test loop —
  those are delegated (Steps 2 and 4). If you catch yourself reading files to
  assess a finding or to review the diff, stop and let the agents do it. The same
  applies to the agent briefs in `references/`: hand over the path, don't read
  them in.
- **Never skip the local review because the bots found plenty.** A long Copilot
  list is not evidence the diff is covered; the two passes look for different
  things. Equally, an empty local-review section is a fine result — report it in
  one line rather than padding it with nits.
- **Re-running on the same PR is not a no-op.** The `eyes` marker dedupes the
  comment half only, so triage surfaces just the NEW unprocessed comments — but
  the local review re-runs from scratch every time, and a PR whose comments are
  all marked processed still gets a full second pass. Findings the user already
  declined stay declined; don't re-litigate them.
- Use exact file paths from the PR diff (not guessed paths) when creating inline
  comments. If a Claude finding names multiple files, comment on the primary one.
- The `eyes` reaction is the "processed" marker — do not use other reactions.
