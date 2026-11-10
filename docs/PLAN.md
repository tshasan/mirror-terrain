# Plan: from v0.5 to a playtest

`docs/SPEC.md` is the design, `docs/RULES.md` is what the simulator runs (v0.5), and `docs/METRICS.md`
defines the scorecard. This file is the only "what next" list in the project.

## Where things stand

- The simulator runs one rule set: own-half shaping, a reveal before the attack, a crossing column, scars
  and a contested draft. There are no rule flags.
- The baseline is `results/v05/` (400 games per matchup). Read `report.md` there before choosing a step.

## Rules for every step

- Change one thing per run. Compare against `results/v05/report.json` at the same game count.
- Check a gate at 1,600 games.
- Log every kept or rejected run in `results/experiments.jsonl`. Never move a target to pass.
- If a step fails its "done when" after the listed tries, log the rejections and stop for a decision.

## Steps

**1. Read the baseline against the kill criteria** (`docs/SPEC.md`), at 1,600 games: C1 below 55%, crossing
guess share above 25%, or rush vs the defender outside 35-65%. If one trips, stop for a design conversation
before tuning anything.

**2. The crossing as a read (C3).** If the guess share is over its bar, try the spec's alternatives one at a
time: three gates instead of nine columns; a fixed centre crossing in round 1; the trailing player choosing
after the leader. *Done when:* C3 passes or all three are logged as rejected.

**3. Scars (C6, C6b).** If scars are too rare or too common, move the **[design]** defaults in the spec's
table (craters per half, minimum deaths). If C6 is under 50% and lead changes are low, report scars as
decoration and ask whether to keep them.

**4. The tension curve (C9).** Per-phase win-probability movement by rollouts on clones, as a probe
(`mt_sim/src/probes.rs`). *Done when:* C9 is scored and `mt_sim verify` still passes.

**5. Exploit search.** A parameterised policy bot (weights over unit preferences, building targets, crossing,
op preferences, draft tags) and `mt_sim evolve [generations] [population]`: a (mu + lambda) evolution
strategy with fitness against the strongest bot over a fixed seed set, deterministic RNG, best genome logged
per generation. Any genome above 65% on a fresh 400-game seed range is a balance bug: fix the rule, add the
genome as a persona, re-run.

**6. Fourth rung (C5).** A two-round lookahead on top of the strongest bot. *Done when:* it beats that bot
≥ 60%. If it can't, the game is solved one round ahead and needs longer-lived decisions; report that.

**7. Playtest.** Five human playtests through the viewer, with the questions in `docs/SPEC.md`.

## Open decisions

These need the human.

- **Three cards that only pay ore.** Grovekeeper, Sapper and Prospector each give +2 ore and nothing else,
  because the ops they unlocked are available from the start. Redesign them, or cut the pool to 12.
- **Path safety reverts both sides.** When the round's ops together cut the last path, every blocking op of
  the round is reverted, including the innocent player's. A per-player rule is possible now that each player
  shapes only their own half.
- **The round-5 bar** and **the scorecard's runtime**: see "Open questions" in `docs/METRICS.md`.
- **Commander starts.** Check `mt_sim commanders` on v0.5 before touching them: the start cards were
  unbalanced under earlier rules, and the fix that balanced them cost close finishes.
- **Clash inputs.** The original idea allowed up to 3 player inputs during a clash; the simulator has none.
  Wait for playtests.
