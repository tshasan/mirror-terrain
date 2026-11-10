//! The words on the screen: what each op, card, unit, building and terrain does. Numbers are read from `mt_core` (`stats()`,
//! `Op::apply`, `move_cost`...) so they cannot drift from the rules; only the effects that are not a number are prose here,
//! and each one is checked against `docs/RULES.md`. No drawing in this file.
//!
//! The default font only has plain ASCII, so there are no arrows, dots or dashes beyond `->` and `-`.

use crate::session::{Phase, Session, BOT, HUMAN};
use mt_core::game::EXTRACTOR_INCOME;
use mt_core::*;

/// What a click on the board does in the shaping phase.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
    None,
    Op(Op),
    Building(BuildingKind),
}

pub const ALL_TERRAIN: [Terrain; 7] = [
    Terrain::Plain,
    Terrain::High,
    Terrain::Trench,
    Terrain::Wall,
    Terrain::Water,
    Terrain::Forest,
    Terrain::Ore,
];

pub fn terrain_name(t: Terrain) -> &'static str {
    ["plain", "high", "trench", "wall", "water", "forest", "ore"][t as usize]
}

pub const BUILDINGS: [BuildingKind; 3] = [BuildingKind::Turret, BuildingKind::Flak, BuildingKind::Extractor];

fn times(cost: u32) -> String {
    let x = cost as f32 / 100.0;
    if x.fract() == 0.0 {
        format!("x{x:.0}")
    } else {
        format!("x{x:.1}")
    }
}

/// What a terrain does to units. Movement comes from `move_cost`; the combat effects are the rules in RULES.md.
pub fn terrain_effect(t: Terrain) -> String {
    let grunt = move_cost(UnitKind::Grunt, t, false);
    let mut s = match grunt {
        None => "Ground units cannot enter; air units fly over.".to_string(),
        Some(100) => "Normal speed for ground units.".to_string(),
        Some(c) => format!("Ground units are {} slower here.", times(c)),
    };
    if grunt.is_some() {
        match move_cost(UnitKind::Rhino, t, false) {
            None => s += " Rhinos cannot enter.",
            Some(c) if Some(c) != grunt => s += &format!(" Rhinos {} slower.", times(c)),
            _ => {}
        }
        if move_cost(UnitKind::Ranger, t, false) < grunt {
            s += " Rangers move freely.";
        }
    }
    s += match t {
        Terrain::High => " Ground ranged units get +2 range and buildings +1; ground units on it take half damage.",
        Terrain::Forest => " Hides ground units from air units and from anything more than 1 tile away; cover cuts damage to 3/4 (1/2 for rangers).",
        Terrain::Wall | Terrain::Water => " On your own half, a unit or building next to a wall deals +50%.",
        Terrain::Ore => " An extractor can be built here.",
        _ => "",
    };
    s
}

/// "raise: plain -> high" with every terrain the op works on, from `Op::apply`.
pub fn op_text(op: Op) -> String {
    let from: Vec<&str> = ALL_TERRAIN.iter().filter(|t| op.apply(**t).is_some()).map(|t| terrain_name(*t)).collect();
    let to = ALL_TERRAIN.iter().find_map(|t| op.apply(*t)).map_or("", terrain_name);
    let mut s = format!("{} ({} TP): {} -> {}.", op.name(), op.cost(), from.join(" or "), to);
    if op.blocks_ground() {
        s += " Blocks ground paths; if your ops together cut the last path between the cores, they are reverted.";
    }
    s
}

fn speed_word(speed: u32) -> &'static str {
    match speed {
        14.. => "very fast",
        12..=13 => "fast",
        10..=11 => "medium",
        _ => "slow",
    }
}

pub fn unit_text(k: UnitKind) -> String {
    let st = k.stats();
    let mut s = format!(
        "{}: {} HP, {} dmg per {} ticks, range {}, {}, {}. Hits {}. Tier {}, {} ore.",
        k.name(),
        st.hp,
        st.dmg,
        st.cooldown,
        st.range,
        if st.air { "flies" } else { "ground" },
        speed_word(st.speed),
        if st.hits_air { "ground and air" } else { "ground only" },
        st.tier,
        st.cost
    );
    s += match k {
        UnitKind::Rhino => " No trench; slow in forest.",
        UnitKind::Ranger => " Moves freely through forest.",
        UnitKind::Archer => " Gains range on high ground.",
        UnitKind::Drone | UnitKind::Gunship => " Ignores terrain.",
        _ => "",
    };
    s
}

