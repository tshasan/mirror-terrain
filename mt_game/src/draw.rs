//! The three layers of the screen. `static_layer` is everything that only changes when the game does, `hover_layer` follows
//! the mouse, `clash_layer` is redrawn every frame of the playback. Each takes a `Painter`, so the tests can run them without
//! a window.

use crate::art::{self, CELL};
use crate::info::*;
use crate::paint::*;
use crate::session::{Phase, Session, BOT, HUMAN};
use crate::ui::*;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use mt_core::game::START_CORE_HP;
use mt_core::*;

/// The description box: what the thing under the mouse is, in words.
const INFO_Y: f32 = 460.0;
const INFO_H: f32 = 88.0;
const INFO_LINES: usize = 5;
const CONTROLS_Y: f32 = 212.0;
const NOTICE_Y: f32 = 552.0;
const LOG_Y: f32 = 586.0;
const LOG_LINES: usize = 5;
const LEGEND_Y: f32 = 672.0;
/// Characters per line in the panel at a font size.
fn cols(size: f32, width: f32) -> usize {
    (width / (size * CHAR_W)) as usize
}

fn v(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

// ---- small shapes shared by the panel and the board ----

/// How the board's pixel art is scaled: sprites are 16 px cells drawn at 2x (32 px) on the board and 1x in the panel, and the
/// grass and water textures at 3x on a one-pixel-trimmed cell (42 px), so every pixel is a whole number of screen pixels.
const SPRITE: f32 = 32.0;
const SWATCH: f32 = 16.0;
const TERRAIN: f32 = TILE - 2.0;

/// A unit as drawn on the board and in the army chips: the pack's sprite on a plate in its side's colour. The plate is a
/// square for ground units and a diamond for air units, and air units also float above a small shadow, so flying reads
/// without the sprite. `(kind, side)` picks the sprite; `plate` and `alpha` are passed in so a fading ghost can reuse this.
fn unit_icon(p: &mut Painter, c: Vec2, (kind, side): (usize, usize), px: f32, plate: Color, alpha: f32, z: f32) {
    let kind = kind.min(5);
    let pad = if px >= SPRITE { 6.0 } else { 4.0 };
    let mut at = c;
    if ALL_UNITS[kind].is_air() {
        p.diamond(c, (px + pad) * 0.78, plate, z);
        p.rect(c + v(0.0, px * 0.47), v(px * 0.6, px * 0.12), INK.with_alpha(0.45 * alpha), z + 0.1);
        at.y -= px / 8.0;
    } else {
        p.rect(c, Vec2::splat(px + pad), plate, z);
    }
    p.sprite(at, px, art::unit(kind, side), false, alpha, z + 0.2);
}

fn building_icon(p: &mut Painter, c: Vec2, side: usize, kind: usize, fill: Color, z: f32) {
    p.rect(c, Vec2::splat(38.0), SIDE[side], z);
    p.rect(c, Vec2::splat(34.0), fill, z + 0.1);
    p.sprite(c, SPRITE, art::building(kind, side), false, 1.0, z + 0.2);
}

fn building(p: &mut Painter, tile: usize, side: usize, kind: usize, hp_share: f32, planned: bool) {
    let c = tile_center(tile);
    building_icon(p, c, side, kind, if planned { BUTTON_ON } else { PAPER }, 2.0);
    if hp_share < 1.0 {
        p.rect(c + v(0.0, 17.0), v(32.0, 4.0), INK.with_alpha(0.7), 2.25);
        p.rect(c + v(-15.0 * (1.0 - hp_share), 17.0), v(30.0 * hp_share, 2.0), BAD, 2.3);
    }
}

fn core_box(p: &mut Painter, side: usize, hp: i32) {
    let c = tile_center(CORE[side]);
    p.rect(c, Vec2::splat(40.0), SIDE[side], 2.0);
    p.sprite(c + v(0.0, -3.0), SPRITE, art::core(side), false, 1.0, 2.1);
    // the HP stays a number: it is the thing the whole game is about. The glyph font brings its own outline, so it needs no plate.
    p.glyphs(c + v(0.0, 14.0), &hp.to_string(), 1.0, Color::WHITE, Anchor::Center, 2.2);
}

/// Which grass cell a plain tile shows: mostly the plain one, about one in six with a few sprouts. It is a fixed
/// function of the tile and its point-mirror tile, so the board stays as symmetric as the map and nothing the game decides
/// (or a random number) can touch it. The legend, which has no tile, always shows plain grass.
fn grass_cell(tile: Option<usize>) -> usize {
    let Some(t) = tile else {
        return art::GRASS;
    };
    // (Flowers were tried: their orange and yellow read as the enemy's colour and as ore, so the board has sprouts only.)
    if (t.min(N - 1 - t) as u32).wrapping_mul(2_654_435_761) >> 20 & 31 < 6 {
        art::GRASS_SPROUTS
    } else {
        art::GRASS
    }
}

/// A terrain tile: the grass or water texture, then the object that says what it is. `base` is the texture's size and
/// `obj` the size of a 16 px object cell, so the board (42, 32) and the legend swatches (16, 16) share this. A trench is a
/// whole tile of its own, ditch and grass rows together.
fn terrain_art(p: &mut Painter, c: Vec2, t: Terrain, base: f32, obj: f32, z: f32, tile: Option<usize>) {
    let inset = base == TERRAIN;
    match t {
        Terrain::Water => p.sprite(c, base, art::WATER, inset, 1.0, z),
        Terrain::Trench => p.sprite(c, base, art::TRENCH, inset, 1.0, z),
        _ => {
            p.sprite(c, base, if t == Terrain::Plain { grass_cell(tile) } else { art::GRASS }, inset, 1.0, z);
            let object = match t {
                Terrain::High => art::HILL,
                Terrain::Wall => art::BARRICADE,
                Terrain::Forest => art::FOREST,
                Terrain::Ore => art::ORE,
                _ => return,
            };
            p.sprite(c, obj, object, false, 1.0, z + 0.1);
        }
    }
}

/// A reveal or plan marker: a ring in the side colour with a white ring inside it, so it shows on grass, water and hills.
fn mark(p: &mut Painter, c: Vec2, color: Color) {
    p.outline(c, Vec2::splat(TERRAIN), 5.0, Color::WHITE.with_alpha(0.85), 1.4);
    p.outline(c, Vec2::splat(TERRAIN), 3.0, color, 1.5);
}

fn bar(p: &mut Painter, tl: Vec2, size: Vec2, share: f32, color: Color) {
    p.rect_tl(tl, size, Color::srgb(0.78, 0.77, 0.73), 4.0);
    p.rect_tl(tl, v(size.x * share.clamp(0.0, 1.0), size.y), color, 4.1);
}

struct Btn<'a> {
    pos: Vec2,
    size: Vec2,
    label: &'a str,
    on: bool,
    enabled: bool,
    action: Action,
    hint: String,
    icon: Option<Icon>,
}

