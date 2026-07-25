#!/usr/bin/env bash
# Split core_sim/CLAUDE.md into a slim hub + path-scoped rule files.
#
# The extraction is BYTE-EXACT: every rule file is a `sed` line-range of the
# original, so no rationale can be silently paraphrased away. The only
# transformation applied to extracted content is a one-level heading promotion
# (`## ` -> `# `, `### ` -> `## `), which is safe because the original has no
# `##`/`###` headings inside fenced code blocks.
#
# Run from the repo root. Idempotent: re-run after editing SECTIONS below.
set -euo pipefail

SRC="core_sim/CLAUDE.md"
OUT=".claude/rules/core_sim"
ORIG="$(mktemp)"
trap 'rm -f "$ORIG"' EXIT

# Always split from the committed original, so re-runs don't cascade.
git show HEAD:"$SRC" > "$ORIG" 2>/dev/null || cp "$SRC" "$ORIG"
TOTAL=$(wc -l < "$ORIG" | tr -d ' ')
echo "source: $SRC ($TOTAL lines)"

mkdir -p "$OUT"

# name|start|end|extra-h1|promote
#
# promote=yes  shifts every heading up one level, so the file's single original
#              `##` section becomes its H1.
# promote=no   keeps levels as-is and prepends an H1. Used where a file bundles
#              TWO peer `##` sections — promoting both would leave two H1s.
# extra-h1     empty means the promoted content already supplies the H1.
SECTIONS=(
  "worldgen|112|1034||yes"
  "fauna|1070|1506||yes"
  "husbandry|1507|1983|# Husbandry — the yield ladder, the \`Tame\` verb, Corral|yes"
  "intensification|1984|2119||yes"
  "flora|2120|2474||yes"
  "cultivation|2475|2746|# Cultivation and the \`Sow\` verb — the plant twin of the pen|yes"
  "graze|2747|3114||yes"
  "combat|3115|3307||yes"
  "yield-forecast|3308|3446||yes"
  "telling|3447|3881||yes"
  "expeditions|3882|4259|# Expeditions — wondrous sites, scouting, and the hunt|no"
  "campaign|4260|4638||yes"
  "ecs-systems|4668|4798|# ECS systems reference — power, crisis, culture, knowledge, fog of war|no"
)

# ---------------------------------------------------------------- paths globs
paths_for() {
  case "$1" in
    worldgen) cat <<'EOF'
  - "core_sim/src/{mapgen,heightfield,hydrology,climate,biome_palette,map_preset,terrain,grid_utils}.rs"
  - "core_sim/src/systems/worldgen.rs"
  - "core_sim/src/data/map_presets.json"
  - "core_sim/tests/{elevation_authority,climate_authority,hydrology_earthlike}.rs"
  - "core_sim/tests/{navigable_mouth_delta,alpine_headwaters,relief_sweep,lake_abundance}.rs"
EOF
    ;;
    fauna) cat <<'EOF'
  - "core_sim/src/{fauna,fauna_config,creatures_config}.rs"
  - "core_sim/src/data/{fauna_config,creatures}.json"
  - "core_sim/tests/fauna_*.rs"
EOF
    ;;
    husbandry) cat <<'EOF'
  - "core_sim/src/{fauna,fauna_config,intensification}.rs"
  - "core_sim/src/data/intensification_ladder.json"
  - "core_sim/tests/{fauna_husbandry,grazing_2d_pen,rollback_tended_survival}.rs"
EOF
    ;;
    intensification) cat <<'EOF'
  - "core_sim/src/{intensification,knowledge_ledger}.rs"
  - "core_sim/src/data/intensification_ladder.json"
EOF
    ;;
    flora) cat <<'EOF'
  - "core_sim/src/{flora_config,forage,food}.rs"
  - "core_sim/src/data/flora_config.json"
  - "core_sim/tests/flora_*.rs"
EOF
    ;;
    cultivation) cat <<'EOF'
  - "core_sim/src/{forage,intensification}.rs"
  - "core_sim/tests/forage_*.rs"
