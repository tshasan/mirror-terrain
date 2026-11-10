//! Layout checks that stand in for looking at the window: every phase is drawn into recorded primitives (round 5 with a full
//! army and every building unlocked included), and no text may overlap other text, leave the window or run out of its box.
//! Set `MT_SVG_DIR=<dir>` to also write each screen as an SVG (`qlmanage -t -s 940 -o <dir> <file>.svg` renders a PNG).

use crate::draw::*;
use crate::info::*;
use crate::paint::*;
use crate::session::*;
use crate::ui::*;
use bevy::prelude::*;
use mt_core::*;

fn render(ui: &mut Ui, hover: Hover) -> Vec<Prim> {
    let mut prims = Vec::new();
    static_layer(&mut Painter::recording(&mut prims, Layer::Static), ui);
    ui.hover = hover;
    hover_layer(&mut Painter::recording(&mut prims, Layer::Hover), ui);
    clash_layer(&mut Painter::recording(&mut prims, Layer::Clash), ui);
    prims
}

fn texts(prims: &[Prim]) -> Vec<(Vec2, Vec2, String, f32)> {
    prims
        .iter()
        .filter_map(|p| match p {
            Prim::Text { p, s, size, anchor, .. } => {
                let (tl, sz) = text_box(*p, s, *size, anchor);
                Some((tl, sz, s.clone(), *size))
            }
            // The glyph font is proportional, so it is measured with its own widths; and it must have every character asked of it.
            Prim::Glyphs { p, s, scale, anchor, .. } => {
                assert!(s.chars().all(crate::art::has_glyph), "the glyph font has no `{s}`");
                let (tl, sz) = glyph_box(*p, s, *scale, anchor);
                Some((tl, sz, s.clone(), 16.0 * scale))
            }
            _ => None,
        })
        .collect()
}

fn overlap(a: (Vec2, Vec2), b: (Vec2, Vec2)) -> bool {
    let (ax, ay, bx, by) = (a.0.x, a.0.y, b.0.x, b.0.y);
    ax + 1.0 < bx + b.1.x && bx + 1.0 < ax + a.1.x && ay + 2.0 < by + b.1.y && by + 2.0 < ay + a.1.y
}

fn check(name: &str, ui: &mut Ui, hover: Hover) {
    let prims = render(ui, hover);
    if let Ok(dir) = std::env::var("MT_SVG_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(format!("{dir}/{name}.svg"), to_svg(&prims)).unwrap();
    }
    let all = texts(&prims);
    // the panel: everything right of the board
    let panel: Vec<_> = all.iter().filter(|t| t.0.x >= PANEL_X - 1.0).collect();
    for t in &all {
        assert!(
            t.0.x >= -1.0 && t.0.y >= -1.0 && t.0.x + t.1.x <= WIN.x + 1.0 && t.0.y + t.1.y <= WIN.y + 1.0,
            "{name}: `{}` leaves the window",
            t.2
        );
    }
    for t in &panel {
        assert!(
            t.0.x + t.1.x <= PANEL_X + PANEL_W + 2.0,
            "{name}: `{}` runs past the panel edge ({:.0})",
            t.2,
            t.0.x + t.1.x
        );
    }
    for (i, a) in panel.iter().enumerate() {
        for b in &panel[i + 1..] {
            assert!(!overlap((a.0, a.1), (b.0, b.1)), "{name}: `{}` overlaps `{}`", a.2, b.2);
        }
    }
    // text must stay inside the button it is on, and inside the description box
    for b in &ui.buttons {
        for t in panel.iter().filter(|t| b.rect.contains(t.0 + Vec2::new(1.0, 1.0))) {
            assert!(
                t.0.x + t.1.x <= b.rect.max.x + 1.0 && t.0.y + t.1.y <= b.rect.max.y + 1.0,
                "{name}: `{}` does not fit its button",
                t.2
            );
        }
    }
    for t in panel.iter().filter(|t| t.0.y >= 460.0 && t.0.y < 548.0) {
        assert!(t.0.y + t.1.y <= 548.0, "{name}: `{}` runs out of the description box", t.2);
    }
    for t in panel.iter().filter(|t| t.0.y >= 44.0 && t.0.y < 102.0) {
        assert!(t.0.y + t.1.y <= 102.0, "{name}: `{}` runs out of the instruction box", t.2);
    }
    // the controls end where the description box starts
    for t in panel.iter().filter(|t| t.0.y >= 206.0 && t.0.y < 460.0) {
        assert!(t.0.y + t.1.y <= 460.0, "{name}: `{}` runs into the description box", t.2);
    }
    // the description for every button must fit the box without being cut
    for (i, b) in ui.buttons.iter().enumerate() {
        ui.hover = Hover { tile: None, button: Some(i) };
        let (text, _) = info_text(ui);
        assert!(
            wrap(&text, ((PANEL_W - 16.0) / (13.0 * CHAR_W)) as usize).len() <= 5,
            "{name}: the hint of {:?} is too long: {text}",
            b.action
        );
    }
}

