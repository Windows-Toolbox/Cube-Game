//! What is shown on top of the run (`hud.py`, `ability_bar.py`): the clock (and the level's goal), cores, stars, the
//! shop and watchtower keys top right; the abilities, the energy and the weapons bottom left.

use super::settings::{self, ABILITY_KEY_NAMES, HINT_ALPHA, TEXT, WEAPON_SWITCH_KEY_NAME};
use super::shapes::{STAR_COLOR, draw_core, draw_shop_icon, draw_star, draw_watchtower_icon, CORE_COLOR};
use super::ship::Ship;
use crate::engine::geom::py_round;
use crate::engine::gfx::Radii;
use crate::engine::ui::draw_text_a;
use crate::engine::{Anchor, Color, Rect, Surface, rgb, vec2};

const RIGHT_MARGIN: i32 = 12;
const READY_ALPHA: f64 = 200.0;
const LINE_HEIGHT: i32 = 22;
const ENERGY_BAR: (i32, i32) = (150, 8);
const ENERGY_COLOR: Color = rgb(255, 225, 90);
/// Not enough for any shot.
const EMPTY_COLOR: Color = rgb(210, 80, 60);
const BAR_BACKGROUND: Color = rgb(45, 45, 52);

/// "01:23".
pub fn clock(seconds: f64) -> String {
    let s = seconds as i64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// Top right. `stars`: `None` while the shop is locked. `towers`: (standing, allowed, price of the next) – `None`
/// without the watchtower. `goal`: seconds to clear the level – `None` once it is cleared (or endless).
pub fn draw_hud(surface: &mut Surface, elapsed: f64, stars: Option<i64>, cores: i64, towers: Option<(usize, i64, i64)>, goal: Option<f64>) {
    let right = surface.width() - RIGHT_MARGIN;
    let text = match goal {
        None => clock(elapsed),
        Some(g) => format!("{} / {}", clock(elapsed), clock(g)),
    };
    let timer = draw_text_a(surface, &text, 30, TEXT, READY_ALPHA, Anchor::TopRight, right, 8);
    let count = draw_text_a(surface, &cores.to_string(), 26, TEXT, READY_ALPHA, Anchor::TopRight, right, timer.bottom() + 4);
    draw_core(surface, vec2((count.left() - 12) as f64, count.centery() as f64), 8.0, CORE_COLOR);
    let Some(stars) = stars else { return };
    let count = draw_text_a(surface, &stars.to_string(), 26, TEXT, READY_ALPHA, Anchor::TopRight, right, count.bottom() + 4);
    draw_star(surface, vec2((count.left() - 12) as f64, count.centery() as f64), 8.0, STAR_COLOR);
    let key = draw_text_a(surface, "T", 22, TEXT, HINT_ALPHA * 2.0, Anchor::TopRight, right, count.bottom() + 8);
    let k = HINT_ALPHA * 2.0 / 255.0;
    let faded = Color::f(py_round(TEXT.r as f64 * k), py_round(TEXT.g as f64 * k), py_round(TEXT.b as f64 * k));
    draw_shop_icon(surface, vec2((key.left() - 14) as f64, key.centery() as f64), faded);
    let Some((standing, allowed, price)) = towers else { return };
    let key = draw_text_a(surface, settings::BUILD_KEY_NAME, 22, TEXT, HINT_ALPHA * 2.0, Anchor::TopRight, right, key.bottom() + 8);
    draw_watchtower_icon(surface, vec2((key.left() - 14) as f64, key.centery() as f64), faded);
    let count = draw_text_a(surface, &format!("{standing}/{allowed}"), 22, TEXT, READY_ALPHA, Anchor::MidRight, key.left() - 28, key.centery());
    if (standing as i64) < allowed {
        let cost = draw_text_a(surface, &price.to_string(), 22, TEXT, READY_ALPHA, Anchor::MidRight, count.left() - 12, count.centery());
        draw_star(surface, vec2((cost.left() - 9) as f64, cost.centery() as f64), 6.0, STAR_COLOR);
    }
}

/// Bottom left: the abilities with their keys, charges and cooldowns, the energy bar, the weapons.
pub fn draw_ability_bar(surface: &mut Surface, ship: &Ship) {
    let bottom = surface.height() - 10;
    let slots: Vec<(&str, &super::abilities::Ability)> = ABILITY_KEY_NAMES.iter().zip(ship.abilities.iter()).filter_map(|(k, a)| a.as_ref().map(|a| (*k, a))).collect();
    let n = slots.len() as i32;
    for (i, (key, a)) in slots.iter().enumerate() {
        let y = bottom - (n - i as i32) * LINE_HEIGHT;
        let mut text = format!("{key}  {}", a.name());
        if a.max_charges > 1.0 {
            text += &format!("  {}/{}", a.charges as i64, a.max_charges as i64);
        }
        if a.charges < a.max_charges {
            text += &format!("  {:.1}s", a.cooldown);
        }
        draw_text_a(surface, &text, 22, TEXT, if a.ready() { READY_ALPHA } else { HINT_ALPHA }, Anchor::TopLeft, 10, y);
    }
    let label = draw_energy(surface, ship, bottom - n * LINE_HEIGHT - 8);
    draw_weapons(surface, ship, label.top() - 6);
}

fn draw_energy(surface: &mut Surface, ship: &Ship, bottom: i32) -> Rect {
    let (w, h) = ENERGY_BAR;
    let bar = Rect::new(10, bottom - h, w, h);
    let empty = ship.weapon().is_some_and(|wp| ship.energy < wp.energy_cost);
    surface.rect_round(BAR_BACKGROUND, bar, 0, Radii::all(3));
    let mut filled = bar;
    filled.w = if ship.max_energy != 0.0 { py_round(w as f64 * ship.energy / ship.max_energy) as i32 } else { 0 };
    if filled.w != 0 {
        surface.rect_round(if empty { EMPTY_COLOR } else { ENERGY_COLOR }, filled, 0, Radii::all(3));
    }
    draw_text_a(surface, &format!("ENERGY  {:.0}", ship.energy), 20, TEXT, READY_ALPHA, Anchor::BottomLeft, bar.left(), bar.top() - 3)
}

fn draw_weapons(surface: &mut Surface, ship: &Ship, bottom: i32) {
    let weapons: Vec<(usize, &super::weapons::Weapon)> = ship.weapons.iter().enumerate().filter_map(|(i, w)| w.as_ref().map(|w| (i, w))).collect();
    let mut x = 10;
    if weapons.len() > 1 {
        x = draw_text_a(surface, WEAPON_SWITCH_KEY_NAME, 22, TEXT, READY_ALPHA, Anchor::BottomLeft, x, bottom).right() + 10;
    }
    let active = ship.weapon_slot();
    for (slot, w) in weapons {
        let is_active = Some(slot) == active;
        x = draw_text_a(surface, w.name(), 22, if is_active { w.color() } else { TEXT }, if is_active { 255.0 } else { HINT_ALPHA }, Anchor::BottomLeft, x, bottom).right() + 12;
    }
}