EOF
    ;;
    graze) cat <<'EOF'
  - "core_sim/src/graze.rs"
  - "core_sim/tests/grazing_*.rs"
EOF
    ;;
    combat) cat <<'EOF'
  - "core_sim/src/combat/**"
  - "core_sim/src/combat_config.rs"
  - "core_sim/src/data/combat_config.json"
  - "core_sim/tests/predators.rs"
EOF
    ;;
    yield-forecast) cat <<'EOF'
  - "core_sim/src/{labor_config,orders}.rs"
  - "core_sim/src/systems/labor.rs"
  - "core_sim/src/snapshot/**"
  - "core_sim/src/data/labor_config.json"
  - "core_sim/tests/labor_allocation.rs"
EOF
    ;;
    telling) cat <<'EOF'
  - "core_sim/src/telling/**"
  - "core_sim/src/data/beat_*.json"
  - "core_sim/tests/telling*.rs"
  - "core_sim/tests/telling_support/**"
EOF
    ;;
    expeditions) cat <<'EOF'
  - "core_sim/src/{sites,sites_config,expedition_config}.rs"
  - "core_sim/src/systems/expeditions.rs"
  - "core_sim/src/data/{sites_config,expedition_config}.json"
  - "core_sim/tests/expedition_hunt.rs"
EOF
    ;;
    campaign) cat <<'EOF'
  - "core_sim/src/{demographics_config,generations,supply,supply_network_config}.rs"
  - "core_sim/src/{sedentarization,sedentarization_config,settlement_stage_config}.rs"
  - "core_sim/src/{wellbeing_config,victory,provinces,start_profile}.rs"
  - "core_sim/src/systems/{population,trade}.rs"
  - "core_sim/src/data/{demographics_config,supply_network_config,sedentarization_config}.json"
  - "core_sim/tests/{supply_network,sedentarization}.rs"
EOF
    ;;
    ecs-systems) cat <<'EOF'
  - "core_sim/src/{power,crisis,crisis_config,culture,culture_corruption_config}.rs"
  - "core_sim/src/{knowledge_ledger,espionage,great_discovery,influencers}.rs"
  - "core_sim/src/{visibility,visibility_systems,visibility_config}.rs"
  - "core_sim/src/systems/power.rs"
  - "core_sim/tests/capability_gating.rs"
EOF
    ;;
  esac
}

# ------------------------------------------------------------------ emit rules
for entry in "${SECTIONS[@]}"; do
  IFS='|' read -r name start end h1 promote <<< "$entry"
  f="$OUT/$name.md"
  {
    echo "---"
    echo "paths:"
    paths_for "$name"
    echo "---"
    echo
    # HTML comments are stripped before this file enters context, so this
    # provenance note is free for Claude and visible to humans.
    echo "<!-- Extracted verbatim from core_sim/CLAUDE.md lines $start-$end."
    echo "     Routing table and shared vocabulary live in core_sim/CLAUDE.md."
    echo "     Regenerate with scripts/split_core_sim_claude_md.sh -->"
    echo
    [ -n "$h1" ] && { echo "$h1"; echo; }
    # Portable BRE: BSD sed (macOS) has no `\+`, so `#\(#\+ \)` silently
    # matches nothing. `##\(#*\) ` works on both BSD and GNU sed.
    if [ "$promote" = "yes" ]; then
      sed -n "${start},${end}p" "$ORIG" | sed 's/^##\(#*\) /#\1 /'
    else
      sed -n "${start},${end}p" "$ORIG"
    fi
  } > "$f"
  printf '  %-18s %4d lines -> %s\n' "$name" "$((end - start + 1))" "$f"
done

