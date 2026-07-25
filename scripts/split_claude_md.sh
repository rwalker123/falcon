#!/usr/bin/env bash
# Split an oversized subsystem CLAUDE.md into a slim hub + path-scoped rule files.
#
# Usage:  scripts/split_claude_md.sh [core_sim|client]     (no arg = both)
#
# The extraction is BYTE-EXACT: every rule file is assembled from `sed` line
# ranges of the committed original, so no rationale can be silently paraphrased
# away. Two transformations may be applied, both declared per section:
#
#   promote=yes   shift every heading up one level (`## `->`# `, `### `->`## `),
#                 so a file carrying exactly one original `##` section gets it
#                 as the H1. Safe: neither original has `##`/`###` inside a
#                 fenced code block.
#   table=yes     the file receives rows lifted out of a "Key Scripts Reference"
#                 markdown table. Re-emit the 2-line table header so the rows
#                 render, under their own `## Key scripts` heading. Row ranges
#                 always sort below the prose ranges, so they land first.
#
# Ranges are `A-B` or `A-B;C-D;...`, emitted in the order written.
#
# Run from the repo root. The source is a PINNED pre-split blob, not a ref: once
# the split lands, `HEAD:` is the slim hub, so re-running against HEAD would
# re-split the hub. Pinning makes the tool re-runnable forever — edit the tables
# below and re-run to re-cut boundaries against the true original.
set -euo pipefail

# Pre-split blobs. Verify with: git cat-file blob <sha> | wc -l
BLOB_CORE_SIM=79501afb38d3e40b5b3b46b53e737ace2283bfdd   # 4804 lines
BLOB_CLIENT=7e98bc9f17e43705f30f8b1e26a04c34ca19055a     # 4268 lines

ORIG="$(mktemp)"; trap 'rm -f "$ORIG"' EXIT

# ---------------------------------------------------------------- helpers
# Emit the concatenation of a `;`-separated range list from $ORIG.
extract() {
  local spec="$1" r
  IFS=';' read -ra r <<< "$spec"
  for range in "${r[@]}"; do
    sed -n "${range%-*},${range#*-}p" "$ORIG"
  done
}

