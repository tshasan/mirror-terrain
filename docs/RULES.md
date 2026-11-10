# Mirror Terrain rules (v0.5, as simulated)

This is what the code does, written for a designer. If this file and the code disagree, the code is right and
this file needs fixing. Numbers live in `mt_core/src/defs.rs` and the top of `mt_core/src/game.rs`. There are
no rule flags: the simulator runs exactly one rule set.

## The board

- 9 columns x 16 rows, portrait. Player A's core is at (4,1) and player B's at (4,14).
- The map **starts point-symmetric**: tile `i` and tile `143 - i` match. Generation builds A's half and mirrors
  it. A map is rejected if a single tile can cut the ground path, if a core has fewer than 2 ore tiles within
  5 tiles, or if high ground sits within 3 tiles of a core.
- Rows 0-7 are A's half and rows 8-15 are B's. You shape only your own half.
- Rows 0-6 are A's build zone and rows 9-15 are B's. Rows 7-8 hold no buildings.

The two halves diverge from round 1: shaping is per player, and scars land where the fighting was.

## A match

Up to **5 rounds**. Each player starts with 20 core HP, 15 ore and the Grunt. Players decide simultaneously
in every phase.

| # | Phase | Hidden or public | What happens |
| --- | --- | --- | --- |
| 1 | Draft | public | Three cards are offered, each with a different tag. Each player submits a ranked order. |
| 2 | Shape and build | hidden | Spend TP on ops on your own half; spend ore on buildings and units. |
| 3 | Reveal | public | Both bases (terrain and buildings) become visible. Units bought this round stay hidden. |
| 4 | Attack plan | hidden | Each player picks a crossing: the column (0-8) where their army crosses the midline. |
| 5 | Clash | public | Up to 220 ticks of automatic battle. |
| 6 | Resolve and scars | public | Breach damage, income, then the scars the fight left. |

The game ends when a core reaches 0 HP or after round 5. The winner has more core HP; ties go to the higher
army value, then to a coin flip from the seed.

### Draft

- Different first choices: each player gets theirs.
- Same first choice: the player behind on core HP gets it, and the other gets their second choice.
- Same first choice with level core HP: both get their own second choice (which may be the same card, so
  mirrored inputs still give mirrored outcomes).

A card you already own pays 2 ore instead.

### Shape and build

- **2 TP** per round (each Earthshaper adds 1). An op changes **one tile on your own half**. Every op is
  available from round 1.
- A tile within one tile of either core, or with a building on it, can't be shaped.
- **Path safety:** if the round's ops together cut every ground path between the cores, every blocking op
  (wall, flood) of that round is reverted, on both sides.
- Buildings go in your own build zone (max 8 standing); units join your roster (max 20).
- You choose your build without seeing the opponent's shaping of the same round.

| Op | Cost | From | To |
| --- | --- | --- | --- |
| Raise | 1 | plain | high |
| Lower | 1 | plain, high | trench |
| Wall | 2 | plain | wall |
| Flood | 2 | plain, trench | water |
| Grow | 1 | plain | forest |
| Clear | 1 | forest, wall, trench | plain |
| Expose | 3 | plain | ore |

### Attack plan

The crossing is a column in your own frame (your left is column 0). Your units spawn on that column and its
two neighbours, march to a waypoint just across the midline on that column, then take the shortest path to
the enemy core. Armies that cross near each other meet in the field; armies that don't race each other's
bases.

### Clash

All effects of a tick are computed from the previous tick's state and applied together. Units engage enemies
within 3 tiles; a unit within one tile of the enemy core **breaches** and leaves the field. Each core has a
small gun (2 damage, range 2).

### Resolve and scars

- **Breach damage** = the breaching units' tiers x (round + 1).
- **Losses stick:** units that died and units that breached leave the roster. Survivors fight again next round.
- **Income:** 6 ore, +3 per extractor.
- **Scars**, computed from the whole clash and applied together:

| Scar | Trigger | Effect |
| --- | --- | --- |
| Crater | On each half, the tile where the most ground units died this clash (at least 2 deaths) | The tile becomes trench |
| Ruin | A building is destroyed | Its tile becomes forest |

No scar lands within one tile of a core or under a standing building. A crater never replaces wall, water
or an existing trench, and ties between tiles go to the smallest `rel(tile, side)`, so mirrored fights leave
mirrored scars. Neither trench nor forest blocks ground, so a scar can't cut the path. Scars are ordinary
terrain: the Clear op repairs them.

## Terrain

| Terrain | Ground movement | Combat |
| --- | --- | --- |
| Plain / Ore | normal | - |
| High | slow (x2) | ground ranged units get +2 range and buildings +1; ground units on it take half damage |
| Trench | slow (x2.5); Rhino can't enter | - |
| Forest | slow (x1.5, Rhino x2.5); Ranger moves freely | hides ground units from air units and from anything further than 1 tile; cover cuts damage to 3/4 (1/2 for Ranger or with Camo) |
| Wall / Water | impassable | a unit or building next to a wall, on its own half, deals +50% |

Air units ignore terrain when moving.

**Home turf.** Units on their own half deal 25% more and take 25% less in rounds 1-2, half that in round 3,
and nothing after.

## Units, buildings and cards

| Unit | HP | Dmg | Range | Air | Hits air | Tier | Cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Grunt | 12 | 3 | 1 | | | 1 | 3 |
| Rhino | 40 | 4 | 1 | | | 2 | 7 |
| Archer | 8 | 3 | 3 | | yes | 1 | 4 |
| Ranger | 10 | 3 | 2 | | yes | 1 | 4 |
| Drone | 7 | 2 | 1 | yes | yes | 1 | 4 |
| Gunship | 22 | 4 | 2 | yes | yes | 2 | 8 |

| Building | HP | Dmg | Range | Hits | Cost |
| --- | --- | --- | --- | --- | --- |
| Turret | 40 | 3 | 3 | ground | 5 |
| Flak | 30 | 5 | 3 | air | 4 |
| Extractor | 20 | - | - | - | 4, on ore only; +3 ore per round |

| Card | Tag | Effect |
| --- | --- | --- |
| Rhino, Archer, Ranger | ground | unlocks the unit |
| Drone (two copies in the pool), Gunship | air | unlocks the unit |
| Turret, Flak | defense | unlocks the building |
| Extractor | economy | unlocks the building |
| Earthshaper | terrain | +1 TP every round |
| Hover Treads | ground | your ground units cross trench at normal speed, Rhino included |
| Camo | terrain | your ground units in forest take half damage |
| Grovekeeper, Sapper | terrain | +2 ore |
| Prospector | economy | +2 ore |

Grovekeeper, Sapper and Prospector once unlocked an op each. Every op is now available from the start, so
they only pay ore; they are candidates for a redesign or removal (`docs/PLAN.md`).

## Commanders

Each player gets a different random commander, which sets their starting card (and the bots' play style).
Skywing starts with Drone, Ironclad with Rhino, Warden with Turret, and Geomancer with Earthshaper.
