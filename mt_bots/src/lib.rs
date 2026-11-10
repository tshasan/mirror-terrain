#![forbid(unsafe_code)]
#![allow(clippy::needless_range_loop)]

use mt_core::rand::seq::SliceRandom;
use mt_core::rand::{Rng as _, RngExt, SeedableRng};
use mt_core::*;

mod fast;
pub use fast::*;

pub trait Bot: Send {
    fn name(&self) -> String;
    /// Draft: a ranked order of the three offers (a permutation of 0, 1, 2).
    fn draft(&mut self, g: &Game, side: usize) -> [usize; 3];
    /// Shape: (tile, op) pairs on the bot's own half.
    fn terraform(&mut self, g: &Game, side: usize) -> Vec<(usize, Op)>;
    /// Build. `g` is `Game::shape_view(side, own plan)`: the opponent's shaping of this round is not on it.
    fn build(&mut self, g: &Game, side: usize) -> BuildCmd;
    /// Attack: pick a crossing column 0-8 in own frame. `g` is `Game::reveal_view(side)`.
    fn attack(&mut self, g: &Game, side: usize) -> u8 {
        default_crossing(g, side)
    }
}

/// Crossing columns in tie-break order: the middle first, then outwards. Every "ties go to the middle" rule uses this.
const TIE_ORDER: [u8; 9] = [4, 3, 5, 2, 6, 1, 7, 0, 8];

/// Own-frame column with the fewest enemy non-extractor buildings within one column; ties go to the middle first.
/// Reads only buildings, which are public at the attack phase.
pub fn default_crossing(g: &Game, side: usize) -> u8 {
    let mut best = (i32::MAX, 4u8);
    for c in TIE_ORDER {
        let d = g
            .buildings
            .iter()
            .filter(|b| b.side != side && b.kind != BuildingKind::Extractor)
            .filter(|b| (xy(rel(b.tile, side)).0 - c as i32).abs() <= 1)
            .count() as i32;
        if d < best.0 {
            best = (d, c);
        }
    }
    best.1
}

/// The draft phase, the single place a loop drafts.
pub fn draft_round(g: &mut Game, a: &mut dyn Bot, b: &mut dyn Bot) {
    let orders = [a.draft(g, 0), b.draft(g, 1)];
    g.apply_draft(orders);
}

/// Give both sides their heuristic crossing. Rollouts that call `clash_and_resolve` use this so the clash never runs
/// on a stale crossing.
fn apply_default_attack(s: &mut Game) {
    let c = [default_crossing(s, 0), default_crossing(s, 1)];
    s.apply_attack(c);
}

/// The builds of both bots, each chosen on its own `shape_view`. Call this BEFORE
/// `apply_terraform`; a build never sees the opponent's shaping of the same round.
pub fn shape_builds(g: &Game, plans: &[Vec<(usize, Op)>; 2], a: &mut dyn Bot, b: &mut dyn Bot) -> [BuildCmd; 2] {
    let (va, vb) = (g.shape_view(0, &plans[0]), g.shape_view(1, &plans[1]));
    [a.build(&va, 0), b.build(&vb, 1)]
}

/// Both crossings, each chosen on the bot's `reveal_view`. Call after `apply_build`.
pub fn attacks(g: &Game, a: &mut dyn Bot, b: &mut dyn Bot) -> [u8; 2] {
    [a.attack(&g.reveal_view(0), 0), b.attack(&g.reveal_view(1), 1)]
}

/// One whole round after the draft, the single place the phase order lives for loops with no instrumentation:
/// shape and build on shape views, apply both, attack on reveal views, apply, clash.
pub fn play_round_after_draft(g: &mut Game, a: &mut dyn Bot, b: &mut dyn Bot) {
    let plans = [a.terraform(g, 0), b.terraform(g, 1)];
    let builds = shape_builds(g, &plans, a, b);
    g.apply_terraform(plans);
    g.apply_build(builds);
    let cross = attacks(g, a, b);
    g.apply_attack(cross);
    g.clash_and_resolve();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Balanced,
    Rush,
    Turtle,
    Air,
    Ground,
}

impl Style {
    pub fn name(self) -> &'static str {
        match self {
            Style::Balanced => "balanced",
            Style::Rush => "rush",
            Style::Turtle => "turtle",
            Style::Air => "air",
            Style::Ground => "ground",
        }
    }
}

fn card_score(g: &Game, side: usize, c: Card, style: Style) -> f64 {
    let p = &g.players[side];
    let fresh = match c {
        Card::Unit(u) => !p.units[u as usize],
        Card::Building(b) => !p.buildings[b as usize],
        _ => true,
    };
    let mut s = match c {
        Card::Unit(u) => 3.0 + if u.stats().tier == 2 { 1.0 } else { 0.0 },
        Card::Building(BuildingKind::Extractor) => 2.5,
        Card::Building(_) => 2.0,
        Card::Earthshaper => 2.5,
        Card::Prospector => 1.5,
        _ => 1.5,
    };
    if !fresh {
        s = 0.7;
    }
    let bonus = match (style, c.tag()) {
        (Style::Air, Tag::Air) | (Style::Ground, Tag::Ground) | (Style::Turtle, Tag::Defense) => 3.0,
        (Style::Rush, Tag::Ground) | (Style::Rush, Tag::Air) => 1.0,
        (Style::Air, Tag::Ground) | (Style::Ground, Tag::Air) => -2.0,
        _ => 0.0,
    };
    s + bonus
}

