//! One human (side A, the bottom of the board) against one bot, phase by phase. No rendering in here: the Bevy front
//! end reads this state and calls these methods, and the tests play whole matches through the same calls.
//!
//! The bot gets exactly what a second player would: it drafts on the public game, shapes and builds on its own
//! `shape_view`, and picks its crossing on its own `reveal_view`. The human's screen is built from `board()`, which
//! applies the same hiding to them.

use mt_bots::{make_bot, Bot};
use mt_core::clash::MAX_TICKS;
use mt_core::game::{Building, BUILDING_CAP};
use mt_core::*;

pub const HUMAN: usize = 0;
pub const BOT: usize = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Draft,
    /// Shape and build, hidden from the opponent until `ready`.
    Plan,
    /// Both bases are revealed; pick a crossing.
    Attack,
    /// The clash has been simulated; `frames` is being played back.
    Clash,
    Resolve,
    Over,
}

/// One thing the player added to their plan, so `undo` can take back the last one.
#[derive(Clone, Copy, Debug)]
enum Step {
    Op,
    Building,
    Unit,
}

#[derive(Clone, Debug)]
pub struct FrameUnit {
    pub id: u32,
    pub side: usize,
    pub kind: usize,
    pub tile: usize,
    pub hp: i32,
}

/// The battlefield every other tick of the clash, as the replay recorder saw it.
#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub tick: u32,
    pub units: Vec<FrameUnit>,
    pub attacks: Vec<(usize, usize)>,
    /// (building id, hp)
    pub building_hp: Vec<(u32, i32)>,
}

/// A unit that left the field during the playback, either killed or spent by breaching the enemy core.
#[derive(Clone, Debug)]
pub struct UnitExit {
    pub id: u32,
    /// The first frame the unit is absent from (clamped to the last frame for units that leave after it).
    pub frame: usize,
    pub side: usize,
    pub kind: usize,
    /// Where it was last seen.
    pub tile: usize,
    pub breached: bool,
}

/// Core HP lost at a frame of the playback.
#[derive(Clone, Copy, Debug)]
pub struct CoreHit {
    pub frame: usize,
    /// The side whose core was hit.
    pub side: usize,
    pub dmg: i32,
}

/// The state the clash started from. The real game already carries its outcome, so the playback shows these instead.
#[derive(Clone, Debug, Default)]
struct ClashStart {
    core: [i32; 2],
    ore: [i32; 2],
    roster: [Vec<UnitKind>; 2],
}

/// What one finished round came to, for the Resolve summary and the end-of-game recap.
#[derive(Clone, Debug)]
pub struct Recap {
    pub round: u32,
    /// Core damage the human dealt and took.
    pub dealt: i32,
    pub taken: i32,
    /// Units each side lost (killed or spent by breaching), from the rosters, so always exact.
    pub lost: [usize; 2],
    /// Of those, how many breached. `None` when the frames could not be matched to the recorded breach total.
    pub breached: Option<[usize; 2]>,
    pub craters: usize,
    pub ruins: usize,
    pub core_after: [i32; 2],
}

/// How a finished game was decided, in the order the rules check it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Decision {
    Knockout,
    CoreHp,
    ArmyValue,
    CoinFlip,
}

pub struct Session {
    pub g: Game,
    bot: Box<dyn Bot>,
    pub bot_name: String,
    pub phase: Phase,
    /// Draft: the offers the player has ranked so far, best first.
    pub order: Vec<usize>,
    pub plan: Vec<(usize, Op)>,
    pub build: BuildCmd,
    steps: Vec<Step>,
    pub frames: Vec<Frame>,
    /// Terrain and buildings as the clash started, for the playback (the real game already carries the scars).
    pub clash_terrain: [Terrain; N],
    pub clash_buildings: Vec<Building>,
    clash_start: ClashStart,
    /// Units leaving the field and core hits of the clash being played back, derived from the frames.
    pub exits: Vec<UnitExit>,
    pub hits: Vec<CoreHit>,
    /// Whether `hits` add up to what the recorder says the breaches were. If not, core HP is held until the playback ends.
    pub hits_exact: bool,
    /// Core damage dealt in the last clash, by side.
    pub last_damage: [i32; 2],
    /// Buildings raised in the last build step, both sides (public from the reveal on).
    pub new_buildings: Vec<u32>,
    pub recaps: Vec<Recap>,
    pub log: Vec<String>,
}

