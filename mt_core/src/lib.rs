#![forbid(unsafe_code)]
#![allow(clippy::needless_range_loop)]

pub mod clash;
pub mod defs;
pub mod game;
pub mod map;
pub mod replay;

/// The one random number generator: PCG-64 (MCG variant), 16 bytes of state, so cloning a game stays cheap. A named
/// algorithm on purpose: `StdRng` and `SmallRng` may change between `rand` versions, which would change every game.
pub type Rng = rand_pcg::Pcg64Mcg;
/// Re-exported so the bots and the simulator draw through the same pinned version as the rules.
pub use rand;

pub use defs::*;
pub use game::{BuildCmd, CROSSINGS, Game, MAX_ROUNDS, OpOutcome, ROSTER_CAP, ScarKind, sanitize_order};

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{RngExt, SeedableRng};

    fn grunts(n: usize) -> BuildCmd {
        BuildCmd {
            buildings: vec![],
            units: vec![UnitKind::Grunt; n],
        }
    }

    fn play_scripted(seed: u64) -> Game {
        let mut g = Game::new(seed);
        while !g.is_over() {
            g.start_round();
            g.apply_draft([[0, 1, 2], [1, 0, 2]]);
            let ops0 = g.legal_ops(0);
            let ops1 = g.legal_ops(1);
            g.apply_terraform([ops0.into_iter().take(2).collect(), ops1.into_iter().rev().take(2).collect()]);
            g.apply_build([grunts(3), grunts(3)]);
            g.apply_attack([1, 7]);
            g.clash_and_resolve();
        }
        g
    }

    /// The distance-field search and the per-thread memo (with its normalisation of kinds and hover) must return exactly
    /// the field the reference Dijkstra gives, on first use and on a cache hit, including terrain with walls, water and trenches.
    #[test]
    fn memoised_dist_field_equals_direct_search() {
        use crate::map::{dist_field, dist_field_dijkstra, dist_field_uncached};
        const ALL: [Terrain; 7] = [
            Terrain::Plain,
            Terrain::High,
            Terrain::Trench,
            Terrain::Wall,
            Terrain::Water,
            Terrain::Forest,
            Terrain::Ore,
        ];
        let mut rng = crate::Rng::seed_from_u64(99);
        for seed in 0..30 {
            let mut t = Game::new(seed).terrain;
            if seed % 2 == 1 {
                for c in t.iter_mut() {
                    *c = ALL[rng.random_range(0..ALL.len())];
                }
            }
            for _pass in 0..2 {
                for target in [0, 17, CORE[0], CORE[1], N - 1] {
                    for kind in ALL_UNITS {
                        for hover in [false, true] {
                            let want = dist_field_dijkstra(&t, target, kind, hover);
                            assert_eq!(dist_field_uncached(&t, target, kind, hover), want);
                            assert_eq!(dist_field(&t, target, kind, hover), want);
                        }
                    }
                }
            }
        }
    }

    /// The compile-time neighbour tables must list exactly what the direction lists give, in the same order (clash
    /// tie-breaks depend on the order).
    #[test]
    fn neighbour_tables_match_direction_lists() {
        const D8: [(i32, i32); 8] = [(0, 1), (1, 0), (0, -1), (-1, 0), (1, 1), (1, -1), (-1, 1), (-1, -1)];
        for i in 0..N {
            for diag in [false, true] {
                let (x, y) = xy(i);
                let want: Vec<usize> = D8[..if diag { 8 } else { 4 }].iter().filter_map(|&(dx, dy)| idx(x + dx, y + dy)).collect();
                assert_eq!(crate::map::neighbors(i, diag).collect::<Vec<_>>(), want);
            }
        }
    }

    #[test]
    fn map_is_point_symmetric_and_connected() {
        for seed in 0..200 {
            let g = Game::new(seed);
            for i in 0..N {
                assert_eq!(g.terrain[i], g.terrain[mirror(i)], "seed {seed} tile {i}");
            }
            assert!(map::ground_connected(&g.terrain));
        }
    }

    #[test]
    fn the_path_survives_any_play_and_replays_are_deterministic() {
        for seed in 0..50 {
            let g = play_scripted(seed);
            assert!(map::ground_connected(&g.terrain));
            assert_eq!(g.hash(), play_scripted(seed).hash());
        }
    }

    /// Seat symmetry: identical players making mirrored choices stay level on everything, every round, and the board
    /// stays point-symmetric (mirrored fights leave mirrored scars).
    #[test]
    fn mirrored_play_stays_mirrored() {
        for seed in 0..40 {
            let mut g = Game::new(seed);
            g.players[1] = g.players[0].clone();
            let mut rng = Rng::seed_from_u64(seed + 99);
            while !g.is_over() {
                g.start_round();
                let pick = rng.random_range(0..3);
                // the same ranked order for both seats: [pick, then the rest ascending]
                let order = [pick, (pick + 1) % 3, (pick + 2) % 3];
                g.apply_draft([order, order]);
                let legal = g.legal_ops(0);
                let plan: Vec<_> = (0..3).filter_map(|_| legal.get(rng.random_range(0..legal.len().max(1))).copied()).collect();
                let plan_b: Vec<_> = plan.iter().map(|&(t, o)| (mirror(t), o)).collect();
                g.apply_terraform([plan, plan_b]);
                let units: Vec<UnitKind> = ALL_UNITS.iter().copied().filter(|u| g.players[0].units[*u as usize]).collect();
                let mut cmd = BuildCmd {
                    buildings: vec![],
                    units: units.iter().cycle().take(4).copied().collect(),
                };
                for kind in [BuildingKind::Turret, BuildingKind::Flak, BuildingKind::Extractor] {
                    if let Some(&t) = g.building_tiles(0, kind).first() {
                        cmd.buildings.push((kind, t));
                    }
                }
                let mut cmd_b = cmd.clone();
                cmd_b.buildings = cmd.buildings.iter().map(|&(k, t)| (k, mirror(t))).collect();
                g.apply_build([cmd, cmd_b]);
                let c = rng.random_range(0..9) as u8;
                g.apply_attack([c, c]);
                g.clash_and_resolve();
                assert_eq!(g.players[0].core_hp, g.players[1].core_hp, "seed {seed} round {}", g.round);
                assert_eq!(g.players[0].ore, g.players[1].ore, "ore: seed {seed} round {}", g.round);
                assert_eq!(g.players[0].roster, g.players[1].roster, "roster: seed {seed} round {}", g.round);
                assert_eq!(g.players[0].picks, g.players[1].picks, "picks: seed {seed} round {}", g.round);
                for i in 0..N {
                    assert_eq!(g.terrain[i], g.terrain[mirror(i)], "terrain: seed {seed} round {} tile {i}", g.round);
                }
            }
        }
    }

    #[test]
    fn forcing_the_dealt_commanders_changes_nothing() {
        for seed in 0..30 {
            let g = Game::new(seed);
            let cmds = [g.players[0].commander, g.players[1].commander];
            let f = Game::new_with_commanders(seed, cmds);
            assert_eq!(g.hash(), f.hash());
            assert_eq!(g.offers, f.offers);
            assert_eq!(g.players[0].picks, f.players[0].picks);
        }
    }

    /// A game up to (not including) the attack of `round`: three grunts each, every earlier round fought down the middle.
    fn at_attack(seed: u64, round: u32) -> Game {
        let mut g = Game::new(seed);
        for r in 1..=round {
            g.start_round();
            g.apply_draft([[0, 1, 2], [1, 0, 2]]);
            g.apply_terraform([vec![], vec![]]);
            g.apply_build([grunts(3), grunts(3)]);
            if r < round {
                g.apply_attack([4, 4]);
                g.clash_and_resolve();
            }
        }
        g
    }

    #[test]
    fn the_crossing_moves_the_clash_and_is_clamped() {
        let outcome = |seed: u64, c: [u8; 2]| {
            let mut g = at_attack(seed, 2);
            g.apply_attack(c);
            g.clash_and_resolve();
            (g.hash(), g.players[0].core_hp, g.players[1].core_hp)
        };
        let differ = (0..30).filter(|&seed| outcome(seed, [0, 0]) != outcome(seed, [8, 8])).count();
        assert!(differ > 0, "crossing 0 and crossing 8 never changed a game");
        let mut g = at_attack(3, 1);
        g.apply_attack([200, 3]);
        assert_eq!((g.players[0].crossing, g.players[1].crossing), (8, 3));
    }

    #[test]
    fn reveal_view_hides_exactly_this_rounds_purchases() {
        for seed in 0..10 {
            let mut g = at_attack(seed, 1);
            g.apply_attack([2, 6]);
            g.clash_and_resolve();
            g.start_round();
            g.apply_draft([[0, 1, 2], [1, 0, 2]]);
            g.apply_terraform([vec![], vec![]]);
            let before: Vec<Vec<UnitKind>> = g.players.iter().map(|p| p.roster.clone()).collect();
            g.apply_build([grunts(4), grunts(4)]);
            g.apply_attack([1, 7]);
            for side in 0..2 {
                let (me, opp) = (side, 1 - side);
                let v = g.reveal_view(side);
                assert_eq!(v.players[opp].roster, before[opp], "seed {seed}: opponent roster is last round's survivors");
                let bought = g.players[opp].roster.len() - before[opp].len();
                assert!(bought > 0);
                assert_eq!(
                    v.players[opp].ore,
                    g.players[opp].ore + bought as i32 * UnitKind::Grunt.stats().cost,
                    "cost refunded"
                );
                assert_eq!(v.players[opp].crossing, 4);
                assert_eq!(v.players[me].roster, g.players[me].roster, "own army untouched");
                assert_eq!(v.players[me].ore, g.players[me].ore);
                assert_eq!(v.players[me].crossing, g.players[me].crossing);
                assert_eq!(v.terrain, g.terrain);
                assert_eq!(v.buildings.len(), g.buildings.len());
                assert!(v.rec.is_none());
            }
        }
    }

    #[test]
    fn shape_view_applies_only_the_callers_plan() {
        for seed in 0..10 {
            let mut g = Game::new(seed);
            g.start_round();
            g.apply_draft([[0, 1, 2], [1, 0, 2]]);
            let (pa, pb) = (
                g.legal_ops(0).into_iter().take(2).collect::<Vec<_>>(),
                g.legal_ops(1).into_iter().take(2).collect::<Vec<_>>(),
            );
            let mut only_a = g.sim_clone();
            only_a.apply_terraform([pa.clone(), vec![]]);
            assert_eq!(g.shape_view(0, &pa).terrain, only_a.terrain);
            assert_eq!(g.shape_view(1, &pb).terrain, {
                let mut only_b = g.sim_clone();
                only_b.apply_terraform([vec![], pb.clone()]);
                only_b.terrain
            });
            assert_eq!(g.round, 1);
        }
    }

    #[test]
    fn shaping_is_confined_to_the_own_half() {
        let mut g = Game::new(7);
        g.start_round();
        g.apply_draft([[0, 1, 2], [0, 1, 2]]);
        for side in 0..2 {
            let legal = g.legal_ops(side);
            assert!(!legal.is_empty());
            assert!(legal.iter().all(|&(t, _)| own_half(t, side) && !near_core(t)));
        }
        // an op aimed at the opponent's half is dropped, whatever the op
        let theirs = g.legal_ops(1)[0];
        let before = g.terrain;
        g.apply_terraform([vec![theirs], vec![]]);
        assert_eq!(g.terrain, before);
        assert!(g.last_ops.is_empty());
    }

    #[test]
    fn a_plan_that_cuts_the_last_path_is_reverted() {
        let mut g = Game::new(7);
        g.terrain = [Terrain::Plain; N];
        g.start_round();
        g.players[0].tp_bonus = 100;
        // a wall across row 5 of side A's half
        let wall: Vec<(usize, Op)> = (0..W as i32).filter_map(|x| idx(x, 5)).map(|t| (t, Op::Wall)).collect();
        g.apply_terraform([wall.clone(), vec![]]);
        assert!(map::ground_connected(&g.terrain));
        assert_eq!(g.last_ops.len(), wall.len());
        assert!(g.last_ops.iter().all(|r| r.outcome == OpOutcome::Reverted));
        assert_eq!(g.terrain, [Terrain::Plain; N]);
        assert_eq!(g.stats.side[0].reverted, wall.len() as i32);
    }

    /// Up to `n` legal ops for `side`, drawn with `rng`.
    fn random_plan(g: &Game, side: usize, n: usize, rng: &mut crate::Rng) -> Vec<(usize, Op)> {
        let legal = g.legal_ops(side);
        (0..n).filter(|_| !legal.is_empty()).map(|_| legal[rng.random_range(0..legal.len())]).collect()
    }

    /// Any units and buildings; `apply_build` drops what the side cannot have or pay for.
    fn random_build(g: &Game, side: usize, rng: &mut crate::Rng) -> BuildCmd {
        let mut cmd = BuildCmd::default();
        for kind in [BuildingKind::Turret, BuildingKind::Flak, BuildingKind::Extractor] {
            let tiles = g.building_tiles(side, kind);
            if !tiles.is_empty() && rng.random_range(0..2) == 0 {
                cmd.buildings.push((kind, tiles[rng.random_range(0..tiles.len())]));
            }
        }
        for _ in 0..rng.random_range(1..7) {
            cmd.units.push(ALL_UNITS[rng.random_range(0..ALL_UNITS.len())]);
        }
        cmd
    }

    /// A game of random drafts, plans, builds and crossings, stopped after the draft of `round` (or when it is over).
    fn random_game(seed: u64, round: u32, record: bool) -> Game {
        let mut rng = crate::Rng::seed_from_u64(seed ^ 0x5eed);
        let mut g = Game::new(seed);
        if record {
            g = g.with_recording();
        }
        while !g.is_over() {
            g.start_round();
            let mut order = || {
                let k = rng.random_range(0..3);
                [k, (k + 1) % 3, (k + 2) % 3]
            };
            g.apply_draft([order(), order()]);
            if g.round == round {
                break;
            }
            let plans = [random_plan(&g, 0, 2, &mut rng), random_plan(&g, 1, 2, &mut rng)];
            g.apply_terraform(plans);
            let builds = [random_build(&g, 0, &mut rng), random_build(&g, 1, &mut rng)];
            g.apply_build(builds);
            g.apply_attack([rng.random_range(0..9), rng.random_range(0..9)]);
            g.clash_and_resolve();
        }
        g
    }

    /// The clash applies a run of pure waiting ticks in one go, except while recording. So a recorded game is the
    /// tick-by-tick reference, and the two must agree on everything, with every unit kind, building and scar in play.
    #[test]
    fn skipping_the_waits_of_a_march_changes_no_fight() {
        for seed in 0..60 {
            let (plain, recorded) = (random_game(seed, 0, false), random_game(seed, 0, true));
            assert_eq!(plain.hash(), recorded.hash(), "seed {seed}");
            assert_eq!(plain.players, recorded.players, "seed {seed}");
            assert_eq!(plain.buildings, recorded.buildings, "seed {seed}");
            assert_eq!(plain.scar_under, recorded.scar_under, "seed {seed}");
        }
    }

    /// `clash::unaffected` is a proof: a board it clears fights the same fight (same players, buildings, scars and
    /// touched tiles). Tried on every single-tile change of terrain, legal op or not, with every unit kind and building
    /// unlocked, armies that are all air on some boards (so a wall changes no path, only the wall bonus) and hover on
    /// others. And it clears enough boards to be worth having.
    #[test]
    fn a_board_proven_unaffected_fights_the_same_fight() {
        const ALL: [Terrain; 7] = [
            Terrain::Plain,
            Terrain::High,
            Terrain::Trench,
            Terrain::Wall,
            Terrain::Water,
            Terrain::Forest,
            Terrain::Ore,
        ];
        let (mut proven, mut tried) = (0, 0);
        for seed in 0..12u64 {
            for round in 1..=4 {
                let mut g = random_game(seed, round, false);
                if g.is_over() {
                    continue;
                }
                let mut rng = crate::Rng::seed_from_u64(seed * 10 + round as u64);
                for (side, p) in g.players.iter_mut().enumerate() {
                    p.units = [true; 6];
                    p.buildings = [true; 3];
                    p.ore += 30;
                    p.hover = (seed + side as u64) % 4 == 0;
                    p.camo = (seed + side as u64) % 3 == 0;
                }
                let mut builds = [random_build(&g, 0, &mut rng), random_build(&g, 1, &mut rng)];
                if seed % 3 == 0 {
                    for p in g.players.iter_mut() {
                        p.roster.retain(|u| u.is_air());
                    }
                    for b in builds.iter_mut() {
                        b.units = vec![UnitKind::Drone, UnitKind::Gunship, UnitKind::Drone];
                    }
                }
                let mut base = g.sim_clone();
                base.apply_terraform([random_plan(&g, 0, 2, &mut rng), random_plan(&g, 1, 2, &mut rng)]);
                base.apply_build(builds);
                let fight = |b: &Game, cross: [u8; 2]| {
                    let mut c = b.sim_clone();
                    c.apply_attack(cross);
                    let touched = c.clash_and_resolve_traced();
                    (c, touched)
                };
                for cross in [[4, 4], [rng.random_range(0..9), rng.random_range(0..9)]] {
                    let (want, touched) = fight(&base, cross);
                    for x in 0..N {
                        for to in ALL {
                            let mut cand = base.sim_clone();
                            cand.terrain[x] = to;
                            if to == base.terrain[x] || near_core(x) || !map::ground_connected(&cand.terrain) {
                                continue;
                            }
                            tried += 1;
                            if !clash::unaffected(&base, &cand, cross, &touched) {
                                continue;
                            }
                            proven += 1;
                            let (got, got_touched) = fight(&cand, cross);
                            let what = format!("seed {seed} round {round} crossings {cross:?}: tile {x} to {to:?}");
                            assert_eq!(got.players, want.players, "{what}");
                            assert_eq!(got.buildings, want.buildings, "{what}");
                            assert_eq!(got.last_scars, want.last_scars, "{what}");
                            assert!(got_touched == touched, "{what}");
                        }
                    }
                }
            }
        }
        assert!(proven * 3 > tried, "only {proven} of {tried} boards proven unaffected");
    }

    /// `field_unmoved_off` is a proof too: when it says a tile change leaves a distance field alone, the field searched
    /// from scratch on the changed terrain is the old one everywhere but on that tile.
    #[test]
    fn a_field_proven_unmoved_is_unmoved() {
        use crate::map::{dist_field_uncached, field_unmoved_off};
        const ALL: [Terrain; 7] = [
            Terrain::Plain,
            Terrain::High,
            Terrain::Trench,
            Terrain::Wall,
            Terrain::Water,
            Terrain::Forest,
            Terrain::Ore,
        ];
        let mut rng = crate::Rng::seed_from_u64(7);
        let (mut proven, mut moved) = (0, 0);
        for seed in 0..40 {
            let mut t = Game::new(seed).terrain;
            // Half the boards get extra clutter, so walls, water and trenches sit on and beside shortest paths.
            for _ in 0..(seed % 2) * 30 {
                t[rng.random_range(0..N)] = ALL[rng.random_range(0..ALL.len())];
            }
            for _ in 0..400 {
                let (x, to) = (rng.random_range(0..N), ALL[rng.random_range(0..ALL.len())]);
                let target = [CORE[0], CORE[1], rng.random_range(0..N)][rng.random_range(0..3)];
                let (kind, hover) = (ALL_UNITS[rng.random_range(0..ALL_UNITS.len())], rng.random_range(0..2) == 0);
                let mut changed = t;
                changed[x] = to;
                let (before, after) = (dist_field_uncached(&t, target, kind, hover), dist_field_uncached(&changed, target, kind, hover));
                let same = (0..N).all(|y| y == x || before[y] == after[y]);
                if field_unmoved_off(&t, x, to, target, kind, hover) {
                    proven += 1;
                    assert!(same, "seed {seed}: tile {x} to {to:?}, target {target}, {kind:?}, hover {hover}");
                }
                moved += usize::from(!same);
            }
        }
        assert!(proven > 4000 && moved > 500, "{proven} proven, {moved} moved: the test does not cover both");
    }

    #[test]
    fn a_recorded_game_is_valid_json_and_recording_changes_nothing() {
        let run = |record: bool| {
            let mut g = Game::new(5);
            if record {
                g = g.with_recording();
            }
            while !g.is_over() {
                g.start_round();
                g.apply_draft([[0, 1, 2], [1, 0, 2]]);
                let ops = g.legal_ops(0).into_iter().take(1).collect();
                g.apply_terraform([ops, vec![]]);
                g.apply_build([grunts(4), grunts(4)]);
                g.apply_attack([3, 5]);
                g.clash_and_resolve();
            }
            g
        };
        let (plain, recorded) = (run(false), run(true));
        assert_eq!(plain.hash(), recorded.hash());
        let json = recorded.rec.as_ref().unwrap().finish(recorded.winner(), ["a \"quoted\" bot", "b"], ["x", "y"]);
        let v: serde_json::Value = serde_json::from_str(&json).expect("the replay parses");
        assert_eq!(v["bots"][0], "a \"quoted\" bot");
        assert_eq!(v["rounds"].as_array().unwrap().len(), recorded.round as usize);
        assert_eq!(v["rounds"][0]["crossings"], serde_json::json!([3, 5]));
        assert!(v["rounds"][0]["frames"].as_array().is_some_and(|f| !f.is_empty()));
    }
}
