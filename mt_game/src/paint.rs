//! Drawing primitives: coloured rectangles, sprites from the sheet in `art.rs`, and text in screen pixels (origin top-left,
//! y down), on one of three layers. A
//! `Painter` either spawns Bevy entities or records plain `Prim`s, so the same drawing code runs in the tests, which check
//! the layout (no text overlapping text) and can dump a screen as SVG to look at.

use crate::art;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use mt_core::*;

pub const WIN: Vec2 = Vec2::new(940.0, 760.0);
pub const TILE: f32 = 44.0;
/// Top-left corner of the board, in screen pixels.
pub const BOARD: Vec2 = Vec2::new(24.0, 28.0);
pub const PANEL_X: f32 = 452.0;
pub const PANEL_W: f32 = 468.0;

pub const SIDE: [Color; 2] = [Color::srgb(0.10, 0.62, 0.60), Color::srgb(0.93, 0.50, 0.16)];
pub const INK: Color = Color::srgb(0.10, 0.11, 0.13);
pub const MUTED: Color = Color::srgb(0.40, 0.42, 0.46);
pub const FAINT: Color = Color::srgb(0.58, 0.59, 0.62);
pub const PAPER: Color = Color::srgb(0.96, 0.95, 0.92);
pub const BOX: Color = Color::srgb(0.91, 0.90, 0.86);
pub const BUTTON: Color = Color::srgb(0.86, 0.85, 0.80);
pub const BUTTON_ON: Color = Color::srgb(0.99, 0.86, 0.42);
pub const BUTTON_OFF: Color = Color::srgb(0.92, 0.91, 0.88);
pub const BAD: Color = Color::srgb(0.80, 0.18, 0.16);
pub const GOOD: Color = Color::srgb(0.12, 0.50, 0.28);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    /// Redrawn when the game state or a click changes something.
    Static,
    /// Redrawn when the mouse moves to another tile or button, or the tool changes.
    Hover,
    /// Redrawn every frame of the clash.
    Clash,
}

#[derive(Component)]
pub struct StaticLayer;
#[derive(Component)]
pub struct HoverLayer;
#[derive(Component)]
pub struct ClashLayer;

#[derive(Clone, Debug)]
pub enum Prim {
    Rect {
        c: Vec2,
        size: Vec2,
        color: Color,
        rot: f32,
        z: f32,
    },
    Text {
        p: Vec2,
        s: String,
        size: f32,
        color: Color,
        anchor: Anchor,
        z: f32,
    },
    /// A rectangle of the sprite sheet (`src`, in source pixels), drawn `size` pixels big. A whole cell is drawn at a whole
    /// multiple of its size so pixels stay square; the middle of a 9-slice frame is the one thing that is stretched.
    Sprite {
        c: Vec2,
        size: Vec2,
        src: Rect,
        /// Multiplies the sprite's colours.
        color: Color,
        z: f32,
    },
    /// A short run of the chunky glyph font, `scale` screen pixels per source pixel (a whole number). `p` is the anchor point.
    Glyphs {
        p: Vec2,
        s: String,
        scale: f32,
        color: Color,
        anchor: Anchor,
        z: f32,
    },
}

pub enum Sink<'a, 'w, 's> {
    /// The commands, and the sprite sheet to draw `Prim::Sprite` from.
    Cmd(&'a mut Commands<'w, 's>, Handle<Image>),
    /// Only the tests record.
    #[cfg_attr(not(test), allow(dead_code))]
    Rec(&'a mut Vec<Prim>),
}

pub struct Painter<'a, 'w, 's> {
    pub sink: Sink<'a, 'w, 's>,
    pub layer: Layer,
}

fn world(p: Vec2, z: f32) -> Transform {
    Transform::from_xyz(p.x - WIN.x / 2.0, WIN.y / 2.0 - p.y, z)
}

impl<'a, 'w, 's> Painter<'a, 'w, 's> {
    pub fn new(cmd: &'a mut Commands<'w, 's>, layer: Layer, sheet: &art::Art) -> Self {
        Painter {
            sink: Sink::Cmd(cmd, sheet.0.clone()),
            layer,
        }
    }
    #[cfg(test)]
    pub fn recording(out: &'a mut Vec<Prim>, layer: Layer) -> Self {
        Painter { sink: Sink::Rec(out), layer }
    }