/// The sprite a button shows before its label.
#[derive(Clone, Copy)]
enum Icon {
    Unit(UnitKind),
    Building(BuildingKind),
}

/// Draws a button and records its click target. A button that cannot be used is drawn flat and grey, and its hint says why.
fn button(p: &mut Painter, buttons: &mut Vec<Button>, b: Btn) {
    let fill = if !b.enabled {
        BUTTON_OFF
    } else if b.on {
        BUTTON_ON
    } else {
        BUTTON
    };
    p.rect_tl(b.pos, b.size, fill, 4.0);
    if !b.enabled {
        p.outline(b.pos + b.size / 2.0, b.size, 1.0, Color::srgb(0.80, 0.79, 0.75), 4.05);
    }
    let size = 13.0;
    let mut left = 8.0;
    if let Some(icon) = b.icon {
        let c = b.pos + v(16.0, b.size.y / 2.0);
        let alpha = if b.enabled { 1.0 } else { 0.4 };
        match icon {
            Icon::Unit(k) => unit_icon(p, c, (k as usize, HUMAN), SWATCH, SIDE[HUMAN].with_alpha(alpha), alpha, 4.1),
            Icon::Building(k) => p.sprite(c, SWATCH, art::building(k as usize, HUMAN), false, alpha, 4.1),
        }
        left = 30.0;
    }
    p.text_at(
        b.pos + v(left, (b.size.y - size * 1.2) / 2.0),
        b.label,
        size,
        if b.enabled { INK } else { FAINT },
        Anchor::TopLeft,
        5.0,
    );
    buttons.push(Button {
        rect: Rect::from_corners(b.pos, b.pos + b.size),
        action: b.action,
        hint: b.hint,
        enabled: b.enabled,
    });
}

// ---- the panel ----

const STRIP: [&str; 5] = ["Draft", "Shape & build", "Attack", "Clash", "Result"];

fn phase_index(phase: Phase) -> usize {
    match phase {
        Phase::Draft => 0,
        Phase::Plan => 1,
        Phase::Attack => 2,
        Phase::Clash => 3,
        Phase::Resolve | Phase::Over => 4,
    }
}

/// The phase strip and, under it, the one thing to do now. Returns the y below them.
fn header(p: &mut Painter, s: &Session, tool: Tool) -> f32 {
    let now = phase_index(s.phase);
    let mut x = PANEL_X;
    for (i, name) in STRIP.iter().enumerate() {
        let w = name.len() as f32 * 13.0 * CHAR_W + 14.0;
        let (fill, ink) = match i.cmp(&now) {
            std::cmp::Ordering::Equal => (INK, Color::WHITE),
            std::cmp::Ordering::Less => (BUTTON, MUTED),
            std::cmp::Ordering::Greater => (BUTTON_OFF, FAINT),
        };
        p.rect_tl(v(x, 12.0), v(w, 24.0), fill, 4.0);
        p.glyph(v(x + w / 2.0, 24.0), name, 13.0, ink, 5.0);
        x += w + 4.0;
    }
    // "Round" stays body text; the numbers are the glyph font, with the label right-aligned just left of them.
    let count = format!("{}/{MAX_ROUNDS}", s.g.round);
    let (_, w) = art::glyph_run(&count);
    p.glyphs(v(PANEL_X + PANEL_W, 24.0), &count, 1.0, Color::WHITE, Anchor::CenterRight, 5.0);
    p.text_right(v(PANEL_X + PANEL_W - w - 6.0, 17.0), "Round", 13.0, MUTED);
    let accent = if matches!(s.phase, Phase::Draft | Phase::Plan | Phase::Attack) {
        SIDE[HUMAN]
    } else {
        INK
    };
    p.rect_tl(v(PANEL_X, 44.0), v(PANEL_W, 58.0), BOX, 3.0);
    p.rect_tl(v(PANEL_X, 44.0), v(6.0, 58.0), accent, 3.1);
    let text = wrap(&instruction(s, tool), cols(18.0, PANEL_W - 28.0));
    p.lines(v(PANEL_X + 16.0, 51.0), &text, 18.0, 22.0, INK);
    110.0
}

/// What the status block shows: it is fed from the game outside the clash and from the playback during it.
struct Status {
    core: [i32; 2],
    counts: [[usize; 6]; 2],
    ordered: [usize; 6],
}

