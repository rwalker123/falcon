# Plan: Battles — an attack resolves in the attacker's turn

Status: **design, early.** The spec for [#850](https://github.com/rwalker123/falcon/issues/850). Parent arc: [#500](https://github.com/rwalker123/falcon/issues/500) (Combat
engine). Raised while designing labor at scale (`docs/plan_labor_at_scale.md`), which leaves *who
defends* to this doc.

## Why this doc exists

Combat today is a single resolution. A predator raid is one call to `combat::resolve_fight` per turn
the pack stays within `predators.raid_radius`, applied to the band side only: the pack takes no
losses and never breaks off. Nobody chooses the defence — the defenders are whatever the Warrior role
is staffed with.

Labor at scale retires the standing Warrior count, so the defence has to be chosen somewhere else. The
answer is a battle with its own flow, the way most 4X games run one.

## Decided

### An attack resolves in the attacker's turn

The attacker attacks during their own turn. A **battle panel** opens and the battle plays out as a
series of combat **rounds**. If it is not resolved after a preset round limit, it **carries over** to
the next turn.

### Both sides take casualties, and either side can end it

The fight is ordinary combat, resolved by the combat system, and both sides take losses. Each side
ends the battle in one of three ways:

- **Routed** — forced to retreat by too much damage;
- **Eliminated**;
- **Retreats** — chooses to break off.

The attacker and the defender have the same three outcomes.

### The defender sees the attack and picks the defence

When a band's camp is attacked, the panel explains the attacker — who it is, its count, its strength —
and shows what the band can field: its workers in camp and the weapons they would carry. The defender
picks how many defend. A work party out past the camp's range is not in camp and cannot defend.
Defenders leave their work while the battle runs and go back to it afterwards.

**A predator pack and another faction's raiders use the same flow.**

### What must survive from the combat engine

From #500, and not up for change here:

- **The gate is a hard subtraction**: `max(0, attack − defense)` is exactly zero below the gate.
- **`resolve_fight` is pure** and deterministic from its seed; callers describe composition
  (contingents), never a single power number.
- **A side is several contingents**, each gated on its own and carrying its own casualties.
- **Damage carries between turns** on `combat::DamageLedger` — which is what a battle carried over
  into the next turn needs.

## Open questions

1. **When a battle resolves.** Today every order is collected and the whole turn is resolved after End
   Turn. A battle that plays out inside the attacker's turn means a battle can be resolved the moment
   it is ordered. How does that fit the turn loop — and with several seats, and AI factions taking
   their turns?
2. **The absent defender.** The defender is not at the controls during the attacker's turn — another
   player, an AI, or the player while a rival attacks. Their side of the panel needs an answer set in
   advance: a defence stance, or rules for who musters. What is it?
3. **Animals have no turn.** A predator attack has no attacker's turn to resolve in. Does it resolve at
   the start of the defending player's turn, so the player sees the panel when their turn begins?
4. **Rounds.** How many rounds before a battle carries over, and what one round is in
   `resolve_fight` terms.
5. **Rout.** How much damage routs a side, and does a routed pack stay away for some turns?

## Related

- [#500](https://github.com/rwalker123/falcon/issues/500) — Combat engine (the arc)
- [#501](https://github.com/rwalker123/falcon/issues/501) — combined arms: ranged pre-phase, wounds, hit chance
- [#365](https://github.com/rwalker123/falcon/issues/365) — emergent predator raids (when a raid starts)
- [#369](https://github.com/rwalker123/falcon/issues/369) — rival raiders reuse the casualty interface
- [#851](https://github.com/rwalker123/falcon/issues/851) — predator raids damage only the band (held for this design)
- `docs/plan_labor_at_scale.md` — retires the standing Warrior count
- `docs/plan_predators.md`, `docs/plan_hunt_through_combat.md` §4 — the raid and the fight as built
- `.claude/rules/core_sim/combat.md` — as-built combat