# Total lines a range list covers.
count() {
  local spec="$1" r total=0
  IFS=';' read -ra r <<< "$spec"
  for range in "${r[@]}"; do
    total=$(( total + ${range#*-} - ${range%-*} + 1 ))
  done
  echo "$total"
}

# emit_rule <outdir> <name> <ranges> <h1> <promote> <table>
emit_rule() {
  local outdir="$1" name="$2" ranges="$3" h1="$4" promote="$5" table="$6"
  local f="$outdir/$name.md"
  {
    echo "---"; echo "paths:"; paths_for "$name"; echo "---"; echo
    # Block-level HTML comments are stripped before a rule enters context, so
    # this provenance note is free for Claude and visible to humans.
    echo "<!-- Extracted verbatim from $SRC lines $ranges."
    echo "     Routing table and shared vocabulary live in $SRC."
    echo "     Regenerate with scripts/split_claude_md.sh -->"
    echo
    [ -n "$h1" ] && { echo "$h1"; echo; }
    [ "$table" = yes ] && {
      echo "## Key scripts"; echo
      echo "| Script | Purpose |"; echo "|--------|---------|"
    }
    # Portable BRE: BSD sed (macOS) has no `\+`, so `#\(#\+ \)` matches nothing.
    if [ "$promote" = yes ]; then extract "$ranges" | sed 's/^##\(#*\) /#\1 /'
    else                          extract "$ranges"; fi
  } > "$f"
  printf '  %-24s %5d lines %7d B -> %s\n' \
    "$name" "$(count "$ranges")" "$(wc -c < "$f" | tr -d ' ')" "$f"
}

# Reassemble each rule file and diff it against the original line ranges.
verify() {
  local outdir="$1" fail=0 entry name ranges h1 promote table n inv
  for entry in "${SECTIONS[@]}"; do
    IFS='|' read -r name ranges h1 promote table <<< "$entry"
    n=$(count "$ranges")
    [ "$promote" = yes ] && inv='s/^#\(#*\) /##\1 /' || inv=''
    if diff -q <(extract "$ranges") \
               <(tail -n "$n" "$outdir/$name.md" | { [ -n "$inv" ] && sed "$inv" || cat; }) \
       >/dev/null
    then printf '  OK    %-24s %5d\n' "$name" "$n"
    else printf '  FAIL  %-24s\n' "$name"
         diff <(extract "$ranges") <(tail -n "$n" "$outdir/$name.md" | { [ -n "$inv" ] && sed "$inv" || cat; }) | head -6
         fail=1; fi
  done
  return $fail
}

# Assert every original line is in the hub or in exactly one rule file.
account() {
  local total="$1" kept="$2" entry moved=0
  for entry in "${SECTIONS[@]}"; do
    IFS='|' read -r _ ranges _ _ _ <<< "$entry"
    moved=$(( moved + $(count "$ranges") ))
  done
  echo "  accounting: $kept kept + $moved moved = $(( kept + moved )) of $total original lines"
  [ $(( kept + moved )) -eq "$total" ] || { echo "  LINE ACCOUNTING MISMATCH"; return 1; }
  echo "  OK: every original line is either kept in the hub or moved to exactly one rule file."
}

# ============================================================ core_sim
split_core_sim() {
  SRC="core_sim/CLAUDE.md"
  local OUT=".claude/rules/core_sim"
  git cat-file blob "$BLOB_CORE_SIM" > "$ORIG"
  local TOTAL; TOTAL=$(wc -l < "$ORIG" | tr -d ' ')
  echo "== $SRC ($TOTAL lines) =="
  mkdir -p "$OUT"

  # name|ranges|extra-h1|promote|table
  SECTIONS=(
    "worldgen|112-1034||yes|no"
    "fauna|1070-1506||yes|no"
    "husbandry|1507-1983|# Husbandry — the yield ladder, the \`Tame\` verb, Corral|yes|no"
    "intensification|1984-2119||yes|no"
    "flora|2120-2474||yes|no"
    "cultivation|2475-2746|# Cultivation and the \`Sow\` verb — the plant twin of the pen|yes|no"
    "graze|2747-3114||yes|no"
    "combat|3115-3307||yes|no"
    "yield-forecast|3308-3446||yes|no"
    "telling|3447-3881||yes|no"
    "expeditions|3882-4259|# Expeditions — wondrous sites, scouting, and the hunt|no|no"
    "campaign|4260-4638||yes|no"
    "ecs-systems|4668-4798|# ECS systems reference — power, crisis, culture, knowledge, fog of war|no|no"
  )
  paths_for() {
    case "$1" in
      worldgen) cat <<'EOF'
  - "core_sim/src/{mapgen,heightfield,hydrology,climate,biome_palette,map_preset,terrain,grid_utils}.rs"
  - "core_sim/src/systems/worldgen.rs"
  - "core_sim/src/data/map_presets.json"
  - "core_sim/tests/{elevation_authority,climate_authority,hydrology_earthlike}.rs"
  - "core_sim/tests/{navigable_mouth_delta,alpine_headwaters,relief_sweep,lake_abundance}.rs"
EOF
;;      fauna) cat <<'EOF'
  - "core_sim/src/{fauna,fauna_config,creatures_config}.rs"
  - "core_sim/src/data/{fauna_config,creatures}.json"
  - "core_sim/tests/fauna_*.rs"
EOF
;;      husbandry) cat <<'EOF'
  - "core_sim/src/{fauna,fauna_config,intensification}.rs"
  - "core_sim/src/data/intensification_ladder.json"
  - "core_sim/tests/{fauna_husbandry,grazing_2d_pen,rollback_tended_survival}.rs"
EOF
;;      intensification) cat <<'EOF'
  - "core_sim/src/{intensification,knowledge_ledger}.rs"
  - "core_sim/src/data/intensification_ladder.json"
EOF
;;      flora) cat <<'EOF'
  - "core_sim/src/{flora_config,forage,food}.rs"
  - "core_sim/src/data/flora_config.json"
  - "core_sim/tests/flora_*.rs"
EOF
;;      cultivation) cat <<'EOF'
  - "core_sim/src/{forage,intensification}.rs"
  - "core_sim/tests/forage_*.rs"