fn status_block(p: &mut Painter, s: &Session, st: &Status, mut y: f32) -> f32 {
    for side in [HUMAN, BOT] {
        let pl = &s.g.players[side];
        let who = if side == HUMAN { "You" } else { "Enemy" };
        p.text(v(PANEL_X, y), &format!("{who} ({})", pl.commander.name()), 15.0, SIDE[side]);
        bar(
            p,
            v(PANEL_X + 160.0, y + 2.0),
            v(110.0, 12.0),
            st.core[side] as f32 / START_CORE_HP as f32,
            SIDE[side],
        );
        p.text(
            v(PANEL_X + 276.0, y + 1.0),
            &format!("core {}/{START_CORE_HP}", st.core[side].max(0)),
            13.0,
            INK,
        );
        if side == HUMAN {
            let tp = if s.phase == Phase::Plan {
                format!("  TP {}/{}", s.tp_left(), s.g.tp(HUMAN))
            } else {
                String::new()
            };
            p.text_right(v(PANEL_X + PANEL_W, y + 1.0), &format!("ore {}{tp}", s.ore_left()), 13.0, INK);
        } else if s.phase == Phase::Attack {
            p.text_right(v(PANEL_X + PANEL_W, y + 26.0), "its new units are hidden", 12.0, FAINT);
        }
        let mut x = PANEL_X;
        let mut any = false;
        for (k, _) in ALL_UNITS.iter().enumerate() {
            let (n, extra) = (st.counts[side][k], if side == HUMAN { st.ordered[k] } else { 0 });
            if n + extra == 0 {
                continue;
            }
            any = true;
            unit_icon(p, v(x + 10.0, y + 31.0), (k, side), SWATCH, SIDE[side], 1.0, 4.0);
            let label = if extra > 0 { format!("{n}+{extra}") } else { n.to_string() };
            p.text(v(x + 22.0, y + 24.0), &label, 13.0, INK);
            x += 28.0 + label.len() as f32 * 13.0 * CHAR_W + 8.0;
        }
        if !any {
            p.text(v(PANEL_X, y + 24.0), "no units yet", 13.0, FAINT);
        }
        y += 52.0;
    }
    y
}

fn log_and_notice(p: &mut Painter, s: &Session, notice: &str) {
    let width = cols(13.0, PANEL_W);
    let note = wrap(notice, width);
    p.lines(v(PANEL_X, NOTICE_Y), &note[..note.len().min(2)], 13.0, 15.0, BAD);
    // The newest entry is the one to read; older ones fade. Entries are shown whole or not at all.
    let mut rows: Vec<(String, usize)> = Vec::new();
    for (age, line) in s.log.iter().rev().enumerate() {
        let w = wrap(line, width);
        if age > 0 && rows.len() + w.len() > LOG_LINES {
            break;
        }
        rows.splice(0..0, w.into_iter().map(|l| (l, age)));
    }
    rows.truncate(LOG_LINES);
    for (i, (line, age)) in rows.iter().enumerate() {
        let color = match age {
            0 => INK,
            1 => MUTED,
            _ => FAINT,
        };
        p.text(v(PANEL_X, LOG_Y + i as f32 * 15.0), line, 13.0, color);
    }
}

fn legend(p: &mut Painter) {
    let mut y = LEGEND_Y;
    let item = |p: &mut Painter, x: &mut f32, y: f32, swatch: &dyn Fn(&mut Painter, Vec2), name: &str| {
        swatch(p, v(*x + 9.0, y + 8.0));
        p.text(v(*x + 22.0, y + 2.0), name, 12.0, MUTED);
        *x += 22.0 + name.len() as f32 * 12.0 * CHAR_W + 8.0;
    };
    let mut x = PANEL_X;
    for t in ALL_TERRAIN {
        item(p, &mut x, y, &|p, c| terrain_art(p, c, t, SWATCH, SWATCH, 4.0, None), terrain_name(t));
    }
    y += 20.0;
    x = PANEL_X;
    for k in 0..6 {
        item(
            p,
            &mut x,
            y,
            &|p, c| unit_icon(p, c, (k, HUMAN), SWATCH, SIDE[HUMAN], 1.0, 4.0),
            ALL_UNITS[k].name(),
        );
    }
    y += 20.0;
    x = PANEL_X;
    for b in BUILDINGS {
        item(
            p,
            &mut x,
            y,
            &|p, c| p.sprite(c, SWATCH, art::building(b as usize, HUMAN), false, 1.0, 4.0),
            b.name(),
        );
    }
    for (kind, name) in [(ScarKind::Crater, "crater"), (ScarKind::Ruin, "ruin")] {
        item(p, &mut x, y, &|p, c| scar_mark(p, c, kind, SWATCH, 4.0), name);
    }
    y += 20.0;
    p.text(
        v(PANEL_X, y + 2.0),
        "Plate: teal you, orange enemy. Diamond plate + shadow = flies.",
        12.0,
        MUTED,
    );
}

// ---- the board ----

/// A few trees, bushes and mushrooms in the margins of the window. Fixed positions
/// (no random numbers), all outside the 9x16 board, so none of it can be read as terrain. The orange autumn tree and the
/// flowers are left out on purpose: orange belongs to the enemy. (A fence under the board was tried and looked like a stray bar.)
fn margin_decor(p: &mut Painter) {
    use art::{BUSH, MUSHROOMS, TREE_PINE};
    // the left margin is 24 px wide, the gutter between the board and the panel 32
    let left = [
        (70.0, TREE_PINE),
        (190.0, BUSH),
        (330.0, TREE_PINE),
        (470.0, MUSHROOMS),
        (600.0, BUSH),
        (700.0, TREE_PINE),
    ];
    let gutter = [
        (46.0, BUSH),
        (160.0, TREE_PINE),
        (300.0, MUSHROOMS),
        (420.0, TREE_PINE),
        (560.0, BUSH),
        (690.0, TREE_PINE),
    ];
    let top = [(250.0, BUSH), (330.0, MUSHROOMS), (398.0, TREE_PINE)];
    for (y, cell) in left {
        p.sprite(v(BOARD.x / 2.0, y), CELL, cell, false, 1.0, 0.1);
    }
    for (y, cell) in gutter {
        p.sprite(v((BOARD.x + 9.0 * TILE + PANEL_X) / 2.0, y), CELL, cell, false, 1.0, 0.1);
    }
    for (x, cell) in top {
        p.sprite(v(x, 14.0), CELL, cell, false, 1.0, 0.1);
    }
}

