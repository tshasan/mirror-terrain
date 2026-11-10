#![no_main]

use libfuzzer_sys::fuzz_target;
use mt_core::*;
use mt_fuzz::Bytes;

fn flip(t: usize) -> usize {
    if t < N { mirror(t) } else { t }
}

fuzz_target!(|data: &[u8]| {
    let mut input = Bytes::new(data);
    let seed = input.u64();
    let mut g = Game::new(seed);
    g.players[1] = g.players[0].clone();
    while !g.is_over() {
        g.start_round();
        let order = input.order();
        g.apply_draft([order, order]);
        let plan = input.plan();
        let plan_b = plan.iter().map(|&(t, o)| (flip(t), o)).collect();
        g.apply_terraform([plan, plan_b]);
        let cmd = input.build();
        let mut cmd_b = cmd.clone();
        cmd_b.buildings = cmd.buildings.iter().map(|&(k, t)| (k, flip(t))).collect();
        g.apply_build([cmd, cmd_b]);
        let cross = input.u8();
        g.apply_attack([cross, cross]);
        g.clash_and_resolve();
        let (a, b) = (&g.players[0], &g.players[1]);
        assert_eq!(a.core_hp, b.core_hp, "core hp diverged in round {}", g.round);
        assert_eq!(a.ore, b.ore, "ore diverged in round {}", g.round);
        assert_eq!(a.roster, b.roster, "roster diverged in round {}", g.round);
        // Mirrored fights leave mirrored scars, so the board stays point-symmetric.
        for i in 0..N {
            assert_eq!(g.terrain[i], g.terrain[mirror(i)], "terrain lost point symmetry at tile {i} in round {}", g.round);
        }
    }
});
