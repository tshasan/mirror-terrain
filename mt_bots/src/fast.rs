//! The shaper's searches: which ops to play (`deep_plan`) and where to put buildings (`searched_placement`).
//!
//! Solving the full 3x9 crossing game for every legal own-half op (about 280 of them, 27 clashes each, two or three
//! passes) is slow, and about four out of five ops change no cell of that matrix at all. So both searches screen first:
//! `deep_plan` runs only the cells where the two armies meet (my crossing c against the opponent's own-frame column
//! 8 - c: one cell per row, 3 clashes) for every op, and completes the full matrix for the few ops whose meeting cells
//! moved. Against the unscreened search the screen lost 0.03 core-HP-equivalents per helpful decision, for a seventh
//! of the cost.
//!
//! On top of that, no clash is run whose result is already known: every solved cell keeps the tiles its clash touched,
//! and a candidate op that `mt_core::clash::unaffected` proves irrelevant to a cell takes that cell's value from the
//! board it was added to. That is a proof, not a second screen: the plans are the same with and without it (the
//! `skipping_proven_clashes_changes_no_plan` test), only cheaper.
//!
//! Everything here is plain `f64` arithmetic over fixed loops: no hashing, no clocks, no transcendental functions.

use super::*;

/// A payoff matrix of at most 3 rows and 9 columns, stored flat on the stack.
#[derive(Clone, Copy, Debug)]
pub struct Mat {
    pub r: usize,
    pub c: usize,
    pub v: [[f64; 9]; 3],
}

impl Mat {
    pub fn new(r: usize, c: usize) -> Mat {
        Mat { r, c, v: [[0.0; 9]; 3] }
    }
}

fn guaranteed(m: &Mat, p: [f64; 3]) -> f64 {
    let mut worst = f64::INFINITY;
    for j in 0..m.c {
        let mut s = 0.0;
        for i in 0..m.r {
            s += p[i] * m.v[i][j];
        }
        if s < worst {
            worst = s;
        }
    }
    worst
}

/// The value of the zero-sum game `m` for the row player, exact for up to 3 rows and any number of columns. The optimum of
/// the lower envelope over the mixing simplex sits at a vertex of the arrangement: a pure row, two rows with one pair of
/// columns tied, or three rows with a triple of columns tied. Enumerating those needs no allocation
/// and no Gaussian elimination; the tests pin it to the reference solver `game_value`.
pub fn game_value_small(m: &Mat) -> f64 {
    let (r, c) = (m.r, m.c);
    if r == 0 || c == 0 {
        return 0.0;
    }
    let mut best = f64::NEG_INFINITY;
    for i in 0..r {
        best = best.max(guaranteed(
            m,
            [if i == 0 { 1.0 } else { 0.0 }, if i == 1 { 1.0 } else { 0.0 }, if i == 2 { 1.0 } else { 0.0 }],
        ));
    }
    for i1 in 0..r {
        for i2 in i1 + 1..r {
            for j1 in 0..c {
                for j2 in j1 + 1..c {
                    let (a1, b1) = (m.v[i1][j1], m.v[i2][j1]);
                    let (a2, b2) = (m.v[i1][j2], m.v[i2][j2]);
                    let d = (a1 - b1) - (a2 - b2);
                    if d.abs() < 1e-12 {
                        continue;
                    }
                    let p = (b2 - b1) / d;
                    if !(-1e-9..=1.0 + 1e-9).contains(&p) {
                        continue;
                    }
                    let p = p.clamp(0.0, 1.0);
                    let mut pv = [0.0; 3];
                    pv[i1] = p;
                    pv[i2] = 1.0 - p;
                    best = best.max(guaranteed(m, pv));
                }
            }
        }
    }
    if r == 3 {
        for j1 in 0..c {
            for j2 in j1 + 1..c {
                for j3 in j2 + 1..c {
                    let a = [m.v[0][j1] - m.v[0][j2], m.v[1][j1] - m.v[1][j2], m.v[2][j1] - m.v[2][j2]];
                    let b = [m.v[0][j2] - m.v[0][j3], m.v[1][j2] - m.v[1][j3], m.v[2][j2] - m.v[2][j3]];
                    let x = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
                    let sum = x[0] + x[1] + x[2];
                    if sum.abs() < 1e-12 {
                        continue;
                    }
                    let p = [x[0] / sum, x[1] / sum, x[2] / sum];
                    if p.iter().any(|&q| q < -1e-9) {
                        continue;
                    }
                    best = best.max(guaranteed(m, [p[0].max(0.0), p[1].max(0.0), p[2].max(0.0)]));
                }
            }
        }
    }
    best
}

/// A board after one candidate shaping plan with both sides' default builds applied, ready to evaluate crossing cells.
/// The rows are my crossings (`GATES`) and the columns every opponent crossing.
pub struct Setup {
    side: usize,
    built: Game,
}

