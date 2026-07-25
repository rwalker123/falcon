<!-- HUB BANNER — source of truth: scripts/hub_banner_client.md, emitted into
     clients/godot_thin_client/CLAUDE.md right after the H1 by scripts/split_claude_md.sh.
     Edit the source file; an edit made only in the hub is reverted by the next
     re-run. Verify the two agree with: scripts/split_claude_md.sh --check -->

> ## ⛔ THIS IS A HUB FILE — rationale does NOT go here
>
> Before adding a paragraph, section, callout, or per-script row **anywhere in this file**, ask:
> **is this true of *all* Godot-client work?**
>
> - **No** — it explains one panel, one overlay, one shader, one HUD module, one script's job →
>   it belongs in the **rule file that owns the arc** (`.claude/rules/client/*.md`, routing table
>   below), and a new script's row goes in that rule's `## Key scripts` table. That is also what
>   keeps two concurrent worktrees off the same file.
> - **Yes** — a build/verify command, a socket/endpoint contract, a genuinely client-wide
>   invariant, or a **new row in the routing table** → here.
>
> This file loads into **every session in this repo**; a rule file loads only when you touch the
> code it describes, so a hub paragraph is paid for by every session forever. **If the owning
> rule's `paths:` already cover the code you changed, a hub copy is pure duplication** — the reader
> who could break the invariant loads the rule anyway. Root `CLAUDE.md` → "The hub files are not
> where rationale goes" has the long form.
