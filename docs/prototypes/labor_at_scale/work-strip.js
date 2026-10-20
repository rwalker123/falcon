// The work strip: one control, used by the Work tab row and the source's sheet alike.
// Left to right: the standing rung's mark, the ⌃ button (or the build in flight, filling) and the
// "Take it to…" card it opens, the floor preset's glyph, ⚠ when the source is short, then the crew
// number with the crew bar under it.
(function () {
  const CSS = `
.wstrip{display:flex;align-items:flex-start;gap:10px}
.marks{display:flex;align-items:flex-start;gap:6px;padding-top:1px;position:relative}
.mk{font-size:13px;color:#bdab90;min-width:12px;text-align:center}
.warnmk{color:#e0a63c}
.ladder{height:22px;min-width:46px;border:1px solid #4a3c2e;background:#14100d;color:#f4ead7;border-radius:4px;font-size:12px;position:relative;overflow:hidden;padding:0 6px;font-family:inherit;cursor:pointer}
.ladder:hover{border-color:#a08d6d}
.ladder.building{border-color:#94a361}
.lfill{position:absolute;left:0;top:0;bottom:0;background:#4b5530}
.ltxt{position:relative}
.rcard{position:absolute;right:0;top:28px;width:280px;z-index:5;background:#1d1812;border:1px solid #3a2f24;border-radius:6px;padding:8px 10px;display:flex;flex-direction:column;gap:4px;box-shadow:0 8px 24px rgba(0,0,0,.6);text-align:left}
.rcard.left{right:auto;left:0}
.rclbl{font-size:10px;letter-spacing:.06em;text-transform:uppercase;color:#8d7e68}
.rmark{display:flex;justify-content:space-between;font-size:12px;color:#f4ead7;padding:2px}
.rmark.banked{color:#8d7e68}
.rmf{color:#bdab90}
.rbtn{display:flex;justify-content:space-between;align-items:center;height:24px;border:1px solid #3a2f24;background:#2b221a;color:#f4ead7;border-radius:12px;font-size:12px;padding:0 10px;position:relative;overflow:hidden;font-family:inherit;cursor:pointer}
.rbtn:hover{border-color:#a08d6d}
.rbtn.building{border-color:#94a361}
.rbtn.far{opacity:.55;cursor:default}
.rfill{position:absolute;left:0;top:0;bottom:0;background:#3a4227}
.rl,.rf{position:relative}
.rf{color:#bdab90}
.crewbox{text-align:right;display:flex;flex-direction:column;align-items:flex-end;gap:3px;min-width:84px}
.crewnum{font-size:18px;color:#f4ead7}
.crewnum.zero{color:#8d7e68}
.cbar{display:flex;width:110px;height:7px;border-radius:2px;overflow:hidden;background:#2f261d;cursor:help}
.cbar span+span{border-left:2px solid #1f1913}`;
  if (typeof document !== 'undefined' && !document.getElementById('work-strip-css')) {
    const s = document.createElement('style'); s.id = 'work-strip-css'; s.textContent = CSS; document.head.appendChild(s);
  }

  const turnsWord = (t) => t + (t === 1 ? ' turn' : ' turns');
  const FLOOR_GLYPH = ['☠', '♻', '⬆'];
  const FLOOR_NAME = ['Everything', 'Best', 'Learning'];
  const KEEP = '#a08d6d', BUILD = '#6f9fc4';

  // o: {
  //   rungs: [{label, mark, verb, verbGlyph, work, cap}] | null, at, build: {q, rem} | null,
  //   builders, turns, buildTip, open, toggle(), start(), stop(),
  //   floor: 0..2 | null, warn: '' | the reason,
  //   crew: {keep, build, gear, bare}, color: the job's colour, tip: the crew bar's sentence
  // }
  function props(o) {
    const rungs = o.rungs, at = o.at, bld = o.build;
    const next = rungs ? rungs[at + 1] || null : null;
    const pct = bld ? Math.round(bld.q * 100) : 0;
    let hasLadder = false, ladderFace = '', ladderTip = '', ladderCls = '', ladderFill = '0%';
    if (bld) {
      hasLadder = true; ladderCls = 'building'; ladderFill = pct + '%';
      ladderFace = next.verbGlyph + ' ' + pct + '%';
      ladderTip = next.verb + ' ' + pct + '%' + (o.builders > 0 ? ' · ' + o.builders + ' building, done in ≈' + turnsWord(o.turns) : ' · no spare hands this turn');
    } else if (next) {
      hasLadder = true; ladderFace = '⌃' + next.verbGlyph; ladderTip = 'Take it to…';
    }
    const card = !rungs ? [] : rungs.map((r, i) => {
      const glyph = i === 0 ? '' : r.verbGlyph;
      const base = { glyph, label: r.label, fill: '0%', tip: '', click: () => {} };
      if (i < at) return Object.assign(base, { isButton: false, isMarker: true, cls: 'banked', face: 'banked' });
      if (i === at) return Object.assign(base, { isButton: false, isMarker: true, cls: '', face: 'where you are' });
      if (i === at + 1 && bld) return Object.assign(base, { isButton: true, isMarker: false, cls: 'building', fill: pct + '%',
        face: Math.round(bld.rem) + ' work left' + (o.builders > 0 ? ' · ≈' + turnsWord(o.turns) : ''),
        tip: (o.builders > 0 ? o.buildTip : 'No spare hands this turn') + ' — click to stop', click: o.stop });
      if (i === at + 1) return Object.assign(base, { isButton: true, isMarker: false, cls: '', face: r.work + ' work',
        tip: r.verb + ' — up to ' + r.cap + ' builders at once', click: o.start });
      return Object.assign(base, { isButton: true, isMarker: false, cls: 'far', face: r.work + ' work', tip: rungs[i - 1].label + ' first' });
    });
    const c = o.crew || { keep: 0, build: 0, gear: 0, bare: 0 };
    const crew = c.keep + c.build + c.gear + c.bare;
    const hasFloor = o.floor !== null && o.floor !== undefined;
    return {
      rungMark: rungs ? rungs[at].mark || '' : '', rungMarkTip: rungs && rungs[at].mark ? rungs[at].label : '',
      hasLadder, ladderFace, ladderTip, ladderCls, ladderFill, ladderClick: o.toggle || (() => {}),
      card, cardOpen: !!o.open && hasLadder,
      policyMark: hasFloor ? FLOOR_GLYPH[o.floor] : '', policyTip: hasFloor ? FLOOR_NAME[o.floor] : '',
      warnMark: o.warn ? '⚠' : '', warnTip: o.warn || '',
      crew: String(crew), zeroCls: crew === 0 ? 'zero' : '',
      keepN: c.keep, buildN: c.build, gearN: c.gear, bareN: c.bare,
      keepC: KEEP, buildC: BUILD, gearC: o.color, bareC: o.color + '73',
      tip: crew === 0 ? 'Nobody here this turn' : o.tip
    };
  }

  const RUNGS = {
    plant: [{ label: 'Wild', mark: '' }, { label: 'Tended Patch', mark: '🌾', verb: 'Cultivate', verbGlyph: '🌱', work: 50, cap: 3, kit: 'hoes' }, { label: 'Field', mark: '▦', verb: 'Sow', verbGlyph: '▦', work: 75, cap: 4, kit: 'hoes' }],
    animal: [{ label: 'Wild', mark: '' }, { label: 'Tamed', mark: '◎', verb: 'Tame', verbGlyph: '◎', work: 50, cap: 2, kit: 'crooks' }, { label: 'Penned', mark: '🐄', verb: 'Corral', verbGlyph: '🐄', work: 75, cap: 3, kit: 'crooks' }],
    // The game's road rungs. Path and trail are worn in by traffic; the two above are built.
    route: [{ label: 'Path', mark: '┄' }, { label: 'Trail', mark: '┅', verbGlyph: '┅' }, { label: 'Dirt road', mark: '═', verb: 'Grade', verbGlyph: '═', work: 300, cap: 3, kit: 'earth' }, { label: 'Paved road', mark: '▤', verb: 'Pave', verbGlyph: '▤', work: 800, cap: 4, kit: 'earth' }],
    // Two storage branches: food (dry food only) and goods (everything else).
    foodStorage: [{ label: 'Storage pit', mark: '◫' }, { label: 'Granary', mark: '▣', verb: 'Raise a granary', verbGlyph: '▣', work: 120, cap: 4, kit: 'earth' }, { label: 'Silo', mark: '◉', verb: 'Build a silo', verbGlyph: '◉', work: 400, cap: 5, kit: 'earth' }],
    goodsStorage: [{ label: 'Bare ground', mark: '' }, { label: 'Storehouse', mark: '⌂', verb: 'Build a storehouse', verbGlyph: '⌂', work: 300, cap: 5, kit: 'earth' }, { label: 'Warehouse', mark: '▥', verb: 'Build a warehouse', verbGlyph: '▥', work: 700, cap: 6, kit: 'earth' }]
  };

  const api = { props, RUNGS };
  if (typeof window !== 'undefined') window.WorkStrip = api;
  if (typeof globalThis !== 'undefined') globalThis.WorkStrip = api;
})();