EOF
;;      graze) cat <<'EOF'
  - "core_sim/src/graze.rs"
  - "core_sim/tests/grazing_*.rs"
EOF
;;      combat) cat <<'EOF'
  - "core_sim/src/combat/**"
  - "core_sim/src/combat_config.rs"
  - "core_sim/src/data/combat_config.json"
  - "core_sim/tests/predators.rs"
EOF
;;      yield-forecast) cat <<'EOF'
  - "core_sim/src/{labor_config,orders}.rs"
  - "core_sim/src/systems/labor.rs"
  - "core_sim/src/snapshot/**"
  - "core_sim/src/data/labor_config.json"
  - "core_sim/tests/labor_allocation.rs"
EOF
;;      telling) cat <<'EOF'
  - "core_sim/src/telling/**"
  - "core_sim/src/data/beat_*.json"
  - "core_sim/tests/telling*.rs"
  - "core_sim/tests/telling_support/**"
EOF
;;      expeditions) cat <<'EOF'
  - "core_sim/src/{sites,sites_config,expedition_config}.rs"
  - "core_sim/src/systems/expeditions.rs"
  - "core_sim/src/data/{sites_config,expedition_config}.json"
  - "core_sim/tests/expedition_hunt.rs"
EOF
;;      campaign) cat <<'EOF'
  - "core_sim/src/{demographics_config,generations,supply,supply_network_config}.rs"
  - "core_sim/src/{sedentarization,sedentarization_config,settlement_stage_config}.rs"
  - "core_sim/src/{wellbeing_config,victory,provinces,start_profile}.rs"
  - "core_sim/src/systems/{population,trade}.rs"
  - "core_sim/src/data/{demographics_config,supply_network_config,sedentarization_config}.json"
  - "core_sim/tests/{supply_network,sedentarization}.rs"
EOF
;;      ecs-systems) cat <<'EOF'
  - "core_sim/src/{power,crisis,crisis_config,culture,culture_corruption_config}.rs"
  - "core_sim/src/{knowledge_ledger,espionage,great_discovery,influencers}.rs"
  - "core_sim/src/{visibility,visibility_systems,visibility_config}.rs"
  - "core_sim/src/systems/power.rs"
  - "core_sim/tests/capability_gating.rs"
EOF
;;    esac
  }

  local entry
  for entry in "${SECTIONS[@]}"; do
    IFS='|' read -r n r h p t <<< "$entry"; emit_rule "$OUT" "$n" "$r" "$h" "$p" "$t"
  done

  { extract "1-20"; cat "$HUB_BLURB_CORE_SIM"; extract "21-111;1035-1069;4639-4667;4799-4804"; } > "$SRC"
  echo "  hub: $(wc -l < "$SRC" | tr -d ' ') lines, $(wc -c < "$SRC" | tr -d ' ') B"
  account "$TOTAL" "$(count '1-20;21-111;1035-1069;4639-4667;4799-4804')"
  echo "  -- verify --"; verify "$OUT"
}

