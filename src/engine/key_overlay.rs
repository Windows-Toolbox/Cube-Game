//! Key display like in esports streams: keycaps that light up while the game sees the key as held – and, if
//! wanted, a mouse whose buttons light up while they are held.
//!
//! Shows what the game actually receives – handy to spot keyboard ghosting (the keyboard not reporting some key
//! combinations). Toggled with F3.

use super::font;
use super::geom::{Anchor, Rect};
use super::gfx::{Color, Radii, Surface, rgba};
use super::window::{Event, K_F3, Key};
use super::Engine;

/// Pixels per keycap (a wide key is several of these, plus the gaps between them).
const KEY_SIZE: i32 = 40;
const GAP: i32 = 6;
const MOUSE_SIZE: (i32, i32) = (54, 86);
const IDLE_FILL: Color = rgba(255, 255, 255, 30);
const IDLE_BORDER: Color = rgba(255, 255, 255, 110);
const IDLE_TEXT: Color = rgba(255, 255, 255, 160);
const HELD_FILL: Color = rgba(255, 255, 255, 230);
const HELD_TEXT: Color = rgba(0, 0, 0, 255);

/// One keycap: its label, the keys that light it (e.g. both shift keys), column, row and width in keycaps.
#[derive(Debug, Clone)]
pub struct Cap {
    pub label: String,
    pub keys: Vec<Key>,
    pub column: i32,
    pub row: i32,
    pub width: i32,
}

pub struct KeyOverlay {
    layout: Vec<Cap>,
    pub visible: bool,
    /// Which mouse buttons to show on a mouse right of the keys (0 left, 2 right) – empty: no mouse.
    mouse_buttons: Vec<usize>,
    mouse_rect: Option<Rect>,
    surface: Surface,
    key_top: i32,
    /// Gap to the bottom right corner of the window.
    inset: i32,
}

impl KeyOverlay {
    pub fn new(layout: Vec<Cap>, inset: i32, visible: bool, mouse_buttons: Vec<usize>) -> Self {
        let columns = layout.iter().map(|c| c.column + c.width).max().unwrap_or(1);
        let rows = layout.iter().map(|c| c.row).max().unwrap_or(0) + 1;
        let keys_width = columns * KEY_SIZE + (columns - 1) * GAP;
        let mut height = rows * KEY_SIZE + (rows - 1) * GAP;
        let mut width = keys_width;
        let mut mouse_rect = None;
        if !mouse_buttons.is_empty() {
            height = height.max(MOUSE_SIZE.1);
            let r = Rect::new(keys_width + 2 * GAP, height - MOUSE_SIZE.1, MOUSE_SIZE.0, MOUSE_SIZE.1);
            width = r.right();
            mouse_rect = Some(r);
        }
        let key_top = height - (rows * KEY_SIZE + (rows - 1) * GAP); // keys sit at the bottom, like the mouse
        KeyOverlay { layout, visible, mouse_buttons, mouse_rect, surface: Surface::new_alpha(width, height), key_top, inset }
    }

    pub fn handle_event(&mut self, event: &Event) {
        if *event == Event::KeyDown(K_F3) {
            self.visible = !self.visible;
        }
    }

    pub fn draw(&mut self, e: &Engine, surface: &mut Surface) {
        if !self.visible {
            return;
        }
        self.surface.fill(rgba(0, 0, 0, 0));
        for i in 0..self.layout.len() {
            let c = self.layout[i].clone();
            let cap = Rect::new(c.column * (KEY_SIZE + GAP), self.key_top + c.row * (KEY_SIZE + GAP), c.width * KEY_SIZE + (c.width - 1) * GAP, KEY_SIZE);
            let size = if font::text_size(&c.label, 26).0 <= cap.w - 8 { 26 } else { 20 };
            let held = c.keys.iter().any(|k| e.key_held(*k));
            self.draw_cap(cap, &c.label, held, size, Radii::all(6));
        }
        if self.mouse_rect.is_some() {
            self.draw_mouse(e);
        }
        let corner = surface.get_rect().inflate(-2 * self.inset, -2 * self.inset).bottomright();
        let r = self.surface.get_rect().placed(Anchor::BottomRight, corner.0, corner.1);
        surface.blit(&self.surface, r.x, r.y);
    }

    fn draw_cap(&mut self, cap: Rect, label: &str, held: bool, size: i32, radii: Radii) {
        let text_color = if held {
            self.surface.rect_round(HELD_FILL, cap, 0, radii);
            HELD_TEXT
        } else {
            self.surface.rect_round(IDLE_FILL, cap, 0, radii);
            self.surface.rect_round(IDLE_BORDER, cap, 2, radii);
            IDLE_TEXT
        };
        if !label.is_empty() {
            let text = font::render(label, size, text_color);
            let (cx, cy) = cap.center();
            let r = text.get_rect().placed(Anchor::Center, cx, cy);
            self.surface.blit(&text, r.x, r.y);
        }
    }

    /// A mouse from above: two buttons on top (lit while held), the body below.
    fn draw_mouse(&mut self, e: &Engine) {
        let Some(m) = self.mouse_rect else { return };
        let pressed = e.mouse_held();
        let button_height = crate::engine::geom::py_round(m.h as f64 * 0.42) as i32;
        let half = (m.w - 2) / 2;
        let left = Rect::new(m.left(), m.top(), half, button_height);
        let right = Rect::new(m.right() - half, m.top(), half, button_height);
        let body = Rect::new(m.left(), m.top() + button_height + 2, m.w, m.h - button_height - 2);
        let radius = m.w / 2;
        let l = self.mouse_buttons.contains(&0) && pressed[0];
        let r = self.mouse_buttons.contains(&2) && pressed[2];
        self.draw_cap(left, "", l, 20, Radii { tl: radius, ..Radii::default() });
        self.draw_cap(right, "", r, 20, Radii { tr: radius, ..Radii::default() });
        self.draw_cap(body, "", false, 20, Radii { bl: radius, br: radius, ..Radii::default() });
    }
}
