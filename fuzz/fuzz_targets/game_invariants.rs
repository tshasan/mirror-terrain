#![no_main]

use libfuzzer_sys::fuzz_target;
use mt_core::game::{BUILDING_CAP, START_CORE_HP};
use mt_core::*;
use mt_fuzz::Bytes;

fn check(g: &Game) {
    assert!(map::ground_connected(&g.terrain), "ground path cut");
    for side in 0..2 {
        let p = &g.players[side];
        assert!((0..=START_CORE_HP).contains(&p.core_hp), "core hp {} out of range", p.core_hp);
        assert!(p.ore >= 0, "negative ore {}", p.ore);
        assert!(p.roster.len() <= ROSTER_CAP, "roster over cap");
        assert!(CROSSINGS.contains(&p.crossing), "crossing {}", p.crossing);
        assert!(g.buildings.iter().filter(|b| b.side == side).count() <= BUILDING_CAP, "building cap");
    }
    for (i, b) in g.buildings.iter().enumerate() {
        assert!(buildable_row(b.tile, b.side) && b.tile != CORE[b.side], "building on illegal tile {}", b.tile);
        assert!(g.buildings[..i].iter().all(|o| o.tile != b.tile), "two buildings on tile {}", b.tile);
    }
    for (&t, _) in &g.scar_under {
        assert!(!near_core(t), "scar beside a core on tile {t}");
        assert!(!g.terrain[t].ground_blocked(), "a scar blocks ground on tile {t}");
    }
}

/// `clash::unaffected` is a proof: a board it clears must fight the same fight as `g` (called at the attack phase). The
/// boards tried differ from `g` on one tile; which tile and what it becomes comes from the game's own hash, so the
/// check takes no bytes from the input.
fn check_unaffected(g: &Game) {
    const ALL: [Terrain; 7] = [
        Terrain::Plain,
        Terrain::High,
        Terrain::Trench,
        Terrain::Wall,
        Terrain::Water,
        Terrain::Forest,
        Terrain::Ore,
    ];
    let cross = [g.players[0].crossing, g.players[1].crossing];
    let mut base = g.sim_clone();
    let touched = base.clash_and_resolve_traced();
    let mut pick = g.hash();
    for _ in 0..6 {
        pick = pick.wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(23) ^ 0x51;
        let (x, to) = ((pick % N as u64) as usize, ALL[(pick >> 32) as usize % ALL.len()]);
        let mut cand = g.sim_clone();
        cand.terrain[x] = to;
        if near_core(x) || !map::ground_connected(&cand.terrain) || !clash::unaffected(g, &cand, cross, &touched) {
            continue;
        }
        let cand_touched = cand.clash_and_resolve_traced();
        assert!(
            cand.players == base.players && cand.buildings == base.buildings && cand_touched == touched,
            "tile {x} to {to:?} was proven unaffected, and the fight changed"
        );
    }
}

fn play(data: &[u8]) -> u64 {
    let mut input = Bytes::new(data);
    let seed = input.u64();
    let mut g = Game::new(seed);
    if input.u8() & 1 == 1 {
        g = g.with_recording();
    }
    check(&g);
    while !g.is_over() {
        g.start_round();
        g.apply_draft([input.order(), input.order()]);
        let before = g.terrain;
        g.apply_terraform([input.plan(), input.plan()]);
        // Shaping is confined to the own half: every op record names a tile on its side's half.
        for r in &g.last_ops {
            assert!(own_half(r.tile, r.side), "side {} shaped tile {} on the other half", r.side, r.tile);
        }
        for t in 0..N {
            assert!(g.terrain[t] == before[t] || g.last_ops.iter().any(|r| r.tile == t), "tile {t} changed without an op");
        }
        check(&g);
        g.apply_build([input.build(), input.build()]);
        check(&g);
        // Out-of-range bytes are on purpose: `apply_attack` must clamp them.
        g.apply_attack([input.u8(), input.u8()]);
        check(&g);
        check_unaffected(&g);
        g.clash_and_resolve();
        check(&g);
    }
    assert!(g.round <= MAX_ROUNDS);
    let w = g.winner();
    assert!(w < 2);
    if let Some(rec) = &g.rec {
        let json = rec.finish(w, ["a", "b"], [g.players[0].commander.name(), g.players[1].commander.name()]);
        assert!(!json.is_empty());
    }
    g.hash()
}

fuzz_target!(|data: &[u8]| {
    assert_eq!(play(data), play(data), "same inputs, different game");
});
