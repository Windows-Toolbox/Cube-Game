//! The game over screen after the ship was crushed (`game_over_scene.py`): how the run went (time, kills, cores) and
//! Retry, Game menu, Main menu. The window becomes a small panel where the arena was.
//!
//! Input is ignored for a moment at the start: the player may still be clicking or holding a key from the run.


use super::difficulty;
use super::hud::clock;
use super::profile::Profile;
use super::settings::{TEXT, TEXT_MUTED};
use super::shapes::{CORE_COLOR, draw_core};
use crate::engine::desktop::Desktop;
use crate::engine::menu::{Button, Menu};
use crate::engine::ui::{draw_panel, draw_text};
use crate::engine::window::Event;
use crate::engine::{Anchor, Color, Engine, Rect, Scene, Surface, Vec2, rgb, vec2};

const SIZE: (i32, i32) = (440, 490);
/// Seconds before the buttons react.
const INPUT_DELAY: f64 = 0.6;
const NEW_BEST_COLOR: Color = rgb(255, 215, 60);
const LINE: Color = rgb(50, 50, 58);

#[derive(Debug, Clone, PartialEq)]
pub struct RunResult {
    /// Seconds survived.
    pub time: f64,
    pub kills: i64,
    /// Collected in this run.
    pub cores: i64,
    pub difficulty: String,
    pub new_best: bool,
    /// Level unlocked in this run.
    pub unlocked: Option<String>,
    /// The level's goal was reached in this run for the first time.
    pub cleared: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Retry,
    GameMenu,
    MainMenu,
}

pub struct GameOverScene {
    profile: Profile,
    result: RunResult,
    rect: Rect,
    waited: f64,
    menu: Menu<Action>,
}

impl GameOverScene {
    pub fn new(profile: Profile, result: RunResult, center: Vec2) -> GameOverScene {
        let mut rect = Rect::new(0, 0, SIZE.0, SIZE.1);
        rect.set_centerv(center);
        rect.clamp_ip(&Desktop::current(profile.settings.all_monitors).area_at(center));
        GameOverScene { profile, result, rect, waited: 0.0, menu: Menu::new(vec![Button::new("Retry", Action::Retry), Button::new("Game menu", Action::GameMenu), Button::new("Main menu", Action::MainMenu)]) }
    }
}

fn level_name(key: &str) -> String {
    difficulty::get(key).map(|d| d.name.clone()).unwrap_or_else(|| key.to_string())
}

impl Scene for GameOverScene {
    fn enter(&mut self, e: &mut Engine) {
        e.grab_mouse(false);
        e.move_window(self.rect);
    }
    fn handle_event(&mut self, e: &mut Engine, event: &Event) {
        if self.waited < INPUT_DELAY {
            return;
        }
        match self.menu.handle_event(event) {
            Some(Action::Retry) => e.change_scene(Box::new(super::play_scene::PlayScene::new(self.profile.clone()))),
            Some(Action::GameMenu) => e.change_scene(Box::new(super::game_menu::GameMenuScene::new(Some(self.profile.clone())))),
            Some(Action::MainMenu) => e.change_scene(Box::new(super::scenes::MainMenuScene::new())),
            None => {}
        }
    }
    fn update(&mut self, _e: &mut Engine, dt: f64) {
        self.waited += dt;
    }
    fn draw(&mut self, _e: &mut Engine, surface: &mut Surface) {
        let panel = surface.get_rect();
        draw_panel(surface, panel, 0);
        draw_text(surface, "GAME OVER", 56, TEXT, Anchor::MidTop, panel.centerx(), 24);
        draw_text(surface, "crushed by the walls", 24, TEXT_MUTED, Anchor::MidTop, panel.centerx(), 74);
        let r = &self.result;
        let rows = [
            ("Time", clock(r.time)),
            ("Kills", r.kills.to_string()),
            ("Cores", format!("+{}   (total {})", r.cores, self.profile.cores)),
            ("Level", level_name(&r.difficulty)),
        ];
        let (left, right) = (60, panel.right() - 60);
        let mut y = 116;
        for (name, value) in rows {
            draw_text(surface, name, 24, TEXT_MUTED, Anchor::TopLeft, left, y);
            let v = draw_text(surface, &value, 24, TEXT, Anchor::TopRight, right, y);
            if name == "Cores" {
                draw_core(surface, vec2((v.left() - 12) as f64, (v.centery() - 1) as f64), 8.0, CORE_COLOR);
            }
            surface.line(LINE, vec2(left as f64, (y + 22) as f64), vec2(right as f64, (y + 22) as f64), 1);
            y += 30;
        }
        y += 6;
        let mut parts = vec![];
        if r.cleared {
            parts.push(format!("{} CLEARED!", level_name(&r.difficulty).to_uppercase()));
        }
        if let Some(u) = &r.unlocked {
            parts.push(format!("{} UNLOCKED!", level_name(u).to_uppercase()));
        }
        if !parts.is_empty() {
            y = draw_text(surface, &parts.join("  "), 28, NEW_BEST_COLOR, Anchor::MidTop, panel.centerx(), y).bottom() + 8;
        }
        if r.new_best {
            draw_text(surface, "NEW BEST TIME!", 28, NEW_BEST_COLOR, Anchor::MidTop, panel.centerx(), y);
        }
        self.menu.draw(surface, panel.centerx(), 310);
    }
    #[cfg(test)]
    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