pub fn commander_style(g: &Game, side: usize) -> Style {
    let forced = std::env::var("MT_STYLE").ok().and_then(|v| match v.as_str() {
        "balanced" => Some(Style::Balanced),
        "rush" => Some(Style::Rush),
        "turtle" => Some(Style::Turtle),
        "air" => Some(Style::Air),
        "ground" => Some(Style::Ground),
        _ => None,
    });
    if let Some(s) = forced {
        return s;
    }
    match g.players[side].commander {
        Commander::Skywing => Style::Air,
        Commander::Ironclad => Style::Ground,
        Commander::Warden => Style::Turtle,
        Commander::Geomancer => Style::Balanced,
    }
}

/// All three offers best first by `card_score`, ties by index.
pub fn ranked_draft(g: &Game, side: usize, style: Style) -> [usize; 3] {
    let score = [0, 1, 2].map(|i| card_score(g, side, g.offers[i], style));
    let mut order = [0usize, 1, 2];
    order.sort_by(|&a, &b| score[b].partial_cmp(&score[a]).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(&b)));
    order
}

/// A uniformly random order: one draw for the first choice, one more to order the rest.
fn random_order(rng: &mut Rng) -> [usize; 3] {
    let d = rng.random_range(0..3);
    let mut rest: Vec<usize> = (0..3).filter(|&i| i != d).collect();
    if rng.random_range(0..2) == 1 {
        rest.reverse();
    }
    [d, rest[0], rest[1]]
}

fn unit_pref(style: Style) -> &'static [UnitKind] {
    use UnitKind::*;
    match style {
        Style::Balanced => &[Gunship, Rhino, Archer, Ranger, Drone, Grunt],
        Style::Rush => &[Grunt, Drone, Ranger, Archer, Rhino, Gunship],
        Style::Turtle => &[Archer, Rhino, Ranger, Gunship, Drone, Grunt],
        Style::Air => &[Gunship, Drone, Archer, Ranger, Rhino, Grunt],
        Style::Ground => &[Rhino, Archer, Ranger, Grunt, Drone, Gunship],
    }
}

fn enemy_air_share(g: &Game, side: usize) -> f64 {
    g.players[1 - side].air_share()
}

pub fn default_build(g: &Game, side: usize, style: Style) -> BuildCmd {
    let p = &g.players[side];
    let mut ore = p.ore;
    let mut cmd = BuildCmd::default();
    let mut taken: Vec<usize> = Vec::new();
    let try_build = |kind: BuildingKind, ore: &mut i32, cmd: &mut BuildCmd, taken: &mut Vec<usize>| {
        if !p.buildings[kind as usize] || *ore < kind.stats().cost {
            return;
        }
        let tile = g
            .building_tiles(side, kind)
            .into_iter()
            .filter(|t| !taken.contains(t))
            .min_by_key(|&t| ((cheb(t, CORE[side]) - 3).abs(), rel(t, side)));
        if let Some(t) = tile {
            *ore -= kind.stats().cost;
            taken.push(t);
            cmd.buildings.push((kind, t));
        }
    };
    let my_bld = |k: BuildingKind| g.buildings.iter().filter(|b| b.side == side && b.kind == k).count();
    if style != Style::Rush && g.round <= 3 && my_bld(BuildingKind::Extractor) < 2 {
        try_build(BuildingKind::Extractor, &mut ore, &mut cmd, &mut taken);
    }
    let def_target = if style == Style::Turtle { 4 } else { 1 };
    if style != Style::Rush {
        if enemy_air_share(g, side) > 0.3 && my_bld(BuildingKind::Flak) < def_target {
            try_build(BuildingKind::Flak, &mut ore, &mut cmd, &mut taken);
        }
        if my_bld(BuildingKind::Turret) < def_target {
            try_build(BuildingKind::Turret, &mut ore, &mut cmd, &mut taken);
        }
        if style == Style::Turtle && my_bld(BuildingKind::Turret) + 1 < def_target {
            try_build(BuildingKind::Turret, &mut ore, &mut cmd, &mut taken);
        }
    }
    let prefs: Vec<UnitKind> = unit_pref(style).iter().copied().filter(|u| p.units[*u as usize]).collect();
    let mut room = ROSTER_CAP.saturating_sub(p.roster.len());
    let mut i = 0;
    let mut stalled = 0;
    while room > 0 && !prefs.is_empty() && stalled < prefs.len() {
        let u = prefs[i % prefs.len()];
        i += 1;
        if ore >= u.stats().cost {
            ore -= u.stats().cost;
            cmd.units.push(u);
            room -= 1;
            stalled = 0;
        } else {
            stalled += 1;
        }
    }
    cmd
}

/// One default build per style, duplicates dropped.
pub fn candidate_builds(g: &Game, side: usize) -> Vec<BuildCmd> {
    let mut out: Vec<BuildCmd> = Vec::new();
    for style in [Style::Balanced, Style::Rush, Style::Turtle, Style::Air, Style::Ground] {
        let c = default_build(g, side, style);
        if !out.iter().any(|o| o.units == c.units && o.buildings == c.buildings) {
            out.push(c);
        }
    }
    out
}

pub fn random_plan(g: &Game, side: usize, rng: &mut Rng) -> Vec<(usize, Op)> {
    let mut legal = g.legal_ops(side);
    legal.shuffle(rng);
    let mut plan = Vec::new();
    let mut budget = g.tp(side);
    for (tile, op) in legal {
        if op.cost() <= budget && !plan.iter().any(|&(t, _): &(usize, Op)| t == tile) {
            budget -= op.cost();
            plan.push((tile, op));
        }
        if budget == 0 {
            break;
        }
    }
    plan
}