fn parse_frames(round: Option<&serde_json::Value>) -> Vec<Frame> {
    let num = |v: &serde_json::Value, i: usize| v[i].as_i64().unwrap_or(0);
    let list = |v: &serde_json::Value| v.as_array().cloned().unwrap_or_default();
    list(&round.map_or(serde_json::Value::Null, |r| r["frames"].clone()))
        .iter()
        .map(|f| Frame {
            tick: f["t"].as_u64().unwrap_or(0) as u32,
            units: list(&f["u"])
                .iter()
                .map(|u| FrameUnit {
                    id: num(u, 0) as u32,
                    side: num(u, 1) as usize,
                    kind: num(u, 2) as usize,
                    tile: num(u, 3) as usize,
                    hp: num(u, 4) as i32,
                })
                .collect(),
            attacks: list(&f["a"]).iter().map(|a| (num(a, 0) as usize, num(a, 1) as usize)).collect(),
            building_hp: list(&f["b"]).iter().map(|b| (num(b, 0) as u32, num(b, 1) as i32)).collect(),
        })
        .collect()
}

/// Who left the field when, and which core took the damage. The recorder has no event for either, so both are read off the
/// frames: a unit is gone from frame `i` but was in `i - 1`. A unit last seen next to the enemy core breaches on the next
/// tick (it never takes damage on that tick), so it is certain to be a breach; one two tiles away may have breached or died,
/// and is told apart by whether anything shot at its tile. The result is checked against the breach tiers the recorder
/// stored; `false` means it did not add up and the caller must not trust the timing of the hits.
fn derive_exits(frames: &[Frame], tiers: [i32; 2], round: u32) -> (Vec<UnitExit>, Vec<CoreHit>, bool) {
    let n = frames.len();
    let mut exits: Vec<UnitExit> = Vec::new();
    // `maybe` marks exits that could be either; the rest are decided.
    let mut maybe: Vec<bool> = Vec::new();
    for i in 1..=n {
        let prev = &frames[i - 1];
        // After the last frame only one more tick can run, and only if the clash ended before the tick limit.
        let at_end = i == n;
        if at_end && prev.tick >= MAX_TICKS {
            break;
        }
        for u in &prev.units {
            if !at_end && frames[i].units.iter().any(|v| v.id == u.id) {
                continue;
            }
            let d = cheb(u.tile, CORE[1 - u.side]);
            let shot = !at_end && frames[i].attacks.iter().any(|&(_, to)| to == u.tile);
            let unsure = d == 2 && !at_end;
            exits.push(UnitExit {
                id: u.id,
                frame: i.min(n - 1),
                side: u.side,
                kind: u.kind,
                tile: u.tile,
                breached: d <= 1 || (unsure && !shot),
            });
            maybe.push(unsure);
        }
    }
    let tier = |e: &UnitExit| ALL_UNITS[e.kind.min(5)].stats().tier;
    let sum = |exits: &[UnitExit], side: usize| exits.iter().filter(|e| e.breached && e.side == side).map(tier).sum::<i32>();
    let matches = |exits: &[UnitExit]| (0..2).all(|s| sum(exits, s) == tiers[s]);
    let mut exact = matches(&exits);
    if !exact {
        // the shot heuristic was wrong somewhere: try "all the unsure ones breached" and "none did"
        for all in [true, false] {
            let mut alt = exits.clone();
            for (e, _) in alt.iter_mut().zip(&maybe).filter(|(_, m)| **m) {
                e.breached = all;
            }
            if matches(&alt) {
                exits = alt;
                exact = true;
                break;
            }
        }
    }
    let hits = exits
        .iter()
        .filter(|e| e.breached)
        .map(|e| CoreHit {
            frame: e.frame,
            side: 1 - e.side,
            dmg: tier(e) * (round as i32 + 1),
        })
        .collect();
    (exits, hits, exact)
}

impl Session {
    pub fn new(seed: u64, bot_name: &str) -> Session {
        let mut g = Game::new(seed).with_recording();
        g.start_round();
        let terrain = g.terrain;
        let mut s = Session {
            g,
            bot: make_bot(bot_name, seed.wrapping_mul(7919) + 2),
            bot_name: bot_name.to_string(),
            phase: Phase::Draft,
            order: Vec::new(),
            plan: Vec::new(),
            build: BuildCmd::default(),
            steps: Vec::new(),
            frames: Vec::new(),
            clash_terrain: terrain,
            clash_buildings: Vec::new(),
            clash_start: ClashStart::default(),
            exits: Vec::new(),
            hits: Vec::new(),
            hits_exact: false,
            last_damage: [0, 0],
            new_buildings: Vec::new(),
            recaps: Vec::new(),
            log: Vec::new(),
        };
        s.say(format!(
            "Map {seed}. You are {}; the enemy is {}.",
            s.g.players[HUMAN].commander.name(),
            s.g.players[BOT].commander.name()
        ));
        s.say("Round 1: rank the cards. Click your first choice, then your second.".into());
        s
    }