/// A session at round 5, past the draft, with every unit and building unlocked and a big army on both sides.
fn late_game(bot: &str) -> Ui {
    let mut s = Session::new(21, bot);
    s.g.round = MAX_ROUNDS;
    s.draft_click(0);
    s.draft_click(1);
    for side in 0..2 {
        let p = &mut s.g.players[side];
        p.units = [true; 6];
        p.buildings = [true; 3];
        p.ore = 80;
        p.roster = ALL_UNITS.iter().cycle().take(15).copied().collect();
    }
    Ui::new(s, 21)
}

fn tiles(s: &Session) -> (usize, usize) {
    let t = s.legal_op_tiles(Op::Raise);
    (t[0], t[t.len() / 2])
}

#[test]
fn the_draft_screen_fits() {
    let mut ui = Ui::new(Session::new(3, "greedy"), 3);
    check("draft", &mut ui, Hover::default());
    ui.act(Action::Draft(1));
    check("draft_hover", &mut ui, Hover { tile: None, button: Some(2) });
    // all three cards of every deal in a few rounds, with long texts and the owned-card note
    for seed in 1..30 {
        let mut ui = Ui::new(Session::new(seed, "greedy"), seed);
        for _ in 0..4 {
            ui.s.draft_click(0);
            ui.s.draft_click(1);
            ui.s.g.start_round();
            ui.s.phase = Phase::Draft;
            ui.touch();
            check("draft_many", &mut ui, Hover::default());
        }
    }
}

#[test]
fn the_shaping_screens_fit_even_in_round_five_with_everything_unlocked() {
    let mut ui = late_game("greedy");
    check("plan_empty", &mut ui, Hover::default());
    ui.act(Action::Pick(Tool::Op(Op::Raise)));
    let (a, b) = tiles(&ui.s);
    ui.touch();
    check("plan_op_hover", &mut ui, Hover { tile: Some(a), button: None });
    ui.click_tile(a);
    ui.act(Action::Pick(Tool::Building(BuildingKind::Turret)));
    let t = ui.s.legal_building_tiles(BuildingKind::Turret)[3];
    ui.click_tile(t);
    ui.act(Action::Pick(Tool::Building(BuildingKind::Flak)));
    let flak = ui.s.legal_building_tiles(BuildingKind::Flak)[5];
    check(
        "plan_building_hover",
        &mut ui,
        Hover {
            tile: Some(flak),
            button: None,
        },
    );
    ui.act(Action::Pick(Tool::Op(Op::Wall)));
    check("plan_refused", &mut ui, Hover { tile: Some(b), button: None });
    check(
        "plan_refused_enemy_half",
        &mut ui,
        Hover {
            tile: Some(N - 5),
            button: None,
        },
    );
    for k in ALL_UNITS {
        let _ = ui.s.try_unit(k);
    }
    ui.act(Action::Unit(UnitKind::Rhino));
    ui.notice = ui.s.try_building(BuildingKind::Extractor, 0).unwrap_err();
    for n in 0..ui.buttons.len() {
        check("plan_button_hover", &mut ui, Hover { tile: None, button: Some(n) });
    }
    ui.tool = Tool::None;
    check("plan_tile_info", &mut ui, Hover { tile: Some(b), button: None });
}

