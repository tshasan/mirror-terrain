# Mirror Terrain scorecard (rules v0.6)

Criteria C1-C11 of `docs/SPEC.md` and the match checks of `docs/METRICS.md`. 400 games per matchup, seat-swapped. A 95% CI follows each rate. *pending* = the simulator cannot measure it yet. *reported* = measured, no bar.

**4 of 10 scored criteria pass; 7 of 9 match checks pass.**

## Criteria

| # | Claim | Measured as | Result | Target | Verdict | Note |
| --- | --- | --- | --- | --- | --- | --- |
| C1 | The base is the hook | `shaper` vs `no_shape` | 67.8% ±4.6 | ≥ 60% | PASS |  |
| C2 | Good shaping beats any shaping | `shaper` vs `rand_shape` | 57.5% ±4.8 | ≥ 60% | **FAIL** | the 60% bar is inside the CI; re-run on another seed range |
| C3 | The attack is a read | `shaper` vs `shaper_randcross` (random crossing); rounds with no pure crossing equilibrium (`search` self-play) | 63.0% ±4.7 and 47.5% | ≥ 60% and ≤ 10% | **FAIL** | `search` vs `search_randcross` 63.0% ±4.7; guess gap 2.66 eval units; big guesses (≥ 4 core HP) 15.4% of rounds; interaction share 47.8% |
| C4 | Players decide the game | share of outcome variance fixed by the deal | 17.1% | ≤ 15% | **FAIL** | 200 deals x 10 replays; 18.0% of deals go 90%+ to one seat; first choices collide in 48.4% of drafts |
| C5 | There is depth | ladder steps won by ≥ 60% (greedy > random, search > greedy, shaper > search) | 2 of 3 (65.0% ±4.7, 71.0% ±4.4, 58.8% ±4.8) | ≥ 4 | **FAIL** | the ladder has three rungs; the fourth (a two-round lookahead) is not built |
| C6 | Scars follow from decisions | `shaper` vs `shaper_scarblind` (the same bot deciding on the board without its scars) | 54.0% ±4.9 | ≥ 55% | **FAIL** | the 55% bar is inside the CI; re-run on another seed range |
| C6b | Scars happen, but not everywhere | scars per game; rounds that leave at least one (`shaper` self-play) | 3.1 and 40.5% | 2-6; 40-80% | PASS | `search` self-play: 2.3 and 35.1% |
| C6c | Scars change play | share of crossings that repeat last round's column | 13.6% (`shaper`), 14.3% (`search`) | lower with scars than without | reported | scars are part of the rules, so there is no scars-off run to compare with |
| C7 | It is not solitaire | `shaper` vs `shaper_fixed` (units and building kinds from `default_build`, blind to the opponent) | 60.8% ±4.8 | ≥ 55% | PASS |  |
| C8 | Defending is viable but not dominant | `persona_rush` vs `persona_fort`; `persona_counter` vs `persona_rush` | 8.8% ±2.8 and 62.5% ±4.7 | ≤ 65%; 40-60% | **FAIL** | defender not dominant: `persona_fort` vs `shaper` 52.5% ±4.9, vs `search` 66.5% ±4.6; the cheap defender, `persona_rush` vs `persona_turtle`: 32.2% ±4.6 |
| C9 | The tension sits on decisions | per-phase movement of win probability | - | build + attack ≥ 50%; deal ≤ 15% | pending | the tension curve is not built |
| C10 | The feel checks hold | close finish, wire-to-wire, lead changes, comeback in `search` self-play, each against its bar | all inside | none outside its bar by more than its CI | PASS |  |
| C11 | A round fits a phone | decisions per round; legal ops per shaping decision (`shaper` self-play) | 4.8 and 278 | reported | reported | judged in playtests |

## Match checks (`search` self-play unless named)

| Check | Result | Bar | Verdict |
| --- | --- | --- | --- |
| Seat bias (`search` mirror) | 51.0% seat A | 50% ±4.9 (95% CI at n=400) | PASS |
| Seat bias (`shaper` mirror) | 50.5% seat A | 50% ±4.9 (95% CI at n=400) | PASS |
| Match length (median rounds) | 5 | 3-4 | **FAIL** |
| Games reaching round 5 | 61.0% | < 10% | **FAIL** |
| Median first core damage | round 1 | ≤ round 2 | PASS |
| Max single-op share of TP | 19.6% | ≤ 50% | PASS |
| Max single-op share of op count | 23.5% | ≤ 50% | PASS |
| Dead ops (< 2% of ops) | none | none | PASS |
| Counterfactual self-check failures | 0 | 0 | PASS |

## Head-to-head

