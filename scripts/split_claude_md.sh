#!/usr/bin/env bash
# Split an oversized subsystem CLAUDE.md into a slim hub + path-scoped rule files.
#
# Usage:  scripts/split_claude_md.sh [core_sim|client]     (no arg = both)
#
# BOUNDARIES ARE ANCHORS, NOT LINE NUMBERS. Each cut is a literal line of the
# source (a heading, a table row, or the lead of a top-level bullet where a
# section has no headings). That is what makes this survive upstream merges:
# a PR that rewrites 70 lines inside "World Generation Pipeline" changes every
# line number after it, but the anchors still resolve, so re-running re-cuts
# correctly with no hand re-mapping. The first cut of this tool was pinned to
# line numbers and needed all 34 ranges re-derived after the first merge.
#
# Every anchor must match EXACTLY ONCE and the list must be in file order; both
# are asserted, so an upstream edit that renames or duplicates a heading fails
# loudly here instead of silently misfiling a section.
#
# The extraction is BYTE-EXACT: a rule file is `sed` line ranges of the source,
# so no rationale can be silently paraphrased. Two declared transformations:
#
#   promote=yes   shift headings up one level (`## `->`# `, `### `->`## `), so a
#                 file carrying one original `##` section gets it as the H1.
#                 Safe: neither source has `##`/`###` inside a fenced block.
#   table=scripts the file receives rows lifted out of "Key Scripts Reference"
#   table=config  ... or out of "Configuration Files". Either way, re-emit the
#                 2-line table header so the rows render, under their own
#                 heading. Row anchors sort above the prose anchors, so the
#                 rows land first — which is why these files use promote=no
#                 and an explicit H1 (a promoted section heading arriving after
#                 the table would leave an H2 above the H1).
#
# SOURCE is a PINNED pre-split blob, not `HEAD:` — once the split lands, `HEAD:`
# is the slim hub and re-running against it would re-split the hub. Re-pin when
# merging upstream changes into the unsplit file. NOTE: after this migration
# lands, edits go straight into the rule files; re-running then re-cuts from the
# pinned original and would drop rule-file edits made since. Re-pin first.
set -euo pipefail

# Pre-split blobs (origin/main @ PR #329/#341/#342 merges).
# Verify with: git cat-file blob <sha> | wc -l
BLOB_CORE_SIM=dcc757587f8c9308590997ee600abc64a34e6712   # 4926 lines
BLOB_CLIENT=20553fb8f9b193b80338a8c06765d511b81b601e     # 4278 lines

ORIG="$(mktemp)"; TMP="$(mktemp)"; trap 'rm -f "$ORIG" "$TMP"' EXIT

# ------------------------------------------------------------------ helpers
# Resolve CUTS -> REGION_DEST[i] / REGION_START[i] / REGION_END[i].
# Asserts each anchor matches exactly once and that the list is in file order.
resolve_cuts() {
  local total; total=$(wc -l < "$ORIG" | tr -d ' ')
  REGION_DEST=(); REGION_START=(); REGION_END=()
  local prev=0 i=0 entry anchor dest n line
  for entry in "${CUTS[@]}"; do
    # Split on the LAST `|`, not the first: table-row anchors are themselves
    # markdown rows and contain `|`, so `%%|*` would yield an empty anchor.
    anchor="${entry%|*}"; dest="${entry##*|}"
    n=$(grep -cF -- "$anchor" "$ORIG" || true)
    [ "$n" -eq 1 ] || { echo "  ANCHOR '$anchor' matched $n times (need exactly 1)" >&2; exit 1; }
    line=$(grep -nF -- "$anchor" "$ORIG" | cut -d: -f1)
    [ "$line" -gt "$prev" ] || { echo "  ANCHOR '$anchor' at line $line is out of order (previous $prev)" >&2; exit 1; }
    [ $i -gt 0 ] && REGION_END[$((i-1))]=$((line-1))
    REGION_DEST[$i]="$dest"; REGION_START[$i]="$line"
    prev=$line; i=$((i+1))
  done
  REGION_END[$((i-1))]=$total
}

# Echo the `;`-separated range list for a destination, in file order.
ranges_for_dest() {
  local want="$1" i out=""
  for i in "${!REGION_DEST[@]}"; do
    [ "${REGION_DEST[$i]%^}" = "$want" ] && out="$out;${REGION_START[$i]}-${REGION_END[$i]}"
  done
  echo "${out#;}"
}

extract() { local r; IFS=';' read -ra r <<< "$1"
  for range in "${r[@]}"; do sed -n "${range%-*},${range#*-}p" "$ORIG"; done; }

