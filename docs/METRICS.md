# What "good" means: the scorecard and how to read it

`mt_sim suite` (or `make suite`) measures everything in this file and writes `report.md`, `report.json` and
five replays. `mt_sim quick` prints the same JSON as one line and writes nothing. The current values are in
`results/v06/report.md`; this file defines the measurements and gives the reason for each bar. Bars are
design decisions: change one only with a written reason.

Every head-to-head is seat-swapped: each seed is played twice, and the two bots (each keeping its own bot
seed) trade seats for the second game. That holds for self-play too, so a mirror gives two different games
per deal. Every matchup
has its own seed range (`mt_sim/src/scorecard.rs`, `seeds`), so a re-run at the same game count reproduces
the numbers exactly.

## Criteria (C1-C11)

The claims and targets come from `docs/SPEC.md`. Each criterion gets one verdict: **pass** or **fail**;
**pending** when the simulator cannot measure it yet; **reported** when it has no bar.

| # | Measured as | Bar |
| --- | --- | --- |
| C1 | `shaper` vs `no_shape`. Split reported: `shape_only` vs `no_shape` (ops alone) and `place_only` vs `no_shape` (placement alone). | ≥ 60% |
| C2 | `shaper` vs `rand_shape`. | ≥ 60% |
| C3 | `shaper` vs `shaper_randcross` (same bot, uniformly random crossing), and the crossing guess share from the `search` self-play probe (below). Both must hold. | ≥ 60% and ≤ 10% |
| C4 | Share of outcome variance fixed by the deal (below). | ≤ 15% |
| C5 | Ladder steps won by ≥ 60%: greedy > random, search > greedy, `shaper` > search. Three rungs exist, so this is **pending** until a fourth is built, and a **fail** if any of the three is lost. | ≥ 4 |
| C6 | `shaper` vs `shaper_scarblind`: the same bot, making every decision on the board with its scars put back to the terrain underneath. | ≥ 55% |
| C6b | Scars per game and the share of rounds that leave at least one, in `shaper` self-play. | 2-6; 40-80% |
| C6c | Share of crossings (from round 2) that repeat the previous round's column. **Reported**: scars are part of the rules, so there is no scars-off run to compare with. | none |
| C7 | `shaper` vs `shaper_fixed`: the same bot, but its units and building kinds come from `default_build`, which never looks at the opponent. | ≥ 55% |
| C8 | `persona_rush` vs `persona_fort` and `persona_counter` vs `persona_rush`. Reported next to it: `persona_fort` vs `shaper` and vs `search` (is the defender dominant?), and `persona_rush` vs the cheap defender `persona_turtle`. | ≤ 65%; 40-60% |
| C9 | The tension curve. **Pending**: not built. | build + attack ≥ 50%; deal ≤ 15% |
| C10 | The four feel metrics below, in `search` self-play. Fails when any is outside its bar by more than its own 95% CI. | see below |
| C11 | `shaper` self-play: decisions per round for one player (draft + ops + buildings + units + crossing) and legal ops per shaping decision. **Reported.** | none |

### C10: feel

| Metric | Bar | Why |
| --- | --- | --- |
| Close finish (final core margin ≤ 4) | 30-50% | Close losses pull players back, when they had control. |
| Wire-to-wire wins (the winner led after every round) | 15-40% | Below: early rounds don't matter. Above: decided early. |
| Lead changes per game | 0.4-1.0 | Plot twists that get rarer and bigger. |
| Comeback (behind after round 2, still wins) | 20-35% | Early play matters but isn't decisive. |

A metric outside its bar but within its CI of it is flagged in the report and does not fail the criterion:
at 400 games these rates move by several points between seed ranges.

### C4: the deal share

A deal is a game seed (map, commanders, offers). For each of D deals, play R `search` vs `search` games that
differ only in the bots' RNG seeds; no seat swap. Outcome = seat A's points (1 or 0). *within* = mean over
deals of the unbiased variance of that deal's outcomes. *between* = max(0, unbiased variance of the deal
means − within / R). **share = between / (between + within).** Also reported: the share of deals where one
seat scores ≥ 90%, and how often the two first choices collide in the draft. `mt_sim deals <deals> <replays>
[seed]`; the scorecard uses games/2 deals and 10 replays. It is the one-way random-effects estimator:
unbiased but noisy at 200 deals.

### C3: the crossing probe

In `search` self-play, each round, all 81 pairs of crossings are replayed on clones with everything else as
played (true armies), payoff `mt_bots::eval` for seat A.

- **Guess share**: rounds with no pure equilibrium (maximin < minimax). A guess round has no right answer.
- **Guess gap**: minimax − maximin in `eval` units, averaged over all rounds: how much a wrong guess is worth.
- **Big-guess share**: rounds whose core-HP-difference matrix has a gap of at least 4 core HP. The guess
  share says how often a guess is needed; this says how often it matters.
- **Swing**: max − min core-HP difference over the 81 cells.
- **Interaction share**: the row x column interaction sum of squares of the core-HP matrix over its total sum
  of squares (two-way decomposition, both sums accumulated over rounds). A high value means the outcome
  depends on whether the armies meet, which no single-sided reading explains.
- **Op value**: the absolute change in the round's core-HP difference when one applied op is removed and
  everything else is replayed.