#[test]
fn the_attack_clash_and_result_screens_fit() {
    for bot in ["greedy", "shaper"] {
        let mut ui = late_game(bot);
        ui.act(Action::Pick(Tool::Op(Op::Raise)));
        let (a, _) = tiles(&ui.s);
        ui.click_tile(a);
        ui.act(Action::Pick(Tool::Building(BuildingKind::Turret)));
        let t = ui.s.legal_building_tiles(BuildingKind::Turret)[0];
        ui.click_tile(t);
        for k in ALL_UNITS {
            let _ = ui.s.try_unit(k);
        }
        ui.act(Action::Ready);
        check("attack", &mut ui, Hover::default());
        check(
            "attack_hover",
            &mut ui,
            Hover {
                tile: Some(idx(6, 9).unwrap()),
                button: None,
            },
        );
        check(
            "attack_hover_edge",
            &mut ui,
            Hover {
                tile: Some(idx(0, 3).unwrap()),
                button: None,
            },
        );
        ui.click_tile(idx(4, 4).unwrap());
        assert_eq!(ui.s.phase, Phase::Clash);
        for frac in [0.0, 0.3, 0.6, 0.95] {
            ui.clash_t = ui.s.frames.len() as f32 * frac;
            check("clash", &mut ui, Hover::default());
            ui.paused = frac > 0.5;
            check("clash_hover", &mut ui, Hover { tile: None, button: Some(1) });
        }
        ui.act(Action::SkipClash);
        check(if ui.s.phase == Phase::Over { "over" } else { "resolve" }, &mut ui, Hover::default());
        let scar = ui.s.g.last_scars.first().map(|s| s.0);
        check(
            "result_hover",
            &mut ui,
            Hover {
                tile: scar.or(Some(5)),
                button: None,
            },
        );
    }
}

#[test]
fn a_clash_with_core_hits_shows_the_flash_and_the_whole_game_ends_with_a_recap() {
    // find a clash where a core is hit, and look at the frame it lands on
    let mut found = false;
    for seed in 1..60u64 {
        let mut ui = Ui::new(Session::new(seed, "greedy"), seed);
        ui.act(Action::Draft(0));
        ui.act(Action::Draft(1));
        while ui.s.try_unit(UnitKind::Grunt).is_ok() {}
        ui.act(Action::Ready);
        ui.click_tile(idx(4, 4).unwrap());
        if let Some(h) = ui.s.hits.first().copied().filter(|_| ui.s.hits_exact) {
            ui.clash_t = h.frame as f32 + 1.5;
            let prims = render(&mut ui, Hover::default());
            assert!(texts(&prims).iter().any(|t| t.2 == format!("-{}", h.dmg)), "the damage number shows");
            check("clash_hit", &mut ui, Hover::default());
            found = true;
            break;
        }
    }
    assert!(found, "no clash with a core hit in 60 seeds");
    let mut ui = Ui::new(Session::new(8, "shaper"), 8);
    while ui.s.phase != Phase::Over {
        match ui.s.phase {
            Phase::Draft => {
                ui.act(Action::Draft(0));
                ui.act(Action::Draft(1));
            }
            Phase::Plan => {
                while ui.s.try_unit(UnitKind::Grunt).is_ok() {}
                ui.act(Action::Ready);
            }
            Phase::Attack => ui.click_tile(idx(3, 3).unwrap()),
            Phase::Clash => ui.act(Action::SkipClash),
            Phase::Resolve => {
                check("resolve_round", &mut ui, Hover::default());
                ui.act(Action::NextRound);
            }
            Phase::Over => {}
        }
    }
    check("over_recap", &mut ui, Hover::default());
    assert_eq!(ui.s.recaps.len(), ui.s.g.round as usize);
}

#[test]
fn instructions_and_hints_fit_their_boxes() {
    let mut ui = late_game("greedy");
    for tool in [Tool::None, Tool::Op(Op::Expose), Tool::Building(BuildingKind::Extractor)] {
        let lines = wrap(&instruction(&ui.s, tool), ((PANEL_W - 28.0) / (18.0 * CHAR_W)) as usize);
        assert!(lines.len() <= 2, "{tool:?}: {lines:?}");
    }
    ui.s.ready();
    assert!(wrap(&instruction(&ui.s, Tool::None), 40).len() <= 2);
    for card in CARD_POOL {
        assert!(
            wrap(&card_text(card, true), ((PANEL_W - 16.0) / (12.0 * CHAR_W)) as usize).len() <= 3,
            "{card:?}"
        );
    }
}