    fn say(&mut self, line: String) {
        self.log.push(line);
    }

    /// The game as the human may see it right now.
    pub fn board(&self) -> Game {
        match self.phase {
            // my own shaping applied, the enemy's not yet known
            Phase::Plan => self.g.shape_view(HUMAN, &self.plan),
            // both bases public, the enemy's purchases of this round hidden
            Phase::Attack => self.g.reveal_view(HUMAN),
            // the real game already holds the outcome of the clash that is still being played back: show where it started
            Phase::Clash => {
                let mut v = self.g.sim_clone();
                v.terrain = self.clash_terrain;
                v.buildings = self.clash_buildings.clone();
                v.last_scars.clear();
                for side in 0..2 {
                    let p = &mut v.players[side];
                    p.core_hp = self.clash_start.core[side];
                    p.ore = self.clash_start.ore[side];
                    p.roster = self.clash_start.roster[side].clone();
                }
                v
            }
            _ => self.g.sim_clone(),
        }
    }

    /// Both cores' HP as the playback shows them at frame `frame`: counted down as breaches land. If the frames could not be
    /// matched to the recorded breaches, the starting HP holds until the playback is over.
    pub fn core_hp_at(&self, frame: usize) -> [i32; 2] {
        let mut hp = self.clash_start.core;
        if self.hits_exact {
            for h in self.hits.iter().filter(|h| h.frame <= frame) {
                hp[h.side] = (hp[h.side] - h.dmg).max(0);
            }
        }
        hp
    }

    /// Units alive at `frame`, per side and kind.
    pub fn alive_at(&self, frame: usize) -> [[usize; 6]; 2] {
        let mut n = [[0; 6]; 2];
        if let Some(f) = self.frames.get(frame) {
            for u in &f.units {
                n[u.side.min(1)][u.kind.min(5)] += 1;
            }
        }
        n
    }

    // ---- draft ----

    /// Rank offer `i` next. After two clicks the third is implied and the draft resolves.
    pub fn draft_click(&mut self, i: usize) {
        if self.phase != Phase::Draft || i >= 3 || self.order.contains(&i) {
            return;
        }
        self.order.push(i);
        if self.order.len() < 2 {
            return;
        }
        let mine = sanitize_order([self.order[0], self.order[1], 3]);
        let theirs = self.bot.draft(&self.g, BOT);
        let picks = self.g.resolve_draft([mine, theirs]);
        let offers = self.g.offers;
        self.g.apply_draft([mine, theirs]);
        let mut line = format!("You get {}; the enemy gets {}.", offers[picks[HUMAN]].name(), offers[picks[BOT]].name());
        if picks[HUMAN] != mine[0] {
            line += " (You both wanted the same card first.)";
        }
        self.say(line);
        self.order.clear();
        self.phase = Phase::Plan;
        self.say("Shape your half and build, then press Ready. The enemy cannot see it yet.".into());
    }

    /// Whether the human already owns what `card` unlocks, in which case taking it again only pays 2 ore.
    pub fn owns(&self, card: Card) -> bool {
        let me = &self.g.players[HUMAN];
        match card {
            Card::Unit(u) => me.units[u as usize],
            Card::Building(b) => me.buildings[b as usize],
            _ => false,
        }
    }

    // ---- shape and build ----

    pub fn tp_left(&self) -> i32 {
        self.g.tp(HUMAN) - self.plan.iter().map(|&(_, o)| o.cost()).sum::<i32>()
    }

    pub fn ore_left(&self) -> i32 {
        // during the playback the real game already holds the income of the round being played
        let have = if self.phase == Phase::Clash {
            self.clash_start.ore[HUMAN]
        } else {
            self.g.players[HUMAN].ore
        };
        let spent: i32 = self.build.buildings.iter().map(|&(k, _)| k.stats().cost).sum::<i32>() + self.build.units.iter().map(|u| u.stats().cost).sum::<i32>();
        have - spent
    }

    /// Why `op` cannot be used at all right now, whatever the tile.
    pub fn op_blocker(&self, op: Op) -> Option<String> {
        (op.cost() > self.tp_left()).then(|| format!("{} costs {} TP; you have {} left.", op.name(), op.cost(), self.tp_left()))
    }

