//! Small drawing helpers for menus and panels: text, panels.

use super::font;
use super::geom::{Anchor, Rect};
use super::gfx::{Color, Radii, Surface, rgb};

pub const PANEL_FILL: Color = rgb(16, 16, 20);
pub const PANEL_BORDER: Color = rgb(255, 255, 255);

pub use super::font::text_size;

/// Draws text and returns its rect. `anchor` positions it, e.g. `Anchor::Center` at (100, 50).
pub fn draw_text(surface: &mut Surface, text: &str, size: i32, color: Color, anchor: Anchor, x: impl Into<f64>, y: impl Into<f64>) -> Rect {
    font::draw_text(surface, text, size, color, 255, anchor, x, y)
}

/// The same, transparent (`alpha` < 255 fades it).
pub fn draw_text_a(surface: &mut Surface, text: &str, size: i32, color: Color, alpha: impl Into<f64>, anchor: Anchor, x: impl Into<f64>, y: impl Into<f64>) -> Rect {
    let a = alpha.into().clamp(0.0, 255.0) as u8;
    font::draw_text(surface, text, size, color, a, anchor, x, y)
}

/// `text` split into lines (at spaces) that are at most `width` pixels wide – a single long word stays whole.
pub fn wrap(text: &str, size: i32, width: i32) -> Vec<String> {
    let mut lines: Vec<String> = vec![];
    for word in text.split_whitespace() {
        if let Some(last) = lines.last_mut() {
            let joined = format!("{last} {word}");
            if text_size(&joined, size).0 <= width {
                *last = joined;
                continue;
            }
        }
        lines.push(word.to_string());
    }
    lines
}

/// A dark box with a white outline – the background of menus and windows inside the game. `radius` 0 for square
/// corners, e.g. when the panel fills the whole window.
pub fn draw_panel(surface: &mut Surface, rect: Rect, radius: i32) {
    surface.rect_round(PANEL_FILL, rect, 0, Radii::all(radius));
    surface.rect_round(PANEL_BORDER, rect, 2, Radii::all(radius));
}