pub fn candidate_plans(g: &Game, side: usize, rng: &mut Rng, k: usize) -> Vec<Vec<(usize, Op)>> {
    let mut out: Vec<Vec<(usize, Op)>> = vec![vec![]];
    let legal = g.legal_ops(side);
    if legal.is_empty() {
        return out;
    }
    let focus: Vec<(usize, Op)> = legal.iter().copied().filter(|&(t, _)| (3..=12).contains(&xy(t).1)).collect();
    let mut tries = 0;
    while out.len() < k && tries < k * 6 {
        tries += 1;
        let pool = if tries % 2 == 0 && !focus.is_empty() { &focus } else { &legal };
        let mut plan = Vec::new();
        let mut budget = g.tp(side);
        let mut guard = 0;
        while budget > 0 && guard < 12 {
            guard += 1;
            let (tile, op) = pool[rng.random_range(0..pool.len())];
            if op.cost() <= budget && !plan.iter().any(|&(t, _): &(usize, Op)| t == tile) {
                budget -= op.cost();
                plan.push((tile, op));
            }
        }
        plan.sort();
        if !out.contains(&plan) {
            out.push(plan);
        }
    }
    out
}

pub fn eval(g: &Game, side: usize) -> f64 {
    let (me, op) = (&g.players[side], &g.players[1 - side]);
    let bval = |s: usize| -> f64 {
        g.buildings
            .iter()
            .filter(|b| b.side == s)
            .map(|b| b.kind.stats().cost as f64 * b.hp as f64 / b.kind.stats().hp as f64)
            .sum()
    };
    let mut v = (me.core_hp - op.core_hp) as f64;
    v += 0.10 * (me.roster_value() - op.roster_value()) as f64;
    v += 0.06 * (bval(side) - bval(1 - side));
    v += 0.04 * (me.ore - op.ore) as f64;
    if g.is_over() && !g.is_coin_flip() {
        v += if g.winner() == side { 30.0 } else { -30.0 };
    }
    v
}

fn rollout_from_terraform(g: &Game, side: usize, mine: &[(usize, Op)], theirs: &[(usize, Op)], style: Style) -> f64 {
    let mut s = g.sim_clone();
    let mut plans = [vec![], vec![]];
    plans[side] = mine.to_vec();
    plans[1 - side] = theirs.to_vec();
    s.apply_terraform(plans);
    let opp_style = commander_style(&s, 1 - side);
    let b = if side == 0 {
        [default_build(&s, 0, style), default_build(&s, 1, opp_style)]
    } else {
        [default_build(&s, 0, opp_style), default_build(&s, 1, style)]
    };
    s.apply_build(b);
    apply_default_attack(&mut s);
    s.clash_and_resolve();
    eval(&s, side)
}

/// `opp_cross` forces the opponent's crossing; the caller's own crossing is the heuristic one.
fn rollout_from_build(g: &Game, side: usize, mine: &BuildCmd, theirs: &BuildCmd, opp_cross: Option<u8>) -> f64 {
    let mut s = g.sim_clone();
    let mut b = [BuildCmd::default(), BuildCmd::default()];
    b[side] = mine.clone();
    b[1 - side] = theirs.clone();
    s.apply_build(b);
    let mut c = [default_crossing(&s, 0), default_crossing(&s, 1)];
    if let Some(x) = opp_cross {
        c[1 - side] = x;
    }
    s.apply_attack(c);
    s.clash_and_resolve();
    eval(&s, side)
}

pub fn regret_matching(m: &[Vec<f64>], iters: usize) -> Vec<f64> {
    let rows = m.len();
    let cols = m.first().map_or(0, |r| r.len());
    if rows == 0 || cols == 0 {
        return vec![];
    }
    let mut rr = vec![0.0; rows];
    let mut cr = vec![0.0; cols];
    let mut rsum = vec![0.0; rows];
    let strat = |reg: &[f64]| -> Vec<f64> {
        let pos: Vec<f64> = reg.iter().map(|r| r.max(0.0)).collect();
        let s: f64 = pos.iter().sum();
        if s > 0.0 {
            pos.iter().map(|p| p / s).collect()
        } else {
            vec![1.0 / reg.len() as f64; reg.len()]
        }
    };
    for _ in 0..iters {
        let rs = strat(&rr);
        let cs = strat(&cr);
        let row_val: Vec<f64> = (0..rows).map(|i| (0..cols).map(|j| m[i][j] * cs[j]).sum()).collect();
        let col_val: Vec<f64> = (0..cols).map(|j| (0..rows).map(|i| -m[i][j] * rs[i]).sum()).collect();
        let rv: f64 = (0..rows).map(|i| rs[i] * row_val[i]).sum();
        let cv: f64 = (0..cols).map(|j| cs[j] * col_val[j]).sum();
        for i in 0..rows {
            rr[i] += row_val[i] - rv;
            rsum[i] += rs[i];
        }
        for j in 0..cols {
            cr[j] += col_val[j] - cv;
        }
    }
    let t: f64 = rsum.iter().sum();
    rsum.iter().map(|x| x / t).collect()
}

fn sample(mix: &[f64], rng: &mut Rng) -> usize {
    let r = (rng.next_u64() % 1_000_000) as f64 / 1_000_000.0;
    let mut acc = 0.0;
    for (i, p) in mix.iter().enumerate() {
        acc += p;
        if r < acc {
            return i;
        }
    }
    mix.len().saturating_sub(1)
}

