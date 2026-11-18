//! Menus: a vertical list of buttons (`Menu`), usable with the mouse (hover + click) and the keyboard (W/S or arrow
//! keys, Enter/Space), and a tiny menu that opens at the mouse (`ContextMenu`).
//!
//! In the Python version a button held the function to call. Here it holds a value (`A`, usually a small enum of
//! the scene); `handle_event` returns it when the button is activated, and the scene does what it stands for.
//!
//! Positions are window coordinates (mouse events are relative to the window, too). The menu is laid out on every
//! draw, so it stays centred even if the window changes size.

use super::font;
use super::geom::{Anchor, Rect};
use super::gfx::{Color, Radii, Surface, rgb};
use super::ui::{PANEL_FILL, draw_text};
use super::window::{Event, K_DOWN, K_ESCAPE, K_KP_ENTER, K_RETURN, K_S, K_SPACE, K_UP, K_W};

const IDLE_BORDER: Color = rgb(150, 150, 150);
const IDLE_TEXT: Color = rgb(210, 210, 210);
const SELECTED_FILL: Color = rgb(255, 255, 255);
const SELECTED_TEXT: Color = rgb(0, 0, 0);

pub struct Button<A> {
    pub label: String,
    pub action: A,
    /// Set by the menu before drawing.
    pub rect: Rect,
}

impl<A> Button<A> {
    pub fn new(label: impl Into<String>, action: A) -> Self {
        Button { label: label.into(), action, rect: Rect::default() }
    }
    pub fn draw(&self, surface: &mut Surface, selected: bool, font_size: i32) {
        let text_color = if selected {
            surface.rect_round(SELECTED_FILL, self.rect, 0, Radii::all(6));
            SELECTED_TEXT
        } else {
            surface.rect_round(IDLE_BORDER, self.rect, 2, Radii::all(6));
            IDLE_TEXT
        };
        let (cx, cy) = self.rect.center();
        draw_text(surface, &self.label, font_size, text_color, Anchor::Center, cx, cy);
    }
}

pub struct Menu<A> {
    pub buttons: Vec<Button<A>>,
    pub selected: usize,
    pub width: i32,
    pub height: i32,
    pub spacing: i32,
    pub font_size: i32,
}

impl<A: Clone> Menu<A> {
    pub fn new(buttons: Vec<Button<A>>) -> Self {
        Menu { buttons, selected: 0, width: 260, height: 46, spacing: 12, font_size: 30 }
    }
    pub fn total_height(&self) -> i32 {
        self.buttons.len() as i32 * self.height + (self.buttons.len() as i32 - 1) * self.spacing
    }
    /// Places the buttons below each other, the first one's top centre at `(x, y)`.
    pub fn layout(&mut self, x: i32, mut y: i32) {
        for b in &mut self.buttons {
            b.rect = Rect::new(0, 0, self.width, self.height);
            b.rect.set_midtop(x, y);
            y += self.height + self.spacing;
        }
    }
    /// The action of the button that was activated, if one was.
    pub fn handle_event(&mut self, event: &Event) -> Option<A> {
        let n = self.buttons.len();
        if n == 0 {
            return None;
        }
        match *event {
            Event::KeyDown(k) if k == K_W || k == K_UP => self.selected = (self.selected + n - 1) % n,
            Event::KeyDown(k) if k == K_S || k == K_DOWN => self.selected = (self.selected + 1) % n,
            Event::KeyDown(k) if k == K_RETURN || k == K_KP_ENTER || k == K_SPACE => return Some(self.buttons[self.selected].action.clone()),
            Event::MouseMotion { x, y } => {
                for (i, b) in self.buttons.iter().enumerate() {
                    if b.rect.collidepoint(x, y) {
                        self.selected = i;
                    }
                }
            }
            // On release, not on press – otherwise the still-held button would fire a shot right after "Play".
            Event::MouseButtonUp { button: 1, x, y } => {
                return self.buttons.iter().find(|b| b.rect.collidepoint(x, y)).map(|b| b.action.clone());
            }
            _ => {}
        }
        None
    }
    pub fn draw(&mut self, surface: &mut Surface, x: i32, y: i32) {
        self.layout(x, y);
        for (i, b) in self.buttons.iter().enumerate() {
            b.draw(surface, i == self.selected, self.font_size);
        }
    }
}