count() { local r total=0; IFS=';' read -ra r <<< "$1"
  for range in "${r[@]}"; do total=$(( total + ${range#*-} - ${range%-*} + 1 )); done; echo "$total"; }

meta_for() { local n; for n in "${META[@]}"; do [ "${n%%|*}" = "$1" ] && { echo "${n#*|}"; return; }; done; echo "||no|no"; }

emit_rule() {
  local outdir="$1" name="$2" ranges h1 promote table m f
  ranges="$(ranges_for_dest "$name")"
  m="$(meta_for "$name")"; h1="${m%%|*}"; m="${m#*|}"; promote="${m%%|*}"; table="${m#*|}"
  f="$outdir/$name.md"
  {
    echo "---"; echo "paths:"; paths_for "$name"; echo "---"; echo
    # Block-level HTML comments are stripped before a rule enters context, so
    # this provenance note is free for Claude and visible to humans.
    # Name the BLOB, not just the path: these line numbers are in the pre-split
    # original, while $SRC itself is now the slim hub, so a reader who takes
    # them literally goes looking for line 4790 in a 213-line file.
    echo "<!-- Extracted verbatim from lines $ranges of $SRC at blob $SRC_BLOB"
    echo "     (the PRE-SPLIT original — read it with \`git cat-file blob $SRC_BLOB\`;"
    echo "     $SRC itself is now the hub, where the routing table lives)."
    echo "     Regenerate with scripts/split_claude_md.sh -->"
    echo
    [ -n "$h1" ] && { echo "$h1"; echo; }
    case "$table" in
      scripts) echo "## Key scripts"; echo; echo "| Script | Purpose |"; echo "|--------|---------|" ;;
      config)  echo "## Config files"; echo; echo "| File | Purpose |"; echo "|------|---------|" ;;
    esac
    # Portable BRE: BSD sed (macOS) has no `\+`, so `#\(#\+ \)` matches nothing.
    if [ "$promote" = yes ]; then extract "$ranges" | sed 's/^##\(#*\) /#\1 /'
    else                          extract "$ranges"; fi
  } > "$f"
  printf '  %-24s %5d lines %7d B\n' "$name" "$(count "$ranges")" "$(wc -c < "$f" | tr -d ' ')"
}

emit_hub() {
  local blurb="$1" i
  : > "$TMP"
  for i in "${!REGION_DEST[@]}"; do
    [ "${REGION_DEST[$i]%^}" = hub ] || continue
    sed -n "${REGION_START[$i]},${REGION_END[$i]}p" "$ORIG" >> "$TMP"
    [ "${REGION_DEST[$i]}" = "hub^" ] && cat "$blurb" >> "$TMP"
  done
  cp "$TMP" "$SRC"
  printf '  hub: %s lines, %s B\n' "$(wc -l < "$SRC" | tr -d ' ')" "$(wc -c < "$SRC" | tr -d ' ')"
}

# Reassemble each rule and diff against the source ranges; assert full coverage.
verify_and_account() {
  local outdir="$1" total fail=0 name ranges m promote n inv moved=0 kept
  total=$(wc -l < "$ORIG" | tr -d ' ')
  for name in $(printf '%s\n' "${REGION_DEST[@]}" | sed 's/\^$//' | grep -v '^hub$' | sort -u); do
    ranges="$(ranges_for_dest "$name")"; n=$(count "$ranges"); moved=$((moved+n))
    m="$(meta_for "$name")"; m="${m#*|}"; promote="${m%%|*}"
    [ "$promote" = yes ] && inv='s/^#\(#*\) /##\1 /' || inv=''
    if diff -q <(extract "$ranges") \
               <(tail -n "$n" "$outdir/$name.md" | { [ -n "$inv" ] && sed "$inv" || cat; }) >/dev/null
    then printf '  OK    %-24s %5d\n' "$name" "$n"
    else printf '  FAIL  %-24s\n' "$name"
         diff <(extract "$ranges") <(tail -n "$n" "$outdir/$name.md" | { [ -n "$inv" ] && sed "$inv" || cat; }) | head -6
         fail=1; fi
  done
  kept=$(count "$(ranges_for_dest hub)")
  echo "  accounting: $kept kept + $moved moved = $((kept+moved)) of $total original lines"
  [ $((kept+moved)) -eq "$total" ] || { echo "  LINE ACCOUNTING MISMATCH"; return 1; }
  [ $fail -eq 0 ] || return 1
  echo "  OK: every original line is in the hub or in exactly one rule file, byte-exact."
}

run() { # <blob> <src> <outdir> <blurb>
  git cat-file blob "$1" > "$ORIG"
  SRC_BLOB="$1"; SRC="$2"; local outdir="$3"
  echo "== $SRC ($(wc -l < "$ORIG" | tr -d ' ') lines) =="
  mkdir -p "$outdir"
  resolve_cuts
  local name
  for name in $(printf '%s\n' "${REGION_DEST[@]}" | sed 's/\^$//' | grep -v '^hub$' | sort -u); do
    emit_rule "$outdir" "$name"
  done
  emit_hub "$4"
  echo "  -- verify --"; verify_and_account "$outdir"
}

# ============================================================ core_sim
split_core_sim() {
  # anchor|destination   (hub^ = emit the routing blurb after this region)
  CUTS=(
    "# core_sim - Simulation Engine|hub^"
    "## Configuration Files|hub"
    "| \`src/data/labor_config.json\` ||yield-forecast"
    "| \`src/data/intensification_ladder.json\` ||intensification"
    "| \`src/data/fauna_config.json\` ||fauna"
    "| \`src/data/flora_config.json\` ||flora"
    "| \`src/data/beat_definitions.json\` ||telling"
    "| \`src/data/sedentarization_config.json\` ||campaign"
    "| \`src/data/sites_config.json\` ||expeditions"
    "| \`src/data/combat_config.json\` ||combat"
    "| \`src/data/creatures.json\` ||fauna"
    "Hot reload: \`reload_config [path]\`|hub"
    "## World Generation Pipeline|worldgen"
    "## Ecosystem Food Modules|hub"
    "## Fauna & Wild Game|fauna"
    "### The husbandry yield ladder — every rung pays MSY|husbandry"
    "## The Intensification Ladder|intensification"
    "## Depletable Forage (Intensification §0-ii)|flora"
    "### Cultivation (Intensification Phase 1a)|cultivation"
    "## The Graze (Pasture) Layer (Grazing Phase 2a)|graze"
    "## Combat & Casualties (Predators arc — Phase 0)|combat"
    "## Pre-commit Yield Forecast (per-source, on the wire)|yield-forecast"
    "## The Telling — the narrative beat engine|telling"
    "## Wondrous Sites|expeditions"
    "## Campaign Loop & System Activation|campaign"
    "## Turn Loop|hub"
    "## ECS Systems Reference|ecs-systems"
    "## See Also|hub"
  )
  META=(
    "husbandry|# Husbandry — the yield ladder, the \`Tame\` verb, Corral|yes|no"
    "cultivation|# Cultivation and the \`Sow\` verb — the plant twin of the pen|yes|no"
    "expeditions|# Expeditions — wondrous sites, scouting, and the hunt|no|config"
    "ecs-systems|# ECS systems reference — power, crisis, culture, knowledge, fog of war|no|no"
    "fauna|# Fauna & wild game|no|config"
    "flora|# Depletable forage and the flora roster|no|config"
    "intensification|# The intensification ladder|no|config"
    "yield-forecast|# Pre-commit yield forecast (per-source, on the wire)|no|config"
    "telling|# The Telling — the narrative beat engine|no|config"
    "campaign|# Campaign loop & system activation|no|config"
    "combat|# Combat & casualties, predation|no|config"
    "worldgen||yes|no" "graze||yes|no"
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
  run "$BLOB_CORE_SIM" "core_sim/CLAUDE.md" ".claude/rules/core_sim" "$HUB_BLURB_CORE_SIM"
}

# ============================================================== client
split_client() {
  # Rows of the Key Scripts Reference table are routed to the rule that owns
  # each script; the boot/menu/settings rows stay in the hub.
  CUTS=(
    "# Godot Thin Client|hub^"
    "| \`MapView.gd\` ||map-renderers"
    "| \`Inspector.gd\` ||inspector-panels"
    "| \`Hud.gd\` ||hud-modules"
    "| \`ui/hud/HudBandLaborState.gd\` ||labor-ui"
    "| \`ui/hud/LegendController.gd\` ||hud-modules"
    "| \`ui/hud/TurnOrbController.gd\` ||turn-orb"
    "| \`ui/hud/SelectionCardController.gd\` ||selection-card"
    "| \`ui/hud/DrawerComposeController.gd\` ||labor-ui"
    "| \`ui/hud/SubjectDrawerController.gd\` ||selection-card"
    "| \`ui/hud/TargetingController.gd\` ||targeting"
    "| \`ui/hud/BandPanelController.gd\` ||band-city-panel"
    "| \`ui/hud/DockScrollFit.gd\` ||hud-modules"
    "| \`ui/hud/ComposeSheet.gd\` ||labor-ui"
    "| \`ui/hud/HudWidgets.gd\` ||hud-modules"
    "| \`ui/hud/BandDetailLines.gd\` ||band-readouts"
    "| \`ui/BandCityPanel.gd\` / \`.tscn\` ||band-city-panel"
    "| \`ui/BandFoodStatus.gd\` ||band-readouts"
    "| \`ui/PenStatus.gd\` ||band-city-panel"
    "| \`ui/TileHabitability.gd\` ||sprites-widgets"
    "| \`ui/TurnOrb.gd\` / \`ui/TurnOrb.tscn\` ||turn-orb"
    "| \`ui/MagnifierButton.gd\` ||sprites-widgets"
    "| \`ui/AutoSizingPanel.gd\` ||panel-framework"
    "| \`ui/HudStyle.gd\` ||sprites-widgets"
    "| \`ui/NarrativeForkPanel.gd\` ||telling-panel"
    "| \`tools/ui_preview.gd\` / \`.tscn\` ||test-harnesses"
    "| \`assets/terrain/TerrainTextureManager.gd\` ||terrain-textures"
    "| \`ui/hud/hud_const.gd\` ·|hud-modules"
    "## Architecture|hub"
    "### Native Extension|native-extension"
    "## Minimap System|map-renderers"
    "## Terrain Texture System|terrain-textures"
    "### Edge Blending — per-pixel biome-blend shader (Approach B)|terrain-blend-shader"
    "## HUD Panel Framework (Docked PanelCards)|panel-framework"
    "## Map markers (MapView hex-icon stack UX)|map-markers"
    "## Command Targeting|targeting"
    "- **The selection card|selection-card"
    "- **Labor allocation UI**|labor-ui"
    "- **Fog gate on live tile contents|herd-readouts"
    "- **Forage-patch cultivation readout**|land-readouts"
    "- **Demographics readout**|band-readouts"
    "- **Band alerts → the turn orb**|turn-orb"
    "- **Targeting: move-band + send-expedition|targeting"
    "## Band/City dockable panel|band-city-panel"
    "## Inspector Panels|inspector-panels"
    "## Overlay Channels|overlay-channels"
    "## Typography & Theming|hub"
    "## Scripting Capability Model|scripting-capability"
    "## Hotkeys|hub"
  )
  META=(
    "native-extension|# Native extension — the GDExtension module map|yes|no"
    "map-renderers|# MapView renderers and the 2D minimap|no|scripts"
    "terrain-textures|# Terrain textures — assets, config, loading, 2D pipeline|no|scripts"
    "terrain-blend-shader|# The terrain blend shader — edge blending, shore, canopy, peaks, rivers|yes|no"
    "panel-framework|# HUD panel framework — docked PanelCards|no|scripts"
    "map-markers||yes|no"
    "selection-card|# The selection card — ONE card, ONE list, ONE drawer|no|scripts"
    "labor-ui|# Labor allocation UI — the compose sheet and forecasts|no|scripts"
    "herd-readouts|# Herd readouts — fog gate, ecology, husbandry, corral, the pen|no|no"
    "land-readouts|# Land readouts — forage, flora, the crop picker, pasture, the meters|no|no"
    "band-readouts|# Band readouts — demographics, food, morale, wellbeing, tile facts|no|scripts"
    "turn-orb|# The turn orb and the attention model|no|scripts"
    "targeting|# Command targeting — move-band and expeditions|no|scripts"
    "band-city-panel|# The Band/City dockable panel|no|scripts"
    "inspector-panels|# Inspector panels|no|scripts"
    "overlay-channels||yes|no"
    "hud-modules|# Hud.gd and the ui/hud module reference|no|scripts"
    "telling-panel|# The Telling panel and the narrative fork|no|scripts"
    "sprites-widgets|# Sprites, icons, styling and small widgets|no|scripts"
    "test-harnesses|# Headless verification harnesses (tools/)|no|scripts"
    "scripting-capability||yes|no"
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
  run "$BLOB_CLIENT" "clients/godot_thin_client/CLAUDE.md" ".claude/rules/client" "$HUB_BLURB_CLIENT"
}

HUB_BLURB_CORE_SIM="${HUB_BLURB_CORE_SIM:-scripts/hub_blurb_core_sim.md}"
HUB_BLURB_CLIENT="${HUB_BLURB_CLIENT:-scripts/hub_blurb_client.md}"

case "${1:-both}" in
  core_sim) split_core_sim ;;
  client)   split_client ;;
  both)     split_core_sim; echo; split_client ;;
  *) echo "usage: $0 [core_sim|client]" >&2; exit 2 ;;
esac