    fn emit(&mut self, prim: Prim) {
        let layer = self.layer;
        match &mut self.sink {
            Sink::Rec(v) => v.push(prim),
            Sink::Cmd(cmd, sheet) => {
                let e = match prim {
                    Prim::Rect { c, size, color, rot, z } => cmd
                        .spawn((Sprite::from_color(color, size), world(c, z).with_rotation(Quat::from_rotation_z(rot))))
                        .id(),
                    Prim::Text { p, s, size, color, anchor, z } => cmd
                        .spawn((Text2d::new(s), TextFont { font_size: size, ..default() }, TextColor(color), anchor, world(p, z)))
                        .id(),
                    Prim::Sprite { c, size, src, color, z } => cmd
                        .spawn((
                            Sprite {
                                image: sheet.clone(),
                                rect: Some(src),
                                custom_size: Some(size),
                                color,
                                ..default()
                            },
                            world(c, z),
                        ))
                        .id(),
                    // A glyph run is one sprite per character, and each one carries the layer tag.
                    Prim::Glyphs { p, s, scale, color, anchor, z } => {
                        for (c, size, src) in glyph_sprites(p, &s, scale, &anchor) {
                            let e = cmd
                                .spawn((
                                    Sprite {
                                        image: sheet.clone(),
                                        rect: Some(src),
                                        custom_size: Some(size),
                                        color,
                                        ..default()
                                    },
                                    world(c, z),
                                ))
                                .id();
                            tag(cmd, e, layer);
                        }
                        return;
                    }
                };
                tag(cmd, e, layer);
            }
        }
    }

    /// A filled rectangle centred on `c`.
    pub fn rect(&mut self, c: Vec2, size: Vec2, color: Color, z: f32) {
        self.emit(Prim::Rect { c, size, color, rot: 0.0, z });
    }
    /// A filled rectangle with its top-left corner at `tl`.
    pub fn rect_tl(&mut self, tl: Vec2, size: Vec2, color: Color, z: f32) {
        self.rect(tl + size / 2.0, size, color, z);
    }
    pub fn diamond(&mut self, c: Vec2, size: f32, color: Color, z: f32) {
        self.emit(Prim::Rect {
            c,
            size: Vec2::splat(size),
            color,
            rot: std::f32::consts::FRAC_PI_4,
            z,
        });
    }
    pub fn line(&mut self, a: Vec2, b: Vec2, color: Color, z: f32) {
        let d = b - a;
        // screen y points down, world y up: flip the angle
        self.emit(Prim::Rect {
            c: (a + b) / 2.0,
            size: Vec2::new(d.length(), 2.0),
            color,
            rot: -d.y.atan2(d.x),
            z,
        });
    }
    /// A hollow rectangle centred on `c`, drawn inside `size`.
    pub fn outline(&mut self, c: Vec2, size: Vec2, thick: f32, color: Color, z: f32) {
        self.rect(c + Vec2::new(0.0, (thick - size.y) / 2.0), Vec2::new(size.x, thick), color, z);
        self.rect(c + Vec2::new(0.0, (size.y - thick) / 2.0), Vec2::new(size.x, thick), color, z);
        self.rect(c + Vec2::new((thick - size.x) / 2.0, 0.0), Vec2::new(thick, size.y - 2.0 * thick), color, z);
        self.rect(c + Vec2::new((size.x - thick) / 2.0, 0.0), Vec2::new(thick, size.y - 2.0 * thick), color, z);
    }
    /// A sprite-sheet cell centred on `c`, `size` pixels square. `inset` trims a pixel off each edge (see `art::cell_rect`).
    /// Callers pass whole multiples of 16 (14 for an inset cell) so the nearest-neighbour pixels stay square.
    pub fn sprite(&mut self, c: Vec2, size: f32, cell: usize, inset: bool, alpha: f32, z: f32) {
        self.sprite_src(c, Vec2::splat(size), art::cell_rect(cell, inset), Color::WHITE.with_alpha(alpha), z);
    }
    /// Any rectangle of the sheet, tinted by `color`.
    pub fn sprite_src(&mut self, c: Vec2, size: Vec2, src: Rect, color: Color, z: f32) {
        self.emit(Prim::Sprite { c, size, src, color, z });
    }
    /// A run of the glyph font (capitals, digits, `-`, `/`, `!`), `scale` screen pixels per source pixel, white or tinted.
    pub fn glyphs(&mut self, p: Vec2, s: &str, scale: f32, color: Color, anchor: Anchor, z: f32) {
        self.emit(Prim::Glyphs {
            p,
            s: s.to_string(),
            scale,
            color,
            anchor,
            z,
        });
    }
    pub fn text_at(&mut self, p: Vec2, s: &str, size: f32, color: Color, anchor: Anchor, z: f32) {
        self.emit(Prim::Text {
            p,
            s: s.to_string(),
            size,
            color,
            anchor,
            z,
        });
    }
    /// Left-aligned text with its top-left corner at `p`.
    pub fn text(&mut self, p: Vec2, s: &str, size: f32, color: Color) {
        self.text_at(p, s, size, color, Anchor::TopLeft, 5.0);
    }
    /// Right-aligned text with its top-right corner at `p`.
    pub fn text_right(&mut self, p: Vec2, s: &str, size: f32, color: Color) {
        self.text_at(p, s, size, color, Anchor::TopRight, 5.0);
    }
    pub fn glyph(&mut self, c: Vec2, s: &str, size: f32, color: Color, z: f32) {
        self.text_at(c, s, size, color, Anchor::Center, z);
    }
    /// Lines of text, `step` pixels apart; returns the y below the last one.
    pub fn lines(&mut self, p: Vec2, lines: &[String], size: f32, step: f32, color: Color) -> f32 {
        for (i, l) in lines.iter().enumerate() {
            self.text(p + Vec2::new(0.0, i as f32 * step), l, size, color);
        }
        p.y + lines.len() as f32 * step
    }
}

