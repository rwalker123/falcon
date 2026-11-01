# The Telling — a spoken narrator

The Telling's lines are read aloud by a local neural voice, and **the voice follows the narrator's
medium**: oral → painted → written each speak in their own voice. The design question this answers
is *"does hearing the telling make it better?"* — so the evaluation build uses the best voice that
can actually ship, not a placeholder.

See also: `docs/plan_the_telling.md` (the beat engine), `docs/plan_the_telling_book_ux.md` (the
book and its mediums), `.claude/rules/client/telling-panel.md`.

## Decisions

| Question | Decision | Why |
|---|---|---|
| Engine | **Kokoro-82M** (Apache-2.0) run through ONNX Runtime | The highest-quality voice that runs offline on the player's machine. Cloud TTS (ElevenLabs etc.) sounds better still but needs a network, an API key and per-line cost — not shippable for a game |
| Where it runs | **The client's native extension**, never the server | Narration is presentation. The server is headless and also feeds AI seats with no speakers |
| Why not pre-recorded audio | Not possible | Every telling is rendered at runtime with live nouns ("Past the ridge, Verdant Basin") — the line does not exist until the beat fires |
| Voice per telling | **One voice per medium**, not random per line | One narrator is the conceit. Random voices read as four strangers; tying the voice to the medium makes the narrator *maturing* audible, which is the medium arc's whole payoff |
| When painted/written speak | **When the page comes into view** | The book never turns its own page in painted/written (the "visible page never moves on its own" rule); speaking a line the player cannot see breaks it. Oral pins to newest, so its arrival IS its showing |
| Phonemizer | **espeak-ng loaded at runtime, optional** for the evaluation build | Kokoro needs IPA phonemes. espeak-ng is GPL-3, so it is never linked or bundled: it is `dlopen`ed if present, and narration is simply off if it is not |

## The opening voice table

A lever, judged by ear in playtest — not a gate on the build.

| Medium | Voice | Speed |
|---|---|---|
| `oral` | `bm_george` (older British male) | 0.85 |
| `painted` | `am_michael` (American male) | 0.90 |
| `written` | `af_heart` (American female) | 0.95 |

Unknown medium ids fall back to `oral`, the same table-with-fallback rule as `MEDIUM_STYLES`.

## What is spoken

- **The Telling panel** — the beats of the page that just became visible: on an oral beat's
  arrival, on a leaf, on `reveal_newest` (turn advance). Never on the backfill of a full snapshot,
  a world reset, a medium-change re-render, or while the panel is collapsed.
- **The opening card** — its turn-0 lines, when it shows.
- **The fork card** — the narration in the active register (re-spoken on a register toggle), not
  the choice buttons. A fork's echo line arrives as a `narrative_fork` feed entry and is spoken by
  the panel rule above.
- **Never** the gloss (the sampled numbers).

The newest request wins: a page turn or a card opening stops the current line and starts the new
one. Lines of one page play in order with a short pause between them.

## Model files

Not in git. `scripts/fetch_narrator_model.sh` downloads the model and voices into a gitignored
directory. The default is the **int8** model (88 MB, ~2× faster than fp32 on CPU); `--fp32`
fetches the 310 MB model, which the client prefers whenever it is present. Measured on the evaluation set (16 tellings, ~2 minutes of audio per
voice, Apple Silicon CPU): fp32 ~15 s, int8 ~8 s per voice — a single telling is ready well
under a second after it is requested either way.

## Before shipping

The runtime espeak-ng dependency is an evaluation-build arrangement. The shippable replacement is
a **build-time pronunciation lexicon**: every telling is composed from authored templates and
finite name catalogs (sites, species, terrain adjectives, leader / band / faction names, numbers),
so the phonemes can be generated offline with espeak-ng as a dev tool and shipped as data, with no
GPL code in the game. Mod-authored text outside the lexicon is the open question that work owns.
Trimming the voices file to the three used voices (~0.5 MB each) and packaging the model belong to
the same pass.
