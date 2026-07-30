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

## Step 1: Identify the PR (orchestrator)

If a PR number was provided in `$ARGUMENTS`, use it. Otherwise detect from the
current branch:

```bash
gh pr view --json number,headRefName,url --jq '{number, headRefName, url}'
```

Also capture repo slug and HEAD SHA — the eval agent and the side-effect calls
both need them:

```bash
gh repo view --json owner,name --jq '.owner.login + "/" + .name'
gh pr view {PR_NUMBER} --json headRefOid --jq '.headRefOid'
```

## Step 2: Delegate the review to two `general-purpose` agents, in parallel

Spawn **both agents in a single message** so they run concurrently (each needs
Bash for `gh` plus Read/Grep). Hand both the PR number, repo slug, and HEAD SHA.
They work in their own contexts and return only a report + manifest — none of
the file reads land in the orchestrator.

- **Agent A — comment triage**: steps 2a–2e below.
- **Agent B — local review**: step 2f below. Do **not** pass it the posted
  comments; its value is that it looks at the diff without knowing what the bots
  already flagged.

Instruct each agent to do the following, verbatim in spirit:

### 2a: Fetch Copilot + human inline review comments

```bash
gh api repos/:owner/:repo/pulls/{PR_NUMBER}/comments --paginate --jq '.[] | {id, node_id, user: .user.login, path, line, original_line, body, created_at, in_reply_to_id, pull_request_review_id}'
```

Keep comments from `copilot-pull-request-reviewer` / `Copilot` and any human
reviewers. **Skip comments that already carry an `eyes` reaction** (processed in
a prior run):

```bash
gh api repos/:owner/:repo/pulls/comments/{COMMENT_ID}/reactions --jq '[.[] | select(.content == "eyes")] | length'
```

NOTE on reaction API paths — they differ by comment type:
- **PR review comments** (Copilot inline): `repos/:owner/:repo/pulls/comments/{COMMENT_ID}/reactions`
- **Issue comments** (Claude flat): `repos/:owner/:repo/issues/comments/{COMMENT_ID}/reactions`

Do NOT include the PR number in the reactions path — it's `pulls/comments/{ID}`,
not `pulls/{PR}/comments/{ID}`.

### 2b: Fetch Claude issue comments and parse them into findings

```bash
gh api repos/:owner/:repo/issues/{PR_NUMBER}/comments --jq '.[] | select(.user.login == "claude[bot]") | {id, node_id, body, created_at}'
```

Skip any with an `eyes` reaction (issue-comment path):

```bash
gh api repos/:owner/:repo/issues/comments/{COMMENT_ID}/reactions --jq '[.[] | select(.content == "eyes")] | length'
```

