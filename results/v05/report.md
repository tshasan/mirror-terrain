# Mirror Terrain scorecard (rules v0.5)

Criteria C1-C11 of `docs/SPEC.md` and the match checks of `docs/METRICS.md`. 400 games per matchup, seat-swapped. A 95% CI follows each rate. *pending* = the simulator cannot measure it yet. *reported* = measured, no bar.

**4 of 9 scored criteria pass; 6 of 9 match checks pass.**

## Criteria

| # | Claim | Measured as | Result | Target | Verdict | Note |
| --- | --- | --- | --- | --- | --- | --- |
| C1 | The base is the hook | `shaper` vs `no_shape` | 68.5% ±4.6 | ≥ 60% | PASS |  |
| C2 | Good shaping beats any shaping | `shaper` vs `rand_shape` | 59.8% ±4.8 | ≥ 60% | **FAIL** | the 60% bar is inside the CI; re-run on another seed range |
| C3 | The attack is a read | `shaper` vs `shaper_randcross` (random crossing); rounds with no pure crossing equilibrium (`search` self-play) | 63.2% ±4.7 and 52.9% | ≥ 60% and ≤ 10% | **FAIL** | `search` vs `search_randcross` 62.3% ±4.8; guess gap 3.56 eval units; big guesses (≥ 4 core HP) 26.1% of rounds; interaction share 53.2% |
| C4 | Players decide the game | share of outcome variance fixed by the deal | 26.3% | ≤ 15% | **FAIL** | 200 deals x 10 replays; 28.0% of deals go 90%+ to one seat; first choices collide in 48.3% of drafts |
| C5 | There is depth | ladder steps won by ≥ 60% (greedy > random, search > greedy, shaper > search) | 3 of 3 (62.7% ±4.7, 61.8% ±4.8, 62.7% ±4.7) | ≥ 4 | pending | the ladder has three rungs; the fourth (a two-round lookahead) is not built |
| C6 | Scars follow from decisions | `shaper` vs `shaper_scarblind` (the same bot deciding on the board without its scars) | 53.2% ±4.9 | ≥ 55% | **FAIL** | the 55% bar is inside the CI; re-run on another seed range |
| C6b | Scars happen, but not everywhere | scars per game; rounds that leave at least one (`shaper` self-play) | 2.2 and 41.0% | 2-6; 40-80% | PASS | `search` self-play: 1.7 and 34.2% |
| C6c | Scars change play | share of crossings that repeat last round's column | 13.0% (`shaper`), 13.4% (`search`) | lower with scars than without | reported | scars are part of the rules, so there is no scars-off run to compare with |
| C7 | It is not solitaire | `shaper` vs `shaper_fixed` (units and building kinds from `default_build`, blind to the opponent) | 57.8% ±4.8 | ≥ 55% | PASS | the 55% bar is inside the CI; re-run on another seed range |
| C8 | Defending is viable but not dominant | `persona_rush` vs `persona_fort`; `persona_counter` vs `persona_rush` | 23.5% ±4.2 and 44.0% ±4.9 | ≤ 65%; 40-60% | PASS | defender not dominant: `persona_fort` vs `shaper` 45.2% ±4.9, vs `search` 60.8% ±4.8; the cheap defender, `persona_rush` vs `persona_turtle`: 71.0% ±4.4 |
| C9 | The tension sits on decisions | per-phase movement of win probability | - | build + attack ≥ 50%; deal ≤ 15% | pending | the tension curve is not built |
| C10 | The feel checks hold | close finish, wire-to-wire, lead changes, comeback in `search` self-play, each against its bar | outside: close_finish | none outside its bar by more than its CI | **FAIL** |  |
| C11 | A round fits a phone | decisions per round; legal ops per shaping decision (`shaper` self-play) | 5.1 and 284 | reported | reported | judged in playtests |

## Match checks (`search` self-play unless named)

| Check | Result | Bar | Verdict |
| --- | --- | --- | --- |
| Seat bias (`search` mirror) | 51.2% seat A | 50% ±4.9 (95% CI at n=400) | PASS |
| Seat bias (`shaper` mirror) | 56.0% seat A | 50% ±4.9 (95% CI at n=400) | **FAIL** |
| Match length (median rounds) | 5 | 3-4 | **FAIL** |
| Games reaching round 5 | 55.8% | < 10% | **FAIL** |
| Median first core damage | round 1 | ≤ round 2 | PASS |
| Max single-op share of TP | 18.8% | ≤ 50% | PASS |
| Max single-op share of op count | 24.1% | ≤ 50% | PASS |
| Dead ops (< 2% of ops) | none | none | PASS |
| Counterfactual self-check failures | 0 | 0 | PASS |

## Head-to-head

