//! The small scenes and menus: the main menu (`main_menu_scene.py`) and the pause menu (`pause_menu.py`).


use super::settings::{self, HINT_ALPHA, TEXT, TEXT_MUTED};
use super::shapes::draw_ship;
use super::tables::ships;
use crate::engine::desktop::work_area;
use crate::engine::menu::{Button, Menu};
use crate::engine::ui::{draw_panel, draw_text, draw_text_a};
use crate::engine::window::Event;
use crate::engine::{Anchor, Engine, Rect, Scene, Surface, vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
enum MainAction {
    Play,
    Quit,
}

/// The main menu shown at start: title, a ship that looks at the mouse, Play (leads to the game menu) and Quit.
pub struct MainMenuScene {
    rect: Rect,
    menu: Menu<MainAction>,
}

impl MainMenuScene {
    pub fn new() -> MainMenuScene {
        let mut rect = Rect::new(0, 0, settings::MENU_WINDOW_SIZE.0, settings::MENU_WINDOW_SIZE.1);
        rect.set_centerv(work_area().centerv());
        MainMenuScene { rect, menu: Menu::new(vec![Button::new("Play", MainAction::Play), Button::new("Quit", MainAction::Quit)]) }
    }
    pub fn window_rect(&self) -> Rect {
        self.rect
    }
}

impl Scene for MainMenuScene {
    fn enter(&mut self, e: &mut Engine) {
        e.grab_mouse(false);
        e.move_window(self.rect);
    }
    fn handle_event(&mut self, e: &mut Engine, event: &Event) {
        match self.menu.handle_event(event) {
            Some(MainAction::Play) => e.change_scene(Box::new(super::game_menu::GameMenuScene::new(None))),
            Some(MainAction::Quit) => e.stop(),
            None => {}
        }
    }
    fn draw(&mut self, e: &mut Engine, surface: &mut Surface) {
        let area = surface.get_rect();
        surface.fill(settings::BACKGROUND);
        surface.rect(settings::BORDER, area, settings::BORDER_WIDTH);
        let ship_center = vec2(area.centerx() as f64, 80.0);
        let mouse = e.mouse_pos();
        let mut aim = vec2(mouse.0 as f64, mouse.1 as f64) - ship_center;
        if aim.length_squared() < 1.0 {
            aim = vec2(0.0, -1.0);
        }
        draw_ship(surface, ship_center, aim.normalize(), 46.0, ships::current().color);
        draw_text(surface, "CUBE GAME", 80, TEXT, Anchor::MidTop, area.centerx(), 130);
        draw_text(surface, "the window is the arena", 26, TEXT_MUTED, Anchor::MidTop, area.centerx(), 192);
        self.menu.draw(surface, area.centerx(), 250);
        let controls = draw_text_a(surface, "SHIFT dash  ·  SPACE rocket  ·  TAB inventory  ·  T shop  ·  ESC menu", 20, TEXT, HINT_ALPHA * 2.0, Anchor::MidBottom, area.centerx(), area.bottom() - 16);
        draw_text_a(surface, "W A S D move  ·  mouse aim  ·  left click shoot", 20, TEXT, HINT_ALPHA * 2.0, Anchor::MidBottom, area.centerx(), controls.top() - 4);
    }
    #[cfg(test)]
    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ---------- the pause menu ----------

/// The window shows only this menu while paused.
pub const PAUSE_SIZE: (i32, i32) = (340, 300);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PauseAction {
    Resume,
    /// Leaves the run – collected cores are kept.
    GameMenu,
    Quit,
}

/// The menu that opens with Esc during the game. The game is paused while it is open.
pub struct PauseMenu {
    menu: Menu<PauseAction>,
}

impl PauseMenu {
    pub fn new() -> PauseMenu {
        PauseMenu { menu: Menu::new(vec![Button::new("Resume", PauseAction::Resume), Button::new("Game menu", PauseAction::GameMenu), Button::new("Quit", PauseAction::Quit)]) }
    }
    /// Always start on "Resume".
    pub fn open(&mut self) {
        self.menu.selected = 0;
    }
    pub fn handle_event(&mut self, event: &Event) -> Option<PauseAction> {
        self.menu.handle_event(event)
    }
    pub fn draw(&mut self, surface: &mut Surface) {
        let panel = surface.get_rect();
        draw_panel(surface, panel, 0);
        draw_text(surface, "PAUSED", 44, TEXT, Anchor::MidTop, panel.centerx(), panel.top() + 26);
        self.menu.draw(surface, panel.centerx(), panel.top() + 90);
    }
}