| Matchup | Win rate of the first bot | Games |
| --- | --- | --- |
| `greedy` vs `random` | 65.0% ±4.7 | 400 |
| `search` vs `greedy` | 71.0% ±4.4 | 400 |
| `shaper` vs `search` | 58.8% ±4.8 | 400 |
| `shaper` vs `no_shape` | 67.8% ±4.6 | 400 |
| `shape_only` vs `no_shape` | 67.8% ±4.6 | 400 |
| `place_only` vs `no_shape` | 56.2% ±4.9 | 400 |
| `shaper` vs `rand_shape` | 57.5% ±4.8 | 400 |
| `shaper` vs `shaper_randcross` | 63.0% ±4.7 | 400 |
| `search` vs `search_randcross` | 63.0% ±4.7 | 400 |
| `shaper` vs `shaper_scarblind` | 54.0% ±4.9 | 400 |
| `shaper` vs `shaper_fixed` | 60.8% ±4.8 | 400 |
| `persona_rush` vs `persona_fort` | 8.8% ±2.8 | 400 |
| `persona_counter` vs `persona_rush` | 62.5% ±4.7 | 400 |
| `persona_rush` vs `persona_turtle` | 32.2% ±4.6 | 400 |
| `persona_fort` vs `shaper` | 52.5% ±4.9 | 400 |
| `persona_fort` vs `search` | 66.5% ±4.6 | 400 |

## Where the C1 edge comes from

All five bots share one build search; they differ in shaping and in building placement.

| Matchup | Shaping | Placement | Win rate |
| --- | --- | --- | --- |
| `shaper` vs `no_shape` (C1) | searched vs none | searched vs default | 67.8% ±4.6 |
| `shape_only` vs `no_shape` | searched vs none | default both | 67.8% ±4.6 |
| `place_only` vs `no_shape` | none both | searched vs default | 56.2% ±4.9 |
| `shaper` vs `rand_shape` (C2) | searched vs random | searched both | 57.5% ±4.8 |
| `shaper` vs `search` | searched vs sampled | searched vs default | 58.8% ±4.8 |

## Shaping

| | `shaper` self-play | `search` self-play |
| --- | --- | --- |
| Shaping decisions where no op helped | 41.8% of 5296 | - |
| TP used | 36.7% | 89.8% |
| Dead ops (< 2% of ops) | expose | none |
| Blocking ops reverted for path safety | 0.0% | 0.0% |

| Op | Share of ops, `shaper` | Share of ops, `search` | Share of TP, `search` |
| --- | --- | --- | --- |
| raise | 24.2% | 22.0% | 16.5% |
| lower | 18.5% | 23.5% | 17.6% |
| wall | 37.2% | 13.0% | 19.6% |
| flood | 2.6% | 12.3% | 18.5% |
| grow | 8.5% | 20.6% | 15.5% |
| clear | 8.5% | 4.7% | 3.5% |
| expose | 0.5% | 3.8% | 8.7% |

One applied op moves the round by 0.26 core HP on average (8531 ops, `search` self-play, the op removed and everything else replayed).

## The crossing (`search` self-play, 2167 rounds)

Each round, all 81 pairs of crossings are replayed on clones with everything else as played.

| | |
| --- | --- |
| Rounds with no pure equilibrium (a guess) | 47.5% |
| Mean guess gap (minimax - maximin, eval units) | 2.66 |
| Big guesses (gap ≥ 4 core HP) | 15.4% |
| Mean swing over the 81 cells (core HP) | 9.2 |
| Interaction share of the core-HP matrix | 47.8% |
| Crossing repeats last round's column | 14.3% (`search`), 13.6% (`shaper`) |

## C10: feel (`search` self-play)

| Metric | Result | Bar | Verdict |
| --- | --- | --- | --- |
| Close finish (final core margin ≤ 4) | 28.5% ±4.4 | 30-50% | outside the bar, within its CI |
| Wire-to-wire wins | 26.0% ±4.3 | 15-40% | ok |
| Lead changes per game | 0.80 ±0.09 | 0.4-1.0 | ok |
| Comeback (behind after R2, still wins) | 34.7% ±4.9 | 20-35% | ok |

Reported: games decided on HP 2.8% (knockout 97.2%); live going into the last round 42.2%.

## C11: one round for one player (`shaper` self-play)

| Decision | Per round | Options |
| --- | --- | --- |
| Draft | 1 | a ranked order of 3 cards |
| Shaping ops | 0.84 | 278 legal (tile, op) choices |
| Buildings placed | 0.25 | own build zone |
| Units bought | 1.75 | unlocked kinds |
| Crossing | 1 | 9 columns |
| **Total** | **4.8** | |

## Commanders (`search` self-play, bot style `own`)

| Commander | Points | Games |
| --- | --- | --- |
| skywing | 29.2% ±5.9 | 226 |
| ironclad | 59.9% ±6.8 | 202 |
| warden | 57.0% ±6.9 | 200 |
| geomancer | 57.6% ±7.4 | 172 |

A tripwire only: both seats of a seed share its commander pair, so the CI is optimistic. `mt_sim commanders` is the precise tool.

## Kill criteria (`docs/SPEC.md`; decide at 1,600 games)

- C1 below 55%: clear (67.8% ±4.6).
- Crossing guess share above 25%: **would trip** (47.5%).
- Rush vs the defender outside 35-65%: **would trip** (8.8% ±2.8).

Total runtime: 87s.
