# Mirror Terrain simulator

Headless, deterministic simulator for the Mirror Terrain core loop: a portrait mobile strategy game in which
two players each shape and build a base on their own half of a 9x16 map that starts point-symmetric, see
both bases, choose where to attack, and watch an automatic clash that scars the ground. This repo answers
one question with numbers: *is the loop good yet?*

Read `docs/SPEC.md` for the design, `docs/RULES.md` for the exact rules the code runs (v0.5),
`docs/METRICS.md` for what "good" means, and `docs/PLAN.md` for what to do next. `docs/RESEARCH.md` explains
why each rule exists.

## Commands

```sh
make check                      # tests + determinism + mirror-symmetry check. Run before trusting any number.
make suite OUT=results/try GAMES=400   # the scorecard: report.md, report.json and 5 replays (about 1.5 minutes on 12 cores)
make quick GAMES=100            # the same scorecard as one JSON line, no files
make viewer OUT=results/try     # embed a suite run into viewer/index.html, then open it in a browser
./target/release/mt_sim help    # every CLI command and bot name
make demo BOT=shaper SEED=7     # play against a bot in a window (first build takes a few minutes)
make profile                    # samply profile of a small quick run (opens in the browser)
make fuzz FUZZ_SECS=60          # cargo-fuzz targets in fuzz/ (nightly). Run after touching game.rs or clash.rs.
make mutants FILE=mt_core/src/game.rs   # cargo-mutants on one file. mutants.out/missed.txt = untested code.
```

There are no rule flags: the simulator runs one rule set, and every output carries its version (`VERSION` in
`mt_sim/src/main.rs`). `MT_DEBUG=1` makes the search bot print its payoff matrices to stderr.
`MT_STYLE=<balanced|rush|turtle|air|ground>` gives every commander the same bot style, to separate a
commander's start card from the plan the bots play with it. `MT_THREADS=<n>` sets the worker thread count
(default all cores). An unknown bot name is an error, not a fallback.

## Layout

- `mt_core/`: the rules. Map generation, draft, own-half shaping, build, attack, clash, scars, replay recorder.
  `#![forbid(unsafe_code)]`, overflow checks on in release.
  - `defs.rs`: geometry, terrain, ops, units, buildings, cards, commanders (most tuning numbers live here).
  - `game.rs`: round phases, economy, scars, win condition (constants at the top).
  - `clash.rs`: the tick simulation. All effects for a tick are computed, then applied at once. Runs of ticks in
    which units only wait to afford their next step are applied in one go, except while recording, so a recorded
    game is the tick-by-tick reference (a test compares the two). `unaffected` proves that a board one tile away
    from one already fought on fights the same fight; a clash change that reads terrain in a new way must keep
    `Clash::touched` and that proof in step.
  - `lib.rs`: the `Rng` type (PCG-64 from `rand_pcg`) and the rules invariant tests.
- `mt_bots/`: random, greedy, personas, the simultaneous-move search bot (payoff matrix + regret matching),
  and the shaper. `fast.rs` holds the shaper's two searches (ops and building placement). Both screen their
  candidates before a full evaluation; keep it that way, a slow bot makes every number slow. The op search also
  skips every clash `clash::unaffected` proves unchanged, which must never change a plan (a test checks).
- `mt_sim/`: the CLI (`match`, `suite`, `quick`, `selfplay`, `deals`, `commanders`, `replay`, `verify`,
  `symcheck`). `scorecard.rs` is the scorecard (criteria C1-C11 and the match checks). `probes.rs` holds the
  counterfactual probes; they must never change a game (`verify` checks).
- `mt_game/`: the playable demo (Bevy 0.15, pinned): a human on side A against any bot. `session.rs` runs the
  phases with no rendering and has the tests; `main.rs` only draws it and turns clicks into `Session` calls.
  It is not a default workspace member, so `cargo build`, `cargo test` and `make check` skip it: use
  `make demo` and `cargo test -p mt_game --profile demo`. Rules never go here; they stay in `mt_core`.
