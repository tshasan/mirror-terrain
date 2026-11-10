use crate::Rng;
use crate::defs::*;
use rand::RngExt;
use rustc_hash::FxHashMap;
use std::cell::RefCell;

/// Neighbour tables, built at compile time in the same order the direction lists give (D4 first four of D8), clipped to
/// the board. `neighbors` is on the hot path of both the clash and the distance search.
const NB_MAX: usize = 8;
type NbTable = [([u8; NB_MAX], u8); N];

const fn build_neighbors(diag: bool) -> NbTable {
    const D8: [(i32, i32); 8] = [(0, 1), (1, 0), (0, -1), (-1, 0), (1, 1), (1, -1), (-1, 1), (-1, -1)];
    let dirs = if diag { 8 } else { 4 };
    let mut t = [([0u8; NB_MAX], 0u8); N];
    let mut i = 0;
    while i < N {
        let (x, y) = ((i % W) as i32, (i / W) as i32);
        let mut d = 0;
        while d < dirs {
            let (nx, ny) = (x + D8[d].0, y + D8[d].1);
            if nx >= 0 && ny >= 0 && nx < W as i32 && ny < H as i32 {
                let n = t[i].1 as usize;
                t[i].0[n] = (ny as usize * W + nx as usize) as u8;
                t[i].1 += 1;
            }
            d += 1;
        }
        i += 1;
    }
    t
}

static NB4: NbTable = build_neighbors(false);
static NB8: NbTable = build_neighbors(true);

pub fn neighbors(i: usize, diag: bool) -> impl Iterator<Item = usize> {
    let (list, n) = if diag { &NB8[i] } else { &NB4[i] };
    list[..*n as usize].iter().map(|&v| v as usize)
}

pub fn ground_connected(t: &[Terrain; N]) -> bool {
    connected_with_block(t, None)
}

fn connected_with_block(t: &[Terrain; N], blocked: Option<usize>) -> bool {
    let mut seen = [false; N];
    let mut stack = vec![CORE[0]];
    seen[CORE[0]] = true;
    while let Some(u) = stack.pop() {
        if u == CORE[1] {
            return true;
        }
        for v in neighbors(u, false) {
            if !seen[v] && !t[v].ground_blocked() && Some(v) != blocked {
                seen[v] = true;
                stack.push(v);
            }
        }
    }
    false
}

pub fn no_single_tile_cut(t: &[Terrain; N]) -> bool {
    (0..N)
        .filter(|&i| i != CORE[0] && i != CORE[1] && !t[i].ground_blocked())
        .all(|i| connected_with_block(t, Some(i)))
}

pub fn generate(rng: &mut Rng) -> [Terrain; N] {
    for _ in 0..200 {
        let mut t = [Terrain::Plain; N];
        let place = |rng: &mut Rng, t: &mut [Terrain; N], kind: Terrain, count: usize, rows: (i32, i32), cluster: bool| {
            let mut placed = 0;
            let mut guard = 0;
            while placed < count && guard < 200 {
                guard += 1;
                let x = rng.random_range(0..W) as i32;
                let y = rng.random_range(rows.0 as usize..=rows.1 as usize) as i32;
                let Some(i) = idx(x, y) else { continue };
                if i >= HALF || near_core(i) || t[i] != Terrain::Plain {
                    continue;
                }
                if kind == Terrain::High && cheb(i, CORE[0]) <= 3 {
                    continue;
                }
                t[i] = kind;
                placed += 1;
                if cluster && rng.random_range(0..2) == 0 {
                    if let Some(j) = idx(x + 1, y) {
                        if j < HALF && !near_core(j) && t[j] == Terrain::Plain {
                            t[j] = kind;
                        }
                    }
                }
            }
        };
        let ore = rng.random_range(2..=3);
        let high = rng.random_range(1..=3);
        let forest = rng.random_range(2..=4);
        let water = rng.random_range(0..=2);
        place(rng, &mut t, Terrain::Ore, ore, (1, 5), false);
        place(rng, &mut t, Terrain::High, high, (3, 7), true);
        place(rng, &mut t, Terrain::Forest, forest, (2, 7), true);
        place(rng, &mut t, Terrain::Water, water, (4, 7), false);
        for i in 0..HALF {
            t[mirror(i)] = t[i];
        }
        let ore_near = (0..N).filter(|&i| t[i] == Terrain::Ore && cheb(i, CORE[0]) <= 5).count();
        if ground_connected(&t) && no_single_tile_cut(&t) && ore_near >= 2 {
            return t;
        }
    }
    [Terrain::Plain; N]
}

/// Movement class of a unit kind: the only thing `move_cost` reads besides terrain and `hover`. Kinds in the same
/// class share one distance field (0 = ordinary ground, 1 = Rhino, 2 = Ranger, 3 = air).
pub(crate) fn move_class(kind: UnitKind) -> u8 {
    if kind.is_air() {
        3
    } else {
        match kind {
            UnitKind::Rhino => 1,
            UnitKind::Ranger => 2,
            _ => 0,
        }
    }
}

type FieldKey = ([u8; N], u16);

/// Per-thread memo size bound; the whole memo is dropped when it is reached (about 6 MB).
const FIELD_CACHE_CAP: usize = 8192;

