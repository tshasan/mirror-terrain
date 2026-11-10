use crate::defs::*;
use crate::game::Game;
use crate::map::{dist_field, field_unmoved_off, move_class, neighbors};

pub const MAX_TICKS: u32 = 220;
pub const FRAME_EVERY: u32 = 2;
pub const AGGRO: i32 = 3;
pub const CORE_GUN: (i32, i32, u32) = (2, 2, 8);

#[derive(Clone, Debug)]
pub struct CUnit {
    pub id: u32,
    pub side: usize,
    pub kind: UnitKind,
    pub tile: usize,
    pub hp: i32,
    cd: u32,
    prog: u32,
    wp_done: bool,
    ord: usize,
    pub alive: bool,
}

#[derive(Clone, Copy)]
enum Tgt {
    Unit(usize),
    Bld(usize),
}

/// What a unit kind on one side needs to move: distance fields to its waypoint and to the enemy core, and the cost of
/// entering each tile (0 = impassable; every real cost is at least 100), so the tick loop never calls `move_cost`.
#[derive(Clone, Copy)]
struct KindMap {
    to_wp: [u32; N],
    to_core: [u32; N],
    cost: [u32; N],
}

const SPAWN_ROWS: [i32; 4] = [3, 4, 2, 5];

/// Spawn columns for a side in its own frame, in priority order: the crossing column and its two neighbours, clipped
/// to the board.
fn spawn_cols(crossing: i32) -> Vec<i32> {
    [crossing, crossing - 1, crossing + 1]
        .into_iter()
        .filter(|x| (0..W as i32).contains(x))
        .collect()
}

/// `probed` gets every tile (in the A frame, like `t`) whose terrain decided the answer.
fn waypoint_aframe(t: &[Terrain; N], col: i32, air: bool, probed: &mut [bool; N]) -> usize {
    for row in [9, 10, 8, 11] {
        for dc in [0, -1, 1, -2, 2, -3, 3] {
            if let Some(i) = idx(col + dc, row) {
                if air {
                    return i;
                }
                probed[i] = true;
                if !t[i].ground_blocked() {
                    return i;
                }
            }
        }
    }
    CORE[1]
}

/// `side`'s waypoints for crossing column `col`, as board tiles: `[ground, air]`. The terrain is read from the side's
/// point of view. Every tile whose terrain decided the answer is marked in `touched`.
fn waypoints(terrain: &[Terrain; N], side: usize, col: i32, touched: &mut [bool; N]) -> [usize; 2] {
    let mut view = *terrain;
    for (i, v) in view.iter_mut().enumerate() {
        *v = terrain[rel(i, side)];
    }
    let mut probed = [false; N];
    let wps = [false, true].map(|air| rel(waypoint_aframe(&view, col, air, &mut probed), side));
    for (i, _) in probed.iter().enumerate().filter(|(_, p)| **p) {
        touched[rel(i, side)] = true;
    }
    wps
}

/// Tiles with a Wall among their eight neighbours. Terrain is fixed for the whole clash, so this is computed once
/// instead of scanning neighbours for every unit every tick.
fn wall_adjacent(g: &Game) -> [bool; N] {
    let mut adj = [false; N];
    for (i, a) in adj.iter_mut().enumerate() {
        *a = neighbors(i, true).any(|n| g.terrain[n] == Terrain::Wall);
    }
    adj
}

fn wall_bonus(adj: &[bool; N], tile: usize, side: usize) -> bool {
    own_half(tile, side) && adj[tile]
}

fn turf_out(pct: i32, dmg: i32, tile: usize, side: usize) -> i32 {
    if pct > 0 && own_half(tile, side) { dmg * (100 + pct) / 100 } else { dmg }
}

fn turf_in(pct: i32, dmg: i32, target: &CUnit) -> i32 {
    if pct > 0 && own_half(target.tile, target.side) {
        (dmg * (100 - pct) / 100).max(1)
    } else {
        dmg
    }
}

fn forest_hidden(g: &Game, target: &CUnit, attacker_air: bool, d: i32) -> bool {
    !target.kind.is_air() && g.terrain[target.tile] == Terrain::Forest && (attacker_air || d > 1)
}

