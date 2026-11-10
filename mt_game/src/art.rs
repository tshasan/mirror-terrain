//! Where each thing lives in `assets/sprites.png`: one sheet of 16x16 cells cut from several Kenney packs (CC0, see
//! `assets/README.md`), 8 cells per row. The teal cells are the pack's blue with its hue turned towards the game's teal.
//! Cells are numbered left to right, top to bottom. Side 0 is the human (teal), side 1 the bot (orange), the same order
//! as `SIDE`, so a side can be added to a base cell index.

use bevy::prelude::*;

/// File name inside the asset folder.
pub const SHEET: &str = "sprites.png";
const COLS: usize = 8;
/// Source pixels per cell.
pub const CELL: f32 = 16.0;

pub const GRASS: usize = 0;
pub const WATER: usize = 1;
pub const HILL: usize = 2;
pub const BARRICADE: usize = 3;
pub const FOREST: usize = 4;
/// Tiny Town's gold nugget, on grass.
pub const ORE: usize = 5;
const CORE: usize = 6;
/// Turret, flak, extractor: two cells each (teal, orange), in `BuildingKind` order.
const BUILDING: usize = 8;
/// Grunt .. gunship for teal, then the same six for orange, in `UnitKind` order.
const UNIT: usize = 14;
/// A dug ditch, drawn by hand in Tiny Farm's furrow colours. It carries its own grass rows, so it replaces the grass under it.
pub const TRENCH: usize = 26;
/// Scars: Tiny Dungeon's pitted dirt, and Tiny Town's rubble with an outline added.
pub const CRATER: usize = 27;
pub const RUIN: usize = 28;
/// A variant of the grass tile with a few sprouts, chosen by tile index (see `draw::grass_cell`).
pub const GRASS_SPROUTS: usize = 29;
/// Pixel Shmup's bursts, spark and smoke, outlined in the pack's plum.
pub const BURST_X: usize = 30;
pub const BURST_STAR: usize = 31;
pub const BURST_SPIN: usize = 32;
pub const SPARK: usize = 33;
pub const SMOKE: usize = 34;
/// Tiny Town decoration for the margins of the window.
pub const TREE_PINE: usize = 35;
pub const BUSH: usize = 36;
pub const MUSHROOMS: usize = 37;
/// The first glyph cell; glyph cells follow `GLYPHS` in order.
const GLYPH0: usize = 38;

/// The chunky capitals and digits of Tiny Ski, in sheet order: `(character, x of its first inked pixel, inked width)`. A
/// glyph is drawn from its inked columns only, so the text is proportional with the outlines touching.
const GLYPHS: [(char, u8, u8); 24] = [
    ('0', 2, 12),
    ('1', 3, 10),
    ('2', 2, 12),
    ('3', 2, 12),
    ('4', 2, 12),
    ('5', 2, 12),
    ('6', 2, 12),
    ('7', 2, 12),
    ('8', 2, 12),
    ('9', 2, 12),
    ('-', 1, 14),
    ('/', 3, 10),
    ('!', 5, 6),
    ('Y', 2, 12),
    ('O', 2, 12),
    ('U', 2, 12),
    ('W', 1, 14),
    ('I', 4, 8),
    ('N', 2, 12),
    ('L', 2, 12),
    ('S', 2, 12),
    ('E', 2, 12),
    ('R', 2, 12),
    ('D', 2, 12),
];
/// Source pixels a space takes.
const SPACE: f32 = 8.0;

/// Whether `ch` can be drawn in the glyph font (it has no lowercase). The layout tests use it to catch a missing character.
#[cfg(test)]
pub fn has_glyph(ch: char) -> bool {
    ch == ' ' || GLYPHS.iter().any(|g| g.0 == ch)
}

/// Where each character of `s` goes: `(x offset in source pixels, source rectangle)`, and the run's width in source pixels.
/// Characters the font lacks are left blank (a space wide); `has_glyph` lets the tests catch them.
pub fn glyph_run(s: &str) -> (Vec<(f32, Rect)>, f32) {
    let mut x = 0.0;
    let mut out = Vec::new();
    for ch in s.chars() {
        match GLYPHS.iter().position(|g| g.0 == ch) {
            Some(i) => {
                let (_, x0, w) = GLYPHS[i];
                let r = cell_rect(GLYPH0 + i, false);
                out.push((x, Rect::new(r.min.x + x0 as f32, r.min.y, r.min.x + (x0 + w) as f32, r.max.y)));
                x += w as f32;
            }
            None => x += SPACE,
        }
    }
    (out, x)
}

pub fn core(side: usize) -> usize {
    CORE + side
}

pub fn building(kind: usize, side: usize) -> usize {
    BUILDING + kind * 2 + side
}

pub fn unit(kind: usize, side: usize) -> usize {
    UNIT + side * 6 + kind
}

/// The cell's rectangle in the sheet, in source pixels. `inset` trims one pixel off every edge: a full-bleed texture
/// (grass, water) becomes 14x14, which draws at exactly 42 px, a whole number of source pixels per tile.
pub fn cell_rect(cell: usize, inset: bool) -> Rect {
    let (x, y) = ((cell % COLS) as f32 * CELL, (cell / COLS) as f32 * CELL);
    let i = if inset { 1.0 } else { 0.0 };
    Rect::new(x + i, y + i, x + CELL - i, y + CELL - i)
}

#[derive(Resource)]
pub struct Art(pub Handle<Image>);

/// Where the sheet is looked for. `cargo run` already points Bevy at `mt_game/assets`; this also makes a binary started
/// by hand from anywhere find them, as long as the source tree is still where it was built.
pub fn asset_dir() -> String {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/assets");
    if std::path::Path::new(dir).is_dir() {
        dir.to_string()
    } else {
        "assets".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet_size() -> (u32, u32) {
        let png = include_bytes!("../assets/sprites.png");
        (
            u32::from_be_bytes(png[16..20].try_into().unwrap()),
            u32::from_be_bytes(png[20..24].try_into().unwrap()),
        )
    }

    #[test]
    fn every_cell_is_inside_the_sheet() {
        let (w, h) = sheet_size();
        let mut cells = vec![
            GRASS,
            WATER,
            HILL,
            BARRICADE,
            FOREST,
            ORE,
            TRENCH,
            CRATER,
            RUIN,
            GRASS_SPROUTS,
            BURST_X,
            BURST_STAR,
            BURST_SPIN,
            SPARK,
            SMOKE,
            TREE_PINE,
            BUSH,
            MUSHROOMS,
        ];
        cells.extend((0..GLYPHS.len()).map(|i| GLYPH0 + i));
        for side in 0..2 {
            cells.push(core(side));
            cells.extend((0..3).map(|k| building(k, side)));
            cells.extend((0..6).map(|k| unit(k, side)));
        }
        let mut seen = std::collections::BTreeSet::new();
        for c in cells {
            assert!(seen.insert(c), "cell {c} is used twice");
            assert!(
                cell_rect(c, false).max.x <= w as f32 && cell_rect(c, false).max.y <= h as f32,
                "cell {c} is outside {w}x{h}"
            );
        }
    }

    #[test]
    fn glyph_runs_are_proportional_and_inside_their_cells() {
        let (runs, w) = glyph_run("1/5");
        assert_eq!(runs.len(), 3);
        assert_eq!(w, 10.0 + 10.0 + 12.0);
        assert!(runs.iter().all(|(_, r)| r.height() == CELL && r.width() <= CELL));
        assert!(has_glyph('7') && has_glyph(' ') && !has_glyph('a'));
        assert_eq!(glyph_run("a").1, SPACE, "a missing character is a blank, not a panic");
    }
}