#[test]
fn reverted_blocking_ops_are_marked_and_announced() {
    // Maps are generated so that no single tile cuts the path; build one by hand: a wall across row 4 with one gap.
    let mut ui = Ui::new(Session::new(5, "greedy"), 5);
    for x in 1..W as i32 {
        ui.s.g.terrain[idx(x, 4).unwrap()] = Terrain::Wall;
    }
    let gap = idx(0, 4).unwrap();
    ui.s.g.terrain[gap] = Terrain::Plain;
    ui.act(Action::Draft(0));
    ui.act(Action::Draft(1));
    assert!(ui.s.would_revert(gap, Op::Wall), "closing the gap cuts the path");
    ui.act(Action::Pick(Tool::Op(Op::Wall)));
    check("plan_would_revert", &mut ui, Hover { tile: Some(gap), button: None });
    ui.hover = Hover { tile: Some(gap), button: None };
    assert!(info_text(&ui).0.contains("reverted"));
    ui.click_tile(gap);
    ui.act(Action::Ready);
    assert!(ui.s.g.last_ops.iter().any(|r| r.tile == gap && r.outcome == OpOutcome::Reverted));
    assert!(instruction(&ui.s, Tool::None).contains("reverted"));
    assert!(reveal_lines(&ui.s).iter().any(|l| l.contains("reverted")));
    check("attack_reverted", &mut ui, Hover::default());
}

#[test]
fn units_that_fall_fade_out_instead_of_vanishing() {
    for seed in 1..60u64 {
        let mut ui = Ui::new(Session::new(seed, "greedy"), seed);
        ui.act(Action::Draft(0));
        ui.act(Action::Draft(1));
        while ui.s.try_unit(UnitKind::Grunt).is_ok() {}
        ui.act(Action::Ready);
        ui.click_tile(idx(4, 4).unwrap());
        if let Some(e) = ui.s.exits.iter().find(|e| !e.breached && e.frame + 1 < ui.s.frames.len()).cloned() {
            ui.clash_t = e.frame as f32 + 1.2;
            let prims = render(&mut ui, Hover::default());
            let smoke = crate::art::cell_rect(crate::art::SMOKE, false);
            assert!(
                prims.iter().any(|p| matches!(p, Prim::Sprite { src, c, .. } if *src == smoke && c.x < PANEL_X)),
                "a fallen unit leaves a puff of smoke behind"
            );
            check("clash_ghost", &mut ui, Hover::default());
            return;
        }
    }
    panic!("no unit died in 60 clashes");
}

#[test]
fn the_result_screen_shows_trench_crater_and_ruin() {
    // A hand-made result: a crater on a trench, a ruin on a forest, a plain trench and a plain forest next to them, so the
    // scars can be compared with the terrain they left and the tiles that were never scarred.
    let mut ui = Ui::new(Session::new(9, "greedy"), 9);
    ui.act(Action::Draft(0));
    ui.act(Action::Draft(1));
    let at = |x, y| idx(x, y).unwrap();
    let g = &mut ui.s.g;
    for (x, y, t) in [
        (1, 6, Terrain::Trench),
        (2, 6, Terrain::Forest),
        (4, 5, Terrain::Trench),
        (5, 5, Terrain::Trench),
        (6, 9, Terrain::Forest),
    ] {
        g.terrain[at(x, y)] = t;
    }
    g.terrain[at(3, 10)] = Terrain::Trench;
    g.terrain[at(5, 10)] = Terrain::Forest;
    g.last_scars = vec![
        (at(1, 6), ScarKind::Crater),
        (at(2, 6), ScarKind::Ruin),
        (at(3, 10), ScarKind::Crater),
        (at(5, 10), ScarKind::Ruin),
    ];
    ui.s.phase = Phase::Resolve;
    ui.touch();
    check("scars", &mut ui, Hover::default());
    check(
        "scars_hover",
        &mut ui,
        Hover {
            tile: Some(at(2, 6)),
            button: None,
        },
    );
    let prims = render(&mut ui, Hover::default());
    for cell in [crate::art::CRATER, crate::art::RUIN, crate::art::TRENCH] {
        let src = crate::art::cell_rect(cell, cell == crate::art::TRENCH);
        assert!(
            prims.iter().any(|p| matches!(p, Prim::Sprite { src: s, .. } if *s == src)),
            "cell {cell} is not drawn"
        );
    }
}