/// Reference solver for small zero-sum games (equal-size supports, Gaussian elimination). The bots use
/// `game_value_small`; the tests pin the two together.
#[cfg(test)]
pub fn game_value(m: &[Vec<f64>]) -> f64 {
    let (rows, cols) = (m.len(), m.first().map_or(0, |r| r.len()));
    if rows == 0 || cols == 0 {
        return 0.0;
    }
    let guaranteed = |p: &[f64]| -> f64 { (0..cols).map(|j| (0..rows).map(|i| p[i] * m[i][j]).sum::<f64>()).fold(f64::INFINITY, f64::min) };
    let mut best = f64::NEG_INFINITY;
    for rmask in 1u32..(1 << rows) {
        let rs: Vec<usize> = (0..rows).filter(|i| rmask >> i & 1 == 1).collect();
        for cmask in 1u32..(1 << cols) {
            let cs: Vec<usize> = (0..cols).filter(|j| cmask >> j & 1 == 1).collect();
            if cs.len() != rs.len() {
                continue;
            }
            let k = rs.len();
            let mut a = vec![vec![0.0; k + 2]; k + 1];
            for (r, &j) in cs.iter().enumerate() {
                for (c, &i) in rs.iter().enumerate() {
                    a[r][c] = m[i][j];
                }
                a[r][k] = -1.0;
            }
            for c in 0..k {
                a[k][c] = 1.0;
            }
            a[k][k + 1] = 1.0;
            let n = k + 1;
            let mut ok = true;
            for col in 0..n {
                let mut piv = col;
                for r in col + 1..n {
                    if a[r][col].abs() > a[piv][col].abs() {
                        piv = r;
                    }
                }
                if a[piv][col].abs() < 1e-12 {
                    ok = false;
                    break;
                }
                a.swap(col, piv);
                for r in 0..n {
                    if r != col {
                        let f = a[r][col] / a[col][col];
                        for c in col..=n {
                            a[r][c] -= f * a[col][c];
                        }
                    }
                }
            }
            if !ok {
                continue;
            }
            let mut p = vec![0.0; rows];
            for (c, &i) in rs.iter().enumerate() {
                let x = a[c][n] / a[c][c];
                if x < -1e-9 {
                    ok = false;
                    break;
                }
                p[i] = x.max(0.0);
            }
            if ok {
                best = best.max(guaranteed(&p));
            }
        }
    }
    best
}

/// The crossings the shaper considers for itself when it values a board: left, centre, right. Nine rows would triple
/// the cost of every candidate op for little gain; the opponent still gets all nine columns.
pub const GATES: [u8; 3] = [1, 4, 7];

/// The attack decision. `g` is the caller's `reveal_view`: both bases are known, the opponent's army this round is
/// not. The hidden purchase is modelled as the opponent's default build (units only; its buildings are already on the
/// board). The 9x9 crossing game is solved by regret matching and the mix is sampled with one `rng` draw.
pub fn searched_crossing(g: &Game, side: usize, rng: &mut Rng) -> u8 {
    let opp = 1 - side;
    let mut s = g.sim_clone();
    let mut b = [BuildCmd::default(), BuildCmd::default()];
    b[opp].units = default_build(g, opp, commander_style(g, opp)).units;
    s.apply_build(b);
    let cell = |a: u8, o: u8| -> f64 {
        let mut c = s.sim_clone();
        c.apply_attack(if side == 0 { [a, o] } else { [o, a] });
        c.clash_and_resolve();
        eval(&c, side)
    };
    let m: Vec<Vec<f64>> = CROSSINGS.iter().map(|&a| CROSSINGS.iter().map(|&o| cell(a, o)).collect()).collect();
    let mix = regret_matching(&m, 400);
    let pick = CROSSINGS[sample(&mix, rng)];
    if std::env::var("MT_DEBUG").is_ok() {
        eprintln!(
            "crossing side {side} round {} mix={:?} pick={pick}",
            g.round,
            mix.iter().map(|x| (x * 100.0).round() / 100.0).collect::<Vec<_>>()
        );
    }
    pick
}

pub struct RandomBot {
    rng: Rng,
}
impl RandomBot {
    pub fn new(seed: u64) -> Self {
        RandomBot { rng: Rng::seed_from_u64(seed) }
    }
}
impl Bot for RandomBot {
    fn name(&self) -> String {
        "random".into()
    }
    fn draft(&mut self, _g: &Game, _side: usize) -> [usize; 3] {
        random_order(&mut self.rng)
    }
    fn terraform(&mut self, g: &Game, side: usize) -> Vec<(usize, Op)> {
        random_plan(g, side, &mut self.rng)
    }
    fn attack(&mut self, _g: &Game, _side: usize) -> u8 {
        CROSSINGS[self.rng.random_range(0..CROSSINGS.len())]
    }
    fn build(&mut self, g: &Game, side: usize) -> BuildCmd {
        let p = &g.players[side];
        let units: Vec<UnitKind> = ALL_UNITS.iter().copied().filter(|u| p.units[*u as usize]).collect();
        let mut cmd = BuildCmd::default();
        let mut ore = p.ore;
        for kind in [BuildingKind::Turret, BuildingKind::Flak, BuildingKind::Extractor] {
            if p.buildings[kind as usize] && self.rng.random_range(0..2) == 0 {
                let tiles = g.building_tiles(side, kind);
                if !tiles.is_empty() && ore >= kind.stats().cost {
                    ore -= kind.stats().cost;
                    cmd.buildings.push((kind, tiles[self.rng.random_range(0..tiles.len())]));
                }
            }
        }
        for _ in 0..20 {
            if units.is_empty() {
                break;
            }
            let u = units[self.rng.random_range(0..units.len())];
            if ore >= u.stats().cost {
                ore -= u.stats().cost;
                cmd.units.push(u);
            }
        }
        cmd
    }
}

pub struct CounterBot {
    inner: GreedyBot,
}
impl CounterBot {
    fn sync(&mut self, g: &Game) {
        self.inner.style = if g.round <= 2 { Style::Turtle } else { Style::Rush };
    }
}
impl Bot for CounterBot {
    fn name(&self) -> String {
        "persona_counter".into()
    }
    fn draft(&mut self, g: &Game, side: usize) -> [usize; 3] {
        self.sync(g);
        self.inner.draft(g, side)
    }
    fn terraform(&mut self, g: &Game, side: usize) -> Vec<(usize, Op)> {
        self.sync(g);
        self.inner.terraform(g, side)
    }
    fn build(&mut self, g: &Game, side: usize) -> BuildCmd {
        self.sync(g);
        self.inner.build(g, side)
    }
}

