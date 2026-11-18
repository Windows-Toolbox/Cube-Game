//! Small shapes the game draws in many places: the ship's arrowhead (`ship_shape.py`) and the icons (`icons.py`):
//! star, core, arrow, shop, watchtower and the level indicators of the game menu.

use crate::engine::geom::py_round;
use crate::engine::gfx::Radii;
use crate::engine::{Color, Rect, Surface, Vec2, rgb, vec2};

pub const STAR_COLOR: Color = rgb(255, 215, 60);
pub const CORE_COLOR: Color = rgb(90, 225, 255);

/// A colour darker by `shade` (0 … 1), each channel rounded as Python's `round` does.
pub fn shade(c: Color, k: f64) -> Color {
    Color::f(py_round(c.r as f64 * k), py_round(c.g as f64 * k), py_round(c.b as f64 * k))
}

/// An arrowhead: nose towards `aim`, two wing tips at the back, a notch in between.
pub fn draw_ship(surface: &mut Surface, center: Vec2, aim: Vec2, size: f64, color: Color) {
    let points = [
        center + aim * size * 0.6,              // nose
        center + aim.rotate(140.0) * size * 0.5, // wing tip
        center - aim * size * 0.15,             // notch at the back
        center + aim.rotate(-140.0) * size * 0.5, // other wing tip
    ];
    surface.polygon(color, &points, 0);
}

/// A five-pointed star.
pub fn draw_star(surface: &mut Surface, center: Vec2, radius: f64, color: Color) {
    let points: Vec<Vec2> = (0..10).map(|i| center + vec2(0.0, -radius * if i % 2 == 0 { 1.0 } else { 0.45 }).rotate(i as f64 * 36.0)).collect();
    surface.polygon(color, &points, 0);
}

/// A small cube seen from above at an angle: a bright top, a medium left and a dark right side.
pub fn draw_core(surface: &mut Surface, center: Vec2, radius: f64, color: Color) {
    let (x, y) = (center.x, center.y);
    let side = radius * 0.87; // cos 30°: the cube's corners left and right
    let top = [vec2(x, y - radius), vec2(x + side, y - radius / 2.0), vec2(x, y), vec2(x - side, y - radius / 2.0)];
    let left = [vec2(x - side, y - radius / 2.0), vec2(x, y), vec2(x, y + radius), vec2(x - side, y + radius / 2.0)];
    let right = [vec2(x + side, y - radius / 2.0), vec2(x, y), vec2(x, y + radius), vec2(x + side, y + radius / 2.0)];
    for (points, k) in [(top, 1.0), (left, 0.7), (right, 0.45)] {
        surface.polygon(shade(color, k), &points, 0);
    }
}

/// A line from `start` to `end` with a small head at the end.
pub fn draw_arrow(surface: &mut Surface, start: Vec2, end: Vec2, color: Color, width: i32) {
    surface.line(color, start, end, width);
    let d = (end - start).normalize() * 6.0;
    surface.polygon(color, &[end + d * 0.5, end - d + d.rotate(90.0) * 0.7, end - d - d.rotate(90.0) * 0.7], 0);
}

/// A small shopping cart: handle, basket, two wheels.
pub fn draw_shop_icon(surface: &mut Surface, center: Vec2, color: Color) {
    let (x, y) = (center.x, center.y);
    surface.line(color, vec2(x - 10.0, y - 6.0), vec2(x - 6.0, y - 6.0), 2);
    surface.polygon(color, &[vec2(x - 6.0, y - 5.0), vec2(x + 9.0, y - 5.0), vec2(x + 6.0, y + 2.0), vec2(x - 4.0, y + 2.0)], 0);
    surface.circle(color, vec2(x - 3.0, y + 5.0), 2.0, 0);
    surface.circle(color, vec2(x + 5.0, y + 5.0), 2.0, 0);
}

/// A small watchtower from above: a square with a dark lens.
pub fn draw_watchtower_icon(surface: &mut Surface, center: Vec2, color: Color) {
    let mut r = Rect::new(0, 0, 13, 13);
    r.set_centerv(center);
    surface.rect_round(color, r, 0, Radii::all(2));
    surface.circle(rgb(0, 0, 0), r.centerv(), 3.0, 0);
}

// ---------- level indicators (game menu, Levels tab) – size: about the icon's width in pixels ----------

/// A swarm: a small cluster of squares.
pub fn draw_swarm_icon(surface: &mut Surface, center: Vec2, color: Color, size: f64) {
    let side = (py_round(size * 0.3) as i32).max(3);
    for (dx, dy) in [(-0.35, -0.3), (0.3, -0.35), (0.0, 0.05), (-0.38, 0.38), (0.35, 0.35)] {
        let mut sq = Rect::new(0, 0, side, side);
        sq.set_center(center.x + dx * size, center.y + dy * size);
        surface.rect(color, sq, 0);
    }
}