# ============================================================== client
split_client() {
  SRC="clients/godot_thin_client/CLAUDE.md"
  local OUT=".claude/rules/client"
  git cat-file blob "$BLOB_CLIENT" > "$ORIG"
  local TOTAL; TOTAL=$(wc -l < "$ORIG" | tr -d ' ')
  echo "== $SRC ($TOTAL lines) =="
  mkdir -p "$OUT"

  # Rows 139-222 are the Key Scripts Reference table, routed row-group by
  # row-group to the rule that owns each script. Row ranges always sort below
  # the prose ranges, so `extract` lays them out first, under the re-emitted
  # table header. Rows 139-144 (boot/menu/settings) stay in the hub.
  SECTIONS=(
    "native-extension|243-336|# Native extension — the GDExtension module map|yes|no"
    "map-renderers|145-151;337-395|# MapView renderers and the 2D minimap|no|yes"
    "terrain-textures|220-221;396-501|# Terrain textures — assets, config, loading, 2D pipeline|no|yes"
    "terrain-blend-shader|502-1446|# The terrain blend shader — edge blending, shore, canopy, peaks, rivers|yes|no"
    "panel-framework|203-203;1447-1552|# HUD panel framework — docked PanelCards|no|yes"
    "map-markers|1553-1605||yes|no"
    "selection-card|178-178;180-180;1612-1775|# The selection card — ONE card, ONE list, ONE drawer|no|yes"
    "labor-ui|171-172;179-179;185-186;1776-2474|# Labor allocation UI — the compose sheet and forecasts|no|yes"
    "herd-readouts|2475-2694|# Herd readouts — fog gate, ecology, husbandry, corral, the pen|no|no"
    "land-readouts|2695-2947|# Land readouts — forage, flora, the crop picker, pasture, the meters|no|no"
    "band-readouts|191-191;193-193;2948-3182|# Band readouts — demographics, food, morale, wellbeing, tile facts|no|yes"
    "turn-orb|176-177;201-201;3183-3280|# The turn orb and the attention model|no|yes"
    "targeting|181-181;1606-1611;3281-3464|# Command targeting — move-band and expeditions|no|yes"
    "band-city-panel|182-182;192-192;194-194;3465-3902|# The Band/City dockable panel|no|yes"
    "inspector-panels|152-168;3903-4016|# Inspector panels|no|yes"
    "overlay-channels|4017-4134||yes|no"
    "hud-modules|169-170;173-175;183-184;187-190;222-222|# Hud.gd and the ui/hud module reference|no|yes"
    "telling-panel|211-213|# The Telling panel and the narrative fork|no|yes"
    "sprites-widgets|195-200;202-202;204-210|# Sprites, icons, styling and small widgets|no|yes"
    "test-harnesses|214-219|# Headless verification harnesses (tools/)|no|yes"
    "scripting-capability|4178-4237||yes|no"
  )
  paths_for() {
    local C="clients/godot_thin_client"
    case "$1" in
      native-extension) cat <<EOF
  - "$C/native/src/**"
  - "$C/native/Cargo.toml"
EOF
;;      map-renderers) cat <<EOF
  - "$C/src/scripts/MapView.gd"
  - "$C/src/scripts/CachedMapRenderer.gd"
  - "$C/src/scripts/ui/{MinimapController,MinimapPanel}.gd"
  - "$C/src/scripts/ui/{BandMarkerRenderer,SecondaryMarkerRenderer,AnnotationRenderer}.gd"
EOF
;;      terrain-textures) cat <<EOF
  - "$C/assets/terrain/{TerrainTextureManager,TerrainDefinitions}.gd"
  - "$C/assets/terrain/terrain_config.json"
  - "$C/src/scripts/ui/TerrainRenderer.gd"
EOF
;;      terrain-blend-shader) cat <<EOF
  - "$C/assets/terrain/*.gdshader"
  - "$C/src/scripts/ui/{TerrainRenderer,RiverEdges}.gd"
  - "$C/tools/blend_probe.gd"
EOF
;;      panel-framework) cat <<EOF
  - "$C/src/scripts/ui/{PanelCard,PanelDock,AutoSizingPanel}.gd"
  - "$C/src/scripts/ui/hud/DockScrollFit.gd"
EOF
;;      map-markers) cat <<EOF
  - "$C/src/scripts/ui/{BandMarkerRenderer,SecondaryMarkerRenderer}.gd"
  - "$C/src/scripts/ui/{IconSprites,FoodIcons,SiteSprites,StageSprites}.gd"
EOF
;;      selection-card) cat <<EOF
  - "$C/src/scripts/ui/hud/{SelectionCardController,SubjectDrawerController}.gd"
  - "$C/src/scripts/ui/hud/{HudSelectionState,hud_selection_vocab}.gd"
EOF
;;      labor-ui) cat <<EOF
  - "$C/src/scripts/ui/hud/{ComposeSheet,ComposeState,DrawerComposeController}.gd"
  - "$C/src/scripts/ui/hud/{HudBandLaborState,SourceForecast,FoodOutlookChart,ArrivalStrip}.gd"
  - "$C/src/scripts/ui/hud/{hud_compose_vocab,hud_work_vocab}.gd"