    /// Whether `op` may be applied to `tile` now. `try_op` and the hover preview both go through here, so they cannot disagree.
    pub fn check_op(&self, tile: usize, op: Op) -> Result<(), String> {
        if self.phase != Phase::Plan {
            return Err("Not the shaping phase.".into());
        }
        if tile >= N || !own_half(tile, HUMAN) {
            return Err("You can only shape your own half (the bottom).".into());
        }
        if near_core(tile) {
            return Err("Tiles next to a core cannot be shaped.".into());
        }
        if self.plan.iter().any(|&(t, _)| t == tile) {
            return Err("You already shape that tile this round.".into());
        }
        if self.build.buildings.iter().any(|&(_, t)| t == tile) || self.g.buildings.iter().any(|b| b.tile == tile) {
            return Err("There is a building on that tile.".into());
        }
        if let Some(why) = self.op_blocker(op) {
            return Err(why);
        }
        let mut next = self.plan.clone();
        next.push((tile, op));
        // the rules have the last word: the plan must survive sanitising unchanged
        if self.g.sanitize_plan(HUMAN, &next) != next {
            return Err(format!("{} does nothing on that terrain.", op.name()));
        }
        Ok(())
    }

    /// What `op` would turn `tile` into, if it may be applied.
    pub fn op_preview(&self, tile: usize, op: Op) -> Option<(Terrain, Terrain)> {
        self.check_op(tile, op).ok()?;
        let from = self.g.terrain[tile];
        op.apply(from).map(|to| (from, to))
    }

    /// Whether adding `op` on `tile` would cut the last ground path, so that the round's blocking ops get reverted.
    pub fn would_revert(&self, tile: usize, op: Op) -> bool {
        if !op.blocks_ground() || self.check_op(tile, op).is_err() {
            return false;
        }
        let mut next = self.plan.clone();
        next.push((tile, op));
        self.g
            .shape_view(HUMAN, &next)
            .last_ops
            .iter()
            .any(|r| r.tile == tile && r.outcome == OpOutcome::Reverted)
    }

    /// Every tile `op` may be applied to right now.
    pub fn legal_op_tiles(&self, op: Op) -> Vec<usize> {
        (0..N).filter(|&t| self.check_op(t, op).is_ok()).collect()
    }

    pub fn try_op(&mut self, tile: usize, op: Op) -> Result<(), String> {
        self.check_op(tile, op)?;
        self.plan.push((tile, op));
        self.steps.push(Step::Op);
        if self.board().last_ops.iter().any(|r| r.outcome == OpOutcome::Reverted) {
            self.say("That would cut the last path to your core, so your blocking ops will be reverted.".into());
        }
        Ok(())
    }

    /// Why `kind` cannot be built at all right now, whatever the tile.
    pub fn building_blocker(&self, kind: BuildingKind) -> Option<String> {
        if self.phase != Phase::Plan {
            return Some("Not the build phase.".into());
        }
        if !self.g.players[HUMAN].buildings[kind as usize] {
            return Some(format!("You have not drafted the {}.", kind.name()));
        }
        if kind.stats().cost > self.ore_left() {
            return Some(format!("A {} costs {} ore; you have {} left.", kind.name(), kind.stats().cost, self.ore_left()));
        }
        let standing = self.g.buildings.iter().filter(|b| b.side == HUMAN).count() + self.build.buildings.len();
        (standing >= BUILDING_CAP).then(|| format!("You may have at most {BUILDING_CAP} buildings."))
    }

    /// Whether `kind` may be placed on `tile` now (shared by `try_building` and the hover tint).
    pub fn check_building(&self, kind: BuildingKind, tile: usize) -> Result<(), String> {
        if let Some(why) = self.building_blocker(kind) {
            return Err(why);
        }
        if self.build.buildings.iter().any(|&(_, t)| t == tile) || !self.board().building_tile_ok(HUMAN, kind, tile) {
            return Err(if kind == BuildingKind::Extractor {
                "An extractor needs a free ore tile in your build zone.".into()
            } else {
                "Buildings need a free plain or high tile in your build zone (the bottom seven rows).".into()
            });
        }
        Ok(())
    }

    /// Every tile `kind` may be placed on right now.
    pub fn legal_building_tiles(&self, kind: BuildingKind) -> Vec<usize> {
        if self.building_blocker(kind).is_some() {
            return Vec::new();
        }
        let view = self.board();
        (0..N)
            .filter(|&t| view.building_tile_ok(HUMAN, kind, t) && !self.build.buildings.iter().any(|&(_, b)| b == t))
            .collect()
    }