fn board(p: &mut Painter, ui: &Ui, view: &Game) {
    let s = &ui.s;
    let clash = s.phase == Phase::Clash;
    margin_decor(p);
    for t in 0..N {
        let c = tile_center(t);
        terrain_art(p, c, view.terrain[t], TERRAIN, SPRITE, 0.0, Some(t));
    }
    // the midline, and the edge of each build zone
    let row_y = |rows_from_top: f32| BOARD.y + rows_from_top * TILE;
    p.rect(v(BOARD.x + 4.5 * TILE, row_y(8.0)), v(9.0 * TILE, 3.0), INK, 1.0);
    for rows in [7.0, 9.0] {
        p.rect(v(BOARD.x + 4.5 * TILE, row_y(rows)), v(9.0 * TILE, 1.0), MUTED, 1.0);
    }
    p.text(v(BOARD.x, 6.0), &format!("ENEMY ({})", s.bot_name), 14.0, SIDE[BOT]);
    p.text(v(BOARD.x, BOARD.y + 16.0 * TILE + 4.0), "YOU", 14.0, SIDE[HUMAN]);
    if !clash {
        for side in 0..2 {
            core_box(p, side, view.players[side].core_hp);
        }
        for b in &view.buildings {
            building(p, b.tile, b.side, b.kind as usize, b.hp as f32 / b.kind.stats().hp as f32, false);
        }
    }
    match s.phase {
        Phase::Plan => {
            for &(tile, _) in &s.plan {
                mark(p, tile_center(tile), SIDE[HUMAN]);
            }
            for &(kind, tile) in &s.build.buildings {
                building(p, tile, HUMAN, kind as usize, 1.0, true);
            }
        }
        Phase::Attack => {
            // what the reveal made public: the tiles each side shaped, and the buildings just raised
            for r in &view.last_ops {
                let c = tile_center(r.tile);
                if r.outcome == OpOutcome::Applied {
                    mark(p, c, SIDE[r.side]);
                } else {
                    mark(p, c, BAD);
                    p.glyph(c + v(13.0, -13.0), "x", 16.0, BAD, 3.0);
                }
            }
            for b in view.buildings.iter().filter(|b| s.new_buildings.contains(&b.id)) {
                let c = tile_center(b.tile) + v(-12.0, -19.0);
                p.rect(c, v(24.0, 11.0), SIDE[b.side], 2.5);
                p.glyph(c, "new", 9.0, Color::WHITE, 2.6);
            }
        }
        Phase::Clash | Phase::Resolve | Phase::Over => {
            for side in 0..2 {
                let col = s.crossing_column(side);
                if clash {
                    // each army's lane: the column it crosses the midline on, from the midline to the enemy half's far end
                    let (top, bottom) = if side == HUMAN {
                        (BOARD.y, row_y(8.0))
                    } else {
                        (row_y(8.0), BOARD.y + 16.0 * TILE)
                    };
                    p.rect_tl(v(BOARD.x + col as f32 * TILE, top), v(TILE, bottom - top), SIDE[side].with_alpha(0.34), 0.5);
                }
                p.diamond(midline_point(col), 16.0, SIDE[side], 3.0);
            }
            if !clash {
                for &(tile, kind) in &s.g.last_scars {
                    scar_mark(p, tile_center(tile), kind, SPRITE, 3.0);
                }
            }
        }
        Phase::Draft => {}
    }
}

/// A scar: the crater or ruin sprite in a white frame. The tile under it already shows what it became (trench, forest), so the
/// frame is what says "this just happened here"; without it the pale ruin would vanish into the forest it leaves.
fn scar_mark(p: &mut Painter, c: Vec2, kind: ScarKind, size: f32, z: f32) {
    let ring = if size >= SPRITE { 3.0 } else { 1.0 };
    p.rect(c, Vec2::splat(size + 2.0 * ring + 2.0), INK, z - 0.1);
    p.rect(c, Vec2::splat(size + 2.0 * ring), Color::WHITE, z - 0.05);
    p.sprite(c, size, if kind == ScarKind::Crater { art::CRATER } else { art::RUIN }, false, 1.0, z);
}

// ---- the three layers ----

pub fn static_layer(p: &mut Painter, ui: &mut Ui) {
    let s = &ui.s;
    let view = s.board();
    let mut buttons: Vec<Button> = Vec::new();
    board(p, ui, &view);
    let tool = ui.tool;
    header(p, s, tool);
    if s.phase != Phase::Clash {
        let mut ordered = [0; 6];
        for u in &s.build.units {
            ordered[*u as usize] += 1;
        }
        let counts = [0, 1].map(|side| {
            let mut n = [0; 6];
            for u in &view.players[side].roster {
                n[*u as usize] += 1;
            }
            n
        });
        let st = Status {
            core: [view.players[0].core_hp, view.players[1].core_hp],
            counts,
            ordered,
        };
        status_block(p, s, &st, 110.0);
    }
    let y = CONTROLS_Y;
    match s.phase {
        Phase::Draft => draft_controls(p, &mut buttons, s, &view, y),
        Phase::Plan => plan_controls(p, &mut buttons, s, tool, &view, y),
        Phase::Attack => attack_controls(p, s, y),
        Phase::Clash => clash_controls(p, &mut buttons, ui, y),
        Phase::Resolve => resolve_controls(p, &mut buttons, s, y),
        Phase::Over => over_controls(p, &mut buttons, s, y),
    }
    log_and_notice(p, s, &ui.notice);
    legend(p);
    ui.buttons = buttons;
}