impl Setup {
    /// `g` with `plan` applied for `side` only, and `style`'s default build for me and the opponent's commander style for them.
    pub fn new(g: &Game, side: usize, plan: &[(usize, Op)], style: Style) -> Setup {
        let mut built = g.shape_view(side, plan);
        let mine = default_build(&built, side, style);
        let theirs = default_build(&built, 1 - side, commander_style(&built, 1 - side));
        // The build does not depend on the crossing, so it is applied once.
        built.apply_build(if side == 0 { [mine, theirs] } else { [theirs, mine] });
        Setup { side, built }
    }

    fn crossings(&self, i: usize, j: usize) -> [u8; 2] {
        let (a, b) = (GATES[i], CROSSINGS[j]);
        if self.side == 0 {
            [a, b]
        } else {
            [b, a]
        }
    }

    /// `eval` for `side` after the clash of my row `i` against their column `j`.
    pub fn cell(&self, i: usize, j: usize) -> f64 {
        self.cell_traced(i, j).0
    }

    /// `cell`, and the tiles its clash touched.
    fn cell_traced(&self, i: usize, j: usize) -> Cell {
        let mut c = self.built.sim_clone();
        c.apply_attack(self.crossings(i, j));
        let touched = c.clash_and_resolve_traced();
        (eval(&c, self.side), touched)
    }

    /// `cell_traced`, without the clash when `base`'s clash of the same cell is proven to go the same way here.
    fn cell_after(&self, base: &Solved, i: usize, j: usize) -> Cell {
        if base.skip && mt_core::clash::unaffected(&base.st.built, &self.built, self.crossings(i, j), &base.touched[i][j]) {
            (base.m.v[i][j], base.touched[i][j])
        } else {
            self.cell_traced(i, j)
        }
    }

    pub fn matrix(&self) -> Mat {
        let mut m = Mat::new(GATES.len(), CROSSINGS.len());
        for i in 0..m.r {
            for j in 0..m.c {
                m.v[i][j] = self.cell(i, j);
            }
        }
        m
    }
}

/// A solved cell: its value and the tiles its clash touched.
type Cell = (f64, [bool; N]);

/// A board with its whole matrix and, cell by cell, the tiles the clash touched.
struct Solved {
    st: Setup,
    m: Mat,
    touched: [[[bool; N]; 9]; 3],
    /// Off only in the test that compares plans with and without skipping.
    skip: bool,
}

impl Solved {
    /// Solves `st`. `known` cells are taken as given; the rest come from `base` where that is proven, else from a clash.
    fn new(st: Setup, base: Option<&Solved>, known: &[((usize, usize), Cell)], skip: bool) -> Solved {
        let mut m = Mat::new(GATES.len(), CROSSINGS.len());
        let mut touched = [[[false; N]; 9]; 3];
        for i in 0..m.r {
            for j in 0..m.c {
                (m.v[i][j], touched[i][j]) = match (known.iter().find(|k| k.0 == (i, j)), base) {
                    (Some(k), _) => k.1,
                    (None, Some(b)) => st.cell_after(b, i, j),
                    (None, None) => st.cell_traced(i, j),
                };
            }
        }
        Solved { st, m, touched, skip }
    }
}

/// One cell per row where the armies meet: my crossing `a` meets the opponent's column `8 - a` in their own frame.
pub const MEETING_CELLS: [(usize, usize); 3] = [(0, 7), (1, 4), (2, 1)];

/// The top this many flagged ops by the screen get a full matrix.
pub const SCREEN_TOP: usize = 24;

/// Greedy shaping by search. Pass by pass: every legal op that fits is scored on the meeting cells only; those whose
/// cells moved (sorted by total change, best first, at most `top`) get the full matrix; the best strict improvement over
/// the current plan joins it, until none helps or the TP run out. Flagged ops are compared in legal-op order, so ties
/// go to the first legal op.
pub fn deep_plan(g: &Game, side: usize, style: Style, top: usize) -> Vec<(usize, Op)> {
    deep_plan_with(g, side, style, top, true)
}