# ------------------------------------------------------------------- emit hub
HUB="core_sim/CLAUDE.md"
{
  sed -n '1,20p' "$ORIG"                       # title + Quick Reference
  cat <<'EOF'
## Where the rest of this document lives

This file is the **hub**: build commands, config layout, the shared food-module
vocabulary, and the turn loop — the things true of *all* `core_sim` work. The
per-arc engineering rationale lives in `.claude/rules/core_sim/`, scoped with
`paths:` frontmatter so a file loads only when you touch the code it describes.

| Rule file | Covers | Loads when you touch |
|---|---|---|
| `worldgen.md` | Pipeline stages, elevation authority, rivers & drainage, fluvial erosion, rain shadow, temperature/climate authority, map presets | `mapgen.rs`, `hydrology.rs`, `climate.rs`, `heightfield.rs`, `map_presets.json` |
| `fauna.md` | Wild game, ecology & depensation, herd movement, hunting policy | `fauna*.rs`, `tests/fauna_*.rs` |
| `husbandry.md` | The husbandry yield ladder, the `Tame` verb, Corral | `fauna.rs`, `tests/fauna_husbandry.rs` |
| `intensification.md` | The 3-rung ladder engine, the knowledge pattern, behavior primitives | `intensification.rs`, `intensification_ladder.json` |
| `flora.md` | Depletable forage, the flora roster, per-tile realization, fodder, cash crops | `flora_config.rs`, `forage.rs`, `tests/flora_*.rs` |
| `cultivation.md` | Cultivation, the `Sow` verb, the Field | `forage.rs`, `tests/forage_*.rs` |
| `graze.md` | The pasture layer, the two food webs, ecological carrying capacity, the pen economy | `graze.rs`, `tests/grazing_*.rs` |
| `combat.md` | Combat & casualties, predation | `combat/`, `tests/predators.rs` |
| `yield-forecast.md` | Pre-commit per-source yield forecast, assign-time seeding | `labor_config.rs`, `snapshot/`, `orders.rs` |
| `telling.md` | The narrative beat engine, `when` grammar, stance, fork tier, memory threads | `telling/`, `tests/telling*.rs` |
| `expeditions.md` | Wondrous sites, scouting & hunting expeditions | `sites.rs`, `expedition_config.rs` |
| `campaign.md` | Start flow, population & demographics, supply network, sedentarization, wellbeing, victory | `supply.rs`, `demographics_config.rs`, `sedentarization*.rs` |
| `ecs-systems.md` | Power, crisis, culture, knowledge & espionage, great discovery, fog of war, trade diffusion | `power.rs`, `crisis.rs`, `culture.rs`, `visibility*.rs` |

**Cross-reference convention.** A quoted phrase like `see "The knowledge pattern"`
names a *section heading*, not a file. Resolve it with
`grep -rn '^#* The knowledge pattern' .claude/rules/core_sim/`. Directional words
("below"/"above") are only reliable *within* one file.

**Adding to these docs.** Put per-arc rationale in the rule file that owns the
arc — that is what keeps two concurrent worktrees off the same file. Only add
here if it is true of all `core_sim` work.

EOF
  sed -n '21,111p'   "$ORIG"                   # Configuration Files, env, ports
  sed -n '1035,1069p' "$ORIG"                  # Ecosystem Food Modules
  sed -n '4639,4667p' "$ORIG"                  # Turn Loop, Snapshot & Rollback
  sed -n '4799,4804p' "$ORIG"                  # See Also
} > "$HUB"

# -------------------------------------------------------------------- verify
echo
echo "hub: $(wc -l < "$HUB" | tr -d ' ') lines"
kept=$(( 20 + (111-21+1) + (1069-1035+1) + (4667-4639+1) + (4804-4799+1) ))
moved=0
for entry in "${SECTIONS[@]}"; do
  IFS="|" read -r _ start end _ _ <<< "$entry"
  moved=$(( moved + end - start + 1 ))
done
echo "accounting: $kept kept + $moved moved = $(( kept + moved )) of $TOTAL original lines"
[ $(( kept + moved )) -eq "$TOTAL" ] || { echo "LINE ACCOUNTING MISMATCH"; exit 1; }
echo "OK: every original line is either kept in the hub or moved to exactly one rule file."
