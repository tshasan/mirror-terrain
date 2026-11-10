use crate::Rng;
use crate::defs::*;
use crate::replay::Recorder;
use rand::seq::SliceRandom;
use rand::{RngExt, SeedableRng};
use std::collections::BTreeMap;

pub const START_CORE_HP: i32 = 20;
pub const START_ORE: i32 = 15;
pub const BASE_INCOME: i32 = 6;
pub const EXTRACTOR_INCOME: i32 = 3;
pub const BASE_TP: i32 = 2;
pub const MAX_ROUNDS: u32 = 5;
pub const ROSTER_CAP: usize = 20;
pub const BUILDING_CAP: usize = 8;
/// The columns an army may cross the midline on, in the chooser's own frame (mapped to the board with `rel`).
pub const CROSSINGS: [u8; 9] = [0, 1, 2, 3, 4, 5, 6, 7, 8];

/// A permutation of 0, 1, 2 from any input: entries that are out of range or repeated are dropped, and the missing
/// indices are appended in ascending order.
pub fn sanitize_order(o: [usize; 3]) -> [usize; 3] {
    let mut out = [0usize; 3];
    let mut n = 0;
    for &x in &o {
        if x < 3 && !out[..n].contains(&x) {
            out[n] = x;
            n += 1;
        }
    }
    for x in 0..3 {
        if n < 3 && !out[..n].contains(&x) {
            out[n] = x;
            n += 1;
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    pub core_hp: i32,
    pub ore: i32,
    pub tp_bonus: i32,
    pub units: [bool; 6],
    pub buildings: [bool; 3],
    pub roster: Vec<UnitKind>,
    /// Attack column in own frame (0..=8): side A's frame, mapped to the board with `rel`.
    pub crossing: u8,
    pub hover: bool,
    pub camo: bool,
    pub picks: Vec<Card>,
    pub commander: Commander,
}

impl Player {
    fn new() -> Self {
        let mut units = [false; 6];
        units[UnitKind::Grunt as usize] = true;
        Player {
            core_hp: START_CORE_HP,
            ore: START_ORE,
            tp_bonus: 0,
            units,
            buildings: [false; 3],
            roster: Vec::new(),
            crossing: 4,
            hover: false,
            camo: false,
            picks: Vec::new(),
            commander: Commander::Geomancer,
        }
    }
    pub fn roster_value(&self) -> i32 {
        self.roster.iter().map(|u| u.stats().cost).sum()
    }
    pub fn air_share(&self) -> f64 {
        if self.roster.is_empty() {
            return 0.0;
        }
        self.roster.iter().filter(|u| u.is_air()).count() as f64 / self.roster.len() as f64
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Building {
    pub id: u32,
    pub side: usize,
    pub kind: BuildingKind,
    pub tile: usize,
    pub hp: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpOutcome {
    Applied,
    /// A blocking op undone because the round's ops together cut the last ground path.
    Reverted,
}

#[derive(Clone, Debug)]
pub struct OpRecord {
    pub tile: usize,
    pub side: usize,
    pub op: Op,
    pub outcome: OpOutcome,
}

#[derive(Clone, Debug, Default)]
pub struct BuildCmd {
    pub buildings: Vec<(BuildingKind, usize)>,
    pub units: Vec<UnitKind>,
}

#[derive(Clone, Debug, Default)]
pub struct SideStats {
    pub tp_by_op: [i32; 7],
    pub count_by_op: [i32; 7],
    pub reverted: i32,
    pub tp_spent: i32,
    pub tp_available: i32,
    pub breach_tiers: i32,
    pub breach_dmg: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScarKind {
    Crater,
    Ruin,
}

#[derive(Clone, Debug, Default)]
pub struct MatchStats {
    pub side: [SideStats; 2],
    /// Scars applied over the match, and the rounds that left at least one.
    pub craters: i32,
    pub ruins: i32,
    pub scar_rounds: i32,
    /// Draft rounds played and those whose first choices collided.
    pub draft_rounds: i32,
    pub draft_collisions: i32,
    pub first_core_damage_round: Option<u32>,
    pub core_hp_after_round: Vec<[i32; 2]>,
}

#[derive(Clone, Debug)]
pub struct Game {
    pub seed: u64,
    rng: Rng,
    pub terrain: [Terrain; N],
    pub players: [Player; 2],
    pub buildings: Vec<Building>,
    pub round: u32,
    next_id: u32,
    pub offers: [Card; 3],
    pub last_ops: Vec<OpRecord>,
    pub stats: MatchStats,
    pub rec: Option<Recorder>,
    /// Each side's roster length just before this round's units were bought. Not hashed. `reveal_view` hides the
    /// units past this index.
    pub hidden_from: [usize; 2],
    /// Not hashed (the terrain already is). For every tile that carries a scar, the terrain it had before the first scar.
    /// An op that changes the tile removes its entry: a repaired or reshaped scar is just terrain.
    pub scar_under: BTreeMap<usize, Terrain>,
    /// Not hashed. The scars the most recent resolve applied, in application order.
    pub last_scars: Vec<(usize, ScarKind)>,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        Self::new_inner(seed, None)
    }

    pub fn new_with_commanders(seed: u64, commanders: [Commander; 2]) -> Self {
        Self::new_inner(seed, Some(commanders))
    }

    fn new_inner(seed: u64, forced: Option<[Commander; 2]>) -> Self {
        let mut rng = Rng::seed_from_u64(seed);
        let terrain = crate::map::generate(&mut rng);
        // Drawn even when the commanders are forced, so the same seed deals the same offers either way.
        let c0 = rng.random_range(0..4);
        let c1 = (c0 + 1 + rng.random_range(0..3)) % 4;
        let commanders = forced.unwrap_or([ALL_COMMANDERS[c0], ALL_COMMANDERS[c1]]);
        let mut g = Game {
            seed,
            rng,
            terrain,
            players: [Player::new(), Player::new()],
            buildings: Vec::new(),
            round: 0,
            next_id: 1,
            offers: [Card::Earthshaper; 3],
            last_ops: Vec::new(),
            stats: MatchStats::default(),
            rec: None,
            hidden_from: [0, 0],
            scar_under: BTreeMap::new(),
            last_scars: Vec::new(),
        };
        for (side, cmd) in commanders.into_iter().enumerate() {
            g.players[side].commander = cmd;
            g.give_card(side, cmd.start_card());
        }
        g
    }

    pub fn with_recording(mut self) -> Self {
        self.rec = Some(Recorder::new(self.seed, &self.terrain));
        self
    }

    pub fn sim_clone(&self) -> Self {
        // Field by field instead of `clone()` + `rec = None`, which deep-copies the replay recorder just to drop it.
        // The exhaustive literal makes a newly added field a compile error here.
        Game {
            seed: self.seed,
            rng: self.rng.clone(),
            terrain: self.terrain,
            players: self.players.clone(),
            buildings: self.buildings.clone(),
            round: self.round,
            next_id: self.next_id,
            offers: self.offers,
            last_ops: self.last_ops.clone(),
            stats: self.stats.clone(),
            rec: None,
            hidden_from: self.hidden_from,
            scar_under: self.scar_under.clone(),
            last_scars: self.last_scars.clone(),
        }
    }

    /// The board as a player who ignores scars believes it is: a clone with every scarred tile put back to the terrain it
    /// had before the scar. The clone has no scars recorded.
    pub fn unscarred_view(&self) -> Game {
        let mut v = self.sim_clone();
        for (&t, &under) in &self.scar_under {
            v.terrain[t] = under;
        }
        v.scar_under.clear();
        v
    }

    pub fn trailing(&self, side: usize) -> bool {
        self.players[side].core_hp < self.players[1 - side].core_hp
    }

    pub fn tp(&self, side: usize) -> i32 {
        BASE_TP + self.players[side].tp_bonus
    }

    /// Home-turf bonus in percent: units on their own half deal that much more and take that much less. It fades
    /// (Jervis: a defence advantage that decays), so early rounds favour the defender and late rounds do not.
    pub fn turf_pct(&self) -> i32 {
        match self.round {
            0..=2 => 25,
            3 => 12,
            _ => 0,
        }
    }

    pub fn is_over(&self) -> bool {
        self.players[0].core_hp <= 0 || self.players[1].core_hp <= 0 || self.round >= MAX_ROUNDS
    }

    /// A finished game level on core HP and on army value: `winner` falls back to a coin flip from the seed.
    pub fn is_coin_flip(&self) -> bool {
        let (a, b) = (&self.players[0], &self.players[1]);
        self.is_over() && a.core_hp == b.core_hp && a.roster_value() == b.roster_value()
    }

    /// More core HP wins, then the higher army value, then a coin flip from the seed.
    pub fn winner(&self) -> usize {
        let (a, b) = (&self.players[0], &self.players[1]);
        if a.core_hp != b.core_hp {
            return usize::from(b.core_hp > a.core_hp);
        }
        if a.roster_value() != b.roster_value() {
            return usize::from(b.roster_value() > a.roster_value());
        }
        (self.seed % 2) as usize
    }

    pub fn start_round(&mut self) {
        self.round += 1;
        self.hidden_from = [self.players[0].roster.len(), self.players[1].roster.len()];
        let mut pool: Vec<Card> = CARD_POOL.to_vec();
        pool.shuffle(&mut self.rng);
        let mut picked: Vec<Card> = Vec::new();
        for c in pool {
            if picked.len() == 3 {
                break;
            }
            if picked.iter().all(|p| p.tag() != c.tag()) {
                picked.push(c);
            }
        }
        while picked.len() < 3 {
            picked.push(Card::Earthshaper);
        }
        self.offers = [picked[0], picked[1], picked[2]];
        if let Some(r) = self.rec.as_mut() {
            r.begin_round(self.round, &self.offers);
        }
    }

    /// The picks two ranked orders resolve to; the orders are sanitised first (see `sanitize_order`). Different first
    /// choices: each gets theirs. Equal first choices: the trailing side (core HP) gets it and the other gets their second
    /// choice; level HP gives both their own second choice, which may be the same card (so mirrored inputs, which have
    /// level HP and equal orders, still give mirrored outcomes).
    pub fn resolve_draft(&self, orders: [[usize; 3]; 2]) -> [usize; 2] {
        let o = [sanitize_order(orders[0]), sanitize_order(orders[1])];
        if o[0][0] != o[1][0] {
            return [o[0][0], o[1][0]];
        }
        match (0..2).find(|&s| self.trailing(s)) {
            Some(t) => {
                let mut picks = [0; 2];
                picks[t] = o[t][0];
                picks[1 - t] = o[1 - t][1];
                picks
            }
            None => [o[0][1], o[1][1]],
        }
    }

    /// Draft phase: each player submits a ranked order of the three offers. Why ranked and contested: the deal fixed
    /// 29.8% of outcome variance when both could take the same card; a contested card gives the two kits a reason to
    /// differ through decisions (SPEC, "Draft").
    pub fn apply_draft(&mut self, orders: [[usize; 3]; 2]) {
        let picks = self.resolve_draft(orders);
        self.stats.draft_rounds += 1;
        self.stats.draft_collisions += i32::from(sanitize_order(orders[0])[0] == sanitize_order(orders[1])[0]);
        for side in 0..2 {
            self.give_card(side, self.offers[picks[side]]);
        }
        if let Some(r) = self.rec.as_mut() {
            r.draft(picks);
        }
    }

    fn give_card(&mut self, side: usize, card: Card) {
        let p = &mut self.players[side];
        p.picks.push(card);
        match card {
            Card::Unit(u) => {
                if p.units[u as usize] {
                    p.ore += 2;
                } else {
                    p.units[u as usize] = true;
                }
            }
            Card::Building(b) => {
                if p.buildings[b as usize] {
                    p.ore += 2;
                } else {
                    p.buildings[b as usize] = true;
                }
            }
            Card::Earthshaper => p.tp_bonus += 1,
            // Every op is available from round 1, so the cards that used to unlock one pay ore instead.
            Card::Grovekeeper | Card::Sapper | Card::Prospector => p.ore += 2,
            Card::HoverTreads => p.hover = true,
            Card::Camo => p.camo = true,
        }
    }

    /// A tile no op may change: one with a building on it.
    fn tile_locked(&self, tile: usize) -> bool {
        self.buildings.iter().any(|b| b.tile == tile)
    }

    /// Shaping is one tile on your own half: the opponent can never edit your base (SPEC, pillar 1).
    pub fn op_legal(&self, side: usize, tile: usize, op: Op) -> bool {
        tile < N && own_half(tile, side) && op.cost() <= self.tp(side) && !near_core(tile) && !self.tile_locked(tile) && op.apply(self.terrain[tile]).is_some()
    }

    /// Every legal (tile, op) for `side`, tiles in the side's own frame order so that mirrored positions list mirrored ops.
    pub fn legal_ops(&self, side: usize) -> Vec<(usize, Op)> {
        let mut v = Vec::new();
        for k in 0..N {
            let tile = rel(k, side);
            for op in ALL_OPS {
                if self.op_legal(side, tile, op) {
                    v.push((tile, op));
                }
            }
        }
        v
    }

    pub fn sanitize_plan(&self, side: usize, plan: &[(usize, Op)]) -> Vec<(usize, Op)> {
        let mut out: Vec<(usize, Op)> = Vec::new();
        let mut budget = self.tp(side);
        for &(tile, op) in plan {
            if out.iter().any(|&(t, _)| t == tile) {
                continue;
            }
            if !self.op_legal(side, tile, op) || op.cost() > budget {
                continue;
            }
            budget -= op.cost();
            out.push((tile, op));
        }
        out
    }

    /// Shape phase. The two plans never touch the same tile (each is confined to its own half), so they apply together.
    /// Path safety: if the result cuts every ground path between the cores, every blocking op (wall, flood) of the round
    /// is reverted, on both sides.
    pub fn apply_terraform(&mut self, plans: [Vec<(usize, Op)>; 2]) {
        let before = self.terrain;
        let mut records: Vec<OpRecord> = Vec::new();
        for side in 0..2 {
            self.stats.side[side].tp_available += self.tp(side);
            for (tile, op) in self.sanitize_plan(side, &plans[side]) {
                let s = &mut self.stats.side[side];
                s.tp_by_op[op as usize] += op.cost();
                s.count_by_op[op as usize] += 1;
                s.tp_spent += op.cost();
                if let Some(to) = op.apply(self.terrain[tile]) {
                    self.terrain[tile] = to;
                }
                records.push(OpRecord {
                    tile,
                    side,
                    op,
                    outcome: OpOutcome::Applied,
                });
            }
        }
        if !crate::map::ground_connected(&self.terrain) {
            for r in records.iter_mut().filter(|r| r.op.blocks_ground()) {
                self.terrain[r.tile] = before[r.tile];
                r.outcome = OpOutcome::Reverted;
                self.stats.side[r.side].reverted += 1;
            }
        }
        if !self.scar_under.is_empty() {
            // A tile an op changed is no longer a scar. Checked after the path-safety revert, which restores `before`.
            let terrain = self.terrain;
            self.scar_under.retain(|&t, _| terrain[t] == before[t]);
        }
        if let Some(rec) = self.rec.as_mut() {
            rec.terraform(&records, &self.terrain);
        }
        self.last_ops = records;
    }

    pub fn building_tile_ok(&self, side: usize, kind: BuildingKind, tile: usize) -> bool {
        if tile >= N || !buildable_row(tile, side) || tile == CORE[side] {
            return false;
        }
        let t = self.terrain[tile];
        let terrain_ok = if kind == BuildingKind::Extractor { t == Terrain::Ore } else { t.buildable() };
        terrain_ok && !self.buildings.iter().any(|b| b.tile == tile)
    }

    pub fn building_tiles(&self, side: usize, kind: BuildingKind) -> Vec<usize> {
        (0..N).filter(|&t| self.building_tile_ok(side, kind, t)).collect()
    }

    pub fn apply_build(&mut self, cmds: [BuildCmd; 2]) {
        for side in 0..2 {
            let cmd = &cmds[side];
            self.hidden_from[side] = self.players[side].roster.len();
            for &(kind, tile) in &cmd.buildings {
                let cost = kind.stats().cost;
                let count = self.buildings.iter().filter(|b| b.side == side).count();
                if !self.players[side].buildings[kind as usize]
                    || self.players[side].ore < cost
                    || count >= BUILDING_CAP
                    || !self.building_tile_ok(side, kind, tile)
                {
                    continue;
                }
                self.players[side].ore -= cost;
                let id = self.next_id();
                self.buildings.push(Building {
                    id,
                    side,
                    kind,
                    tile,
                    hp: kind.stats().hp,
                });
            }
            for &u in &cmd.units {
                let p = &mut self.players[side];
                let cost = u.stats().cost;
                if !p.units[u as usize] || p.ore < cost || p.roster.len() >= ROSTER_CAP {
                    continue;
                }
                p.ore -= cost;
                p.roster.push(u);
            }
        }
        if let Some(rec) = self.rec.as_mut() {
            rec.build(&self.buildings, &self.players);
        }
    }

    /// Attack phase: each side picks a crossing column in its own frame, clamped to the board. Why after the build: the
    /// blind lane it replaces was a guess worth 8.5 core HP; choosing after the reveal can be a read (SPEC, pillar 2).
    pub fn apply_attack(&mut self, crossings: [u8; 2]) {
        for side in 0..2 {
            self.players[side].crossing = crossings[side].min(8);
        }
        if let Some(rec) = self.rec.as_mut() {
            rec.attack([self.players[0].crossing, self.players[1].crossing]);
        }
    }

    /// What `side` may see at the attack phase: both bases (terrain and buildings) but not the opponent's
    /// units bought this round (hidden, and their cost refunded to the opponent's ore) or crossing.
    pub fn reveal_view(&self, side: usize) -> Game {
        let mut v = self.sim_clone();
        let opp = 1 - side;
        let keep = self.hidden_from[opp].min(v.players[opp].roster.len());
        let refund: i32 = v.players[opp].roster[keep..].iter().map(|u| u.stats().cost).sum();
        v.players[opp].roster.truncate(keep);
        v.players[opp].ore += refund;
        v.players[opp].crossing = 4;
        v.hidden_from[opp] = keep;
        v
    }

    /// What `side` sees while choosing its build: the board with only its own terraform plan applied.
    pub fn shape_view(&self, side: usize, plan: &[(usize, Op)]) -> Game {
        let mut v = self.sim_clone();
        let mut plans = [vec![], vec![]];
        plans[side] = plan.to_vec();
        v.apply_terraform(plans);
        v
    }

    pub fn clash_and_resolve(&mut self) {
        self.clash_and_resolve_traced();
    }

    /// `clash_and_resolve`, returning the tiles the clash touched (`Clash::touched`) for `clash::unaffected`.
    pub fn clash_and_resolve_traced(&mut self) -> [bool; N] {
        let clash = crate::clash::run(self);
        let breach = clash.breach;
        for side in 0..2 {
            // Rising stakes: a breach is worth more each round, so late rounds decide the match.
            let dmg = breach[side] * (self.round as i32 + 1);
            self.stats.side[side].breach_tiers += breach[side];
            self.stats.side[side].breach_dmg += dmg;
            if dmg > 0 && self.stats.first_core_damage_round.is_none() {
                self.stats.first_core_damage_round = Some(self.round);
            }
            let opp = 1 - side;
            self.players[opp].core_hp = (self.players[opp].core_hp - dmg).max(0);
        }
        for side in 0..2 {
            let ex = self.buildings.iter().filter(|b| b.side == side && b.kind == BuildingKind::Extractor).count() as i32;
            self.players[side].ore += BASE_INCOME + EXTRACTOR_INCOME * ex;
        }
        self.stats.core_hp_after_round.push([self.players[0].core_hp, self.players[1].core_hp]);
        self.apply_scars(&clash.deaths, &clash.destroyed);
        if let Some(rec) = self.rec.as_mut() {
            rec.scars(&self.last_scars);
            rec.resolve(&self.players, breach);
        }
        clash.touched
    }

    /// A tile a scar may land on: not within Chebyshev distance 1 of either core and not under a standing building.
    fn scar_eligible(&self, t: usize) -> bool {
        !near_core(t) && !self.buildings.iter().any(|b| b.tile == t)
    }

    /// Fighting changes the ground (SPEC, "Scars"): every scar is computed from the terrain as the clash left it, then
    /// applied together. Ruins: each destroyed building's tile becomes Forest (replacing Plain, High or Ore; Wall and Water
    /// cannot carry a building). Craters: on each half, among tiles with at least 2 ground deaths that are eligible, not
    /// Wall, Water or already Trench, and not a ruin tile of this resolve, the tile with the most deaths becomes Trench;
    /// ties go to the smallest `rel(tile, side of that half)`, so mirrored fights leave mirrored scars. Neither Trench nor
    /// Forest blocks ground, so a scar cannot cut the path. A tile that already carries a scar keeps the terrain from
    /// before its first scar in `scar_under`.
    fn apply_scars(&mut self, deaths: &[u16; N], destroyed: &[usize]) {
        self.last_scars.clear();
        let ruins: Vec<usize> = destroyed
            .iter()
            .copied()
            .filter(|&t| self.scar_eligible(t) && !self.terrain[t].ground_blocked() && self.terrain[t] != Terrain::Forest)
            .collect();
        let mut craters: Vec<usize> = Vec::new();
        for side in 0..2 {
            let best = (0..N)
                .filter(|&t| {
                    own_half(t, side)
                        && deaths[t] >= 2
                        && self.scar_eligible(t)
                        && !matches!(self.terrain[t], Terrain::Wall | Terrain::Water | Terrain::Trench)
                        && !ruins.contains(&t)
                })
                .min_by_key(|&t| (std::cmp::Reverse(deaths[t]), rel(t, side)));
            craters.extend(best);
        }
        for (t, kind, to) in craters
            .iter()
            .map(|&t| (t, ScarKind::Crater, Terrain::Trench))
            .chain(ruins.iter().map(|&t| (t, ScarKind::Ruin, Terrain::Forest)))
        {
            let old = self.terrain[t];
            self.scar_under.entry(t).or_insert(old);
            self.terrain[t] = to;
            self.last_scars.push((t, kind));
            match kind {
                ScarKind::Crater => self.stats.craters += 1,
                ScarKind::Ruin => self.stats.ruins += 1,
            }
        }
        if !self.last_scars.is_empty() {
            self.stats.scar_rounds += 1;
        }
        debug_assert!(crate::map::ground_connected(&self.terrain), "a scar cut the ground path");
    }

    pub(crate) fn next_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |v: u64| {
            for b in v.to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        eat(self.round as u64);
        for t in self.terrain {
            eat(t as u64);
        }
        for p in &self.players {
            eat(p.core_hp as u64);
            eat(p.ore as u64);
            for u in &p.roster {
                eat(*u as u64);
            }
        }
        for b in &self.buildings {
            eat(b.tile as u64);
            eat(b.hp as u64);
            eat(b.kind as u64);
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(seed: u64) -> Game {
        let mut g = Game::new(seed);
        // a flat board so the tests choose every tile's terrain themselves
        g.terrain = [Terrain::Plain; N];
        g
    }

    fn deaths_at(list: &[(usize, u16)]) -> [u16; N] {
        let mut d = [0u16; N];
        for &(t, n) in list {
            d[t] = n;
        }
        d
    }

    fn at(x: i32, y: i32) -> usize {
        idx(x, y).unwrap()
    }

    #[test]
    fn one_crater_per_half_on_the_deadliest_eligible_tile() {
        let mut g = flat(1);
        g.terrain[at(2, 3)] = Terrain::Wall; // 9 deaths, but a crater never replaces a wall
        g.terrain[at(6, 3)] = Terrain::Water;
        g.buildings.push(Building {
            id: 1,
            side: 0,
            kind: BuildingKind::Turret,
            tile: at(5, 4),
            hp: 10,
        });
        let d = deaths_at(&[
            (at(4, 2), 8),  // next to core 0 (4,1): not eligible
            (at(2, 3), 9),  // wall
            (at(6, 3), 9),  // water
            (at(5, 4), 7),  // under a standing building
            (at(1, 5), 3),  // the winner on half 0
            (at(7, 5), 3),  // ties with (1,5); loses on rel
            (at(3, 6), 2),  // fewer deaths
            (at(0, 7), 1),  // under the minimum
            (at(4, 13), 8), // next to core 1 (4,14)
            (at(3, 10), 2), // the only eligible tile on half 1
        ]);
        g.apply_scars(&d, &[]);
        let craters: Vec<usize> = g.last_scars.iter().filter(|s| s.1 == ScarKind::Crater).map(|s| s.0).collect();
        assert_eq!(craters, vec![at(1, 5), at(3, 10)]);
        assert!(g.last_scars.iter().all(|s| s.1 == ScarKind::Crater));
        for &t in &craters {
            assert_eq!(g.terrain[t], Terrain::Trench);
            assert_eq!(g.scar_under[&t], Terrain::Plain);
        }
        assert_eq!(g.terrain[at(2, 3)], Terrain::Wall);
        assert_eq!(g.terrain[at(6, 3)], Terrain::Water);
        assert_eq!((g.stats.craters, g.stats.ruins, g.stats.scar_rounds), (2, 0, 1));
        assert!(crate::map::ground_connected(&g.terrain));
    }

    #[test]
    fn a_crater_needs_two_deaths_and_skips_trenches() {
        let mut g = flat(2);
        g.apply_scars(&deaths_at(&[(at(3, 4), 1), (at(3, 11), 1)]), &[]);
        assert!(g.last_scars.is_empty() && g.scar_under.is_empty());
        assert_eq!(g.stats.scar_rounds, 0);
        // an existing trench is not eligible: the next-best tile takes the crater
        g.terrain[at(3, 4)] = Terrain::Trench;
        g.apply_scars(&deaths_at(&[(at(3, 4), 5), (at(5, 5), 2)]), &[]);
        assert_eq!(g.last_scars, vec![(at(5, 5), ScarKind::Crater)]);
        assert!(!g.scar_under.contains_key(&at(3, 4)));
    }

    #[test]
    fn mirrored_deaths_leave_mirrored_craters_even_on_ties() {
        let mut g = flat(3);
        let mut list = Vec::new();
        for t in [at(1, 4), at(4, 4), at(7, 6)] {
            list.push((t, 2));
            list.push((mirror(t), 2));
        }
        g.apply_scars(&deaths_at(&list), &[]);
        let c: Vec<usize> = g.last_scars.iter().map(|s| s.0).collect();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0], mirror(c[1]));
        for i in 0..N {
            assert_eq!(g.terrain[i], g.terrain[mirror(i)]);
        }
    }

    #[test]
    fn a_destroyed_building_leaves_forest_unless_next_to_a_core() {
        let mut g = flat(4);
        g.terrain[at(2, 4)] = Terrain::Ore;
        g.terrain[at(5, 5)] = Terrain::High;
        // ruins land on Ore and High alike; a building beside the core leaves nothing
        g.apply_scars(&[0; N], &[at(2, 4), at(5, 5), at(3, 2), at(6, 10)]);
        assert_eq!(g.terrain[at(2, 4)], Terrain::Forest);
        assert_eq!(g.terrain[at(5, 5)], Terrain::Forest);
        assert_eq!(g.terrain[at(6, 10)], Terrain::Forest);
        assert_eq!(g.terrain[at(3, 2)], Terrain::Plain);
        assert_eq!(g.scar_under[&at(2, 4)], Terrain::Ore);
        assert_eq!(g.scar_under[&at(5, 5)], Terrain::High);
        assert!(g.last_scars.iter().all(|s| s.1 == ScarKind::Ruin));
        assert_eq!((g.stats.craters, g.stats.ruins), (0, 3));
        // a ruin tile is never also a crater in the same resolve
        let mut g = flat(4);
        g.apply_scars(&deaths_at(&[(at(4, 5), 6), (at(2, 5), 2)]), &[at(4, 5)]);
        assert_eq!(g.terrain[at(4, 5)], Terrain::Forest);
        assert_eq!(g.terrain[at(2, 5)], Terrain::Trench);
    }

    #[test]
    fn unscarred_view_restores_the_ground_and_forgets_the_scars() {
        let mut g = flat(5);
        g.terrain[at(2, 4)] = Terrain::Ore;
        g.apply_scars(&deaths_at(&[(at(3, 5), 4)]), &[at(2, 4)]);
        assert_eq!(g.terrain[at(2, 4)], Terrain::Forest);
        assert_eq!(g.terrain[at(3, 5)], Terrain::Trench);
        let v = g.unscarred_view();
        assert_eq!(v.terrain[at(2, 4)], Terrain::Ore);
        assert_eq!(v.terrain[at(3, 5)], Terrain::Plain);
        assert!(v.scar_under.is_empty());
        assert_eq!(g.terrain[at(3, 5)], Terrain::Trench, "the real game is untouched");
        let mut clean = flat(5);
        clean.terrain[at(2, 4)] = Terrain::Ore;
        assert_eq!(v.terrain, clean.terrain);
    }

    #[test]
    fn an_op_on_a_scar_removes_it_from_scar_under() {
        for (op, to) in [(Op::Clear, Terrain::Plain), (Op::Flood, Terrain::Water)] {
            let mut g = flat(6);
            g.start_round();
            let crater = at(3, 4); // own half of side 0
            g.apply_scars(&deaths_at(&[(crater, 3)]), &[]);
            assert_eq!(g.terrain[crater], Terrain::Trench);
            assert!(g.op_legal(0, crater, op), "{op:?} must be legal on an own-half scar");
            g.apply_terraform([vec![(crater, op)], vec![]]);
            assert_eq!(g.terrain[crater], to);
            assert!(g.scar_under.is_empty(), "{op:?} reshaped the scar");
        }
        // a ruin is repaired by Clear too
        let mut g = flat(6);
        g.start_round();
        g.apply_scars(&[0; N], &[at(2, 4)]);
        g.apply_terraform([vec![(at(2, 4), Op::Clear)], vec![]]);
        assert_eq!(g.terrain[at(2, 4)], Terrain::Plain);
        assert!(g.scar_under.is_empty());
        // an op elsewhere leaves a scar alone
        let mut g = flat(6);
        g.start_round();
        g.apply_scars(&deaths_at(&[(at(3, 4), 3)]), &[]);
        g.apply_terraform([vec![(at(5, 5), Op::Raise)], vec![]]);
        assert_eq!(g.scar_under.len(), 1);
    }

    #[test]
    fn real_clashes_scar_legally_and_never_cut_the_path() {
        let mut total = 0;
        for seed in 0..40 {
            let mut g = Game::new(seed);
            while !g.is_over() {
                g.start_round();
                g.apply_draft([[0, 1, 2], [1, 0, 2]]);
                let cmd = BuildCmd {
                    buildings: vec![],
                    units: vec![UnitKind::Grunt; 8],
                };
                g.apply_terraform([vec![], vec![]]);
                g.apply_build([cmd.clone(), cmd]);
                g.apply_attack([4, 4]);
                g.clash_and_resolve();
                assert!(crate::map::ground_connected(&g.terrain));
                assert!(g.last_scars.iter().filter(|s| s.1 == ScarKind::Crater).count() <= 2);
                for &(t, _) in &g.last_scars {
                    assert!(cheb(t, CORE[0]) > 1 && cheb(t, CORE[1]) > 1, "scar beside a core");
                    assert!(!g.buildings.iter().any(|b| b.tile == t), "scar under a building");
                    assert!(!g.terrain[t].ground_blocked());
                }
                total += g.last_scars.len();
            }
        }
        assert!(total > 0, "forty games of grunt brawls left no scar at all");
    }

    #[test]
    fn sanitized_orders_are_permutations() {
        assert_eq!(sanitize_order([2, 0, 1]), [2, 0, 1]);
        assert_eq!(sanitize_order([1, 1, 1]), [1, 0, 2]);
        assert_eq!(sanitize_order([7, 2, 2]), [2, 0, 1]);
        assert_eq!(sanitize_order([usize::MAX, usize::MAX, usize::MAX]), [0, 1, 2]);
        assert_eq!(sanitize_order([2, 9, 0]), [2, 0, 1]);
    }

    fn drafted(hp: [i32; 2], orders: [[usize; 3]; 2]) -> (Game, [usize; 2]) {
        let mut g = Game::new(11);
        g.start_round();
        g.players[0].core_hp = hp[0];
        g.players[1].core_hp = hp[1];
        let before = [g.players[0].picks.len(), g.players[1].picks.len()];
        g.apply_draft(orders);
        let got = [0, 1].map(|s| g.offers.iter().position(|c| *c == g.players[s].picks[before[s]]).unwrap());
        (g, got)
    }

    #[test]
    fn draft_different_firsts_get_their_firsts() {
        let (g, got) = drafted([20, 20], [[0, 1, 2], [2, 1, 0]]);
        assert_eq!(got, [0, 2]);
        assert_eq!((g.stats.draft_rounds, g.stats.draft_collisions), (1, 0));
    }

    #[test]
    fn draft_collision_goes_to_the_trailing_side() {
        // offers have different tags, so the card position identifies the pick
        let (g, got) = drafted([14, 20], [[1, 0, 2], [1, 2, 0]]);
        assert_eq!(got, [1, 2], "side 0 trails: it keeps card 1, side 1 takes its second choice");
        assert_eq!((g.stats.draft_rounds, g.stats.draft_collisions), (1, 1));
        let (_, got) = drafted([20, 14], [[1, 0, 2], [1, 2, 0]]);
        assert_eq!(got, [0, 1], "side 1 trails");
    }

    #[test]
    fn draft_collision_with_level_hp_gives_both_their_second_choice() {
        let (_, got) = drafted([20, 20], [[1, 0, 2], [1, 2, 0]]);
        assert_eq!(got, [0, 2]);
        // equal orders (mirrored inputs): both get the same second choice, the same outcome for both seats
        let (_, got) = drafted([20, 20], [[2, 1, 0], [2, 1, 0]]);
        assert_eq!(got, [1, 1]);
    }

    #[test]
    fn draft_sanitises_bad_orders() {
        let (_, got) = drafted([20, 20], [[5, 5, 5], [1, 1, 1]]);
        assert_eq!(got, [0, 1]);
    }
}
