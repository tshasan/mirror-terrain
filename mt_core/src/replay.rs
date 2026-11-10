use crate::clash::CUnit;
use crate::defs::*;
use crate::game::{Building, OpOutcome, OpRecord, Player, ScarKind};
use serde_json::{Map, Value, json};

/// Builds the replay JSON the viewer reads. It only ever copies state out of the game, so recording cannot change play.
#[derive(Clone, Debug)]
pub struct Recorder {
    head: Map<String, Value>,
    rounds: Vec<Value>,
    cur: Map<String, Value>,
    frames: Vec<Value>,
}

fn terrain_json(t: &[Terrain; N]) -> Value {
    t.iter().map(|x| *x as u8).collect()
}

fn names(cards: &[Card]) -> Value {
    cards.iter().map(|c| c.name()).collect()
}

impl Recorder {
    pub fn new(seed: u64, terrain: &[Terrain; N]) -> Self {
        let mut head = Map::new();
        head.insert("seed".into(), json!(seed));
        head.insert("w".into(), json!(W));
        head.insert("h".into(), json!(H));
        head.insert("cores".into(), json!(CORE));
        head.insert("terrain0".into(), terrain_json(terrain));
        Recorder {
            head,
            rounds: Vec::new(),
            cur: Map::new(),
            frames: Vec::new(),
        }
    }

    fn set(&mut self, key: &str, v: Value) {
        self.cur.insert(key.into(), v);
    }

    pub fn begin_round(&mut self, round: u32, offers: &[Card; 3]) {
        self.cur = Map::new();
        self.frames.clear();
        self.set("round", json!(round));
        self.set("offers", names(offers));
    }

    pub fn draft(&mut self, picks: [usize; 2]) {
        self.set("picks", json!(picks));
    }

    pub fn terraform(&mut self, recs: &[OpRecord], terrain: &[Terrain; N]) {
        let ops: Value = recs
            .iter()
            .map(|r| {
                let out = match r.outcome {
                    OpOutcome::Applied => "applied",
                    OpOutcome::Reverted => "reverted",
                };
                json!({"tile": r.tile, "side": r.side, "op": r.op.name(), "out": out})
            })
            .collect();
        self.set("ops", ops);
        self.set("terrain", terrain_json(terrain));
    }

    pub fn build(&mut self, buildings: &[Building], players: &[Player; 2]) {
        let b: Value = buildings.iter().map(|b| json!([b.id, b.side, b.kind as u8, b.tile, b.hp])).collect();
        let roster: Value = players.iter().map(|p| p.roster.iter().map(|u| *u as u8).collect::<Value>()).collect();
        let decks: Value = players.iter().map(|p| names(&p.picks)).collect();
        self.set("buildings", b);
        self.set("roster", roster);
        self.set("decks", decks);
        self.set("ore", json!([players[0].ore, players[1].ore]));
    }

    pub fn attack(&mut self, crossings: [u8; 2]) {
        self.set("crossings", json!(crossings));
    }

    pub fn frame(&mut self, tick: u32, units: &[CUnit], attacks: &[(usize, usize)], buildings: &[Building], bhp: &[i32]) {
        let u: Value = units
            .iter()
            .filter(|u| u.alive)
            .map(|u| json!([u.id, u.side, u.kind as u8, u.tile, u.hp]))
            .collect();
        let a: Value = attacks.iter().map(|&(f, t)| json!([f, t])).collect();
        let b: Value = buildings.iter().zip(bhp).map(|(b, h)| json!([b.id, h])).collect();
        self.frames.push(json!({"t": tick, "u": u, "a": a, "b": b}));
    }

    /// The scars the resolve step applied, as `[tile, kind]` pairs (kind 0 = crater, 1 = ruin).
    pub fn scars(&mut self, scars: &[(usize, ScarKind)]) {
        self.set("scars", scars.iter().map(|&(t, k)| json!([t, k as u8])).collect());
    }

    pub fn resolve(&mut self, players: &[Player; 2], breach: [i32; 2]) {
        let frames = std::mem::take(&mut self.frames);
        self.set("frames", Value::Array(frames));
        self.set("breach", json!(breach));
        self.set("core", json!([players[0].core_hp, players[1].core_hp]));
        self.rounds.push(Value::Object(std::mem::take(&mut self.cur)));
    }

    /// The most recently resolved round, as recorded: the clash frames a front end animates are under `"frames"`.
    pub fn last_round(&self) -> Option<&Value> {
        self.rounds.last()
    }

    pub fn finish(&self, winner: usize, names: [&str; 2], commanders: [&str; 2]) -> String {
        let mut out = self.head.clone();
        out.insert("bots".into(), json!(names));
        out.insert("commanders".into(), json!(commanders));
        out.insert("winner".into(), json!(winner));
        out.insert("rounds".into(), Value::Array(self.rounds.clone()));
        Value::Object(out).to_string()
    }
}