    pub fn try_building(&mut self, kind: BuildingKind, tile: usize) -> Result<(), String> {
        self.check_building(kind, tile)?;
        self.build.buildings.push((kind, tile));
        self.steps.push(Step::Building);
        Ok(())
    }

    /// Why `kind` cannot be bought right now.
    pub fn unit_blocker(&self, kind: UnitKind) -> Option<String> {
        if self.phase != Phase::Plan {
            return Some("Not the build phase.".into());
        }
        if !self.g.players[HUMAN].units[kind as usize] {
            return Some(format!("You have not drafted the {}.", kind.name()));
        }
        if kind.stats().cost > self.ore_left() {
            return Some(format!("A {} costs {} ore; you have {} left.", kind.name(), kind.stats().cost, self.ore_left()));
        }
        (self.g.players[HUMAN].roster.len() + self.build.units.len() >= ROSTER_CAP).then(|| format!("Your army is full ({ROSTER_CAP} units)."))
    }

    pub fn try_unit(&mut self, kind: UnitKind) -> Result<(), String> {
        if let Some(why) = self.unit_blocker(kind) {
            return Err(why);
        }
        self.build.units.push(kind);
        self.steps.push(Step::Unit);
        Ok(())
    }

    /// Take back the last op, building or unit of this round's plan.
    pub fn undo(&mut self) {
        if self.phase != Phase::Plan {
            return;
        }
        match self.steps.pop() {
            Some(Step::Op) => drop(self.plan.pop()),
            Some(Step::Building) => drop(self.build.buildings.pop()),
            Some(Step::Unit) => drop(self.build.units.pop()),
            None => {}
        }
    }

    /// Commit the plan. The bot commits its own at the same moment, then both bases are revealed.
    pub fn ready(&mut self) {
        if self.phase != Phase::Plan {
            return;
        }
        let bot_plan = self.bot.terraform(&self.g, BOT);
        let bot_build = self.bot.build(&self.g.shape_view(BOT, &bot_plan), BOT);
        let standing: Vec<u32> = self.g.buildings.iter().map(|b| b.id).collect();
        self.g.apply_terraform([std::mem::take(&mut self.plan), bot_plan]);
        self.g.apply_build([std::mem::take(&mut self.build), bot_build]);
        self.steps.clear();
        self.new_buildings = self.g.buildings.iter().map(|b| b.id).filter(|id| !standing.contains(id)).collect();
        let enemy_ops: Vec<&str> = self
            .g
            .last_ops
            .iter()
            .filter(|r| r.side == BOT && r.outcome == OpOutcome::Applied)
            .map(|r| r.op.name())
            .collect();
        let built = self.g.buildings.iter().filter(|b| b.side == BOT && self.new_buildings.contains(&b.id)).count();
        self.say(format!(
            "Reveal: the enemy shaped {} tile(s){} and raised {built} building(s). Its new units are hidden.",
            enemy_ops.len(),
            if enemy_ops.is_empty() {
                String::new()
            } else {
                format!(" ({})", enemy_ops.join(", "))
            }
        ));
        if self.g.last_ops.iter().any(|r| r.outcome == OpOutcome::Reverted) {
            self.say("Blocking ops were reverted: together they cut the last path between the cores.".into());
        }
        self.phase = Phase::Attack;
        self.say("Pick your crossing: click the column your army should cross the midline on.".into());
    }

    // ---- attack, clash, resolve ----

    /// Choose the crossing (a board column, which is the human's own frame) and fight the clash.
    pub fn attack(&mut self, column: u8) {
        if self.phase != Phase::Attack {
            return;
        }
        let theirs = self.bot.attack(&self.g.reveal_view(BOT), BOT);
        self.g.apply_attack([column, theirs]);
        self.clash_terrain = self.g.terrain;
        self.clash_buildings = self.g.buildings.clone();
        self.clash_start = ClashStart {
            core: [self.g.players[0].core_hp, self.g.players[1].core_hp],
            ore: [self.g.players[0].ore, self.g.players[1].ore],
            roster: [self.g.players[0].roster.clone(), self.g.players[1].roster.clone()],
        };
        self.g.clash_and_resolve();
        let before = self.clash_start.core;
        self.last_damage = [before[1] - self.g.players[1].core_hp, before[0] - self.g.players[0].core_hp];
        let round = self.g.rec.as_ref().and_then(|r| r.last_round());
        self.frames = parse_frames(round);
        let tiers = round.map_or([0, 0], |r| {
            [r["breach"][0].as_i64().unwrap_or(0) as i32, r["breach"][1].as_i64().unwrap_or(0) as i32]
        });
        (self.exits, self.hits, self.hits_exact) = derive_exits(&self.frames, tiers, self.g.round);
        self.phase = Phase::Clash;
    }