EOF
;;      herd-readouts) cat <<EOF
  - "$C/src/scripts/ui/{PenStatus,FaunaSprites}.gd"
  - "$C/src/scripts/ui/hud/BandDetailLines.gd"
  - "$C/src/scripts/ui/inspector/FaunaPanel.gd"
EOF
;;      land-readouts) cat <<EOF
  - "$C/src/scripts/ui/hud/hud_flora_vocab.gd"
  - "$C/src/scripts/ui/{FoodIcons,TileHabitability,TileClimate}.gd"
EOF
;;      band-readouts) cat <<EOF
  - "$C/src/scripts/ui/hud/{BandDetailLines,TopBarReadouts,DetailFormat}.gd"
  - "$C/src/scripts/ui/{BandFoodStatus,TileHabitability,TileClimate}.gd"
EOF
;;      turn-orb) cat <<EOF
  - "$C/src/scripts/ui/hud/{AttentionController,TurnOrbController,hud_attention_vocab}.gd"
  - "$C/src/scripts/ui/TurnOrb.gd"
EOF
;;      targeting) cat <<EOF
  - "$C/src/scripts/ui/hud/{TargetingController,hud_expedition_vocab}.gd"
  - "$C/src/scripts/ui/AnnotationRenderer.gd"
EOF
;;      band-city-panel) cat <<EOF
  - "$C/src/scripts/ui/{BandCityPanel,BandFoodStatus,PenStatus}.gd"
  - "$C/src/scripts/ui/hud/BandPanelController.gd"
  - "$C/tools/band_panel_preview.gd"
EOF
;;      inspector-panels) cat <<EOF
  - "$C/src/scripts/Inspector.gd"
  - "$C/src/scripts/ui/inspector/**"
EOF
;;      overlay-channels) cat <<EOF
  - "$C/src/scripts/ui/{BandOverlayRenderer,AnnotationRenderer}.gd"
  - "$C/src/scripts/ui/inspector/OverlayPanel.gd"
EOF
;;      hud-modules) cat <<EOF
  - "$C/src/scripts/Hud.gd"
  - "$C/src/scripts/ui/hud/**"
EOF
;;      telling-panel) cat <<EOF
  - "$C/src/scripts/ui/{TellingPanel,NarrativeForkPanel}.gd"
EOF
;;      sprites-widgets) cat <<EOF
  - "$C/src/scripts/ui/{HudStyle,IconSprites,FoodIcons,FaunaSprites}.gd"
  - "$C/src/scripts/ui/{SiteSprites,WonderSprites,StageSprites,MagnifierButton}.gd"
  - "$C/src/scripts/ui/{TileHabitability,TileClimate,RiverEdges,MinimapPanel}.gd"
  - "$C/src/scripts/{SnapshotStream,CommandBridge}.gd"
EOF
;;      test-harnesses) cat <<EOF
  - "$C/tools/**"
  - "$C/tests/**"
EOF
;;      scripting-capability) cat <<EOF
  - "$C/src/scripts/scripting/**"
  - "$C/src/scripts/ui/inspector/ScriptManagerPanel.gd"
EOF
;;    esac
  }

  local entry
  for entry in "${SECTIONS[@]}"; do
    IFS='|' read -r n r h p t <<< "$entry"; emit_rule "$OUT" "$n" "$r" "$h" "$p" "$t"
  done

  { extract "1-144"; cat "$HUB_BLURB_CLIENT"; extract "223-242;4135-4177;4238-4268"; } > "$SRC"
  echo "  hub: $(wc -l < "$SRC" | tr -d ' ') lines, $(wc -c < "$SRC" | tr -d ' ') B"
  account "$TOTAL" "$(count '1-144;223-242;4135-4177;4238-4268')"
  echo "  -- verify --"; verify "$OUT"
}

HUB_BLURB_CORE_SIM="${HUB_BLURB_CORE_SIM:-scripts/hub_blurb_core_sim.md}"
HUB_BLURB_CLIENT="${HUB_BLURB_CLIENT:-scripts/hub_blurb_client.md}"

case "${1:-both}" in
  core_sim) split_core_sim ;;
  client)   split_client ;;
  both)     split_core_sim; echo; split_client ;;
  *) echo "usage: $0 [core_sim|client]" >&2; exit 2 ;;
esac
