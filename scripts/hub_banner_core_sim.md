<!-- HUB BANNER — source of truth: scripts/hub_banner_core_sim.md, emitted into
     core_sim/CLAUDE.md right after the H1 by scripts/split_claude_md.sh.
     Edit the source file; an edit made only in the hub is reverted by the next
     re-run. Verify the two agree with: scripts/split_claude_md.sh --check -->

> ## ⛔ THIS IS A HUB FILE — rationale does NOT go here
>
> Before adding a paragraph, section, callout, or config row **anywhere in this file**, ask:
> **is this true of *all* `core_sim` work?**
>
> - **No** — it explains one arc's system, one config's keys, one bug's mechanism, one as-built
>   note → it belongs in the **rule file that owns the arc** (`.claude/rules/core_sim/*.md`, routing
>   table below). That is also what keeps two concurrent worktrees off the same file.
> - **Yes** — a build command, an environment override, a boot-config row, a genuinely
>   subsystem-wide invariant, or a **new row in the routing table** → here.
>
> This file loads into **every session in this repo**; a rule file loads only when you touch the
> code it describes, so a hub paragraph is paid for by every session forever. **If the owning
> rule's `paths:` already cover the code you changed, a hub copy is pure duplication** — the reader
> who could break the invariant loads the rule anyway. Root `CLAUDE.md` → "The hub files are not
> where rationale goes" has the long form.