fn draft_controls(p: &mut Painter, buttons: &mut Vec<Button>, s: &Session, view: &Game, mut y: f32) {
    for (i, card) in view.offers.iter().enumerate() {
        let rank = s.order.iter().position(|&o| o == i);
        let owned = s.owns(*card);
        let text = card_text(*card, owned);
        let label = format!(
            "[{}] {}  ({}){}",
            i + 1,
            card.name(),
            tag_name(*card),
            rank.map_or(String::new(), |r| format!("   choice {}", r + 1))
        );
        let size = v(PANEL_W, 70.0);
        let pos = v(PANEL_X, y);
        button(
            p,
            buttons,
            Btn {
                pos,
                size,
                label: "",
                on: rank.is_some(),
                enabled: true,
                action: Action::Draft(i),
                hint: text.clone(),
                icon: None,
            },
        );
        p.text(pos + v(8.0, 6.0), &label, 15.0, INK);
        let desc = wrap(&text, cols(12.0, PANEL_W - 16.0));
        p.lines(pos + v(8.0, 28.0), &desc[..desc.len().min(3)], 12.0, 14.0, MUTED);
        y += size.y + 5.0;
    }
    let rule = "Same first pick: the player behind on core HP gets it.";
    p.lines(v(PANEL_X, y + 2.0), &wrap(rule, cols(12.0, PANEL_W)), 12.0, 14.0, FAINT);
}

fn plan_controls(p: &mut Painter, buttons: &mut Vec<Button>, s: &Session, tool: Tool, view: &Game, mut y: f32) {
    let me = &view.players[HUMAN];
    p.text(
        v(PANEL_X, y),
        &format!("Ops (TP cost in brackets): {} TP left. Keys 1-7.", s.tp_left()),
        13.0,
        MUTED,
    );
    y += 16.0;
    for (i, op) in ALL_OPS.iter().enumerate() {
        let blocker = s.op_blocker(*op);
        let mut hint = op_text(*op);
        if let Some(why) = &blocker {
            hint += &format!(" Not now: {why}");
        }
        button(
            p,
            buttons,
            Btn {
                pos: v(PANEL_X + (i % 4) as f32 * 118.0, y + (i / 4) as f32 * 32.0),
                size: v(114.0, 28.0),
                label: &format!("{} {} ({})", i + 1, op.name(), op.cost()),
                on: tool == Tool::Op(*op),
                enabled: blocker.is_none(),
                action: Action::Pick(Tool::Op(*op)),
                hint,
                icon: None,
            },
        );
    }
    y += 66.0;
    p.text(
        v(PANEL_X, y),
        &format!("Buildings (ore cost): {} ore left. Keys Q W E.", s.ore_left()),
        13.0,
        MUTED,
    );
    y += 16.0;
    let mut x = 0.0;
    for (n, kind) in BUILDINGS.into_iter().enumerate().filter(|(_, k)| me.buildings[*k as usize]) {
        let blocker = s.building_blocker(kind);
        let mut hint = building_text(kind);
        if let Some(why) = &blocker {
            hint += &format!(" Not now: {why}");
        }
        button(
            p,
            buttons,
            Btn {
                pos: v(PANEL_X + x, y),
                size: v(150.0, 28.0),
                label: &format!("{} {} ({})", ["Q", "W", "E"][n], kind.name(), kind.stats().cost),
                on: tool == Tool::Building(kind),
                enabled: blocker.is_none(),
                action: Action::Pick(Tool::Building(kind)),
                hint,
                icon: Some(Icon::Building(kind)),
            },
        );
        x += 156.0;
    }
    if x == 0.0 {
        p.text(v(PANEL_X, y + 7.0), "none drafted yet (turret, flak and extractor are cards)", 12.0, FAINT);
    }
    y += 34.0;
    p.text(v(PANEL_X, y), "Units: click to buy one (ore cost).", 13.0, MUTED);
    y += 16.0;
    for (n, kind) in ALL_UNITS.into_iter().filter(|k| me.units[*k as usize]).enumerate() {
        let blocker = s.unit_blocker(kind);
        let mut hint = unit_text(kind);
        if let Some(why) = &blocker {
            hint += &format!(" Not now: {why}");
        }
        button(
            p,
            buttons,
            Btn {
                pos: v(PANEL_X + (n % 3) as f32 * 156.0, y + (n / 3) as f32 * 32.0),
                size: v(150.0, 28.0),
                label: &format!("{} ({})", kind.name(), kind.stats().cost),
                on: false,
                enabled: blocker.is_none(),
                action: Action::Unit(kind),
                hint,
                icon: Some(Icon::Unit(kind)),
            },
        );
    }
    y += 66.0;
    let hint = "Takes back your last op, building or unit.".to_string();
    button(
        p,
        buttons,
        Btn {
            pos: v(PANEL_X, y),
            size: v(150.0, 30.0),
            label: "Undo  [Z]",
            on: false,
            enabled: true,
            action: Action::Undo,
            hint,
            icon: None,
        },
    );
    let hint = "Commits your plan. The enemy commits its own at the same moment, then both bases are revealed.".to_string();
    button(
        p,
        buttons,
        Btn {
            pos: v(PANEL_X + 156.0, y),
            size: v(200.0, 30.0),
            label: "Ready  [Enter]",
            on: true,
            enabled: true,
            action: Action::Ready,
            hint,
            icon: None,
        },
    );
}

fn attack_controls(p: &mut Painter, s: &Session, mut y: f32) {
    p.text(v(PANEL_X, y), "What the reveal showed", 13.0, MUTED);
    y += 18.0;
    let width = cols(14.0, PANEL_W);
    let lines: Vec<String> = reveal_lines(s).iter().flat_map(|l| wrap(l, width)).collect();
    y = p.lines(v(PANEL_X, y), &lines, 14.0, 18.0, INK) + 6.0;
    // the marks on the board
    let mut x = PANEL_X;
    for (color, name) in [(SIDE[HUMAN], "your shaping"), (SIDE[BOT], "enemy shaping"), (BAD, "reverted")] {
        p.outline(v(x + 7.0, y + 8.0), Vec2::splat(14.0), 3.0, color, 4.0);
        p.text(v(x + 18.0, y + 2.0), name, 12.0, MUTED);
        x += 18.0 + name.len() as f32 * 12.0 * CHAR_W + 12.0;
    }
    p.rect(v(x + 11.0, y + 8.0), v(24.0, 11.0), MUTED, 4.0);
    p.glyph(v(x + 11.0, y + 8.0), "new", 9.0, Color::WHITE, 4.1);
    p.text(v(x + 26.0, y + 2.0), "just built", 12.0, MUTED);
    y += 28.0;
    let how = "Your units spawn on the column you pick and its two neighbours, walk to the midline there, then take the shortest path to the enemy core. The enemy picks its column in secret. Armies that cross near each other meet in the field.";
    p.lines(v(PANEL_X, y), &wrap(how, width), 14.0, 18.0, MUTED);
}