/// A defender that plays the whole game well: `Style::Turtle` units and building kinds, searched placement, searched
/// shaping, a searched crossing. With `counter` it is `CounterBot` on top of that (fort in rounds 1-2, then `Style::Rush`
/// builds). `persona_turtle` is the cheap defender: default placement, sampled shaping, heuristic crossing.
pub struct FortBot {
    rng: Rng,
    counter: bool,
    style: Style,
}
impl FortBot {
    pub fn new(seed: u64, counter: bool) -> Self {
        FortBot {
            rng: Rng::seed_from_u64(seed),
            counter,
            style: Style::Turtle,
        }
    }
    fn sync(&mut self, g: &Game) {
        if self.counter {
            self.style = if g.round <= 2 { Style::Turtle } else { Style::Rush };
        }
    }
}
impl Bot for FortBot {
    fn name(&self) -> String {
        if self.counter { "persona_fort_counter" } else { "persona_fort" }.into()
    }
    fn draft(&mut self, g: &Game, side: usize) -> [usize; 3] {
        self.sync(g);
        ranked_draft(g, side, self.style)
    }
    fn terraform(&mut self, g: &Game, side: usize) -> Vec<(usize, Op)> {
        self.sync(g);
        deep_plan(g, side, self.style, SCREEN_TOP)
    }
    fn build(&mut self, g: &Game, side: usize) -> BuildCmd {
        self.sync(g);
        searched_placement(g, side, default_build(g, side, self.style))
    }
    fn attack(&mut self, g: &Game, side: usize) -> u8 {
        searched_crossing(g, side, &mut self.rng)
    }
}

