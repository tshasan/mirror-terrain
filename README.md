# Mirror Terrain simulator

A headless, deterministic simulator for the **Mirror Terrain** core loop, with AI bots, a scorecard and a
single-file HTML replay viewer. It answers *is the loop good yet?* with numbers.

Mirror Terrain is a portrait mobile strategy game for two players on a map that starts point-symmetric. Each
round you draft a card, shape and build a base on your own half, see both bases, choose where your army
crosses, and watch an automatic clash that scars the ground. A match lasts up to 5 rounds.

- **The design** is in `docs/SPEC.md`, and **the exact rules the code runs** (v0.5) are in `docs/RULES.md`.
- **What "good" means** is in `docs/METRICS.md`; the current numbers are in `results/v05/report.md`.
- **What to do next** is in `docs/PLAN.md`.

## Quick start

Needs Rust 1.85 or later (`rustup update stable`), `make` and Python 3 (for the viewer only).

```sh
make check                  # tests, determinism, seat-symmetry check (under a minute)
make suite OUT=results/try  # the scorecard as report.md + report.json, plus replays (about 2 minutes on 12 cores)
make quick GAMES=100        # the same scorecard as one JSON line, smaller and faster
make viewer OUT=results/try # then open viewer/index.html in a browser
make demo                   # play the game yourself against a bot (Bevy window)
./target/release/mt_sim help
```

## Where things are

| Path | What |
| --- | --- |
| `mt_core/` | The rules: map, draft, shaping, build, attack, clash, scars, replay recorder. No unsafe. |
| `mt_bots/` | Random, greedy, personas, the search bot, the shaper, and the control bots the criteria need. |
| `mt_sim/` | The CLI, the scorecard and the probes. |
| `mt_game/` | The playable demo: you against a bot, drawn with rectangles and letters (Bevy). |
| `viewer/` | `template.html` + `build_viewer.py` → `viewer/index.html` (replays and results). |
| `fuzz/` | cargo-fuzz targets, a separate workspace (nightly only). |
| `results/` | Kept runs. `v05/` is the baseline; `experiments.jsonl` logs every kept or rejected run. |
| `docs/` | `SPEC.md`, `RULES.md`, `METRICS.md`, `PLAN.md`, `RESEARCH.md`. |
| `CLAUDE.md` | Invariants, the experiment protocol, and how to change a rule or add a metric. Read it before changing code. |

Tuning numbers: match constants are at the top of `mt_core/src/game.rs`; unit, building, op and card stats
are in `mt_core/src/defs.rs`; bars are in `mt_sim/src/scorecard.rs`.

Dependencies are few and pinned to exact versions (`mt_core/Cargo.toml`): `rand` and `rand_pcg` for random
numbers, `serde_json` for replays and reports, `rustc-hash` for one cache. A dependency update must never
change a game, so update them on purpose and re-run `make check`.

## Profiling, fuzzing and mutation testing

Optional tools: `cargo install --locked samply`; `rustup toolchain install nightly`, then
`cargo install cargo-fuzz cargo-mutants`.

```sh
make profile                             # samply profile of a small scorecard run, opens in the browser
make fuzz FUZZ_SECS=300                  # both fuzz targets, 5 min each
make mutants FILE=mt_core/src/clash.rs   # mutation testing of one file
```

`game_invariants` plays whole matches from fuzzer bytes and checks for panics, determinism, a connected
ground path, shaping confined to the own half, legal scars and state bounds. `mirrored_draw` checks that
identical players making mirrored choices stay level and leave a mirrored board. Crashes land in
`fuzz/artifacts/<target>/`; `mutants.out/missed.txt` lists changes no test noticed.