/// Marks an entity as belonging to a layer, so the layer can be despawned and redrawn.
fn tag(cmd: &mut Commands, e: Entity, layer: Layer) {
    match layer {
        Layer::Static => cmd.entity(e).insert(StaticLayer),
        Layer::Hover => cmd.entity(e).insert(HoverLayer),
        Layer::Clash => cmd.entity(e).insert(ClashLayer),
    };
}

/// The box a glyph run covers on screen, `(top-left, size)`: proportional width, 16 source pixels tall.
pub fn glyph_box(p: Vec2, s: &str, scale: f32, anchor: &Anchor) -> (Vec2, Vec2) {
    let (_, w) = art::glyph_run(s);
    let size = Vec2::new(w * scale, art::CELL * scale);
    let a = anchor.as_vec();
    (Vec2::new(p.x - (a.x + 0.5) * size.x, p.y - (0.5 - a.y) * size.y), size)
}

/// One screen sprite per character of a glyph run: `(centre, size, source rectangle)`.
pub fn glyph_sprites(p: Vec2, s: &str, scale: f32, anchor: &Anchor) -> Vec<(Vec2, Vec2, Rect)> {
    let (tl, size) = glyph_box(p, s, scale, anchor);
    art::glyph_run(s)
        .0
        .into_iter()
        .map(|(x, src)| {
            let w = src.width() * scale;
            (Vec2::new(tl.x + x * scale + w / 2.0, tl.y + size.y / 2.0), Vec2::new(w, size.y), src)
        })
        .collect()
}

/// Centre of a tile on screen. Side A's rows are at the bottom, as on a phone held by player A.
pub fn tile_center(tile: usize) -> Vec2 {
    let (x, y) = xy(tile);
    BOARD + Vec2::new((x as f32 + 0.5) * TILE, ((H as i32 - 1 - y) as f32 + 0.5) * TILE)
}

pub fn tile_at(p: Vec2) -> Option<usize> {
    let q = (p - BOARD) / TILE;
    if q.x < 0.0 || q.y < 0.0 {
        return None;
    }
    idx(q.x as i32, H as i32 - 1 - q.y as i32)
}

/// Where a column meets the midline, on screen.
pub fn midline_point(column: usize) -> Vec2 {
    Vec2::new(BOARD.x + (column as f32 + 0.5) * TILE, BOARD.y + 8.0 * TILE)
}

/// Width of one character of the default font (FiraMono, advance 0.6 em).
pub const CHAR_W: f32 = 0.6;

#[cfg(test)]
/// The screen box a text prim covers, from the monospace metrics: `(top-left, size)`.
pub fn text_box(p: Vec2, s: &str, size: f32, anchor: &Anchor) -> (Vec2, Vec2) {
    let lines: Vec<&str> = s.split('\n').collect();
    let w = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32 * size * CHAR_W;
    let h = lines.len() as f32 * size * 1.2;
    let a = anchor.as_vec();
    (Vec2::new(p.x - (a.x + 0.5) * w, p.y - (0.5 - a.y) * h), Vec2::new(w, h))
}