/// `skip` off runs every clash: the reference `deep_plan` is tested against.
pub(crate) fn deep_plan_with(g: &Game, side: usize, style: Style, top: usize, skip: bool) -> Vec<(usize, Op)> {
    let mut plan: Vec<(usize, Op)> = Vec::new();
    let mut budget = g.tp(side);
    let legal = g.legal_ops(side);
    let mut base = Solved::new(Setup::new(g, side, &plan, style), None, &[], skip);
    let mut base_v = game_value_small(&base.m);
    while budget > 0 {
        let cells = MEETING_CELLS;
        let base_vals: Vec<f64> = cells.iter().map(|&(i, j)| base.m.v[i][j]).collect();
        // (change, index into `legal`, setup, the cells: value and touched tiles)
        let mut flagged: Vec<(f64, usize, Setup, Vec<Cell>)> = Vec::new();
        for (li, &(tile, op)) in legal.iter().enumerate() {
            if op.cost() > budget || plan.iter().any(|&(t, _)| t == tile) {
                continue;
            }
            let mut cand = plan.clone();
            cand.push((tile, op));
            let st = Setup::new(g, side, &cand, style);
            let got: Vec<Cell> = cells.iter().map(|&(i, j)| st.cell_after(&base, i, j)).collect();
            if got.iter().map(|c| c.0).ne(base_vals.iter().copied()) {
                let delta: f64 = got.iter().zip(&base_vals).map(|(a, b)| a.0 - b).sum();
                flagged.push((delta, li, st, got));
            }
        }
        flagged.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
        flagged.truncate(top);
        flagged.sort_by_key(|f| f.1);
        let mut best: Option<(f64, usize, Solved)> = None;
        for (_, li, st, got) in flagged {
            let known: Vec<((usize, usize), Cell)> = cells.iter().copied().zip(got).collect();
            let solved = Solved::new(st, Some(&base), &known, skip);
            let v = game_value_small(&solved.m);
            if v > base_v && best.as_ref().is_none_or(|b| v > b.0) {
                best = Some((v, li, solved));
            }
        }
        match best {
            Some((v, li, solved)) => {
                let (tile, op) = legal[li];
                budget -= op.cost();
                plan.push((tile, op));
                base_v = v;
                base = solved;
            }
            None => break,
        }
    }
    plan
}

/// Columns of the placement screen: the opponent crossings that hurt the incumbent placement most.
const PLACE_SCREEN_COLS: usize = 3;
/// Tiles that survive the placement screen and get all columns.
const PLACE_TOP: usize = 3;

/// Building placement by search. It keeps the kinds and count of the build and, building by building, scores every
/// candidate tile on the `PLACE_SCREEN_COLS` opponent crossings that are worst for the building where it stands; the
/// best `PLACE_TOP` tiles (ties in tile order) are scored on all nine crossings, and a tile replaces the incumbent only
/// if it strictly beats it there. It draws no random numbers.
pub fn searched_placement(g: &Game, side: usize, mut cmd: BuildCmd) -> BuildCmd {
    if cmd.buildings.is_empty() {
        return cmd;
    }
    let opp = default_build(g, 1 - side, commander_style(g, 1 - side));
    let cols = CROSSINGS;
    let one = |cmd: &BuildCmd, j: usize| -> f64 { rollout_from_build(g, side, cmd, &opp, Some(cols[j])) };
    let all = |cmd: &BuildCmd| -> Vec<f64> { (0..cols.len()).map(|j| one(cmd, j)).collect() };
    for i in 0..cmd.buildings.len() {
        let kind = cmd.buildings[i].0;
        let inc = all(&cmd);
        let mut best = inc.iter().sum::<f64>() / cols.len() as f64;
        let mut tiles = g.building_tiles(side, kind);
        tiles.sort_by_key(|&t| rel(t, side));
        // the screen columns: worst first, ties toward the middle
        let mut order: Vec<usize> = (0..cols.len()).collect();
        order.sort_by(|&a, &b| {
            inc[a]
                .partial_cmp(&inc[b])
                .unwrap_or(std::cmp::Ordering::Equal)
                .then((cols[a] as i32 - 4).abs().cmp(&(cols[b] as i32 - 4).abs()))
        });
        let screen: Vec<usize> = order.into_iter().take(PLACE_SCREEN_COLS).collect();
        let mut scored: Vec<(f64, usize, BuildCmd, Vec<f64>)> = Vec::new();
        for (k, &t) in tiles.iter().enumerate() {
            if cmd.buildings.iter().any(|&(_, u)| u == t) {
                continue;
            }
            let mut cand = cmd.clone();
            cand.buildings[i].1 = t;
            let vals: Vec<f64> = screen.iter().map(|&j| one(&cand, j)).collect();
            scored.push((vals.iter().sum(), k, cand, vals));
        }
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
        scored.truncate(PLACE_TOP);
        scored.sort_by_key(|s| s.1);
        for (_, _, cand, vals) in scored {
            let mut full = vec![0.0; cols.len()];
            for j in 0..cols.len() {
                full[j] = match screen.iter().position(|&s| s == j) {
                    Some(p) => vals[p],
                    None => one(&cand, j),
                };
            }
            let v = full.iter().sum::<f64>() / cols.len() as f64;
            if v > best {
                best = v;
                cmd = cand;
            }
        }
    }
    cmd
}