fn clash_controls(p: &mut Painter, buttons: &mut Vec<Button>, ui: &Ui, y: f32) {
    let mut x = PANEL_X;
    let paused = ui.paused;
    button(
        p,
        buttons,
        Btn {
            pos: v(x, y),
            size: v(104.0, 30.0),
            label: if paused { "Resume [P]" } else { "Pause [P]" },
            on: paused,
            enabled: true,
            action: Action::Pause,
            hint: "Pauses or resumes the playback.".into(),
            icon: None,
        },
    );
    x += 108.0;
    for (i, sp) in SPEEDS.iter().enumerate() {
        button(
            p,
            buttons,
            Btn {
                pos: v(x, y),
                size: v(70.0, 30.0),
                label: &format!("{sp}x [{}]", i + 1),
                on: ui.speed == i,
                enabled: true,
                action: Action::Speed(i),
                hint: format!("Plays the clash at {sp}x speed."),
                icon: None,
            },
        );
        x += 74.0;
    }
    button(
        p,
        buttons,
        Btn {
            pos: v(x, y),
            size: v(PANEL_X + PANEL_W - x, 30.0),
            label: "Skip [Space]",
            on: false,
            enabled: true,
            action: Action::SkipClash,
            hint: "Jumps to the result of this round.".into(),
            icon: None,
        },
    );
    let note = "Faint columns are the two armies' lanes: teal yours, orange the enemy's. A fading unit fell (red) or breached a core (white); each breach flashes the core it hit. The cores' HP counts down as hits land.";
    p.lines(v(PANEL_X, y + 80.0), &wrap(note, cols(13.0, PANEL_W)), 13.0, 16.0, MUTED);
}

fn resolve_controls(p: &mut Painter, buttons: &mut Vec<Button>, s: &Session, mut y: f32) {
    if let Some(r) = s.recaps.last() {
        p.text(v(PANEL_X, y), &format!("Round {} summary", r.round), 16.0, INK);
        y += 26.0;
        let lost = |side: usize| match r.breached {
            Some(b) => format!("{} ({} breached, {} killed)", r.lost[side], b[side], r.lost[side].saturating_sub(b[side])),
            None => r.lost[side].to_string(),
        };
        let rows = [
            ("Core damage", format!("you dealt {}, took {}", r.dealt, r.taken)),
            ("Your losses", lost(HUMAN)),
            ("Enemy losses", lost(BOT)),
            ("Scars left", format!("{} crater(s), {} ruin(s)", r.craters, r.ruins)),
            ("Cores now", format!("you {}, enemy {}", r.core_after[0], r.core_after[1])),
        ];
        for (label, value) in rows {
            p.text(v(PANEL_X, y), label, 14.0, MUTED);
            p.text(v(PANEL_X + 120.0, y), &value, 14.0, INK);
            y += 20.0;
        }
        y = p.lines(
            v(PANEL_X, y + 2.0),
            &wrap(
                "A framed crater (dark pits) is now trench; a framed ruin (grey rubble) is now forest. Hover a scar on the board to read it.",
                cols(12.0, PANEL_W),
            ),
            12.0,
            14.0,
            MUTED,
        ) + 8.0;
    }
    button(
        p,
        buttons,
        Btn {
            pos: v(PANEL_X, y + 4.0),
            size: v(220.0, 32.0),
            label: "Next round  [Enter]",
            on: true,
            enabled: true,
            action: Action::NextRound,
            hint: "Starts the next round with a new draft.".into(),
            icon: None,
        },
    );
}

fn over_controls(p: &mut Painter, buttons: &mut Vec<Button>, s: &Session, mut y: f32) {
    let won = s.g.winner() == HUMAN;
    p.glyphs(
        v(PANEL_X, y),
        if won { "YOU WIN!" } else { "YOU LOSE" },
        2.0,
        SIDE[if won { HUMAN } else { BOT }],
        Anchor::TopLeft,
        5.0,
    );
    y += 40.0;
    y = p.lines(v(PANEL_X, y), &wrap(&decision_text(s), cols(14.0, PANEL_W)), 14.0, 18.0, INK) + 8.0;
    let row = |a: &str, b: &str, c: &str, d: &str, e: &str| format!("{a:<3}{b:>8}{c:>8}{d:>14}{e:>12}");
    p.text(v(PANEL_X, y), &row("Rd", "you dealt", "you took", "units lost y/e", "cores y-e"), 13.0, MUTED);
    y += 18.0;
    for r in &s.recaps {
        let row = row(
            &r.round.to_string(),
            &r.dealt.to_string(),
            &r.taken.to_string(),
            &format!("{}/{}", r.lost[0], r.lost[1]),
            &format!("{}-{}", r.core_after[0], r.core_after[1]),
        );
        p.text(v(PANEL_X, y), &row, 13.0, INK);
        y += 17.0;
    }
    button(
        p,
        buttons,
        Btn {
            pos: v(PANEL_X, y + 10.0),
            size: v(220.0, 32.0),
            label: "New game",
            on: true,
            enabled: true,
            action: Action::NewGame,
            hint: "Starts a new match on a new map.".into(),
            icon: None,
        },
    );
}

// ---- hover ----