Claude's comments are markdown with numbered findings grouped under severity
headers (### Critical, ### Important, ### Code Quality / Nice-to-have). Parse each
into: `severity` (Critical/Important/Code Quality), `title` (the bold title),
`description` (full text), `file_path` (backtick-wrapped path, resolved to repo
root), `line` (primary line number), `source` = "claude".

### 2c: Fetch review-thread node IDs for later resolution

```bash
gh api graphql -f query='
  query($owner: String!, $repo: String!, $pr: Int!) {
    repository(owner: $owner, name: $repo) {
      pullRequest(number: $pr) {
        reviewThreads(first: 100) {
          nodes {
            id
            isResolved
            comments(first: 1) {
              nodes { id databaseId body author { login } path line }
            }
          }
        }
      }
    }
  }
' -f owner="{OWNER}" -f repo="{REPO}" -F pr={PR_NUMBER}
```

Match threads to comments by `databaseId` to get each thread's `id`.

### 2d: Assess each unprocessed finding

For each finding: read the referenced file and surrounding context (Read tool,
not `cat`) and classify it as one of:
- **Valid — fix needed**: exists in current code, should be fixed
- **Valid — already fixed**: code no longer matches what the reviewer described
- **Valid — but out of scope**: real but not for this PR
- **Style nit**: subjective preference, not a bug
- **Disagree**: reviewer is wrong (explain why)

Ground assessments in `.github/copilot-instructions.md` and the relevant
subsystem `CLAUDE.md` (`core_sim/CLAUDE.md`,
`clients/godot_thin_client/CLAUDE.md`). High-signal categories:
- FlatBuffers contract changes (`sim_schema/schemas/*.fbs`) without regenerated bindings
- Hand-edits to generated code under `shadow_scale_flatbuffers/src/generated/`
- New `unwrap()`/`expect()`/`panic!` in simulation/server hot paths
- Clippy suppressions (`#[allow(...)]`) added just to silence `-D warnings`
- Hardcoded tunables that belong in a `core_sim/src/data/*.json` config
- ECS systems added outside the correct `TurnStage` ordering
- Godot panels that reimplement sizing instead of reusing `AutoSizingPanel.gd`

### 2e: Agent A returns ONLY these two things

1. The **report** (markdown), grouped by assessment:

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

2. A **JSON manifest** — one object per finding, so the orchestrator can drive
   fixes and side-effects without re-fetching anything:

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
    "comment_type": "pulls|issues",  // which reactions/reply path applies
    "thread_id": "PRRT_...",         // GraphQL node id, or null if no thread yet
    "in_reply_to_id": 123,           // for threaded replies
    "needs_inline_comment": true     // true for Claude findings with file+line and no thread yet
  }
]
```

The agent must **not** create comments, add reactions, resolve threads, or touch
the working tree. It is read-only.

### 2f: Agent B — independent local review of the diff

This agent reviews the PR's changes itself. It is **not** given the posted
review comments, so it can surface what Copilot and Claude missed.

Get the diff and the changed files:

```bash
gh pr diff {PR_NUMBER}
gh pr diff {PR_NUMBER} --name-only
gh pr view {PR_NUMBER} --json title,body --jq '{title, body}'
```

Review against the PR's *stated intent* (title + body + any linked
`docs/plan_*.md` or issue), not just internal consistency — code that is
self-consistent but doesn't do what the PR claims is the most expensive defect
to find late. Read whole files around each hunk with Read, not just the diff
context: a hunk can be correct in isolation and wrong against its caller, and
the diff never shows the caller.

Ground the review in `.github/copilot-instructions.md`, `CLAUDE.md`, and the
`.claude/rules/**` files whose `paths:` frontmatter matches the changed code —
those rules are this repo's real invariants, and a violation of one is a finding
even when the code compiles and passes tests. Look hardest at:

- **Correctness**: off-by-one and boundary hexes, integer/float division and
  truncation, sign and unit errors, `unwrap()`/`expect()`/`panic!` on paths that
  can actually be reached, error cases swallowed rather than handled
- **Contracts**: `.fbs` changes without regenerated bindings; a field written by
  the sim but never read by the client (or vice versa); field-order changes in
  an append-only schema
- **Sim semantics**: ECS systems in the wrong `TurnStage`, state read in the same
  turn it's written, determinism breaks (iteration order over a `HashMap`,
  unseeded randomness, wall-clock time)
- **Repo rules**: magic numbers that should be a config lever or named constant,
  back-compat/fallback code for a game that has no shipped saves, rationale
  parked in a hub `CLAUDE.md` instead of the owning rule file
- **Tests**: does a new behavior have a test that would fail without the change?
  Does a changed behavior have a test asserting the *shipped* artifact rather
  than an in-process value?
- **Dead ends**: code added but never called, a config key added but never read,
  a snapshot field populated but never consumed

Report only defects that would change the code, and state each as a concrete
failure — *given this input, this happens* — not as a preference. If it can't
name what goes wrong, it's a nit; drop it. Do not report the diff back as a
summary, and do not restate what the PR does well.

Agent B returns the same two things as Agent A — a report section and a JSON
manifest — using `"source": "local"`, `"severity"` of
`Critical|Important|Nice-to-have`, `"assessment": "fix-needed"` (or `style-nit`),
and `null` for `comment_id`, `thread_id`, and `in_reply_to_id`, with
`needs_inline_comment: false`. It is read-only: no comments, no reactions, no
edits to the working tree.

## Step 3: Present the merged report and get approval (orchestrator)

Merge both agents' reports into one, de-duplicating where a local finding and a
posted comment describe the same defect (keep the posted one — it has a thread —
and note that the local review independently found it). Add a section for the
local findings:

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
the user's eye. Drive them from the JSON manifest.

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
  assess a finding or to review the diff, stop and let the agents do it.
- **Never skip the local review because the bots found plenty.** A long Copilot
  list is not evidence the diff is covered; the two passes look for different
  things. Equally, an empty local-review section is a fine result — report it in
  one line rather than padding it with nits.
- Re-running on the same PR re-runs the local review from scratch (there is no
  `eyes` marker for it). Findings the user already declined stay declined —
  don't re-litigate them; the comment triage half is what dedupes via reactions.
- Use exact file paths from the PR diff (not guessed paths) when creating inline
  comments. If a Claude finding names multiple files, comment on the primary one.
- Handle pagination — PRs can have many comments across pages.
- The `eyes` reaction is the "processed" marker — do not use other reactions.
- Re-running `/review-pr` on the same PR should surface only NEW unprocessed
  comments.