pub struct GreedyBot {
    rng: Rng,
    style: Style,
    k: usize,
}
impl GreedyBot {
    pub fn new(seed: u64, style: Style) -> Self {
        GreedyBot {
            rng: Rng::seed_from_u64(seed),
            style,
            k: 8,
        }
    }
    /// The plain greedy bot plays its commander's style; a persona keeps its own.
    fn style_for(&self, g: &Game, side: usize) -> Style {
        if self.style == Style::Balanced {
            commander_style(g, side)
        } else {
            self.style
        }
    }
}
impl Bot for GreedyBot {
    fn name(&self) -> String {
        if self.style == Style::Balanced {
            "greedy".into()
        } else {
            format!("persona_{}", self.style.name())
        }
    }
    fn draft(&mut self, g: &Game, side: usize) -> [usize; 3] {
        ranked_draft(g, side, self.style_for(g, side))
    }
    fn terraform(&mut self, g: &Game, side: usize) -> Vec<(usize, Op)> {
        let cands = candidate_plans(g, side, &mut self.rng, self.k);
        let st = self.style_for(g, side);
        let mut best = (f64::MIN, 0);
        for (i, c) in cands.iter().enumerate() {
            let v = rollout_from_terraform(g, side, c, &[], st);
            if v > best.0 {
                best = (v, i);
            }
        }
        cands[best.1].clone()
    }
    fn build(&mut self, g: &Game, side: usize) -> BuildCmd {
        if self.style != Style::Balanced {
            return default_build(g, side, self.style);
        }
        let opp = default_build(g, 1 - side, commander_style(g, 1 - side));
        let cands = candidate_builds(g, side);
        let mut best = (f64::MIN, 0);
        for (i, c) in cands.iter().enumerate() {
            let v = rollout_from_build(g, side, c, &opp, None);
            if v > best.0 {
                best = (v, i);
            }
        }
        cands[best.1].clone()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TerraMode {
    /// Sample candidate plans for both sides and solve the plan game.
    Search,
    Never,
    Random,
    /// `deep_plan`: greedy search over every legal op, behind a cheap screen.
    Deep,
}

/// How a `SearchBot` picks its crossing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CrossMode {
    Searched,
    Random,
    Default,
}

pub struct SearchBot {
    rng: Rng,
    pub k_me: usize,
    pub k_opp: usize,
    pub mode: TerraMode,
    pub style: Style,
    pub rand_draft: bool,
    pub place: bool,
    pub cross: CrossMode,
    /// Build units and building kinds from `default_build` (no look at the opponent); placement may still be searched.
    pub fixed_build: bool,
    pub label: Option<&'static str>,
    /// A second stream for the control bots' random choices, so that a control and the bot it is compared with draw
    /// the same numbers from `rng` everywhere else.
    aux: Rng,
}
impl SearchBot {
    pub fn new(seed: u64, mode: TerraMode) -> Self {
        SearchBot {
            aux: Rng::seed_from_u64(seed ^ 0x9e37_79b9_7f4a_7c15),
            rand_draft: false,
            place: false,
            cross: CrossMode::Searched,
            fixed_build: false,
            label: None,
            rng: Rng::seed_from_u64(seed),
            k_me: 7,
            k_opp: 4,
            mode,
            style: Style::Balanced,
        }
    }
}
impl Bot for SearchBot {
    fn name(&self) -> String {
        if let Some(l) = self.label {
            return l.into();
        }
        "search".into()
    }
    fn draft(&mut self, g: &Game, side: usize) -> [usize; 3] {
        if self.rand_draft {
            return random_order(&mut self.aux);
        }
        ranked_draft(g, side, commander_style(g, side))
    }
    fn terraform(&mut self, g: &Game, side: usize) -> Vec<(usize, Op)> {
        self.style = commander_style(g, side);
        match self.mode {
            TerraMode::Never => return vec![],
            TerraMode::Random => return random_plan(g, side, &mut self.aux),
            TerraMode::Deep => return deep_plan(g, side, self.style, SCREEN_TOP),
            TerraMode::Search => {}
        }
        let mine = candidate_plans(g, side, &mut self.rng, self.k_me);
        let theirs = candidate_plans(g, 1 - side, &mut self.rng, self.k_opp);
        let m: Vec<Vec<f64>> = mine
            .iter()
            .map(|a| theirs.iter().map(|b| rollout_from_terraform(g, side, a, b, self.style)).collect())
            .collect();
        let mix = regret_matching(&m, 400);
        mine[sample(&mix, &mut self.rng)].clone()
    }
    fn attack(&mut self, g: &Game, side: usize) -> u8 {
        match self.cross {
            CrossMode::Random => CROSSINGS[self.aux.random_range(0..CROSSINGS.len())],
            CrossMode::Default => default_crossing(g, side),
            CrossMode::Searched => searched_crossing(g, side, &mut self.rng),
        }
    }
    fn build(&mut self, g: &Game, side: usize) -> BuildCmd {
        if self.fixed_build {
            // Burn the draw `sample` takes in the searched path so later decisions use the same stream as `shaper`.
            let _ = self.rng.next_u64();
            let cmd = default_build(g, side, commander_style(g, side));
            return if self.place { searched_placement(g, side, cmd) } else { cmd };
        }
        let mine = candidate_builds(g, side);
        let mut theirs = candidate_builds(g, 1 - side);
        theirs.truncate(self.k_opp.max(3));
        let m: Vec<Vec<f64>> = mine
            .iter()
            .map(|a| theirs.iter().map(|b| rollout_from_build(g, side, a, b, None)).collect())
            .collect();
        let mix = regret_matching(&m, 400);
        if std::env::var("MT_DEBUG").is_ok() {
            eprintln!("side {side} round {} m={:?} mix={:?}", g.round, m, mix);
        }
        let cmd = mine[sample(&mix, &mut self.rng)].clone();
        if self.place {
            searched_placement(g, side, cmd)
        } else {
            cmd
        }
    }
}

/// `shaper` that does not see scars: every decision is made on `Game::unscarred_view` of the game or view it is handed, so
/// craters and rubble look like the ground they replaced. The real game sanitises its plans as usual. It makes exactly the
/// random draws `shaper` makes. The control for criterion C6.
pub struct ScarBlindBot {
    inner: Box<dyn Bot>,
}
impl Bot for ScarBlindBot {
    fn name(&self) -> String {
        "shaper_scarblind".into()
    }
    fn draft(&mut self, g: &Game, side: usize) -> [usize; 3] {
        self.inner.draft(g, side)
    }
    fn terraform(&mut self, g: &Game, side: usize) -> Vec<(usize, Op)> {
        self.inner.terraform(&g.unscarred_view(), side)
    }
    fn build(&mut self, g: &Game, side: usize) -> BuildCmd {
        self.inner.build(&g.unscarred_view(), side)
    }
    fn attack(&mut self, g: &Game, side: usize) -> u8 {
        self.inner.attack(&g.unscarred_view(), side)
    }
}

/// Every name `make_bot` accepts.
pub const BOT_NAMES: [&str; 20] = [
    "random",
    "greedy",
    "search",
    "search_randcross",
    "search_randdraft",
    "shaper",
    "shaper_randcross",
    "shaper_fixed",
    "shaper_scarblind",
    "no_shape",
    "rand_shape",
    "shape_only",
    "place_only",
    "persona_rush",
    "persona_turtle",
    "persona_counter",
    "persona_fort",
    "persona_fort_counter",
    "persona_air",
    "persona_ground",
];

/// `SearchBot` with a label: `mode` shapes, `place` searches building placement.
fn search_bot(seed: u64, label: &'static str, mode: TerraMode, place: bool) -> SearchBot {
    let mut b = SearchBot::new(seed, mode);
    b.place = place;
    b.label = Some(label);
    b
}

/// The bot called `name` (one of `BOT_NAMES`). Panics on an unknown name:
/// a typo must not silently measure a different bot.
pub fn make_bot(name: &str, seed: u64) -> Box<dyn Bot> {
    use TerraMode::*;
    let with = |mut b: SearchBot, f: &dyn Fn(&mut SearchBot)| -> Box<dyn Bot> {
        f(&mut b);
        Box::new(b)
    };
    let rand_cross = |b: &mut SearchBot| b.cross = CrossMode::Random;
    let fixed = |b: &mut SearchBot| b.fixed_build = true;
    match name {
        "random" => Box::new(RandomBot::new(seed)),
        "greedy" => Box::new(GreedyBot::new(seed, Style::Balanced)),
        "search" => Box::new(SearchBot::new(seed, Search)),
        "search_randcross" => with(search_bot(seed, "search_randcross", Search, false), &rand_cross),
        "search_randdraft" => with(search_bot(seed, "search_randdraft", Search, false), &|b| b.rand_draft = true),
        "shaper" => Box::new(search_bot(seed, "shaper", Deep, true)),
        "shaper_randcross" => with(search_bot(seed, "shaper_randcross", Deep, true), &rand_cross),
        "shaper_fixed" => with(search_bot(seed, "shaper_fixed", Deep, true), &fixed),
        "shaper_scarblind" => Box::new(ScarBlindBot {
            inner: Box::new(search_bot(seed, "shaper", Deep, true)),
        }),
        "no_shape" => Box::new(search_bot(seed, "no_shape", Never, false)),
        "rand_shape" => Box::new(search_bot(seed, "rand_shape", Random, true)),
        "shape_only" => Box::new(search_bot(seed, "shape_only", Deep, false)),
        "place_only" => Box::new(search_bot(seed, "place_only", Never, true)),
        "persona_rush" => Box::new(GreedyBot::new(seed, Style::Rush)),
        "persona_turtle" => Box::new(GreedyBot::new(seed, Style::Turtle)),
        "persona_counter" => Box::new(CounterBot {
            inner: GreedyBot::new(seed, Style::Turtle),
        }),
        "persona_fort" => Box::new(FortBot::new(seed, false)),
        "persona_fort_counter" => Box::new(FortBot::new(seed, true)),
        "persona_air" => Box::new(GreedyBot::new(seed, Style::Air)),
        "persona_ground" => Box::new(GreedyBot::new(seed, Style::Ground)),
        _ => panic!("unknown bot `{name}`; known bots: {}", BOT_NAMES.join(" ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regret_matching_solves_rps() {
        let m = vec![vec![0.0, -1.0, 1.0], vec![1.0, 0.0, -1.0], vec![-1.0, 1.0, 0.0]];
        let mix = regret_matching(&m, 5000);
        for p in mix {
            assert!((p - 1.0 / 3.0).abs() < 0.05);
        }
    }

    fn play_whole(x: &str, y: &str, seed: u64) -> Game {
        let mut g = Game::new(seed);
        let mut a = make_bot(x, seed * 3 + 1);
        let mut b = make_bot(y, seed * 3 + 2);
        while !g.is_over() {
            g.start_round();
            draft_round(&mut g, a.as_mut(), b.as_mut());
            play_round_after_draft(&mut g, a.as_mut(), b.as_mut());
        }
        g
    }

    /// Three fixed games. This fails after any intentional change to rules, maps or bots: paste the new values. If it
    /// fails after a refactor, the refactor changed behaviour.
    #[test]
    fn golden_games() {
        let got: Vec<u64> = [("search", "greedy", 1), ("search", "greedy", 2), ("shaper", "persona_rush", 1)]
            .iter()
            .map(|&(x, y, s)| play_whole(x, y, s).hash())
            .collect();
        let want: [u64; 3] = GOLDEN;
        assert_eq!(got, want, "golden hashes moved; new values: {got:#x?}");
    }

    #[test]
    fn every_bot_name_makes_a_bot_that_answers_to_it() {
        for name in BOT_NAMES {
            assert_eq!(make_bot(name, 1).name(), name);
        }
    }

    /// A game at the start of the shape phase of `round`, played by two `search` bots.
    fn at_shape(seed: u64, round: u32) -> Game {
        let mut g = Game::new(seed);
        let mut a = make_bot("search", seed * 3 + 1);
        let mut b = make_bot("search", seed * 3 + 2);
        for r in 1..=round {
            g.start_round();
            draft_round(&mut g, a.as_mut(), b.as_mut());
            if r == round {
                break;
            }
            play_round_after_draft(&mut g, a.as_mut(), b.as_mut());
        }
        g
    }

    /// The same game one step before the attack of `round`.
    fn at_attack(seed: u64, round: u32) -> Game {
        let mut g = at_shape(seed, round);
        let mut a = make_bot("search", seed * 5 + 1);
        let mut b = make_bot("search", seed * 5 + 2);
        let plans = [a.terraform(&g, 0), b.terraform(&g, 1)];
        let builds = shape_builds(&g, &plans, a.as_mut(), b.as_mut());
        g.apply_terraform(plans);
        g.apply_build(builds);
        g
    }

    #[test]
    fn shaping_bots_return_legal_plans_within_budget_and_are_deterministic() {
        for seed in 1..=2 {
            let g = at_shape(seed, 2);
            for name in ["shape_only", "rand_shape", "search", "greedy", "random"] {
                for side in 0..2 {
                    let plan = make_bot(name, seed).terraform(&g, side);
                    assert_eq!(make_bot(name, seed).terraform(&g, side), plan, "{name} seed {seed}");
                    assert!(plan.iter().map(|&(_, o)| o.cost()).sum::<i32>() <= g.tp(side));
                    assert!(plan.iter().all(|&(t, _)| own_half(t, side)), "{name} shaped outside its half");
                    assert_eq!(g.sanitize_plan(side, &plan), plan, "{name} seed {seed}: {plan:?}");
                }
            }
        }
    }

    #[test]
    fn placement_search_keeps_the_build_and_is_legal() {
        for seed in 1..=3 {
            let g = at_shape(seed, 2);
            for side in 0..2 {
                let base = default_build(&g, side, Style::Turtle);
                for place in [searched_placement] {
                    let placed = place(&g, side, base.clone());
                    assert_eq!(placed.units, base.units);
                    assert_eq!(placed.buildings.len(), base.buildings.len());
                    for (i, &(kind, tile)) in placed.buildings.iter().enumerate() {
                        assert_eq!(kind, base.buildings[i].0);
                        assert!(g.building_tile_ok(side, kind, tile), "seed {seed} side {side}: {kind:?} on {tile}");
                        assert!(!placed.buildings[..i].iter().any(|&(_, t)| t == tile));
                    }
                    assert_eq!(place(&g, side, base.clone()).buildings, placed.buildings);
                }
            }
        }
    }

    #[test]
    fn game_value_matches_known_games() {
        let rps = vec![vec![0.0, -1.0, 1.0], vec![1.0, 0.0, -1.0], vec![-1.0, 1.0, 0.0]];
        assert!(game_value(&rps).abs() < 1e-9);
        assert!((game_value(&[vec![1.0, 5.0], vec![3.0, 4.0]]) - 3.0).abs() < 1e-9);
        assert!((game_value(&[vec![1.0, 4.0], vec![4.0, 1.0]]) - 2.5).abs() < 1e-9);
    }

    #[test]
    fn game_value_handles_non_square_games() {
        // 2x3: the third column caps the value at 1 while the mix of the two rows reaches 1.2 against the others
        assert!((game_value(&[vec![2.0, 0.0, 1.0], vec![0.0, 3.0, 1.0]]) - 1.0).abs() < 1e-9);
        // the same game, transposed to 3x2 (row player now has the extra option): mixing rows 0 and 1 gives 1.2
        assert!((game_value(&[vec![2.0, 0.0], vec![0.0, 3.0], vec![1.0, 1.0]]) - 1.2).abs() < 1e-9);
        // 2x3 where three lines meet at p = 0.5
        assert!((game_value(&[vec![3.0, 1.0, 0.0], vec![0.0, 2.0, 3.0]]) - 1.5).abs() < 1e-9);
        // 3x9 against regret matching, which converges to the same value from below
        let mut rng = Rng::seed_from_u64(11);
        for _ in 0..5 {
            let m: Vec<Vec<f64>> = (0..3)
                .map(|_| (0..9).map(|_| (rng.random_range(0..41) as f64 - 20.0) / 4.0).collect())
                .collect();
            let mix = regret_matching(&m, 20000);
            let guaranteed = (0..9).map(|j| (0..3).map(|i| mix[i] * m[i][j]).sum::<f64>()).fold(f64::INFINITY, f64::min);
            let v = game_value(&m);
            assert!(v + 1e-9 >= guaranteed && v - guaranteed < 0.1, "value {v} vs regret matching {guaranteed}");
            // the allocation-free solver the fast pipeline uses must agree with the reference
            let mut small = Mat::new(3, 9);
            for i in 0..3 {
                small.v[i].copy_from_slice(&m[i]);
            }
            assert!((game_value_small(&small) - v).abs() < 1e-9);
        }
    }

    #[test]
    fn searched_crossing_is_deterministic_and_uses_one_draw() {
        for seed in 1..=3 {
            let g = at_attack(seed, 2);
            for side in 0..2 {
                let v = g.reveal_view(side);
                let (mut r1, mut r2, mut fresh) = (Rng::seed_from_u64(5), Rng::seed_from_u64(5), Rng::seed_from_u64(5));
                let c1 = searched_crossing(&v, side, &mut r1);
                assert_eq!(c1, searched_crossing(&v, side, &mut r2), "seed {seed} side {side}");
                assert!(c1 <= 8);
                fresh.next_u64();
                assert_eq!(r1.next_u64(), fresh.next_u64(), "exactly one draw");
            }
        }
    }

    #[test]
    fn every_bot_attacks_on_a_legal_column() {
        let g = at_attack(1, 3);
        for name in BOT_NAMES {
            for side in 0..2 {
                let c = make_bot(name, 7).attack(&g.reveal_view(side), side);
                assert!(CROSSINGS.contains(&c), "{name} returned {c}");
            }
        }
        // with no buildings to avoid, the default crossing is the middle
        let fresh = Game::new(1);
        assert_eq!((default_crossing(&fresh, 0), default_crossing(&fresh, 1)), (4, 4));
    }

    #[test]
    fn the_scar_blind_bot_sees_the_board_without_its_scars() {
        let mut g = at_shape(2, 3);
        assert!(!g.scar_under.is_empty(), "two rounds of search play left no scar; pick another seed");
        let blind = make_bot("shaper_scarblind", 4).terraform(&g, 0);
        let on_unscarred = make_bot("shaper", 4).terraform(&g.unscarred_view(), 0);
        assert_eq!(blind, on_unscarred);
        g.apply_terraform([blind, vec![]]);
    }

    #[test]
    fn control_bots_and_personas_play_whole_games() {
        for (x, y) in [
            ("shaper_randcross", "shaper_fixed"),
            ("persona_fort", "persona_turtle"),
            ("persona_fort_counter", "persona_counter"),
            ("random", "search_randdraft"),
            ("place_only", "no_shape"),
        ] {
            let g = play_whole(x, y, 3);
            assert!(g.is_over() && g.winner() < 2);
        }
    }

    /// `deep_plan` takes a cell's value from the board it adds an op to when `clash::unaffected` proves the clash would
    /// go the same way. That is a shortcut, not a second screen: the plan is the one the search finds with every clash run.
    #[test]
    fn skipping_proven_clashes_changes_no_plan() {
        let mut shaped = 0;
        for seed in 1..=6 {
            for round in 1..=4 {
                let g = at_shape(seed, round);
                for side in 0..2 {
                    let style = commander_style(&g, side);
                    let plan = fast::deep_plan_with(&g, side, style, SCREEN_TOP, true);
                    assert_eq!(
                        plan,
                        fast::deep_plan_with(&g, side, style, SCREEN_TOP, false),
                        "seed {seed} round {round} side {side}"
                    );
                    shaped += usize::from(!plan.is_empty());
                }
            }
        }
        assert!(shaped >= 12, "only {shaped} of 48 decisions shaped anything");
    }

    /// Timing aid (run with --ignored --nocapture): the cost of one clash, one clone and one 3x9 solve.
    #[test]
    #[ignore]
    fn bench_clash() {
        for round in [1, 3, 4] {
            let g = at_attack(2, round);
            let t = std::time::Instant::now();
            let n = 2000;
            for i in 0..n {
                let mut c = g.sim_clone();
                c.apply_attack([(i % 9) as u8, ((i / 9) % 9) as u8]);
                c.clash_and_resolve();
            }
            eprintln!(
                "round {round}: {:.1} us per clash+clone, rosters {} {}",
                t.elapsed().as_secs_f64() * 1e6 / n as f64,
                g.players[0].roster.len(),
                g.players[1].roster.len()
            );
            let t = std::time::Instant::now();
            for _ in 0..n {
                let _ = g.sim_clone();
            }
            eprintln!("  clone only {:.2} us", t.elapsed().as_secs_f64() * 1e6 / n as f64);
        }
    }

    const GOLDEN: [u64; 3] = [0xa009d6cfc597ad79, 0x3aae85da8f59e429, 0xf54e934c593cc609];
}