fn unit_dmg(g: &Game, base: i32, target: &CUnit) -> i32 {
    if target.kind.is_air() {
        return base;
    }
    match g.terrain[target.tile] {
        Terrain::High => (base / 2).max(1),
        Terrain::Forest if target.kind == UnitKind::Ranger || g.players[target.side].camo => (base / 2).max(1),
        Terrain::Forest => (base * 3 / 4).max(1),
        _ => base,
    }
}

/// The bounding box of one side's living units: `[min x, max x, min y, max y]`, empty (`EMPTY_BOX`) when it has none.
type Box4 = [i32; 4];
/// Further than any range from every tile, without overflow.
const EMPTY_BOX: Box4 = [1000, -1000, 1000, -1000];

/// A lower bound on the Chebyshev distance from `tile` to anything inside `b`. Exact pruning: when it is above a range,
/// no unit of that side is within the range, so a scan over them could not find one.
fn box_gap(b: &Box4, tile: usize) -> i32 {
    let (x, y) = xy(tile);
    (b[0] - x).max(x - b[1]).max(b[2] - y).max(y - b[3])
}

/// What one clash leaves for the resolve step: breach tiers per side, ground deaths per tile and the tiles of destroyed
/// buildings. Recorded for the scars; the clash itself never reads it.
pub struct Clash {
    pub breach: [i32; 2],
    /// Ground units killed by damage on each tile (the tile the unit stood on at the end of its last tick). Air deaths and
    /// breaching units are not counted.
    pub deaths: [u16; N],
    /// Tiles of buildings destroyed this clash, in building order.
    pub destroyed: Vec<usize>,
    /// Every tile whose own terrain the clash or its scars can have read: tiles a unit stood on or was about to step
    /// onto, spawn slots and waypoint candidates that were tested, and every building's tile. What the clash reads
    /// around these tiles (walls beside them, whether a neighbour is passable, the distance fields) is `unaffected`'s
    /// business. A trace, not state: nothing in the rules reads it.
    pub touched: [bool; N],
}

/// Whether the clash and resolve of `cand` at `crossings` is certain to go exactly as it went for `base`, whose clash at
/// the same crossings touched `touched` (`Game::clash_and_resolve_traced`): the same fights tick for tick, so the same
/// players and buildings afterwards and the same `touched`. Both games are at the attack phase. It is a proof, never a
/// guess: `false` only means "not proven", and the clash has to be run. Bots use it to skip clashes on boards that
/// differ from one they have already fought on by a tile the fight never came near.
///
/// The proof: with everything but the terrain equal and the terrain different on one tile `x`, the clash reads terrain
/// in four ways. Directly, on `touched` tiles only, so `x` must not be one. As `wall_adjacent` of a touched tile, so `x`
/// may not gain or lose a Wall beside one. As "can this kind enter the neighbour", so `x` may not open or close for a
/// kind in play beside one. And through the distance fields, which must be equal wherever a unit can have looked: on
/// touched tiles and their neighbours. If every value read is equal, every tick decides the same, by induction.
pub fn unaffected(base: &Game, cand: &Game, crossings: [u8; 2], touched: &[bool; N]) -> bool {
    if base.seed != cand.seed || base.round != cand.round || base.players != cand.players || base.buildings != cand.buildings {
        return false;
    }
    let mut diff = (0..N).filter(|&i| base.terrain[i] != cand.terrain[i]);
    let Some(x) = diff.next() else { return true };
    // Two or more tiles differ (a path-safety revert): not analysed.
    if diff.next().is_some() || touched[x] {
        return false;
    }
    let looked_at = |y: usize| touched[y] || neighbors(y, true).any(|n| touched[n]);
    let beside = looked_at(x);
    let (from, to) = (base.terrain[x], cand.terrain[x]);
    if beside && (from == Terrain::Wall) != (to == Terrain::Wall) {
        return false;
    }
    for side in 0..2 {
        // The waypoints depend on tiles that are all in `touched`, so they are the same on both boards.
        let wps = waypoints(&base.terrain, side, crossings[side].min(8) as i32, &mut [false; N]);
        let hover = base.players[side].hover;
        let mut seen = [false; 4];
        for &kind in &base.players[side].roster {
            if std::mem::replace(&mut seen[move_class(kind) as usize], true) {
                continue;
            }
            let (a, b) = (move_cost(kind, from, hover), move_cost(kind, to, hover));
            if a == b {
                continue;
            }
            if beside && a.is_some() != b.is_some() {
                return false;
            }
            for target in [wps[kind.is_air() as usize], CORE[1 - side]] {
                // The cheap local proof first; the two full fields only when it fails.
                if field_unmoved_off(&base.terrain, x, to, target, kind, hover) {
                    continue;
                }
                let (f, h) = (dist_field(&base.terrain, target, kind, hover), dist_field(&cand.terrain, target, kind, hover));
                if (0..N).any(|y| f[y] != h[y] && looked_at(y)) {
                    return false;
                }
            }
        }
    }
    true
}

