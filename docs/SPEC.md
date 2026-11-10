# Spec: own-half bases, a reveal, and scars

This is the design the simulator implements as v0.5 (`docs/RULES.md` has the exact numbers). Values marked
**[design]** are defaults: change one only with a written reason, as with a threshold.

## Pillars

1. **You own your base.** You shape and build on your own half, and the opponent cannot edit it.
2. **You attack with information.** You see both bases before you choose where to attack.
3. **Fighting changes the ground.** You cannot target terrain on the other side. The clash scars both bases,
   and you build the next round around the scars.
4. **You can name the decision that won.** After a clash, you can say in one sentence which decision won or
   lost the round.

The mirror gives a fair start: the map begins point-symmetric, and the two halves diverge from round 1.

## The round

Up to 5 rounds, 20 core HP, simultaneous decisions, automatic clash.

| # | Phase | Hidden or public | The player's question |
| --- | --- | --- | --- |
| 1 | Draft | public | Who am I becoming? |
| 2 | Build and shape | hidden, simultaneous | What base do I want standing when they see it? |
| 3 | Reveal | public | Did they prepare for me? |
| 4 | Attack plan | hidden, simultaneous | Where do I hit? |
| 5 | Clash | public, automatic | Was I right? |
| 6 | Resolve and scars | public | What did the fight leave behind? |

### 1. Draft
The game offers three cards, and a contested card goes to one player only, so the two kits differ. Each
player submits a ranked order of the three. If the first choices collide, the trailing player gets the card
and the other gets their second choice; if core HP is level, both get their second choice. Players with
different kits shape different ground, so two bases on the same starting map diverge.

### 2. Build and shape
- Spend ore on buildings and units (own build zone, 8 buildings, 20 units).
- Spend **2 TP** on ops that change **one tile on your own half**. Earthshaper adds 1 TP.
- Your base persists across rounds. Ten ops over a match is too few to build a maze.
- Scars from earlier rounds are ordinary terrain: build around them, or spend an op to repair them.
- The game keeps a ground path between the cores: it reverts an op that would cut the last path.
- No shared objective tile. **[design]** A midfield objective (a tile that pays whoever last stood on it) can
  come later, outside both bases.

### 3. Reveal
Both players see both bases: terrain and buildings. Army composition stays hidden until the clash, so you
still have something to read. You make no decision here; you look at their base and wait.

### 4. Attack plan
One choice: the **crossing**, the column (0-8) where your army crosses the midline. From there units take
the shortest path to the enemy core. Armies that cross near each other meet in the field; armies that don't
race each other's bases.

You cannot change the opponent's terrain.

**[design]** Crossing granularity. Default: 9 columns. Alternatives if the crossing stays a guess (criterion
C3): 3 gates (left, centre, right); a fixed centre crossing in round 1, when there is no base to read; or
letting the trailing player choose after seeing the leader's crossing.

### 5. Clash
An automatic tick simulation. It records where units die and which buildings fall. The viewer points at the
two crossings, because the clash tests those two predictions.

### 6. Resolve and scars
Breach damage (rising each round), losses that stick, income. Then the game applies the scars, all at once,
from what the clash recorded.

## Scars

The defaults need no new terrain type, and a scar cannot cut the ground path (neither trench nor forest
blocks ground).

| Scar | Trigger | Effect | Why |
| --- | --- | --- | --- |
| Crater | On each half, the tile where the most ground units died this clash (at least 2 deaths) | The tile becomes trench: slow for ground, impassable for Rhino | The killing ground gets worse to attack through. Hitting the same spot twice costs more, so the attacker has to vary. |
| Ruin | A building is destroyed | Its tile becomes rubble (forest stats in the prototype: slow, gives cover) | A lost building changes the base: its rubble can shelter the next attack. |

- The game computes scars from the whole clash and applies them together in the resolve step.
- Ties between tiles go through `rel(tile, side)`, so mirrored fights leave mirrored scars.
- No scar on a tile next to a core or under a standing building. A crater does not replace wall or water.
- You repair a scar with the ordinary Clear op (1 TP), so a repair competes with shaping for a budget of 2.

**[design]** decisions, with defaults:

| Question | Default | Alternative |
| --- | --- | --- |
| How many craters per half per clash? | 1 | 2, if scars turn out too rare to matter |
| Minimum deaths for a crater | 2 | 3, if every skirmish leaves one |
| Do air unit deaths count? | No: only ground units mark the ground | Yes, as a lighter scar |
| What is rubble? | Forest stats, drawn differently | Its own terrain type, once the idea is proven |
| Does a breach leave a scar? | No | A crater on the last tile before the core's ring |