pub fn building_text(k: BuildingKind) -> String {
    let st = k.stats();
    if st.dmg == 0 {
        return format!(
            "{}: {} HP. +{EXTRACTOR_INCOME} ore every round. Only on a free ore tile in your build zone. {} ore.",
            k.name(),
            st.hp,
            st.cost
        );
    }
    format!(
        "{}: {} HP, {} dmg every {} ticks, range {}, hits {} only. Build zone only (your bottom 7 rows). {} ore.",
        k.name(),
        st.hp,
        st.dmg,
        st.cooldown,
        st.range,
        if st.hits_air { "air" } else { "ground" },
        st.cost
    )
}

/// What a draft card gives.
pub fn card_text(card: Card, owned: bool) -> String {
    let mut s = match card {
        Card::Unit(u) => format!("Unlocks {}", unit_text(u)),
        Card::Building(b) => format!("Unlocks {}", building_text(b)),
        Card::Earthshaper => "+1 TP every round: one more op each round.".to_string(),
        Card::Grovekeeper | Card::Sapper | Card::Prospector => "+2 ore now.".to_string(),
        Card::HoverTreads => "Your ground units cross trench at normal speed, Rhino included.".to_string(),
        Card::Camo => "Your ground units in forest take half damage.".to_string(),
    };
    if owned {
        s += " You own it: pays 2 ore instead.";
    }
    s
}

pub fn tag_name(card: Card) -> &'static str {
    match card.tag() {
        Tag::Ground => "ground",
        Tag::Air => "air",
        Tag::Defense => "defense",
        Tag::Terrain => "terrain",
        Tag::Economy => "economy",
    }
}

/// Breaks `text` into lines of at most `width` characters (the default font is monospace), on spaces.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split_whitespace() {
            if !line.is_empty() && line.len() + 1 + word.len() > width {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line += word;
        }
        lines.push(line);
    }
    lines
}

/// The one thing to do now, in words.
pub fn instruction(s: &Session, tool: Tool) -> String {
    match s.phase {
        Phase::Draft => match s.order.len() {
            0 => "Rank the cards: click the one you want most.".into(),
            _ => "Now click your second choice. The third is implied.".into(),
        },
        Phase::Plan => {
            let hint = match tool {
                Tool::None => "Pick an op or a building, then click a tile on your half.".to_string(),
                Tool::Op(op) => format!("Click a lit tile to {} it.", op.name()),
                Tool::Building(k) => format!("Click a lit tile for the {}.", k.name()),
            };
            if tool == Tool::None {
                hint
            } else {
                format!("{hint} Then buy units and press Ready.")
            }
        }
        Phase::Attack if s.g.last_ops.iter().any(|r| r.outcome == OpOutcome::Reverted) => {
            "Pick a column to cross the midline. Blocking ops were reverted (red x).".into()
        }
        Phase::Attack => "Pick a column for your army to cross the midline.".into(),
        Phase::Clash => "Watch the clash. Armies that cross near each other fight.".into(),
        Phase::Resolve => "Round over. Look at the scars, then start the next round.".into(),
        Phase::Over => "Game over. Hover the board to review the scars.".into(),
    }
}

/// What the reveal showed about the enemy's turn, for the Attack panel.
pub fn reveal_lines(s: &Session) -> Vec<String> {
    let view = s.board();
    let ops = |side: usize, out: OpOutcome| view.last_ops.iter().filter(|r| r.side == side && r.outcome == out).count();
    let enemy_ops: Vec<&str> = view
        .last_ops
        .iter()
        .filter(|r| r.side == BOT && r.outcome == OpOutcome::Applied)
        .map(|r| r.op.name())
        .collect();
    let enemy_built: Vec<&str> = view
        .buildings
        .iter()
        .filter(|b| b.side == BOT && s.new_buildings.contains(&b.id))
        .map(|b| b.kind.name())
        .collect();
    let list = |v: &[&str]| if v.is_empty() { "nothing".to_string() } else { v.join(", ") };
    let mut lines = vec![
        format!("Enemy shaped: {}.", list(&enemy_ops)),
        format!("Enemy built: {}.", list(&enemy_built)),
        format!("You shaped {} tile(s).", ops(HUMAN, OpOutcome::Applied)),
    ];
    let reverted = ops(HUMAN, OpOutcome::Reverted) + ops(BOT, OpOutcome::Reverted);
    if reverted > 0 {
        lines.push(format!(
            "{reverted} blocking op(s) reverted: together they cut the last path between the cores."
        ));
    }
    lines
}

