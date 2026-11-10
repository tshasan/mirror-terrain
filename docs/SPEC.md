# Spec v0.7: your base is your strategy

This is the **target design for v0.7**. The simulator still runs v0.6 (`docs/RULES.md` has the exact rules the
code runs); `docs/PLAN.md` lists the steps that take it from v0.6 to this spec, one change per run. Values
marked **[design]** are defaults: change one only with a written reason, as with a threshold.

## The thesis

Skill shows in two places: **how you design your base** and **which units you buy**. You never aim your
army. Where the fight happens, and who wins it, follows from what you built and how you shaped your ground.

Every mechanic must serve one of those two skills or be cut. When a question is open and the simulator has no
answer yet, the default is what Mechabellum does, unless it would move skill away from the base (free unit
placement) or add a system to learn (tech trees, levelling, active abilities).

## Pillars

1. **You own your base.** You shape and build on your own half, and the opponent cannot edit it.
2. **Your base aims your army.** Routes come from the ground. Shaping your half decides where the fight
   happens; nobody picks a lane.
3. **You answer what you saw.** You see the opponent's base and standing army at every reveal, and you answer
   with units and with your base next round.
4. **Fighting changes the ground.** The clash scars both bases, and you build the next round around the scars.
5. **You can name the decision that won.** After a clash, you can say in one sentence which decision won or
   lost the round.

The mirror gives a fair start: the map begins point-symmetric, and the two halves diverge from round 1.

## The round

20 core HP, simultaneous decisions, automatic clash. The game ends when a core reaches 0; round 12 is a
backstop that should almost never be reached.

| # | Phase | Hidden or public | The player's question | Decides |
| --- | --- | --- | --- | --- |
| 1 | Draft | own offer | What do I want more of? | one card |
| 2 | Build and shape | hidden, simultaneous | What base and army do I want standing when they see it? | units, buildings, ops |
| 3 | Reveal | public | Did they prepare for me? | nothing |
| 4 | Clash | public, automatic | Was I right? | nothing |
| 5 | Resolve and scars | public | What did the fight leave behind? | nothing |

There is no attack-plan phase and no crossing.

### 1. Draft
Each player gets **their own offer of three cards and picks one**. No ranking and no contested cards.
**[design]** Alternative if the deal's share of the outcome rises past its bar (C4): one shared offer, and both
players may take the same card.

### 2. Build and shape
- **Every unit and every building can be bought from round 1.** Cards no longer unlock kinds.
- Spend ore on buildings (own build zone, max 8) and units (max 20 in the army).
- Spend **2 TP** on ops that change terrain on your own half.
- **Income grows.** **[design]** 6 ore, +2 per completed round, +3 per extractor.
- Your base persists across rounds, and scars from earlier rounds are ordinary terrain.
- The game keeps a ground path between the cores: it reverts an op that would cut the last path.

### 3. Reveal
Both players see both bases (terrain and buildings) and both routes. Units bought this round stay hidden until
the clash; the army that stood last round is known.

### 4. Clash
- **Routing.** Units appear around their own core and take the shortest ground path to the enemy core for
  their movement class; air units fly straight. Ties go through `rel(tile, side)`.
- **Your army comes back.** Every unit you own fights again next round, whether it died, breached or
  survived. An army is something you build up, not something you spend.
- **Targeting.** **[design]** One rule: a unit keeps to its route and fights what is in its range; it stops
  only for something that blocks the route or is shooting at it. It never leaves the route to chase.
- Batch resolution, 220 ticks, as now.

### 5. Resolve and scars
- **Breach damage** is dealt by enemy units that reach your core, by tier. **[design]** The per-round
  multiplier stays until step 7 of the plan tests whether a growing army makes it unnecessary.
- Income, then scars, all applied together. Scar rules are unchanged from v0.6 (crater where the most ground
  units died on each half, ruin where a building fell).

## Units, cards and commanders