/// The text of the description box and the colour of its first line, for what the mouse is over.
pub fn info_text(ui: &Ui) -> (String, Color) {
    let s = &ui.s;
    if let Some(b) = ui.hover.button.and_then(|i| ui.buttons.get(i)) {
        return (b.hint.clone(), if b.enabled { INK } else { BAD });
    }
    if let Some(tile) = ui.hover.tile {
        let col = xy(tile).0 + 1;
        match (s.phase, ui.tool) {
            (Phase::Plan, Tool::Op(op)) => {
                return match s.check_op(tile, op) {
                    Ok(()) => {
                        let (from, to) = s.op_preview(tile, op).unwrap_or((Terrain::Plain, Terrain::Plain));
                        let mut t = format!(
                            "{}: {} -> {}. Costs {} TP ({} left after).",
                            op.name(),
                            terrain_name(from),
                            terrain_name(to),
                            op.cost(),
                            s.tp_left() - op.cost()
                        );
                        if s.would_revert(tile, op) {
                            t += " Careful: this would cut the last path between the cores, so your blocking ops will be reverted.";
                            return (t, BAD);
                        }
                        (t, GOOD)
                    }
                    Err(why) => (why, BAD),
                };
            }
            (Phase::Plan, Tool::Building(kind)) => {
                return match s.check_building(kind, tile) {
                    Ok(()) => (format!("Place the {} here. {}", kind.name(), building_text(kind)), GOOD),
                    Err(why) => (why, BAD),
                };
            }
            (Phase::Attack, _) => {
                return (
                    format!("Cross on column {col}: your units spawn here and on its neighbours, march to the midline on this column, then go for the enemy core. Click to commit."),
                    INK,
                );
            }
            _ => return (tile_text(s, tile), INK),
        }
    }
    match (s.phase, ui.tool) {
        (Phase::Plan, Tool::Op(op)) => (op_text(op), INK),
        (Phase::Plan, Tool::Building(k)) => (building_text(k), INK),
        _ => ("Hover a button or a tile to see what it does.".into(), FAINT),
    }
}

pub fn hover_layer(p: &mut Painter, ui: &Ui) {
    let s = &ui.s;
    let tile_size = Vec2::splat(TILE - 2.0);
    // the tiles the selected tool may be used on right now
    let legal: Vec<usize> = match (s.phase, ui.tool) {
        (Phase::Plan, Tool::Op(op)) => s.legal_op_tiles(op),
        (Phase::Plan, Tool::Building(k)) => s.legal_building_tiles(k),
        _ => Vec::new(),
    };
    let gold = Color::srgb(0.99, 0.80, 0.20);
    for &t in &legal {
        p.rect(tile_center(t), tile_size, gold.with_alpha(0.55), 0.5);
        p.outline(tile_center(t), tile_size, 2.0, Color::srgb(0.80, 0.52, 0.05), 0.6);
    }
    if let Some(tile) = ui.hover.tile {
        let c = tile_center(tile);
        let col = xy(tile).0 as usize;
        match s.phase {
            Phase::Attack => {
                // the whole column, and where the army would cross the midline
                for (dc, alpha) in [(-1i32, 0.14), (0, 0.38), (1, 0.14)] {
                    let x = col as i32 + dc;
                    if (0..W as i32).contains(&x) {
                        p.rect_tl(v(BOARD.x + x as f32 * TILE, BOARD.y), v(TILE, 16.0 * TILE), SIDE[HUMAN].with_alpha(alpha), 0.5);
                    }
                }
                let m = midline_point(col);
                p.diamond(m, 20.0, Color::WHITE, 6.0);
                p.diamond(m, 14.0, SIDE[HUMAN], 6.1);
                tooltip(p, m + v(0.0, -TILE / 2.0 - 2.0), "cross here", SIDE[HUMAN]);
            }
            _ => {
                let ok = legal.contains(&tile);
                let tool_on = matches!(ui.tool, Tool::Op(_) | Tool::Building(_)) && s.phase == Phase::Plan;
                p.outline(c, Vec2::splat(TILE), 3.0, if tool_on && !ok { BAD } else { INK }, 6.0);
                if tool_on && ok {
                    let tip = match ui.tool {
                        Tool::Op(op) => s.op_preview(tile, op).map_or(String::new(), |(f, t)| {
                            format!(
                                "{} -> {}{}",
                                terrain_name(f),
                                terrain_name(t),
                                if s.would_revert(tile, op) { " (reverted!)" } else { "" }
                            )
                        }),
                        Tool::Building(k) => format!("place {}", k.name()),
                        Tool::None => String::new(),
                    };
                    tooltip(p, c + v(0.0, -TILE / 2.0), &tip, INK);
                } else if tool_on {
                    p.glyph(c, "x", 22.0, BAD, 6.0);
                }
            }
        }
    }
    // the description box
    p.rect_tl(v(PANEL_X, INFO_Y), v(PANEL_W, INFO_H), BOX, 3.0);
    let (text, color) = info_text(ui);
    let lines = wrap(&text, cols(13.0, PANEL_W - 16.0));
    let mut shown: Vec<String> = lines.iter().take(INFO_LINES).cloned().collect();
    if lines.len() > INFO_LINES {
        if let Some(l) = shown.last_mut() {
            l.truncate(l.len().saturating_sub(3));
            *l += "...";
        }
    }
    p.lines(v(PANEL_X + 8.0, INFO_Y + 6.0), &shown, 13.0, 16.0, color);
}

/// A small label on the board, centred above `at` (or below if it would leave the board).
fn tooltip(p: &mut Painter, at: Vec2, text: &str, fill: Color) {
    if text.is_empty() {
        return;
    }
    let w = text.len() as f32 * 12.0 * CHAR_W + 10.0;
    let x = (at.x - w / 2.0).clamp(BOARD.x, BOARD.x + 9.0 * TILE - w);
    let y = if at.y - 20.0 < BOARD.y { at.y + TILE } else { at.y - 20.0 };
    p.rect_tl(v(x, y), v(w, 18.0), fill, 7.0);
    p.text_at(v(x + 5.0, y + 2.0), text, 12.0, Color::WHITE, Anchor::TopLeft, 7.1);
}

