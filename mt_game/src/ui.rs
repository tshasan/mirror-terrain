//! What the front end keeps besides the match: the selected tool, the click targets of the current screen, the hover and the
//! clash playback controls. Clicks and keys become `Session` calls here, so they can be tested without a window.

use crate::info::{Tool, BUILDINGS};
use crate::session::{Phase, Session};
use bevy::math::{Rect, Vec2};
use mt_core::*;

/// Clash frames shown per second at 1x (a frame is two ticks of the simulation).
pub const FRAMES_PER_SEC: f32 = 14.0;
pub const SPEEDS: [f32; 3] = [0.5, 1.0, 2.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Draft(usize),
    Pick(Tool),
    Unit(UnitKind),
    Undo,
    Ready,
    SkipClash,
    Pause,
    Speed(usize),
    NextRound,
    NewGame,
}

pub struct Button {
    pub rect: Rect,
    pub action: Action,
    /// What the description box says while the mouse is over the button.
    pub hint: String,
    /// False for a button the player cannot use right now; it stays clickable so that the refusal is explained.
    pub enabled: bool,
}

/// What the mouse is over.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Hover {
    pub tile: Option<usize>,
    pub button: Option<usize>,
}

pub struct Ui {
    pub s: Session,
    pub seed: u64,
    pub tool: Tool,
    /// A one-line answer to the last click (why an op was refused, for example).
    pub notice: String,
    pub buttons: Vec<Button>,
    /// Playback position in clash frames.
    pub clash_t: f32,
    pub paused: bool,
    /// Index into `SPEEDS`.
    pub speed: usize,
    pub hover: Hover,
    /// The hover the hover layer was last drawn for; it is redrawn when this differs or `hover_dirty` is set.
    pub hover_drawn: Option<Hover>,
    pub hover_dirty: bool,
    /// The static layer needs redrawing.
    pub dirty: bool,
}

impl Ui {
    pub fn new(s: Session, seed: u64) -> Ui {
        Ui {
            s,
            seed,
            tool: Tool::None,
            notice: String::new(),
            buttons: Vec::new(),
            clash_t: 0.0,
            paused: false,
            speed: 1,
            hover: Hover::default(),
            hover_drawn: None,
            hover_dirty: true,
            dirty: true,
        }
    }

    /// Marks both layers stale.
    pub fn touch(&mut self) {
        self.dirty = true;
        self.hover_dirty = true;
    }

    pub fn set_hover(&mut self, cursor: Option<Vec2>) {
        self.hover = match cursor {
            None => Hover::default(),
            Some(p) => {
                let button = self.buttons.iter().position(|b| b.rect.contains(p));
                Hover {
                    tile: if button.is_some() { None } else { crate::paint::tile_at(p) },
                    button,
                }
            }
        };
    }

    pub fn act(&mut self, action: Action) {
        self.notice.clear();
        let result = match action {
            Action::Draft(i) => {
                self.s.draft_click(i);
                Ok(())
            }
            Action::Pick(tool) => {
                self.tool = if self.tool == tool { Tool::None } else { tool };
                Ok(())
            }
            Action::Unit(kind) => self.s.try_unit(kind),
            Action::Undo => {
                self.s.undo();
                Ok(())
            }
            Action::Ready => {
                self.s.ready();
                self.tool = Tool::None;
                Ok(())
            }
            Action::SkipClash => {
                self.s.finish_clash();
                Ok(())
            }
            Action::Pause => {
                self.paused = !self.paused;
                Ok(())
            }
            Action::Speed(i) => {
                self.speed = i.min(SPEEDS.len() - 1);
                Ok(())
            }
            Action::NextRound => {
                self.s.next_round();
                self.tool = Tool::None;
                Ok(())
            }
            Action::NewGame => {
                self.seed += 1;
                self.s = Session::new(self.seed, &self.s.bot_name.clone());
                self.tool = Tool::None;
                Ok(())
            }
        };
        if let Err(why) = result {
            self.notice = why;
        }
        self.touch();
    }

    pub fn click_tile(&mut self, tile: usize) {
        self.notice.clear();
        let result = match (self.s.phase, self.tool) {
            (Phase::Plan, Tool::Op(op)) => self.s.try_op(tile, op),
            (Phase::Plan, Tool::Building(kind)) => self.s.try_building(kind, tile),
            (Phase::Plan, Tool::None) => Err("Pick an op or a building on the right first.".into()),
            (Phase::Attack, _) => {
                self.s.attack(xy(tile).0 as u8);
                self.clash_t = 0.0;
                self.paused = false;
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(why) = result {
            self.notice = why;
        }
        self.touch();
    }

    /// Esc: put the tool down.
    pub fn drop_tool(&mut self) {
        if self.tool != Tool::None {
            self.tool = Tool::None;
            self.notice.clear();
            self.touch();
        }
    }

    /// Number keys: the n-th choice of the current phase (an offer in the draft, an op while shaping, a clash speed).
    pub fn number_key(&mut self, n: usize) {
        match self.s.phase {
            Phase::Draft if n < 3 => self.act(Action::Draft(n)),
            Phase::Plan if n < ALL_OPS.len() => self.act(Action::Pick(Tool::Op(ALL_OPS[n]))),
            Phase::Clash if n < SPEEDS.len() => self.act(Action::Speed(n)),
            _ => {}
        }
    }

    /// Q, W, E: the buildings, while shaping.
    pub fn building_key(&mut self, n: usize) {
        if self.s.phase == Phase::Plan && n < BUILDINGS.len() {
            self.act(Action::Pick(Tool::Building(BUILDINGS[n])));
        }
    }

    /// Moves the clash playback on by `dt` seconds of real time and ends it when the last frame has been shown.
    pub fn advance_clash(&mut self, dt: f32) {
        if self.s.phase != Phase::Clash || self.paused {
            return;
        }
        self.clash_t += dt * FRAMES_PER_SEC * SPEEDS[self.speed];
        if self.clash_t >= self.s.frames.len() as f32 {
            self.s.finish_clash();
            self.touch();
        }
    }

    /// The frame of the clash being shown.
    pub fn clash_frame(&self) -> usize {
        (self.clash_t as usize).min(self.s.frames.len().saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_clicks_drive_the_same_actions() {
        let mut ui = Ui::new(Session::new(4, "greedy"), 4);
        ui.number_key(2);
        assert_eq!(ui.s.order, vec![2]);
        ui.number_key(0);
        assert_eq!(ui.s.phase, Phase::Plan);
        ui.number_key(3);
        assert_eq!(ui.tool, Tool::Op(Op::Flood));
        ui.number_key(3);
        assert_eq!(ui.tool, Tool::None, "the same key puts the tool down");
        ui.building_key(0);
        assert_eq!(ui.tool, Tool::Building(BuildingKind::Turret));
        ui.drop_tool();
        assert_eq!(ui.tool, Tool::None);
        ui.act(Action::Ready);
        assert_eq!(ui.s.phase, Phase::Attack);
        ui.click_tile(4);
        assert_eq!(ui.s.phase, Phase::Clash);
        let frames = ui.s.frames.len() as f32;
        ui.paused = true;
        ui.advance_clash(100.0);
        assert_eq!(ui.s.phase, Phase::Clash, "paused: nothing moves");
        ui.paused = false;
        ui.speed = 2;
        ui.advance_clash(frames / (FRAMES_PER_SEC * 2.0) + 0.01);
        assert!(matches!(ui.s.phase, Phase::Resolve | Phase::Over), "the playback ends by itself");
    }
}