- **Units.** Six kinds, each with one job and one counter a player can say in a line (for example: "Rhino
  soaks turret fire; Archers on high ground kill it"). Costs and stats are tuned to make that true; no new
  unit kinds.
- **Cards are bonuses.** **[design]** A unit or building card makes that kind cost 1 ore less. Earthshaper,
  Hover Treads and Camo keep their effects. The three cards that only pay ore keep doing so until they are
  redesigned.
- **Commanders.** A commander is its start card, shown to the player as exactly that, with the one line about
  what the unit is for.

## What the player sees (mt_game, no rule change)

- Both routes drawn on the board at the reveal and while shaping, updating as an op is placed.
- Turret and flak ranges, with the high-ground bonus.
- This round's stakes: the breach multiplier, and home turf while it exists.
- After the clash: where the armies met, what each lost, and which tile became a scar.

## Cut from v0.6, and why

| Cut | Why |
| --- | --- |
| The crossing and the attack-plan phase | 47.5% of rounds had no right crossing; in play it was either irrelevant or decided by one column |
| Ranked, contested draft | It is a decision about the draft, not about the base or the army |
| Unit and building unlocks behind cards | Unit selection can't be a skill when the offer decides what you may buy |
| Losses that stick | Losing round 1 decided the game by round 4 in play |
| **[design]** The rising breach multiplier and the fading home turf | Two numbers the player can't see; cut one at a time if games still end without them |

## Risks

- **Solitaire.** With no attack choice, all interaction runs through the reveal, the hidden purchases and the
  scars. Criterion C7 is the main gate.
- **Respawning was rejected once.** When rosters respawned under the crossing rules, attacking cost nothing
  and the pure defender was the broken strategy (`docs/RESEARCH.md`). Automatic routing removes the choice to
  attack, so it is retested, not assumed.
- **The deal.** Own-offer drafts may raise the share of the outcome fixed by the deal (17.1% in v0.6).
- **Sameness.** If the route rarely changes, rounds repeat. The route-change metric below watches for it.
- **Terrain still doesn't matter.** In v0.6 one op was worth about 0.4 core HP and a played game found 0 of 5
  ops changed a round. If routing does not fix that, ops get bigger (two or three connected tiles).

## Criteria

`docs/METRICS.md` defines how each one is measured; it is updated as each step lands. Win rates are
seat-swapped head-to-heads; check a gate at 1,600 games. Bars marked *proposed* are waiting for approval.

### Gates: the idea works

| # | Claim | Measured as | Target |
| --- | --- | --- | --- |
| C1 | The base is the hook | Searched ops and placement vs no shaping and default placement | ≥ 60% |
| C2 | Good shaping beats any shaping | Searched ops vs random ops | ≥ 60% |
| C3 | *Retired.* | There is no crossing | - |
| C4 | Players decide the game | Share of outcome variance fixed by the deal | ≤ 15% |
| C5 | There is depth | Ladder steps won by ≥ 60% | ≥ 4 |
| C7 | It is not solitaire | A bot that builds against the revealed base and army vs the same bot on a fixed build order | ≥ 55% |
| C12 | An op matters | Mean change in the round's core-HP difference when one op is removed | ≥ 1.5 core HP *(proposed)* |
| C13 | Shaping is never empty | Shaping decisions where the strongest bot finds no useful op | ≤ 20% *(proposed)* |

### Diagnostics: it works for the right reasons

| # | Claim | Measured as | Target |
| --- | --- | --- | --- |
| C6 | Scars follow from decisions | Scar-aware bot vs scar-blind bot | ≥ 55% |
| C6b | Scars happen, but not everywhere | Scars per game; rounds that leave one | 2-6; 40-80% |
| C8 | Defending is viable but not dominant | A building-heavy persona vs an army-heavy persona, both ways | 35-65% *(proposed, two-sided)* |
| C10 | The feel checks hold | Close finish, wire-to-wire, lead changes, comeback | each inside its bar |
| C11 | A round fits a phone | Decisions per round | ≤ 5 *(proposed)* |
| C14 | Every unit has a job | Each unit's share of purchases | none above 40% or below 5% *(proposed)* |
| C15 | Round 1 doesn't decide it | The loser of round 1 still wins | 20-35% *(proposed)* |
| C16 | Ops move the fight | Share of ops that change a route | reported |
| C9 | The tension sits on decisions | Per-phase movement of win probability | pending, as before |

Pacing bars are restated for an uncapped game *(proposed)*: 90% of games end by round 8, and fewer than 1%
reach round 12.

### Stop conditions

Stop and reconsider the direction if, after routing, respawning and one terrain fix are in:

- C12 is under 1 core HP, or
- C7 is under 55%, or
- most rounds are fought on the same route.

### What only people can judge

- Can a player say in one sentence why they lost a round?
- Do they say "my base"?
- Can they say why they placed their last op?
- Can they say what each unit is for?
- Does a round fit in 90 seconds?