// ---- the clash ----

/// A small fixed offset per unit, so units sharing a tile do not hide each other.
fn jitter(id: u32) -> Vec2 {
    let a = (id.wrapping_mul(2_654_435_761) % 360) as f32 * std::f32::consts::PI / 180.0;
    Vec2::new(a.cos(), a.sin()) * 9.0
}

/// How long (in frames) a hit flash, a damage number and the ghost of a unit that left the field stay on screen.
const FLASH_FRAMES: f32 = 6.0;

pub fn clash_layer(p: &mut Painter, ui: &Ui) {
    let s = &ui.s;
    if s.phase != Phase::Clash || s.frames.is_empty() {
        return;
    }
    let i = ui.clash_frame();
    let (frame, next) = (&s.frames[i], s.frames.get(i + 1));
    let f = ui.clash_t.fract();
    let t = ui.clash_t.min(s.frames.len() as f32);

    // live numbers: core HP and who is still alive
    let st = Status {
        core: s.core_hp_at(i),
        counts: s.alive_at(i),
        ordered: [0; 6],
    };
    status_block(p, s, &st, 110.0);
    // progress
    let y = CONTROLS_Y + 42.0;
    let share = t / s.frames.len() as f32;
    bar(p, v(PANEL_X, y + 2.0), v(PANEL_W - 130.0, 10.0), share, SIDE[HUMAN]);
    p.text_right(
        v(PANEL_X + PANEL_W, y),
        &format!("tick {}/{}", frame.tick, mt_core::clash::MAX_TICKS),
        13.0,
        MUTED,
    );
    if ui.paused {
        p.text(v(PANEL_X, y + 18.0), "paused", 13.0, BAD);
    }

    // the board: buildings, shots, units
    for b in &s.clash_buildings {
        let hp = frame.building_hp.iter().find(|x| x.0 == b.id).map_or(b.hp, |x| x.1);
        if hp > 0 {
            building(p, b.tile, b.side, b.kind as usize, hp as f32 / b.kind.stats().hp as f32, false);
        }
    }
    for &(from, to) in &frame.attacks {
        p.line(tile_center(from), tile_center(to), BAD.with_alpha(0.6), 3.5);
        // a shot landing: a small spark where the line ends, over the target
        p.sprite(tile_center(to), 16.0, art::SPARK, false, 0.9, 4.6);
    }
    for u in &frame.units {
        // glide towards where the unit stands in the next frame
        let to = next.and_then(|n| n.units.iter().find(|v| v.id == u.id)).map_or(u.tile, |v| v.tile);
        // whole pixels, so the sprite is not resampled between texels as it glides
        let c = (tile_center(u.tile).lerp(tile_center(to), f) + jitter(u.id)).round();
        unit_icon(p, c, (u.kind, u.side), SPRITE, SIDE[u.side], 1.0, 4.0);
        let kind = ALL_UNITS[u.kind.min(5)];
        let share = u.hp as f32 / kind.stats().hp as f32;
        p.rect(c + v(0.0, 20.0), v(30.0, 4.0), Color::WHITE.with_alpha(0.8), 4.4);
        p.rect(c + v(-14.0 * (1.0 - share), 20.0), v(28.0 * share, 2.0), INK, 4.5);
    }
    // units that just left the field: a fading box, red for a death, white for a breach (it drifts into the core)
    for e in &s.exits {
        let age = t - e.frame as f32;
        if !(0.0..FLASH_FRAMES).contains(&age) || frame.units.iter().any(|u| u.id == e.id) {
            continue;
        }
        let k = age / FLASH_FRAMES;
        let from = tile_center(e.tile);
        if e.breached {
            let c = from.lerp(tile_center(CORE[1 - e.side]), k);
            unit_icon(p, c, (e.kind, e.side), SPRITE, Color::WHITE.with_alpha(0.9 * (1.0 - k)), 1.0 - k, 3.5);
        } else {
            let c = from + jitter(e.kind as u32 * 31 + e.tile as u32);
            unit_icon(p, c, (e.kind, e.side), SPRITE, BAD.with_alpha(0.85 * (1.0 - k)), 1.0 - k, 3.5);
            // the unit goes up in smoke: a puff that grows a size half way
            p.sprite(c, if k < 0.5 { 2.0 * CELL } else { 3.0 * CELL }, art::SMOKE, false, 1.0 - k, 3.8);
        }
    }
    // cores: HP as of now, a flash and the damage number for each hit
    let hp = s.core_hp_at(i);
    for side in 0..2 {
        core_box(p, side, hp[side]);
    }
    for h in &s.hits {
        let age = t - h.frame as f32;
        if !s.hits_exact || !(0.0..FLASH_FRAMES * 1.5).contains(&age) {
            continue;
        }
        let k = age / (FLASH_FRAMES * 1.5);
        let c = tile_center(CORE[h.side]);
        p.outline(c, Vec2::splat(38.0 + 24.0 * k), 4.0, BAD.with_alpha(1.0 - k), 5.0);
        // the burst over the core: a cross-shaped flash, then a star, then it flies apart into sparks (3x: 48 px)
        let burst = if k < 0.25 {
            art::BURST_X
        } else if k < 0.6 {
            art::BURST_STAR
        } else {
            art::BURST_SPIN
        };
        p.sprite(c, 3.0 * CELL, burst, false, 1.0 - k * 0.6, 4.5);
        p.glyphs(
            c + v(0.0, if h.side == HUMAN { 34.0 } else { -34.0 } - 14.0 * k),
            &format!("-{}", h.dmg),
            1.0,
            // lighter than `BAD`: the glyph's outline is dark, so the fill has to carry the red against the grass
            Color::srgb(1.0, 0.42, 0.38).with_alpha(1.0 - k * 0.5),
            Anchor::Center,
            8.0,
        );
    }
}