pub fn run(g: &mut Game) -> Clash {
    let terrain = g.terrain;
    let turf = g.turf_pct();
    let wall_adj = wall_adjacent(g);
    let mut touched = [false; N];
    for b in &g.buildings {
        touched[b.tile] = true;
    }
    let mut units: Vec<CUnit> = Vec::new();
    for side in 0..2 {
        let cols = spawn_cols(g.players[side].crossing as i32);
        let slots: Vec<usize> = SPAWN_ROWS.iter().flat_map(|&r| cols.iter().filter_map(move |&c| idx(c, r))).collect();
        // Taken rather than cloned: `next_id` needs `&mut g` inside the loop. Put back below.
        let roster = std::mem::take(&mut g.players[side].roster);
        for (k, &kind) in roster.iter().enumerate() {
            let mut tile = CORE[side];
            for s in 0..slots.len() {
                let a = slots[(k + s) % slots.len()];
                let real = rel(a, side);
                if kind.is_air() {
                    tile = real;
                    break;
                }
                touched[real] = true;
                if !terrain[real].ground_blocked() {
                    tile = real;
                    break;
                }
            }
            touched[tile] = true;
            let id = g.next_id();
            units.push(CUnit {
                id,
                side,
                kind,
                tile,
                hp: kind.stats().hp,
                cd: 0,
                prog: 0,
                wp_done: false,
                ord: k,
                alive: true,
            });
        }
        g.players[side].roster = roster;
    }
    // Indexed by (side, kind as usize); `dist_field` memoises across clashes, this only avoids recomputing per unit.
    let mut maps: Vec<KindMap> = Vec::with_capacity(4);
    let mut slot = [[u8::MAX; 6]; 2];
    let mut wps = [[0usize; 2]; 2];
    for side in 0..2 {
        wps[side] = waypoints(&terrain, side, g.players[side].crossing as i32, &mut touched);
        let hover = g.players[side].hover;
        for &kind in &g.players[side].roster {
            if slot[side][kind as usize] != u8::MAX {
                continue;
            }
            slot[side][kind as usize] = maps.len() as u8;
            maps.push({
                let wp = wps[side][kind.is_air() as usize];
                let mut cost = [0u32; N];
                for (c, &t) in cost.iter_mut().zip(terrain.iter()) {
                    *c = move_cost(kind, t, hover).unwrap_or(0);
                }
                KindMap {
                    to_wp: dist_field(&terrain, wp, kind, hover),
                    to_core: dist_field(&terrain, CORE[1 - side], kind, hover),
                    cost,
                }
            });
        }
    }
    let mut bhp: Vec<i32> = g.buildings.iter().map(|b| b.hp).collect();
    let mut bcd: Vec<u32> = vec![0; g.buildings.len()];
    let mut core_cd = [0u32; 2];
    let mut breach = [0i32; 2];
    let mut lost: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    let mut deaths = [0u16; N];
    let mut pending_attacks: Vec<(usize, usize)> = Vec::new();

    if let Some(r) = g.rec.as_mut() {
        r.frame(0, &units, &[], &g.buildings, &bhp);
    }

    let mut unit_dmg_acc = vec![0i32; units.len()];
    let mut bld_dmg_acc = vec![0i32; g.buildings.len()];
    let mut breachers: Vec<usize> = Vec::new();
    let mut foes: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    let mut new_state: Vec<(usize, usize, u32, u32, bool)> = Vec::new();
    // (unit, cost of the step it is saving up for), for the units that marched this tick without moving.
    let mut saving: Vec<(usize, u32)> = Vec::new();

    let mut tick = 0;
    while tick < MAX_TICKS {
        tick += 1;
        if !units.iter().any(|u| u.alive) {
            break;
        }
        // Per-tick scratch, reused across ticks (cleared here, so nothing carries over).
        unit_dmg_acc.iter_mut().for_each(|d| *d = 0);
        bld_dmg_acc.iter_mut().for_each(|d| *d = 0);
        breachers.clear();
        // Who is alive is fixed for the whole tick (deaths apply at the end), so index each side's living units once.
        // Ascending index order is kept, which the first-wins tie-breaks below rely on.
        for f in foes.iter_mut() {
            f.clear();
        }
        let mut boxes = [EMPTY_BOX; 2];
        for (i, u) in units.iter().enumerate() {
            if u.alive {
                foes[u.side].push(i);
                let (x, y) = xy(u.tile);
                let b = &mut boxes[u.side];
                *b = [b[0].min(x), b[1].max(x), b[2].min(y), b[3].max(y)];
            }
        }
        new_state.clear();
        saving.clear();
        // Stays true while this tick is a pure wait: nobody fights, chases, breaches or moves (see the skip below).
        let mut still = true;

        for (i, u) in units.iter().enumerate() {
            if !u.alive {
                continue;
            }
            let st = u.kind.stats();
            if cheb(u.tile, CORE[1 - u.side]) <= 1 {
                breachers.push(i);
                still = false;
                continue;
            }
            // Ground ranged units see further from high ground.
            let range = st.range + if !st.air && st.range >= 2 && terrain[u.tile] == Terrain::High { 2 } else { 0 };
            let ubonus = wall_bonus(&wall_adj, u.tile, u.side);
            // Most ticks are a march with no enemy in reach: skip the scans the box proves empty.
            let gap = box_gap(&boxes[1 - u.side], u.tile);
            let mut best: Option<((i32, usize, usize), Tgt)> = None;
            for &j in if gap <= range { &foes[1 - u.side][..] } else { &[] } {
                let v = &units[j];
                let d = cheb(u.tile, v.tile);
                if d > range {
                    continue;
                }
                if v.kind.is_air() && !st.hits_air {
                    continue;
                }
                if st.air && !v.kind.is_air() && terrain[v.tile] == Terrain::Forest {
                    continue;
                }
                if forest_hidden(g, v, st.air, d) {
                    continue;
                }
                let key = (d, rel(v.tile, u.side), v.ord);
                if best.as_ref().is_none_or(|(k, _)| key < *k) {
                    best = Some((key, Tgt::Unit(j)));
                }
            }
            if best.is_none() {
                for (j, b) in g.buildings.iter().enumerate() {
                    if b.side == u.side || bhp[j] <= 0 {
                        continue;
                    }
                    let d = cheb(u.tile, b.tile);
                    if d > range {
                        continue;
                    }
                    let key = (d, rel(b.tile, u.side), 0);
                    if best.as_ref().is_none_or(|(k, _)| key < *k) {
                        best = Some((key, Tgt::Bld(j)));
                    }
                }
            }
            if let Some((_, tgt)) = best {
                still = false;
                if u.cd == 0 {
                    match tgt {
                        Tgt::Unit(j) => {
                            let base = turf_out(turf, if ubonus { st.dmg * 3 / 2 } else { st.dmg }, u.tile, u.side);
                            unit_dmg_acc[j] += turf_in(turf, unit_dmg(g, base, &units[j]), &units[j]);
                            pending_attacks.push((u.tile, units[j].tile));
                        }
                        Tgt::Bld(j) => {
                            bld_dmg_acc[j] += turf_out(turf, if ubonus { st.dmg * 3 / 2 } else { st.dmg }, u.tile, u.side);
                            pending_attacks.push((u.tile, g.buildings[j].tile));
                        }
                    }
                    new_state.push((i, u.tile, st.cooldown, u.prog, u.wp_done));
                } else {
                    new_state.push((i, u.tile, u.cd - 1, u.prog, u.wp_done));
                }
                continue;
            }
            let wp = wps[u.side][st.air as usize];
            let wp_done = u.wp_done || cheb(u.tile, wp) <= 1;
            let Some(km) = maps.get(slot[u.side][u.kind as usize] as usize) else {
                continue;
            };
            let field = if wp_done { &km.to_core } else { &km.to_wp };
            let here = field[u.tile];
            let mut chase: Option<((i32, usize, usize), usize)> = None;
            for &j in if gap <= AGGRO { &foes[1 - u.side][..] } else { &[] } {
                let v = &units[j];
                if (v.kind.is_air() && !st.hits_air) || (st.air && !v.kind.is_air() && terrain[v.tile] == Terrain::Forest) {
                    continue;
                }
                let d = cheb(u.tile, v.tile);
                if d <= AGGRO {
                    let key = (d, rel(v.tile, u.side), v.ord);
                    if chase.as_ref().is_none_or(|(k, _)| key < *k) {
                        chase = Some((key, v.tile));
                    }
                }
            }
            let mut next: Option<usize> = None;
            still &= chase.is_none();
            if let Some((_, goal)) = chase {
                let cur = cheb(u.tile, goal);
                for n in neighbors(u.tile, st.air) {
                    if km.cost[n] == 0 || field[n] == u32::MAX {
                        continue;
                    }
                    let dn = cheb(n, goal);
                    if dn < cur && next.is_none_or(|m| (dn, rel(n, u.side)) < (cheb(m, goal), rel(m, u.side))) {
                        next = Some(n);
                    }
                }
            }
            if next.is_none() {
                for n in neighbors(u.tile, st.air) {
                    if field[n] < here && next.is_none_or(|m| (field[n], rel(n, u.side)) < (field[m], rel(m, u.side))) {
                        next = Some(n);
                    }
                }
            }
            let mut tile = u.tile;
            let mut prog = (u.prog + st.speed).min(600);
            if let Some(n) = next {
                // Its exact cost is read here even if the unit cannot afford the step yet.
                touched[n] = true;
                let c = km.cost[n];
                if c != 0 && prog >= c {
                    prog -= c;
                    tile = n;
                    still = false;
                } else if c != 0 {
                    saving.push((i, c));
                }
            }
            new_state.push((i, tile, u.cd.saturating_sub(1), prog, wp_done));
        }

        for (j, b) in g.buildings.iter().enumerate() {
            let bs = b.kind.stats();
            if bhp[j] <= 0 || bs.dmg == 0 {
                continue;
            }
            let range = bs.range + if terrain[b.tile] == Terrain::High { 1 } else { 0 };
            let empty = box_gap(&boxes[1 - b.side], b.tile) > range;
            // Judged whether or not the gun is loaded: a wait may only be skipped if it could not fire anyway.
            still &= empty;
            if bcd[j] > 0 {
                bcd[j] -= 1;
                continue;
            }
            if empty {
                continue;
            }
            let mut best: Option<((i32, usize, usize), usize)> = None;
            for &i in &foes[1 - b.side] {
                let v = &units[i];
                let air = v.kind.is_air();
                if (air && !bs.hits_air) || (!air && !bs.hits_ground) {
                    continue;
                }
                let d = cheb(b.tile, v.tile);
                if d > range || forest_hidden(g, v, false, d) {
                    continue;
                }
                let key = (d, rel(v.tile, b.side), v.ord);
                if best.as_ref().is_none_or(|(k, _)| key < *k) {
                    best = Some((key, i));
                }
            }
            if let Some((_, i)) = best {
                let base = if wall_bonus(&wall_adj, b.tile, b.side) { bs.dmg * 3 / 2 } else { bs.dmg };
                unit_dmg_acc[i] += turf_in(turf, unit_dmg(g, base, &units[i]), &units[i]);
                pending_attacks.push((b.tile, units[i].tile));
                bcd[j] = bs.cooldown;
            }
        }

        for side in 0..2 {
            let (dmg, range, cd) = CORE_GUN;
            let empty = box_gap(&boxes[1 - side], CORE[side]) > range;
            still &= empty;
            if core_cd[side] > 0 {
                core_cd[side] -= 1;
                continue;
            }
            if empty {
                continue;
            }
            let mut best: Option<((i32, usize, usize), usize)> = None;
            for &i in &foes[1 - side] {
                let v = &units[i];
                let d = cheb(CORE[side], v.tile);
                if d > range || forest_hidden(g, v, false, d) {
                    continue;
                }
                let key = (d, rel(v.tile, side), v.ord);
                if best.as_ref().is_none_or(|(k, _)| key < *k) {
                    best = Some((key, i));
                }
            }
            if let Some((_, i)) = best {
                unit_dmg_acc[i] += unit_dmg(g, dmg, &units[i]);
                pending_attacks.push((CORE[side], units[i].tile));
                core_cd[side] = cd;
            }
        }

        for &i in &breachers {
            let u = &mut units[i];
            u.alive = false;
            breach[u.side] += u.kind.stats().tier;
            // The attack wears itself out (Clausewitz): a unit that breaches is spent, like one that dies.
            lost[u.side].push(u.ord);
        }
        for &(i, tile, cd, prog, wp_done) in &new_state {
            let u = &mut units[i];
            u.tile = tile;
            u.cd = cd;
            u.prog = prog;
            u.wp_done = wp_done;
        }
        for (i, d) in unit_dmg_acc.iter().enumerate() {
            if *d > 0 && units[i].alive {
                units[i].hp -= d;
                if units[i].hp <= 0 {
                    units[i].alive = false;
                    if !units[i].kind.is_air() {
                        deaths[units[i].tile] += 1;
                    }
                    lost[units[i].side].push(units[i].ord);
                }
            }
        }
        for (j, d) in bld_dmg_acc.iter().enumerate() {
            bhp[j] -= d;
        }

        if tick % FRAME_EVERY == 0 {
            if let Some(r) = g.rec.as_mut() {
                r.frame(tick, &units, &pending_attacks, &g.buildings, &bhp);
            }
            pending_attacks.clear();
        }

        // Marching is mostly waiting: a step costs 100 or more and a unit saves 9 to 14 a tick. After a tick in which
        // nobody fought, chased, breached or moved and no gun had an enemy near, the next tick finds the same
        // positions and decides the same, and so on until the first unit can pay for its step. Those ticks only count
        // down cooldowns and add up progress, so they are applied in one go. Not while recording: the replay wants
        // its frames.
        if still && g.rec.is_none() {
            let mut wait = MAX_TICKS - tick;
            for &(i, c) in &saving {
                let u = &units[i];
                // `prog < c` here, or the unit would have moved.
                wait = wait.min((c - u.prog).div_ceil(u.kind.stats().speed) - 1);
            }
            if wait > 0 {
                for u in units.iter_mut().filter(|u| u.alive) {
                    u.prog = (u.prog + wait * u.kind.stats().speed).min(600);
                    u.cd = u.cd.saturating_sub(wait);
                }
                for (j, b) in g.buildings.iter().enumerate() {
                    if bhp[j] > 0 && b.kind.stats().dmg != 0 {
                        bcd[j] = bcd[j].saturating_sub(wait);
                    }
                }
                for cd in core_cd.iter_mut() {
                    *cd = cd.saturating_sub(wait);
                }
                tick += wait;
            }
        }
    }

    for (j, b) in g.buildings.iter_mut().enumerate() {
        b.hp = bhp[j];
    }
    let destroyed: Vec<usize> = g.buildings.iter().filter(|b| b.hp <= 0).map(|b| b.tile).collect();
    g.buildings.retain(|b| b.hp > 0);
    for side in 0..2 {
        let mut l = std::mem::take(&mut lost[side]);
        l.sort_unstable();
        l.dedup();
        for &k in l.iter().rev() {
            if k < g.players[side].roster.len() {
                g.players[side].roster.remove(k);
            }
        }
    }
    Clash {
        breach,
        deaths,
        destroyed,
        touched,
    }
}