| Matchup | Win rate of the first bot | Games |
| --- | --- | --- |
| `greedy` vs `random` | 62.7% ±4.7 | 400 |
| `search` vs `greedy` | 61.8% ±4.8 | 400 |
| `shaper` vs `search` | 62.7% ±4.7 | 400 |
| `shaper` vs `no_shape` | 68.5% ±4.6 | 400 |
| `shape_only` vs `no_shape` | 61.5% ±4.8 | 400 |
| `place_only` vs `no_shape` | 54.8% ±4.9 | 400 |
| `shaper` vs `rand_shape` | 59.8% ±4.8 | 400 |
| `shaper` vs `shaper_randcross` | 63.2% ±4.7 | 400 |
| `search` vs `search_randcross` | 62.3% ±4.8 | 400 |
| `shaper` vs `shaper_scarblind` | 53.2% ±4.9 | 400 |
| `shaper` vs `shaper_fixed` | 57.8% ±4.8 | 400 |
| `persona_rush` vs `persona_fort` | 23.5% ±4.2 | 400 |
| `persona_counter` vs `persona_rush` | 44.0% ±4.9 | 400 |
| `persona_rush` vs `persona_turtle` | 71.0% ±4.4 | 400 |
| `persona_fort` vs `shaper` | 45.2% ±4.9 | 400 |
| `persona_fort` vs `search` | 60.8% ±4.8 | 400 |

## Where the C1 edge comes from

All five bots share one build search; they differ in shaping and in building placement.

| Matchup | Shaping | Placement | Win rate |
| --- | --- | --- | --- |
| `shaper` vs `no_shape` (C1) | searched vs none | searched vs default | 68.5% ±4.6 |
| `shape_only` vs `no_shape` | searched vs none | default both | 61.5% ±4.8 |
| `place_only` vs `no_shape` | none both | searched vs default | 54.8% ±4.9 |
| `shaper` vs `rand_shape` (C2) | searched vs random | searched both | 59.8% ±4.8 |
| `shaper` vs `search` | searched vs sampled | searched vs default | 62.7% ±4.7 |

## Shaping

| | `shaper` self-play | `search` self-play |
| --- | --- | --- |
| Shaping decisions where no op helped | 44.0% of 3690 | - |
| TP used | 43.6% | 89.0% |
| Dead ops (< 2% of ops) | flood, expose | none |
| Blocking ops reverted for path safety | 0.0% | 0.0% |

| Op | Share of ops, `shaper` | Share of ops, `search` | Share of TP, `search` |
| --- | --- | --- | --- |
| raise | 26.1% | 23.3% | 18.0% |
| lower | 18.5% | 24.1% | 18.6% |
| wall | 34.9% | 12.1% | 18.8% |
| flood | 1.1% | 11.8% | 18.2% |
| grow | 12.8% | 22.4% | 17.3% |
| clear | 5.5% | 3.5% | 2.7% |
| expose | 1.1% | 2.8% | 6.5% |

One applied op moves the round by 0.39 core HP on average (5903 ops, `search` self-play, the op removed and everything else replayed).

## The crossing (`search` self-play, 1787 rounds)

Each round, all 81 pairs of crossings are replayed on clones with everything else as played.

| | |
| --- | --- |
| Rounds with no pure equilibrium (a guess) | 52.9% |
| Mean guess gap (minimax - maximin, eval units) | 3.56 |
| Big guesses (gap ≥ 4 core HP) | 26.1% |
| Mean swing over the 81 cells (core HP) | 11.0 |
| Interaction share of the core-HP matrix | 53.2% |
| Crossing repeats last round's column | 13.4% (`search`), 13.0% (`shaper`) |

## C10: feel (`search` self-play)

| Metric | Result | Bar | Verdict |
| --- | --- | --- | --- |
| Close finish (final core margin ≤ 4) | 24.8% ±4.2 | 30-50% | **outside the bar** |
| Wire-to-wire wins | 31.5% ±4.6 | 15-40% | ok |
| Lead changes per game | 0.65 ±0.08 | 0.4-1.0 | ok |
| Comeback (behind after R2, still wins) | 35.6% ±4.8 | 20-35% | outside the bar, within its CI |

Reported: games decided on HP 22.8% (knockout 77.2%); live going into the last round 35.8%.

## C11: one round for one player (`shaper` self-play)

| Decision | Per round | Options |
| --- | --- | --- |
| Draft | 1 | a ranked order of 3 cards |
| Shaping ops | 0.78 | 284 legal (tile, op) choices |
| Buildings placed | 0.17 | own build zone |
| Units bought | 2.11 | unlocked kinds |
| Crossing | 1 | 9 columns |
| **Total** | **5.1** | |

## Commanders (`search` self-play, bot style `own`)

| Commander | Points | Games |
| --- | --- | --- |
| skywing | 42.7% ±6.4 | 232 |
| ironclad | 66.5% ±7.0 | 176 |
| warden | 46.3% ±6.7 | 214 |
| geomancer | 47.8% ±7.3 | 178 |

A tripwire only: both seats of a seed share its commander pair, so the CI is optimistic. `mt_sim commanders` is the precise tool.

## Kill criteria (`docs/SPEC.md`; decide at 1,600 games)

- C1 below 55%: clear (68.5% ±4.6).
- Crossing guess share above 25%: **would trip** (52.9%).
- Rush vs the defender outside 35-65%: **would trip** (23.5% ±4.2).

Total runtime: 114s.