thread_local! {
    static FIELD_CACHE: RefCell<FxHashMap<FieldKey, [u32; N]>> = RefCell::new(FxHashMap::default());
}

/// Distance field to `target` over `t` for a unit kind. Memoised per thread on (terrain, target, movement class, hover):
/// bots run many clashes on identical terrain. The memo is only ever probed by key, so it cannot affect results.
pub fn dist_field(t: &[Terrain; N], target: usize, kind: UnitKind, hover: bool) -> [u32; N] {
    let class = move_class(kind);
    // `hover` changes nothing for air units; normalise it so equal fields share a slot.
    let hover = hover && class != 3;
    let mut terr = [0u8; N];
    for (o, &c) in terr.iter_mut().zip(t.iter()) {
        *o = c as u8;
    }
    let key: FieldKey = (terr, target as u16 | (class as u16) << 8 | (hover as u16) << 10);
    FIELD_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(f) = c.get(&key) {
            return *f;
        }
        let f = dist_field_uncached(t, target, kind, hover);
        if c.len() >= FIELD_CACHE_CAP {
            c.clear();
        }
        c.insert(key, f);
        f
    })
}

/// Exact shortest distances to `target`, where leaving tile `u` costs `move_cost(u)`. A FIFO label-correcting search
/// over a fixed ring (no heap, no allocation): on this 144-tile grid with a handful of cost values it settles in
/// about one pass, and any label-correcting order converges to the same exact distances as Dijkstra.
pub fn dist_field_uncached(t: &[Terrain; N], target: usize, kind: UnitKind, hover: bool) -> [u32; N] {
    let mut cost = [0u32; N]; // 0 = impassable (every real cost is at least 100)
    for (c, &tt) in cost.iter_mut().zip(t.iter()) {
        *c = move_cost(kind, tt, hover).unwrap_or(0);
    }
    let mut dist = [u32::MAX; N];
    let mut ring = [0u16; N];
    let mut queued = [false; N];
    let (mut head, mut len) = (0usize, 0usize);
    dist[target] = 0;
    ring[0] = target as u16;
    queued[target] = true;
    len += 1;
    let diag = kind.is_air();
    while len > 0 {
        let u = ring[head] as usize;
        head = if head + 1 == N { 0 } else { head + 1 };
        len -= 1;
        queued[u] = false;
        let cu = cost[u];
        if cu == 0 {
            continue;
        }
        let nd = dist[u].saturating_add(cu);
        for v in neighbors(u, diag) {
            if cost[v] != 0 && nd < dist[v] {
                dist[v] = nd;
                if !queued[v] {
                    queued[v] = true;
                    ring[(head + len) % N] = v as u16;
                    len += 1;
                }
            }
        }
    }
    dist
}

/// Whether changing tile `x` of `t` to `to` is certain to leave the distance field to `target` as it is on every tile
/// except `x` itself. A local proof on the memoised field of `t`: the field is the only solution of "a passable tile's
/// distance is the least `distance + cost` over its passable neighbours", `x`'s cost appears in the equations of
/// `x`'s neighbours only, and `x`'s own distance does not depend on its own cost. So if every neighbour of `x` keeps
/// its distance with `x` at the new cost, the old field with `x`'s new distance solves the new equations and is the
/// new field. `false` means "not proven", not "moved".
pub fn field_unmoved_off(t: &[Terrain; N], x: usize, to: Terrain, target: usize, kind: UnitKind, hover: bool) -> bool {
    if x == target {
        return false;
    }
    let d = dist_field(t, target, kind, hover);
    let cost = |y: usize| move_cost(kind, if y == x { to } else { t[y] }, hover).unwrap_or(0);
    let diag = kind.is_air();
    // What tile `v` gets from its neighbours when `x` has distance `dx`.
    let relaxed = |v: usize, dx: u32| -> u32 {
        let mut best = u32::MAX;
        for u in neighbors(v, diag) {
            let du = if u == x { dx } else { d[u] };
            if cost(u) != 0 && du != u32::MAX {
                best = best.min(du.saturating_add(cost(u)));
            }
        }
        best
    };
    let dx = if cost(x) == 0 { u32::MAX } else { relaxed(x, u32::MAX) };
    neighbors(x, diag).all(|v| v == target || cost(v) == 0 || relaxed(v, dx) == d[v])
}

/// The original heap-based Dijkstra, kept as the reference the tests compare the fast search against.
#[cfg(test)]
pub fn dist_field_dijkstra(t: &[Terrain; N], target: usize, kind: UnitKind, hover: bool) -> [u32; N] {
    let mut dist = [u32::MAX; N];
    let mut heap = std::collections::BinaryHeap::new();
    dist[target] = 0;
    heap.push(std::cmp::Reverse((0u32, target)));
    let diag = kind.is_air();
    while let Some(std::cmp::Reverse((d, u))) = heap.pop() {
        if d > dist[u] {
            continue;
        }
        let Some(cu) = move_cost(kind, t[u], hover) else { continue };
        for v in neighbors(u, diag) {
            if move_cost(kind, t[v], hover).is_none() {
                continue;
            }
            let nd = d.saturating_add(cu);
            if nd < dist[v] {
                dist[v] = nd;
                heap.push(std::cmp::Reverse((nd, v)));
            }
        }
    }
    dist
}