    /// The column, on the board, where `side`'s army crosses.
    pub fn crossing_column(&self, side: usize) -> usize {
        xy(rel(self.g.players[side].crossing as usize, side)).0 as usize
    }

    /// Called when the playback has finished (or was skipped).
    pub fn finish_clash(&mut self) {
        if self.phase != Phase::Clash {
            return;
        }
        let [dealt, taken] = self.last_damage;
        let scars = self.g.last_scars.clone();
        let (craters, ruins) = (
            scars.iter().filter(|s| s.1 == ScarKind::Crater).count(),
            scars.iter().filter(|s| s.1 == ScarKind::Ruin).count(),
        );
        let lost = [0, 1].map(|s| self.clash_start.roster[s].len().saturating_sub(self.g.players[s].roster.len()));
        let breached = self
            .hits_exact
            .then(|| [0, 1].map(|s| self.exits.iter().filter(|e| e.breached && e.side == s).count()));
        self.recaps.push(Recap {
            round: self.g.round,
            dealt,
            taken,
            lost,
            breached,
            craters,
            ruins,
            core_after: [self.g.players[0].core_hp, self.g.players[1].core_hp],
        });
        self.say(format!("Round {}: you dealt {dealt} core damage and took {taken}.", self.g.round));
        if craters + ruins > 0 {
            self.say(format!("The fight left {craters} crater(s) and {ruins} ruin(s)."));
        }
        if self.g.is_over() {
            self.phase = Phase::Over;
            let won = self.g.winner() == HUMAN;
            let how = if self.g.is_coin_flip() { " on a coin flip" } else { "" };
            self.say(format!(
                "{}{how}. Cores: you {}, enemy {}.",
                if won { "You win" } else { "You lose" },
                self.g.players[0].core_hp,
                self.g.players[1].core_hp
            ));
        } else {
            self.phase = Phase::Resolve;
        }
    }

    /// How the finished game was decided (the order the rules check it in).
    pub fn decided_by(&self) -> Decision {
        let (a, b) = (&self.g.players[0], &self.g.players[1]);
        if a.core_hp != b.core_hp {
            return if a.core_hp.min(b.core_hp) <= 0 {
                Decision::Knockout
            } else {
                Decision::CoreHp
            };
        }
        if a.roster_value() != b.roster_value() {
            Decision::ArmyValue
        } else {
            Decision::CoinFlip
        }
    }

