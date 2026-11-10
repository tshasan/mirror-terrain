//! A playable demo of Mirror Terrain: you (teal, bottom) against a bot (orange, top).
//!
//! All the rules live in `mt_core` and the opponent in `mt_bots`; `session.rs` runs the phases. This file is only the Bevy
//! app: it reads the mouse and keys into `ui.rs`, and spawns what `draw.rs` paints. The static layer is rebuilt when
//! something changes, the hover layer when the mouse moves to another tile or button, and the clash layer every frame.

#![allow(clippy::needless_range_loop)]

mod art;
mod draw;
mod info;
mod paint;
#[cfg(test)]
mod screens;
mod session;
mod ui;

use art::Art;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use paint::*;
use session::{Phase, Session};
use ui::{Action, Ui};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bot = args.get(1).cloned().unwrap_or_else(|| "shaper".into());
    if !mt_bots::BOT_NAMES.contains(&bot.as_str()) {
        eprintln!("unknown bot `{bot}`; choose one of: {}", mt_bots::BOT_NAMES.join(" "));
        std::process::exit(2);
    }
    // No seed given: a different map each launch. This is the front end, so the clock is fine here.
    let seed = args.get(2).and_then(|s| s.parse().ok()).unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_secs() % 100_000)
    });
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Mirror Terrain demo".into(),
                        resolution: WIN.into(),
                        resizable: false,
                        ..default()
                    }),
                    ..default()
                })
                // Pixel art: sample the nearest texel, never blend.
                .set(ImagePlugin::default_nearest())
                .set(AssetPlugin {
                    file_path: art::asset_dir(),
                    ..default()
                }),
        )
        .insert_resource(ClearColor(PAPER))
        .insert_non_send_resource(Ui::new(Session::new(seed, &bot), seed))
        .add_systems(Startup, |mut commands: Commands, assets: Res<AssetServer>| {
            commands.spawn(Camera2d);
            commands.insert_resource(Art(assets.load(art::SHEET)));
        })
        .add_systems(Update, (input, advance_clash, draw_static, draw_hover, draw_clash).chain())
        .run();
}

fn input(mut ui: NonSendMut<Ui>, mouse: Res<ButtonInput<MouseButton>>, keys: Res<ButtonInput<KeyCode>>, windows: Query<&Window, With<PrimaryWindow>>) {
    const DIGITS: [KeyCode; 7] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
    ];
    const BUILD_KEYS: [KeyCode; 3] = [KeyCode::KeyQ, KeyCode::KeyW, KeyCode::KeyE];
    if keys.just_pressed(KeyCode::Space) && ui.s.phase == Phase::Clash {
        ui.act(Action::SkipClash);
    }
    if keys.just_pressed(KeyCode::KeyP) && ui.s.phase == Phase::Clash {
        ui.act(Action::Pause);
    }
    if keys.just_pressed(KeyCode::KeyZ) {
        ui.act(Action::Undo);
    }
    if keys.just_pressed(KeyCode::Escape) {
        ui.drop_tool();
    }
    for (n, key) in DIGITS.iter().enumerate() {
        if keys.just_pressed(*key) {
            ui.number_key(n);
        }
    }
    for (n, key) in BUILD_KEYS.iter().enumerate() {
        if keys.just_pressed(*key) {
            ui.building_key(n);
        }
    }
    if keys.just_pressed(KeyCode::Enter) {
        match ui.s.phase {
            Phase::Plan => ui.act(Action::Ready),
            Phase::Resolve => ui.act(Action::NextRound),
            _ => {}
        }
    }
    // The window is not resizable and the camera is centred, so the cursor position is already in screen pixels.
    let cursor = windows.get_single().ok().and_then(|w| w.cursor_position());
    ui.set_hover(cursor);
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(p) = cursor else {
        return;
    };
    if let Some(action) = ui.buttons.iter().find(|b| b.rect.contains(p)).map(|b| b.action) {
        ui.act(action);
    } else if let Some(tile) = tile_at(p) {
        ui.click_tile(tile);
    }
}

fn advance_clash(mut ui: NonSendMut<Ui>, time: Res<Time>) {
    ui.advance_clash(time.delta_secs());
}

fn draw_static(mut commands: Commands, art: Res<Art>, mut ui: NonSendMut<Ui>, old: Query<Entity, With<StaticLayer>>) {
    if !ui.dirty {
        return;
    }
    ui.dirty = false;
    for e in &old {
        commands.entity(e).despawn();
    }
    draw::static_layer(&mut Painter::new(&mut commands, Layer::Static, &art), &mut ui);
}

fn draw_hover(mut commands: Commands, art: Res<Art>, mut ui: NonSendMut<Ui>, old: Query<Entity, With<HoverLayer>>) {
    if !ui.hover_dirty && ui.hover_drawn == Some(ui.hover) {
        return;
    }
    ui.hover_dirty = false;
    ui.hover_drawn = Some(ui.hover);
    for e in &old {
        commands.entity(e).despawn();
    }
    draw::hover_layer(&mut Painter::new(&mut commands, Layer::Hover, &art), &ui);
}

fn draw_clash(mut commands: Commands, art: Res<Art>, ui: NonSend<Ui>, old: Query<Entity, With<ClashLayer>>) {
    for e in &old {
        commands.entity(e).despawn();
    }
    draw::clash_layer(&mut Painter::new(&mut commands, Layer::Clash, &art), &ui);
}