/// A wave of one type (a phase with more of it): a warning triangle with an exclamation mark.
pub fn draw_wave_icon(surface: &mut Surface, center: Vec2, color: Color, size: f64) {
    let (x, y, half) = (center.x, center.y, size / 2.0);
    surface.polygon(color, &[vec2(x, y - half), vec2(x + half, y + half * 0.8), vec2(x - half, y + half * 0.8)], 2);
    surface.line(color, vec2(x, y - half * 0.25), vec2(x, y + half * 0.25), 2);
    surface.rect(color, Rect::new(x - 1.0, y + half * 0.45, 2, 2), 0);
}

/// Enemies hit harder: a spiky burst.
pub fn draw_damage_icon(surface: &mut Surface, center: Vec2, color: Color, size: f64) {
    let points: Vec<Vec2> = (0..16).map(|i| center + vec2(0.0, -size / 2.0 * if i % 2 == 0 { 1.0 } else { 0.45 }).rotate(i as f64 * 22.5)).collect();
    surface.polygon(color, &points, 0);
}

/// Enemies get more hit points: a thick plus.
pub fn draw_hp_icon(surface: &mut Surface, center: Vec2, color: Color, size: f64) {
    let (x, y) = (center.x, center.y);
    let thick = (py_round(size * 0.32)).max(3.0);
    surface.rect(color, Rect::new(x - size / 2.0, y - thick / 2.0, size, thick), 0);
    surface.rect(color, Rect::new(x - thick / 2.0, y - size / 2.0, thick, size), 0);
}

/// Harder walls: a little brick wall – a block with dark mortar lines.
pub fn draw_wall_icon(surface: &mut Surface, center: Vec2, color: Color, size: f64) {
    let mut wall = Rect::new(0.0, 0.0, size, py_round(size * 0.8));
    wall.set_centerv(center);
    surface.rect(color, wall, 0);
    let mortar = shade(color, 0.3);
    let middle = wall.centery() as f64;
    surface.line(mortar, vec2(wall.left() as f64, middle), vec2((wall.right() - 1) as f64, middle), 1);
    surface.line(mortar, vec2(wall.centerx() as f64, wall.top() as f64), vec2(wall.centerx() as f64, middle), 1);
    for x in [wall.left() + wall.w / 4, wall.right() - wall.w / 4] {
        surface.line(mortar, vec2(x as f64, middle), vec2(x as f64, (wall.bottom() - 1) as f64), 1);
    }
}

/// Something else is announced: a diamond.
pub fn draw_notice_icon(surface: &mut Surface, center: Vec2, color: Color, size: f64) {
    let (x, y, half) = (center.x, center.y, size / 2.0);
    surface.polygon(color, &[vec2(x, y - half), vec2(x + half, y), vec2(x, y + half), vec2(x - half, y)], 0);
}

/// One of several things, picked at random: a die showing five.
pub fn draw_random_icon(surface: &mut Surface, center: Vec2, color: Color, size: f64) {
    let mut die = Rect::new(0.0, 0.0, size, size);
    die.set_centerv(center);
    surface.rect_round(color, die, 0, Radii::all((size as i32 / 5).max(2)));
    let pip = (size as i32 / 9).max(1) as f64;
    let dark = shade(color, 0.2);
    for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (0.0, 0.0), (-1.0, 1.0), (1.0, 1.0)] {
        surface.circle(dark, vec2(die.centerx() as f64 + dx * size * 0.25, die.centery() as f64 + dy * size * 0.25), pip, 0);
    }
}

/// Points of interest appear: a map pin – a round head with a hole, a point below.
pub fn draw_pin_icon(surface: &mut Surface, center: Vec2, color: Color, size: f64) {
    let (x, y) = (center.x, center.y);
    let radius = size * 0.36;
    let head = vec2(x, y - size * 0.14);
    surface.polygon(color, &[vec2(x - radius * 0.8, head.y + radius * 0.5), vec2(x + radius * 0.8, head.y + radius * 0.5), vec2(x, y + size / 2.0)], 0);
    surface.circle(color, head, radius, 0);
    surface.circle(shade(color, 0.25), head, (radius * 0.4).max(1.0), 0);
}

/// A tick: cleared.
pub fn draw_check(surface: &mut Surface, center: Vec2, color: Color, size: f64) {
    let (x, y) = (center.x, center.y);
    surface.lines(color, false, &[vec2(x - size / 2.0, y), vec2(x - size / 8.0, y + size * 0.38), vec2(x + size / 2.0, y - size * 0.4)], 3);
}