    pub fn next_round(&mut self) {
        if self.phase != Phase::Resolve {
            return;
        }
        self.g.start_round();
        self.phase = Phase::Draft;
        self.say(format!("Round {}: rank the cards.", self.g.round));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scripted player: ranks the cards in order, shapes one tile, buys what it can, crosses on `column`.
    fn play_round(s: &mut Session, column: u8) {
        assert_eq!(s.phase, Phase::Draft);
        s.draft_click(0);
        assert_eq!(s.phase, Phase::Draft, "one click does not finish the draft");
        s.draft_click(0);
        s.draft_click(2);
        assert_eq!(s.phase, Phase::Plan);
        let legal = s.g.legal_ops(HUMAN);
        if let Some(&(tile, op)) = legal.iter().find(|&&(_, op)| op == Op::Raise) {
            assert!(s.try_op(tile, op).is_ok());
            assert!(s.try_op(tile, Op::Grow).is_err(), "one op per tile");
        }
        while s.try_unit(UnitKind::Grunt).is_ok() {}
        assert!(s.ore_left() >= 0 && s.ore_left() < UnitKind::Grunt.stats().cost || s.g.players[HUMAN].roster.len() + s.build.units.len() == ROSTER_CAP);
        s.ready();
        assert_eq!(s.phase, Phase::Attack);
        s.attack(column);
        assert_eq!(s.phase, Phase::Clash);
        assert!(!s.frames.is_empty());
        s.finish_clash();
    }

    #[test]
    fn a_whole_match_can_be_played_through_the_session() {
        for (seed, bot) in [(1, "greedy"), (2, "search"), (3, "shaper")] {
            let mut s = Session::new(seed, bot);
            let mut rounds = 0;
            while s.phase != Phase::Over {
                play_round(&mut s, (rounds * 3 % 9) as u8);
                rounds += 1;
                if s.phase == Phase::Resolve {
                    s.next_round();
                }
                assert!(rounds <= MAX_ROUNDS as usize);
            }
            assert!(s.g.is_over());
            assert_eq!(s.g.round as usize, rounds);
        }
    }

    #[test]
    fn the_plan_is_checked_against_the_rules_and_can_be_undone() {
        let mut s = Session::new(5, "greedy");
        s.draft_click(1);
        s.draft_click(0);
        let theirs = s.g.legal_ops(BOT)[0];
        assert!(s.try_op(theirs.0, theirs.1).is_err(), "the enemy half is off limits");
        assert!(s.try_op(CORE[HUMAN] + 1, Op::Raise).is_err(), "next to the core");
        let (tile, op) = s.g.legal_ops(HUMAN).into_iter().find(|&(_, op)| op.cost() == 2).unwrap();
        s.try_op(tile, op).unwrap();
        assert_eq!(s.tp_left(), s.g.tp(HUMAN) - 2);
        assert_eq!(
            s.board().terrain[tile],
            op.apply(s.g.terrain[tile]).unwrap(),
            "the player sees their own shaping"
        );
        assert_eq!(s.g.terrain[tile], Game::new(5).terrain[tile], "the real game is untouched until Ready");
        let ore = s.ore_left();
        s.try_unit(UnitKind::Grunt).unwrap();
        assert_eq!(s.ore_left(), ore - UnitKind::Grunt.stats().cost);
        assert!(s.try_unit(UnitKind::Gunship).is_err() || s.g.players[HUMAN].units[UnitKind::Gunship as usize]);
        s.undo();
        assert_eq!(s.ore_left(), ore);
        s.undo();
        assert!(s.plan.is_empty() && s.tp_left() == s.g.tp(HUMAN));
        s.undo(); // nothing left: a no-op
    }

    #[test]
    fn the_enemys_new_units_stay_hidden_until_the_clash() {
        let mut s = Session::new(7, "greedy");
        s.draft_click(0);
        s.draft_click(1);
        s.ready();
        let (shown, real) = (s.board().players[BOT].roster.len(), s.g.players[BOT].roster.len());
        assert!(real > 0, "the greedy bot bought nothing in round 1");
        assert_eq!(shown, 0, "the attack screen must not show this round's purchases");
        s.attack(4);
        assert_eq!(s.crossing_column(HUMAN), 4);
        assert!(s.frames[0].units.iter().any(|u| u.side == BOT), "the clash shows them");
    }

    /// Draft, buy grunts, and cross on `column`, stopping with the clash ready to play back.
    fn to_clash(s: &mut Session, column: u8) {
        s.draft_click(0);
        s.draft_click(1);
        while s.try_unit(UnitKind::Grunt).is_ok() {}
        s.ready();
        s.attack(column);
        assert_eq!(s.phase, Phase::Clash);
    }

    #[test]
    fn the_playback_does_not_show_the_outcome_before_it_happens() {
        let (mut exact, mut total, mut any_hit) = (0, 0, false);
        for seed in 1..=12u64 {
            let mut s = Session::new(seed, if seed % 2 == 0 { "greedy" } else { "shaper" });
            let mut round = 0;
            while s.phase != Phase::Over {
                to_clash(&mut s, (seed + round) as u8 % 9);
                let before = s.clash_start.core;
                let shown = s.board();
                assert_eq!(
                    [shown.players[0].core_hp, shown.players[1].core_hp],
                    before,
                    "the board shows the pre-clash cores"
                );
                assert_eq!(shown.players[0].roster, s.clash_start.roster[0], "and the pre-clash army");
                assert_eq!(s.core_hp_at(0), before);
                let last = s.frames.len() - 1;
                let after = [s.g.players[0].core_hp, s.g.players[1].core_hp];
                let mut prev = before;
                for f in 0..=last {
                    let now = s.core_hp_at(f);
                    assert!(now[0] <= prev[0] && now[1] <= prev[1], "core HP only goes down during a clash");
                    prev = now;
                }
                total += 1;
                any_hit |= !s.hits.is_empty();
                if s.hits_exact {
                    exact += 1;
                    assert_eq!(
                        s.core_hp_at(last),
                        after,
                        "seed {seed} round {}: the countdown must end where the game says",
                        s.g.round
                    );
                } else {
                    assert_eq!(s.core_hp_at(last), before, "an unmatched clash holds the old HP");
                }
                let alive = s.alive_at(0);
                assert_eq!(alive[0].iter().sum::<usize>(), s.clash_start.roster[0].len());
                s.finish_clash();
                let r = s.recaps.last().unwrap();
                assert_eq!(r.core_after, after);
                assert_eq!([r.dealt, r.taken], s.last_damage);
                if let Some(b) = r.breached {
                    assert!(b[0] <= r.lost[0] && b[1] <= r.lost[1]);
                }
                if s.phase == Phase::Resolve {
                    s.next_round();
                }
                round += 1;
            }
        }
        // The frames are matched to the recorder's breach totals; a miss only costs the live countdown, but it should be rare.
        assert!(any_hit, "no clash in the sample breached a core, so the countdown was never exercised");
        assert!(exact * 10 >= total * 9, "only {exact} of {total} clashes matched their breach totals");
    }

    #[test]
    fn the_hover_check_and_the_click_always_agree() {
        let mut s = Session::new(9, "greedy");
        s.draft_click(0);
        s.draft_click(1);
        for _ in 0..3 {
            for op in ALL_OPS {
                for tile in 0..N {
                    let check = s.check_op(tile, op);
                    let plan_len = s.plan.len();
                    let did = s.try_op(tile, op);
                    assert_eq!(check, did, "tile {tile} {op:?}");
                    assert_eq!(s.plan.len(), plan_len + usize::from(did.is_ok()));
                    if did.is_ok() {
                        s.undo();
                    }
                    assert_eq!(s.legal_op_tiles(op).contains(&tile), check.is_ok());
                }
            }
            // spend a point of TP so the second pass has a different budget
            if let Some((t, o)) = s.g.legal_ops(HUMAN).into_iter().find(|&(t, o)| s.check_op(t, o).is_ok()) {
                s.try_op(t, o).unwrap();
            }
        }
        for kind in [BuildingKind::Turret, BuildingKind::Flak, BuildingKind::Extractor] {
            let legal = s.legal_building_tiles(kind);
            for tile in 0..N {
                let check = s.check_building(kind, tile);
                assert_eq!(check.is_ok(), legal.contains(&tile));
            }
            if let Some(&t) = legal.first() {
                assert!(s.try_building(kind, t).is_ok());
                assert!(s.check_building(kind, t).is_err(), "the tile is taken now");
            }
        }
        assert_eq!(s.op_blocker(Op::Expose).is_some(), Op::Expose.cost() > s.tp_left());
    }

    #[test]
    fn a_preview_names_the_terrain_it_makes_and_reverts_are_spotted() {
        let mut s = Session::new(5, "greedy");
        s.draft_click(0);
        s.draft_click(1);
        let (tile, _) = s.g.legal_ops(HUMAN).into_iter().find(|&(_, op)| op == Op::Raise).unwrap();
        assert_eq!(s.op_preview(tile, Op::Raise), Some((Terrain::Plain, Terrain::High)));
        assert_eq!(s.op_preview(tile, Op::Clear), None, "clearing plain does nothing");
        // a wall is only ever refused for cutting the path if it does; the check must agree with what Ready would do
        for (t, op) in s.g.legal_ops(HUMAN).into_iter().filter(|&(_, op)| op.blocks_ground()) {
            let mut probe = s.g.shape_view(HUMAN, &[(t, op)]);
            let reverted = probe.last_ops.iter().any(|r| r.outcome == OpOutcome::Reverted);
            assert_eq!(s.would_revert(t, op), reverted);
            probe.last_ops.clear();
        }
    }

    #[test]
    fn the_reveal_marks_what_was_built_and_the_end_says_how_it_was_decided() {
        let mut s = Session::new(7, "greedy");
        s.draft_click(0);
        s.draft_click(1);
        let free = s.legal_building_tiles(BuildingKind::Turret);
        if let Some(&t) = free.first() {
            s.try_building(BuildingKind::Turret, t).unwrap();
        }
        let planned = s.build.buildings.len();
        s.ready();
        let raised: Vec<_> = s.g.buildings.iter().filter(|b| s.new_buildings.contains(&b.id)).collect();
        assert!(raised.iter().filter(|b| b.side == HUMAN).count() <= planned);
        assert_eq!(raised.len(), s.new_buildings.len(), "every marked building exists");
        assert!(s.g.last_ops.iter().all(|r| r.outcome == OpOutcome::Applied || r.op.blocks_ground()));

        let mut s = Session::new(2, "greedy");
        s.g.players[1].core_hp = 0;
        assert_eq!(s.decided_by(), Decision::Knockout);
        s.g.players[1].core_hp = 5;
        assert_eq!(s.decided_by(), Decision::CoreHp);
        s.g.players[0].core_hp = 5;
        s.g.players[0].roster.push(UnitKind::Rhino);
        assert_eq!(s.decided_by(), Decision::ArmyValue);
        s.g.players[0].roster.clear();
        assert_eq!(s.decided_by(), Decision::CoinFlip);
    }
}