#[cfg(test)]
/// A screen as an SVG, to look at (`qlmanage -t` renders it). Layers are painted in z order.
pub fn to_svg(prims: &[Prim]) -> String {
    fn col(c: Color) -> String {
        let s = c.to_srgba();
        format!(
            "fill=\"rgb({},{},{})\" fill-opacity=\"{:.2}\"",
            (s.red * 255.0) as u8,
            (s.green * 255.0) as u8,
            (s.blue * 255.0) as u8,
            s.alpha
        )
    }
    let mut order: Vec<usize> = (0..prims.len()).collect();
    order.sort_by(|&a, &b| {
        let z = |p: &Prim| match p {
            Prim::Rect { z, .. } | Prim::Text { z, .. } | Prim::Sprite { z, .. } | Prim::Glyphs { z, .. } => *z,
        };
        z(&prims[a]).total_cmp(&z(&prims[b]))
    });
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{0}\" height=\"{0}\" viewBox=\"0 0 {0} {0}\"><rect width=\"{0}\" height=\"{1}\" fill=\"rgb(245,242,235)\"/>",
        WIN.x, WIN.y
    );
    // The sheet is embedded once and each sprite is a clipped, nearest-neighbour view of it.
    let png = include_bytes!("../assets/sprites.png");
    let (sheet_w, sheet_h) = (
        u32::from_be_bytes(png[16..20].try_into().unwrap()),
        u32::from_be_bytes(png[20..24].try_into().unwrap()),
    );
    out += &format!(
        "<defs><image id=\"sheet\" width=\"{sheet_w}\" height=\"{sheet_h}\" style=\"image-rendering:pixelated\" href=\"data:image/png;base64,{}\"/></defs>",
        base64(png)
    );
    // A sprite is a clipped, nearest-neighbour view of the sheet; a tint is a multiply, done by one colour-matrix filter per colour.
    let sprite_svg = |c: Vec2, size: Vec2, src: Rect, color: Color| {
        let s = color.to_srgba();
        let filter = if (s.red, s.green, s.blue) == (1.0, 1.0, 1.0) {
            String::new()
        } else {
            format!(
                " filter=\"url(#tint{:02x}{:02x}{:02x})\"",
                (s.red * 255.0) as u8,
                (s.green * 255.0) as u8,
                (s.blue * 255.0) as u8
            )
        };
        format!(
            "<svg x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" preserveAspectRatio=\"none\" opacity=\"{:.2}\" shape-rendering=\"crispEdges\"{filter}><use href=\"#sheet\"/></svg>",
            c.x - size.x / 2.0,
            c.y - size.y / 2.0,
            size.x,
            size.y,
            src.min.x,
            src.min.y,
            src.width(),
            src.height(),
            s.alpha
        )
    };
    // one multiply filter per tint colour used on the screen
    let mut tints = std::collections::BTreeSet::new();
    for p in prims {
        let color = match p {
            Prim::Sprite { color, .. } | Prim::Glyphs { color, .. } => *color,
            _ => continue,
        };
        let s = color.to_srgba();
        if (s.red, s.green, s.blue) != (1.0, 1.0, 1.0) {
            tints.insert(((s.red * 255.0) as u8, (s.green * 255.0) as u8, (s.blue * 255.0) as u8));
        }
    }
    if !tints.is_empty() {
        out += "<defs>";
        for (r, g, b) in tints {
            out += &format!(
                "<filter id=\"tint{r:02x}{g:02x}{b:02x}\" color-interpolation-filters=\"sRGB\"><feColorMatrix type=\"matrix\" values=\"{:.3} 0 0 0 0  0 {:.3} 0 0 0  0 0 {:.3} 0 0  0 0 0 1 0\"/></filter>",
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0
            );
        }
        out += "</defs>";
    }
    for i in order {
        match &prims[i] {
            Prim::Sprite { c, size, src, color, .. } => out += &sprite_svg(*c, *size, *src, *color),
            Prim::Glyphs {
                p, s, scale, color, anchor, ..
            } => {
                for (c, size, src) in glyph_sprites(*p, s, *scale, anchor) {
                    out += &sprite_svg(c, size, src, *color);
                }
            }
            Prim::Rect { c, size, color, rot, .. } => {
                out += &format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" {} transform=\"rotate({} {} {})\"/>",
                    c.x - size.x / 2.0,
                    c.y - size.y / 2.0,
                    size.x,
                    size.y,
                    col(*color),
                    -rot.to_degrees(),
                    c.x,
                    c.y
                );
            }
            Prim::Text { p, s, size, color, anchor, .. } => {
                for (n, line) in s.split('\n').enumerate() {
                    let (tl, _) = text_box(*p, s, *size, anchor);
                    let a = anchor.as_vec();
                    let w = line.chars().count() as f32 * size * CHAR_W;
                    let full = s.split('\n').map(|l| l.chars().count()).max().unwrap_or(0) as f32 * size * CHAR_W;
                    let x = tl.x + (a.x + 0.5) * (full - w);
                    let line = line.replace('&', "&amp;").replace('<', "&lt;");
                    out += &format!(
                        "<text x=\"{}\" y=\"{}\" xml:space=\"preserve\" font-family=\"Menlo, monospace\" font-size=\"{}\" textLength=\"{}\" {}>{}</text>",
                        x,
                        tl.y + n as f32 * size * 1.2 + size * 0.95,
                        size,
                        w,
                        col(*color),
                        line
                    );
                }
            }
        }
    }
    out + "</svg>"
}

#[cfg(test)]
fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for ch in data.chunks(3) {
        let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= ch.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