/// A one-line description of the terrain and building on a tile.
pub fn tile_text(s: &Session, tile: usize) -> String {
    let view = s.board();
    let t = view.terrain[tile];
    let mut out = format!("{}. {}", terrain_name(t), terrain_effect(t));
    if tile == CORE[HUMAN] || tile == CORE[BOT] {
        let side = usize::from(tile == CORE[BOT]);
        out = format!(
            "{} core: {} HP. Units that get within 1 tile breach it. It also fires (2 dmg, range 2).",
            if side == HUMAN { "Your" } else { "Enemy" },
            view.players[side].core_hp
        );
    }
    if let Some(b) = view.buildings.iter().find(|b| b.tile == tile) {
        out = format!(
            "{} {} on {}: {}/{} HP. {}",
            if b.side == HUMAN { "Your" } else { "Enemy" },
            b.kind.name(),
            terrain_name(t),
            b.hp,
            b.kind.stats().hp,
            building_text(b.kind)
        );
    }
    if let Some(&(_, kind)) = s.g.last_scars.iter().find(|(x, _)| *x == tile) {
        if matches!(s.phase, Phase::Resolve | Phase::Over) {
            out = format!("{} {}", scar_text(kind), terrain_effect(t));
        }
    }
    out
}

pub fn scar_text(kind: ScarKind) -> &'static str {
    match kind {
        ScarKind::Crater => "Crater: ground units died here, so it became trench.",
        ScarKind::Ruin => "Ruin: a building was destroyed here, so it became forest.",
    }
}

/// Why the game ended the way it did, in a sentence.
pub fn decision_text(s: &Session) -> String {
    use crate::session::Decision::*;
    let (a, b) = (s.g.players[HUMAN].core_hp, s.g.players[BOT].core_hp);
    match s.decided_by() {
        Knockout => format!("Knockout: a core fell to 0 HP (you {a}, enemy {b})."),
        CoreHp => format!("After round {MAX_ROUNDS} the higher core HP wins (you {a}, enemy {b})."),
        ArmyValue => format!(
            "Cores level at {a}; the bigger army wins (value you {}, enemy {}).",
            s.g.players[HUMAN].roster_value(),
            s.g.players[BOT].roster_value()
        ),
        CoinFlip => format!("Cores level at {a} and armies equal: decided by a coin flip from the seed."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptions_come_from_the_rules() {
        assert_eq!(op_text(Op::Raise), "raise (1 TP): plain -> high.");
        assert!(op_text(Op::Lower).contains("plain or high -> trench"));
        assert!(op_text(Op::Clear).contains("trench or wall or forest") || op_text(Op::Clear).contains("-> plain"));
        assert!(op_text(Op::Wall).contains("reverted"));
        assert!(terrain_effect(Terrain::Trench).contains("x2.5") && terrain_effect(Terrain::Trench).contains("Rhinos cannot enter"));
        assert!(terrain_effect(Terrain::Forest).contains("Rangers move freely") && terrain_effect(Terrain::Forest).contains("Rhinos x2.5"));
        assert!(terrain_effect(Terrain::Water).contains("cannot enter"));
        assert!(unit_text(UnitKind::Gunship).contains("22 HP") && unit_text(UnitKind::Gunship).contains("flies"));
        assert!(unit_text(UnitKind::Grunt).contains("ground only"));
        assert!(building_text(BuildingKind::Flak).contains("hits air only"));
        assert!(card_text(Card::Unit(UnitKind::Rhino), true).contains("2 ore"));
        for card in CARD_POOL {
            assert!(!card_text(card, false).is_empty());
        }
    }

    #[test]
    fn wrapping_keeps_every_line_within_the_width() {
        for line in wrap("one two three four five six seven eight\nnine ten", 12) {
            assert!(line.len() <= 12, "{line}");
        }
        assert_eq!(wrap("a b", 80), vec!["a b"]);
    }

    #[test]
    fn every_phase_has_an_instruction() {
        let mut s = Session::new(3, "greedy");
        assert!(instruction(&s, Tool::None).contains("Rank"));
        s.draft_click(0);
        s.draft_click(1);
        assert!(instruction(&s, Tool::Op(Op::Raise)).contains("raise"));
        s.ready();
        assert!(instruction(&s, Tool::None).contains("column"));
        assert!(reveal_lines(&s).len() >= 3);
        assert!(!tile_text(&s, 0).is_empty());
        assert!(tile_text(&s, CORE[BOT]).contains("Enemy core"));
    }
}