// ---------- the context menu ----------

const ITEM_HEIGHT: i32 = 30;
const TITLE_HEIGHT: i32 = 26;
const PADDING: i32 = 6;
const MIN_WIDTH: i32 = 120;
const FONT_SIZE: i32 = 24;
const TITLE_SIZE: i32 = 19;
const CM_BORDER: Color = rgb(110, 110, 125);
const HOVER_FILL: Color = rgb(44, 44, 56);
const CM_TEXT: Color = rgb(255, 255, 255);
const CM_TITLE: Color = rgb(150, 150, 160);

/// A tiny menu that opens at the mouse, e.g. on a right click: an optional title and a short list of items. Click an
/// item to do it; a click anywhere else or Esc closes the menu without doing anything.
///
/// While it is open, the scene passes every event to `handle_event` first – it says whether it used the event (the
/// scene then ignores it), so a click that closes the menu does not also click whatever is under it.
pub struct ContextMenu<A> {
    pub items: Vec<(String, A)>,
    pub title: Option<String>,
    pub open: bool,
    pub hovered: Option<usize>,
    pub rect: Rect,
}

/// What a context menu did with an event.
pub enum Used<A> {
    /// Not its business – the scene handles it.
    No,
    Yes,
    /// An item was chosen.
    Chosen(A),
}

impl<A: Clone> ContextMenu<A> {
    /// `(x, y)`: where the mouse is (the menu's top left corner, moved in to stay inside `bounds`).
    pub fn new(x: i32, y: i32, items: Vec<(String, A)>, bounds: Rect, title: Option<String>) -> Self {
        let mut width = MIN_WIDTH;
        for (label, _) in &items {
            width = width.max(font::text_size(label, FONT_SIZE).0 + 4 * PADDING);
        }
        if let Some(t) = &title {
            width = width.max(font::text_size(t, TITLE_SIZE).0 + 4 * PADDING);
        }
        let height = 2 * PADDING + items.len() as i32 * ITEM_HEIGHT + if title.is_some() { TITLE_HEIGHT } else { 0 };
        let rect = Rect::new(x, y, width, height).clamp(&bounds);
        ContextMenu { items, title, open: true, hovered: None, rect }
    }
    fn item_rect(&self, index: usize) -> Rect {
        let top = self.rect.top() + PADDING + if self.title.is_some() { TITLE_HEIGHT } else { 0 } + index as i32 * ITEM_HEIGHT;
        Rect::new(self.rect.left() + PADDING, top, self.rect.w - 2 * PADDING, ITEM_HEIGHT)
    }
    fn item_at(&self, x: i32, y: i32) -> Option<usize> {
        (0..self.items.len()).find(|&i| self.item_rect(i).collidepoint(x, y))
    }
    pub fn handle_event(&mut self, event: &Event) -> Used<A> {
        match *event {
            Event::MouseMotion { x, y } => {
                self.hovered = self.item_at(x, y);
                Used::Yes // what is under the menu does not light up meanwhile
            }
            Event::MouseButtonDown { .. } => Used::Yes,
            Event::MouseButtonUp { button, x, y } => {
                let index = self.item_at(x, y);
                self.open = false;
                match index {
                    Some(i) if button == 1 => Used::Chosen(self.items[i].1.clone()),
                    _ => Used::Yes,
                }
            }
            Event::KeyDown(k) if k == K_ESCAPE => {
                self.open = false;
                Used::Yes
            }
            _ => Used::No,
        }
    }
    pub fn draw(&self, surface: &mut Surface) {
        surface.rect_round(PANEL_FILL, self.rect, 0, Radii::all(6));
        surface.rect_round(CM_BORDER, self.rect, 1, Radii::all(6));
        if let Some(t) = &self.title {
            draw_text(surface, t, TITLE_SIZE, CM_TITLE, Anchor::MidLeft, self.rect.left() + 2 * PADDING, self.rect.top() + PADDING + TITLE_HEIGHT / 2);
        }
        for (i, (label, _)) in self.items.iter().enumerate() {
            let r = self.item_rect(i);
            if Some(i) == self.hovered {
                surface.rect_round(HOVER_FILL, r, 0, Radii::all(4));
            }
            draw_text(surface, label, FONT_SIZE, CM_TEXT, Anchor::MidLeft, r.left() + PADDING, r.centery());
        }
    }
}
