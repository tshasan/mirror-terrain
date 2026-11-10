# Why the rules are the way they are

One entry per rule that came from an idea outside the game, with what the simulator said when it went in.
`docs/RULES.md` says what each rule is; this file says why it exists.

## Rules in the game

- **Terrain effects are big and readable** (Advance Wars, Into the Breach). High ground halves damage and
  extends range, forest hides, trench stops the heaviest unit. Small modifiers were invisible in play: terrain
  only decided games once its effects were large.
- **Every op is available from round 1** (Jaffe, restricted play). Ops gated behind cards were dead for most
  of a match.
- **Breach damage rises each round** (Marvel Snap, Teamfight Tactics). Rising stakes shorten matches and
  make the last rounds decide them.
- **The attack culminates** (Clausewitz, *On War*, Books 6-7: defence is the stronger form, and an attack
  wears itself out). Units that die or breach leave the roster. When rosters respawned every round, attacking
  cost nothing and the pure defender was the broken strategy.
- **A home-turf advantage that fades** (Jervis 1978, the security dilemma; the Dupuy Institute, against a
  fixed 3:1 defender multiplier). A flat defender bonus barely moved the rush-versus-defender matchup; a
  side-dependent bonus that decays over the rounds did, and it leaves the late game to the attacker.
- **You shape only your own half.** A shaping op that lands on both halves cancels between similar armies:
  it moved a round by about half a core HP, and a bot that shaped at random beat one that searched. Own-half
  shaping makes the base the thing a player owns.
- **The crossing is chosen after the reveal.** A blind three-way lane choice swung a round by 8.5 core HP
  and had no right answer in 31% of rounds. Moving the choice after the reveal is meant to turn the guess
  into a read (criterion C3 measures whether it does).
- **A contested draft.** When both players could take the same card, the deal fixed about 30% of outcome
  variance. A card that goes to one player gives the two kits a reason to differ through decisions.
- **Scars instead of a strike.** A targeted strike on the opponent's base was considered and rejected: the
  tiles left to strike are the empty ones, so strikes collapse onto edges and a few predictable spots. With
  scars players aim nothing, so they have nothing to exploit.

## Where the checks come from

- **Self-determination theory** (Ryan, Rigby & Przybylski 2006): competence and autonomy predict enjoyment.
  These are the skill ladder (C5) and the persona balance checks (C8).
- **Suspense and surprise** (Ely, Frankel & Kamenica 2015): plot twists should get rarer and bigger. These
  are the lead-change and wire-to-wire bars, and the tension curve (C9).
- **Near misses** (Clark et al. 2009): close losses motivate players when they had control. This is the
  close-finish bar. Close games must come from the rules, never from rigging.
- **Schelling focal points**: a contested tile is where skill shows. The game has none at the moment; a
  midfield objective outside both bases is the open idea (`docs/SPEC.md`).

## Tried and dropped

Worth knowing before proposing them again.

- Stronger or cheaper defences near the core: no effect on rush versus defender.
- Splash damage from turrets (Lanchester's square law): no effect, massing wasn't the problem.
- Extra TP, ore or priority for the trailing player: overcorrected once losses stuck (5% wire-to-wire wins).
- Commander starts without a unit: balanced the commanders but cost close finishes and lead changes.
- Crystal tiles that pay whoever shaped them last: players paid them as a tax (half of all ops).

## Sources

- Jervis, "Cooperation under the Security Dilemma", *World Politics* 30(2), 1978. https://doi.org/10.2307/2009958
- Ryan, Rigby & Przybylski, *Motivation and Emotion* 30, 2006. https://www.rochester.edu/warner/lida/wp-content/uploads/2022/11/02bfe513dd59366750000000.pdf
- Ely, Frankel & Kamenica, "Suspense and Surprise", *JPE* 123(1), 2015. https://econpapers.repec.org/RePEc:ucp:jpolec:doi:10.1086/677350
- Clark et al., "Gambling Near-Misses Enhance Motivation...", *Neuron* 61, 2009. https://pmc.ncbi.nlm.nih.gov/articles/PMC2658737/
- The Dupuy Institute (force ratios): https://dupuyinstitute.org/
- Clausewitz, *On War*, Books 6-7. Commentary: https://www.clausewitz.com/

## v0.6 changes

- **Round cap 5 -> 12.** Kept as a backstop. It barely changes game length: 42-47% of games already ended by
  knockout before round 5 because breach damage is `breach * (round + 1)` and has no ceiling. The old round-5 mass
  spreads over rounds 5-8; 0.3% of games reach 12. HP-decided games drop from about 23% to about 2%.
- **Corner cores.** Core at (1,1) / (7,14) instead of the centre column. Same ring, build zone and crossing choice; only
  the coordinates and the tests that named them changed. Map generation retries 1.44x as often (30.5% of attempts rejected,
  mostly the ore-within-5-of-the-core rule, which now excludes columns 7-8), never falling back to a flat board in 20000
  maps. Rush vs the fort fell from about 16% to about 9%: a corner is easier to seal, and the defender's path is shorter.
  Big crossing guesses fell from 25% to 17% of rounds.
- **Commander balance by existing numbers only**, one lever per run (see `results/experiments.jsonl`). Kept: Rhino cost 7
  -> 8 (Ironclad 57.3% -> 50.9% balanced), Earthshaper +1 TP -> +2 TP (Geomancer 46.1 -> 48.1), Drone cost 4 -> 6
  (Skywing 54.1 -> 50.2). Rejected: Drone HP 6, speed 12 or 16, cooldown 8 and BASE_TP 1. Drone cost 5 did nothing; the
  effect is a threshold, not a slope.
  With every commander on the `balanced` bot style, all four sit inside 48-52% on three seed ranges. With each
  commander's own bot style Skywing stays near 35% and Warden near 58%: the air and turtle personas are the gap, not the
  start card. Making the air persona stronger is a bot change, not a rule change, and is left open.
- **Growing income (+1 ore per round)** was tried and rejected: no movement outside the CI on any fast-loop metric.