Scars have three jobs:

- **Story.** By round 4 the board is a record of the match. A player can point at a crater and say what
  happened there.
- **Negative feedback on repetition.** A crater on the defender's killing ground makes the same crossing
  worse next round. That pushes lead changes without a catch-up rule.
- **A reason to revisit your base.** Without scars, a good base in round 2 is a good base in round 5.

## Risks

- **Solitaire.** Each player shapes only their own half, so all the interaction runs through the crossing,
  the hidden army and the scars. Criteria C3, C6 and C7 test whether that is enough. This is the main risk of
  choosing scars over a targeted strike on the opponent's base.
- **Turtling.** Free own-half walls and craters both favour the defender. Criterion C8.
- **The crossing is still a guess.** Two armies choose crossings at the same time. Criterion C3.
- **Scars are noise.** If luck decides where units die, scars add randomness that no player chose,
  against pillar 4. Criterion C6.
- **Two stories at once.** Each player attacks and defends in the same clash. Bots cannot judge whether
  that reads on a phone; playtests must.

## Criteria

`docs/METRICS.md` defines how each one is measured. Win rates are seat-swapped head-to-heads; check a gate
at 1,600 games.

### Gates: the idea works

| # | Claim | Measured as | Target |
| --- | --- | --- | --- |
| C1 | The base is the hook | A bot that searches its own-half ops and building placement vs the same bot that never shapes and places by default | ≥ 60% |
| C2 | Good shaping beats any shaping | Searched own-half ops vs random own-half ops | ≥ 60% |
| C3 | The attack is a read | Searched crossing vs random crossing, and the share of rounds whose crossing game has no pure equilibrium | ≥ 60% and ≤ 10% |
| C4 | Players decide the game | Share of outcome variance fixed by the deal (`mt_sim deals`) | ≤ 15% |
| C5 | There is depth | Ladder steps won by ≥ 60% | ≥ 4 (needs a fourth rung) |

### Diagnostics: it works for the right reasons

| # | Claim | Measured as | Target |
| --- | --- | --- | --- |
| C6 | Scars follow from decisions | A bot that accounts for scars vs the same bot that ignores them | ≥ 55% |
| C6b | Scars happen, but not everywhere | Scars per game, and the share of rounds that leave at least one | 2-6 per game; 40-80% of rounds |
| C6c | Scars change play | Share of crossings that repeat last round's column | lower with scars than without |
| C7 | It is not solitaire | A bot that builds against the opponent's revealed base and kit vs the same bot on a fixed build order | ≥ 55% |
| C8 | Defending is viable but not dominant | Rush vs a defender; defend-then-strike vs rush | ≤ 65%; 40-60% |
| C9 | The tension sits on decisions | Per-phase movement of win probability (see below): share carried by build + attack plan, and by the deal | build + attack ≥ 50%; deal ≤ 15% |
| C10 | The feel checks hold | Close finish, wire-to-wire, lead changes, comeback | each inside its bar |
| C11 | A round fits a phone | Decisions per round and legal options per decision | reported; judged in playtests |

C7 is the weakest definition here: "builds against the opponent" depends on how good the adaptive bot is.
Treat a failure as a prompt to look at replays before concluding anything.

**The tension curve (C9).** For each game in self-play, estimate seat A's win probability after the deal and
after each phase of each round, by rollouts on clones. A phase's share is its mean absolute change in win
probability. This is the suspense-and-surprise measure from Ely, Frankel and Kamenica (`docs/RESEARCH.md`),
per phase. The decision phases should carry most of the movement, and the clash should settle it.

### Kill criteria

Stop tuning and reconsider the design if, at 1,600 games:

- C1 is below 55%, or
- the crossing guess share is above 25%, or
- rush vs the defender is outside 35-65% and stays there after the listed alternatives are tried.

Scars have their own, smaller test: if C6 is under 50% (accounting for scars does not help) and lead changes
are low, scars are decoration. Keep them only if playtesters want them for the story.

### What only people can judge

For the five human playtests:

- Can a player say in one sentence why they lost a round?
- Do they say "my base"?
- Can they point at a scar and say what happened there?
- Does a round fit in 90 seconds?
