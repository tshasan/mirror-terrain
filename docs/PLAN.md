# Plan: from v0.6 to v0.7

`docs/SPEC.md` is the target design (v0.7), `docs/RULES.md` is what the simulator runs (v0.6), and
`docs/METRICS.md` defines the scorecard. This file is the only "what next" list in the project.

## Where things stand

- The simulator runs v0.6: own-half shaping, a reveal, a crossing column, scars, a contested draft, corner
  cores and a 12-round backstop.
- The baseline is `results/v06/` (400 games per matchup). Read `report.md` there before choosing a step.
- v0.6 problems the spec answers: one op is worth about 0.4 core HP; 47.5% of rounds have no right crossing;
  rush beats the fort 8.8% of the time; losing round 1 snowballs.

## Rules for every step

- Change one thing per run. Compare against the last kept step at the same game count and seeds.
- Iterate in a scratch copy with the fast loop (`mt_sim selfplay 400 <seed> search full` and one or two
  `match` runs, about 10 seconds); run the full suite only on a step that is about to be kept.
- Check a gate at 1,600 games.
- Log every kept or rejected run in `results/experiments.jsonl`. Never move a target to pass.
- If a step fails its "done when" after the listed tries, log the rejections and stop for a decision.

## Steps

**0. Measure first.** Add to the fast loop, as reported values: op value (C12), empty shaping decisions (C13),
unit purchase shares (C14), round-1 loser wins (C15). They exist as probes or counters in part; none changes a
game. *Done when:* the v0.6 numbers for all four are logged and `mt_sim verify` passes.

**1. Automatic routing.** Units appear around their own core and take the shortest path to the enemy core;
the crossing, the attack-plan phase and the bots' crossing search are removed. C3 is retired in the scorecard.
*Done when:* `make check` passes with symcheck at 0, the bots are no slower than in v0.6, and C12, C13, C7 and
the share of rounds fought on a changed route are logged.

**2. Armies come back.** Every owned unit fights again next round. *Done when:* C15 is inside 20-35% and the
building-heavy persona does not beat the army-heavy one more than 65% (the earlier respawn rule failed
exactly there).

**3. Growing income.** 6 ore, +2 per completed round. Try +1 and +3 if +2 is off. Report how often the
8-building and 20-unit limits bind. *Done when:* 90% of games end by round 8 and C10 holds.

**4. Every kind buyable; cards become bonuses.** Unit and building cards give 1 ore off their kind. One
pass on unit costs so that C14 holds and each unit has a counter the bots actually use. *Done when:* C14 and
C7 pass.

**5. Own-offer draft.** Three cards each, pick one. *Done when:* C4 is no worse than v0.6 past its CI; if it
is, try the shared offer with duplicates allowed.

**6. Bigger ops.** Only if C12 is still under 1.5 core HP: an op changes two or three connected tiles. *Done
when:* C12 and C13 pass and no op is above 50% of ops.

**7. Fewer hidden numbers.** Remove the per-round breach multiplier, then the home-turf bonus, one at a time.
*Done when:* pacing and C10 hold without each; keep whichever is still needed.

**8. One targeting rule.** Units keep to their route and stop only for what blocks or shoots them. *Done
when:* no replay in a 20-game sample shows a unit leaving its route.

**9. Demo.** Routes, ranges and stakes drawn in `mt_game`; the commander shown as a start card. No rules.

**10. Baseline and playtest.** Bump `VERSION` to 0.7, update `docs/RULES.md`, `docs/METRICS.md` and
`docs/RESEARCH.md`, run `results/v07`, then five human playtests with the questions in `docs/SPEC.md`.

## Open decisions

These need the human.

- **The proposed bars** in `docs/SPEC.md` (C8 two-sided, C11 to C15, pacing).
- **Spawn point.** Step 1 uses the core. A gate building the player places is the alternative if routes feel
  the same every round.
- **The three cards that only pay ore.** Kept for now; a redesign fits in step 4.
- **Path safety reverts both sides.** A per-player rule is possible now that each player shapes only their own
  half, and matters more once routing depends on it.
- **Who takes damage.** Kept as "units that reach your core". Mechabellum's "only the round's loser takes
  damage" is the alternative if rounds are hard to read.
- **Clash inputs.** Still parked until playtests.