Probes run on `sim_clone()`s, never call a bot and never draw from an RNG. `mt_sim verify` plays the same
seed with probes off and on and fails unless the game is identical, and every probed round is replayed
unchanged as a self-check.

## Match checks

Scored in `search` self-play unless named.

| Group | Check | Bar | Why |
| --- | --- | --- | --- |
| Fairness | Seat bias, `search` mirror and `shaper` mirror | 50% within the 95% CI | Seat A must not win more. `symcheck` is the stricter test. |
| Pacing | Median rounds | 3-4 | Session length on mobile. |
| Pacing | Games reaching round 5 | < 10% | Most matches should end by knockout. See "Open questions". |
| Pacing | Median first core damage | ≤ round 2 | Something happens early. |
| Decisions | Max single-op share of TP | ≤ 50% | No dominant op. |
| Decisions | Max single-op share of op count | ≤ 50% | Same, by count. |
| Decisions | Dead ops (< 2% of ops) | none | Every op should earn its place. |
| Health | Counterfactual self-check failures | 0 | A probe that cannot reproduce the round it measures is wrong. |

Reported without a bar: the share of games decided on HP rather than knockout, the share still live going
into the last round, TP used, blocking ops reverted for path safety, the share of shaping decisions where
`shaper` found no helpful op, and per-commander points in `search` self-play.

## The bots

`mt_sim help` lists every name. All the `shaper` family and the controls are one bot (`SearchBot`) with
switches, so two of them differ only in the decision named. Controls draw their random choices from a second
RNG stream, so their main stream matches the bot they are compared with.

| Bot | Shaping | Build | Crossing |
| --- | --- | --- | --- |
| `random` | random legal ops | random | random |
| `greedy` | best of 8 sampled plans by one rollout | best default build by one rollout | heuristic (`default_crossing`) |
| `search` | sampled plans for both sides, plan game solved by regret matching | default builds for both sides, build game solved | searched |
| `shaper` | `deep_plan`: greedy over every legal op, screened, then valued by the solved crossing game | as `search`, then `searched_placement` | searched |
| `shape_only` / `place_only` / `no_shape` | `deep_plan` / none / none | default placement / searched / default | searched |
| `rand_shape` | random legal ops | as `shaper` | searched |
| `shaper_randcross` | as `shaper` | as `shaper` | uniformly random |
| `shaper_fixed` | as `shaper` | units and kinds from `default_build`, tiles searched | searched |
| `shaper_scarblind` | as `shaper`, on the unscarred board | same | same |
| `persona_rush` / `_turtle` / `_air` / `_ground` | as `greedy` | the style's default build | heuristic |
| `persona_counter` | turtle in rounds 1-2, then rush | same | heuristic |
| `persona_fort` | `deep_plan` with the turtle style | turtle build, searched placement | searched |

**How the searches see the game.** `searched_crossing` reads only `Game::reveal_view(side)`: both bases and
my own army, with the opponent's units bought this round removed and refunded. It models the hidden purchase
as the opponent's default units, builds the 9x9 crossing matrix and samples the regret-matching mix.
`deep_plan` values a board by the game whose rows are my crossings restricted to three gates (columns 1, 4,
7) and whose columns are all nine of the opponent's crossings, because the attacker chooses after seeing the
base. `searched_placement` moves each building to the legal tile with the best one-round `eval`, averaged
over all nine opponent crossings.

**Every bot is fast.** Both shaper searches screen their candidates before paying for a full evaluation
(`mt_bots/src/fast.rs`): `deep_plan` scores every op on the three cells where the armies meet and completes
the 3x9 matrix only for the 24 ops that moved those cells most; `searched_placement` scores every tile on
the three worst opponent crossings and completes the best three. The unscreened searches were about 7 times
slower for 0.03 core-HP-equivalents per decision, and are gone. A `shaper` game takes about 0.2 s on one
core, against 0.02 s for `search`; a 400-game suite takes about 2 minutes on 12 cores.

**Limits.** C7 is only as strong as `default_build` is weak: `shaper_fixed` still chooses placement and
crossing, so it isolates the unit and building-kind choice. The searched crossing's model of the hidden army
slightly under-buys (the default build also plans buildings). C8 is a bar between two personas; the
"defender not dominant" rows show whether the fort is strong in absolute terms.

## Open questions

- **Round 5.** The "< 10% of games reach round 5" bar is old. The share of games decided on HP rather than
  knockout may be the better measure; it is reported so the decision can be made on data.
- **C6c** has no bar while scars cannot be switched off. A scars-off run needs a temporary code change.

## Reading the numbers

- A 400-game win rate has a 95% CI of about ±4.9 points. At 1,600 games it's about ±2.5.
- Pooled self-play rates (comeback, HP-decided share) vary across seed ranges more than the printed binomial
  CI says, because the two seat-swapped games of a seed share a deal. Re-run a borderline value on other seed
  ranges with `mt_sim selfplay <games> <seed>`.
- Bots are proxies. A check passing means the *bots* don't find a problem. Human playtests decide fun,
  readability and whether a round fits in 90 seconds.

- **Commander balance (proposed bar, not in the scorecard).** Every commander 48-52% overall and every pairing 45-55%
  in `mt_sim commanders 1600`, with `MT_STYLE=balanced` and with each commander's own style. Overall rows have a 95% CI
  of about +/-1.4 at 1600 games per pair.