- `viewer/`: `template.html` + `build_viewer.py` produce a single-file HTML replay viewer and results page.
  The results page renders whatever `report.json` contains; labels, bars and verdicts come from the simulator.
- `fuzz/`: cargo-fuzz targets, a separate workspace. `game_invariants` checks state invariants and determinism
  on arbitrary inputs; `mirrored_draw` checks seat symmetry. A new invariant belongs in one of them too.
- `results/`: kept runs. `v05/` is the current baseline. Don't hand-edit anything in here.

## Invariants (never break these)

1. **Determinism.** Same seed + same inputs = same game, bit for bit, on any machine. No `HashMap` iteration in
   game logic (use `BTreeMap`; the one `FxHashMap` is a cache that is only probed by key), no wall-clock time,
   no thread-dependent order, no transcendental floats (`exp`, `ln`, `sin`...) anywhere a result can feed back
   into play. Bots use plain `f64` arithmetic only. Random numbers come from `mt_core::Rng` only: never
   `StdRng`, `SmallRng` or `rand::rng()`, whose output is not stable.
2. **Seat symmetry.** Mirrored inputs must give mirrored outcomes. Every side-dependent tie-break goes through
   `rel(tile, side)`. Any time you read terrain "from a side's point of view", build a view with `rel`
   (see `waypoint_aframe` in `clash.rs`, which was a real bug). `make check` runs `symcheck`, which must report
   0 divergences.
3. **Batch resolution.** In `clash.rs`, decisions read last tick's state; damage, deaths and moves apply
   together. Never apply an effect mid-loop. Scars are computed from the whole clash and applied together.
4. **Everything simulatable.** Every rule must be expressible as a discrete decision for a bot. No real-time input.
5. **Hidden information stays hidden.** A bot chooses its build on `Game::shape_view` and its crossing on
   `Game::reveal_view`. Every round loop must follow the phase order in `mt_bots::play_round_after_draft`.
6. **Dependencies are pinned.** Every crate is pinned to an exact version (`=x.y.z`), because an update to
   `rand` can change every game. Ask before adding a crate or changing a version, and re-run `make check`
   after either.

## How to change a rule

There is one rule set, so a rule change is an edit, not a flag.

1. Change it in `mt_core`, with a comment saying what it does and why.
2. `make check`. The golden hashes will move: paste the new values.
3. `make suite OUT=results/try GAMES=400` and compare with `results/v05/report.json`, which was run at the
   same game count and seeds.
4. Kept: update `docs/RULES.md`, add a short entry to `docs/RESEARCH.md`, bump `VERSION`, and re-run the
   baseline in `results/`. Rejected: revert the code. Either way, log the run.

## How to add a metric

`GameResult` (per game) → `pooled()` / `Pooled` (aggregate) → a `Criterion` or `Check` in
`mt_sim/src/scorecard.rs` with its bar, and a field in `Scorecard::json`. The Markdown report and the viewer
pick up criteria and checks without further changes. Add it to `docs/METRICS.md` with the reason for its bar.

## Experiment protocol

- Change **one** thing per run. Compare against `results/v05/report.json` at the same game count.
- At 400 games the 95% CI on a win rate is about ±4.9 points. Treat differences smaller than that as noise.
  Re-run with another seed range (`match <a> <b> 1600 <seed>`) before believing a borderline result.
- A change is kept only if the targeted metric moved past its CI and no passing check in the scorecard
  regressed past its CI.
- Log every run you keep or reject as a line in `results/experiments.jsonl` (the `quick` output, with a
  `"note"` saying what changed and the verdict).
- Don't move a bar to make a check pass. If a bar looks wrong, say so and propose the change with a reason.
  That is a design decision for the human.

## Golden hashes

`mt_bots` has a `golden_games` test with three fixed game hashes. It fails after any intentional change to
rules, maps or bots. That's expected: paste the new values from the failure message and mention it. If it
fails after a change you thought was a refactor, you changed behaviour; find out why before continuing.

## Style

`cargo fmt` (max width 160, see `rustfmt.toml`), `cargo clippy --release` must stay clean. Prefer small pure
functions in `mt_core`. Comment *why*, especially where a rule comes from research.
