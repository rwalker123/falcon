## THE STANDING RUNG, DERIVED — the whole test tree's one transcription of the sim's own rule.
##
## Every fixture patch and every fixture herd carries `current_rung`, because that is the field the
## client now asks *"has this rung been built"* of (`SourceForecast.improvement_is_done`). Stating it
## as a string literal beside the flags it must agree with is how a fixture stages a source the wire
## cannot produce — a Field standing on `plant:wild`, a penned herd standing on `animal:pastoral` —
## and then proves something about it. So no fixture in `tools/` spells a rung key: they all come
## through here, off the SAME `is_cultivated` / `is_field` / `domestication` / `corralled` values the
## row already carries.
##
## **IT IS THE SIM'S DERIVATION, RESTATED, NOT AN INVENTION.** `forage::patch_rung_key` is *"sown →
## field, cultivated → tended, else wild"* and `fauna`'s animal twin is *"penned → pen, tamed →
## pastoral, else wild"*. Spelled with `SourceForecast.RUNG_KEY_*` rather than literals, so the
## client's one vocabulary is the harness's too.
##
## **ALL-`static`, NO STATE** — the `input_probe.gd` shape, so `map_preview`, `band_panel_preview`,
## `snapshot_alias_guard` and any chapter can `preload` it without owning a node. `SourceForecast` is
## reached by its global `class_name`, never preloaded into a const: that would shadow the global and
## refuse to load.

## The plant web's rung, off the pair of bools a patch row carries.
static func patch_rung_key(tended: bool, field: bool) -> String:
	if field:
		return SourceForecast.RUNG_KEY_FIELD
	return SourceForecast.RUNG_KEY_TENDED if tended else SourceForecast.RUNG_KEY_WILD_PLANT

## The animal web's rung, off the meter and the flag a herd row carries. `domestication` is compared
## against `DOMESTICATION_COMPLETE` for the same reason the sim stamps `animal:pastoral` there: taming
## has no bool of its own, its achievement IS its meter.
static func herd_rung_key(domestication: float, corralled: bool) -> String:
	if corralled:
		return SourceForecast.RUNG_KEY_PEN
	if domestication >= SourceForecast.DOMESTICATION_COMPLETE:
		return SourceForecast.RUNG_KEY_PASTORAL
	return SourceForecast.RUNG_KEY_WILD_ANIMAL

## **STAMP A PATCH DICT WITH THE RUNG ITS OWN FLAGS IMPLY**, and hand it back for chaining. This is
## the form to reach for after MUTATING a fixture: a chapter that flips `is_cultivated` on a copy of a
## base fixture and forgets the rung has silently built nothing, and re-deriving from the dict cannot
## disagree with it. An absent flag reads `false`, exactly as the decoder's own default does.
##
## The flag names come from `SourceForecast`'s own tables rather than being typed here, so the harness
## cannot stamp a rung off a key the client has stopped publishing.
##
## `prefix` is `""` for a bare wire row and `patch_` for a `tile_info` cross-ref — the SAME prefix the
## reader will spell, which is why it is one argument and not a second function.
static func stamp_patch(patch: Dictionary, prefix: String = "") -> Dictionary:
	patch[prefix + SourceForecast.FORECAST_CURRENT_RUNG_KEY] = patch_rung_key(
		bool(patch.get(prefix + String(SourceForecast.FORECAST_DONE_FLAG_KEYS[
			SourceForecast.IMPROVEMENT_CULTIVATE]), false)),
		bool(patch.get(prefix + String(SourceForecast.FORECAST_DONE_FLAG_KEYS[
			SourceForecast.IMPROVEMENT_SOW]), false)))
	return patch

## The animal twin. Herd rows are bare-keyed everywhere in this tree (a herd has no `tile_info`
## cross-ref), so the prefix is offered for symmetry and is `""` in every call today.
static func stamp_herd(herd: Dictionary, prefix: String = "") -> Dictionary:
	herd[prefix + SourceForecast.FORECAST_CURRENT_RUNG_KEY] = herd_rung_key(
		float(herd.get(prefix + String(SourceForecast.FORECAST_BUILD_METER_KEYS[
			SourceForecast.IMPROVEMENT_TAME]), 0.0)),
		bool(herd.get(prefix + String(SourceForecast.FORECAST_DONE_FLAG_KEYS[
			SourceForecast.IMPROVEMENT_CORRAL]), false)))
	return herd

## **THE WHOLE-ARRAY FORMS**, for a fixture function that returns its rows straight out of a literal:
## `return RUNG_FX.stamp_patches([...])` stamps every row and hands the same array back. They exist so
## a fixture's rung rides its RETURN — one place per function, which is one place per function to
## forget — rather than being spelled into each row beside the flags it must agree with.
##
## Stamp AFTER any mutation. A row whose flags are flipped by a caller must be re-stamped there
## (`stamp_patch` / `stamp_herd` on the row), or it carries the rung it had before the edit.
static func stamp_patches(patches: Array, prefix: String = "") -> Array:
	for row in patches:
		stamp_patch(row, prefix)
	return patches

static func stamp_herds(herds: Array, prefix: String = "") -> Array:
	for row in herds:
		stamp_herd(row, prefix)
	return herds
