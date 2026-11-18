//! Buildings: things the player puts into the world during a run. For now there is one, the watchtower
//! (`buildings/watchtower.py`, `skins/watchtower.py`). Its values come from `content\buildings.json`
//! (`tables::buildings`).
//!
//! B throws a tower towards the mouse; it flies, lands, and for `lifetime` seconds its own window watches there
//! (the scene opens it), enemies in that window take `damage_bonus` percent more damage, and melee enemies nearby go
//! for it instead of the ship.

use std::cell::RefCell;
use std::rc::Rc;

use super::shop_data;
use super::tables::buildings;
use crate::engine::geom::py_round;
use crate::engine::gfx::Radii;
use crate::engine::sprite_cache;
use crate::engine::{Color, FRect, Rect, Surface, Vec2, ViewId, rgb, vec2};

const KEY: &str = "watchtower";

/// On the ship: how many towers it may have standing at once, how far they see, what the next one costs.
#[derive(Debug, Clone, PartialEq)]
pub struct WatchtowerKit {
    pub max_count: i64,
    /// Side length of a tower's window – upgradeable (card "vision").
    pub vision: f64,
    /// Towers thrown in this run: each one costs price_step more.
    pub bought: i64,
}

impl WatchtowerKit {
    pub fn new(max_count: i64) -> WatchtowerKit {
        WatchtowerKit { max_count, vision: buildings::get(KEY, "vision"), bought: 0 }
    }
    /// Stars for the next tower (`shop.json`).
    pub fn price(&self) -> i64 {
        let (price, step) = shop_data::tower();
        py_round(price + step * self.bought as f64) as i64
    }
}

pub type TowerRef = Rc<RefCell<Watchtower>>;

pub struct Watchtower {
    pub values: super::tables::Table,
    pub rect: FRect,
    pub start: Vec2,
    pub target: Vec2,
    /// Seconds in the air so far.
    pub flight: f64,
    /// Seconds since it landed.
    pub standing: f64,
    /// Seconds since it was thrown (for the animation).
    pub age: f64,
    /// Its window – `None` until it lands / if there is none.
    pub view: Option<ViewId>,
    /// Its area on the screen once it has landed (the scene sets it).
    pub area: Option<Rect>,
}

impl Watchtower {
    /// Flies from `start` to `target`, then stands there.
    pub fn new(start: Vec2, target: Vec2) -> Watchtower {
        let values = buildings::section(KEY);
        let size = values.get("size").and_then(|v| v.as_f64()).unwrap_or(24.0);
        let mut rect = FRect::new(0.0, 0.0, size, size);
        rect.set_centerv(start);
        Watchtower { values, rect, start, target, flight: 0.0, standing: 0.0, age: 0.0, view: None, area: None }
    }
    pub fn value(&self, name: &str) -> f64 {
        self.values.get(name).and_then(|v| v.as_f64()).unwrap_or(0.0)
    }
    pub fn landed(&self) -> bool {
        self.flight >= self.value("throw_time")
    }
    /// 0 when thrown … 1 when it lands.
    pub fn progress(&self) -> f64 {
        (self.flight / self.value("throw_time")).min(1.0)
    }
    /// How high up it is in its flight (0 on the ground, 1 at the top).
    pub fn height(&self) -> f64 {
        (std::f64::consts::PI * self.progress()).sin()
    }
    /// Seconds until it is gone (counting from landing).
    pub fn time_left(&self) -> f64 {
        (self.value("lifetime") - self.standing).max(0.0)
    }
    pub fn alive(&self) -> bool {
        self.time_left() > 0.0
    }
    pub fn center(&self) -> Vec2 {
        self.rect.centerv()
    }
    /// True once, in the tick it lands.
    pub fn update(&mut self, dt: f64) -> bool {
        self.age += dt;
        if self.landed() {
            self.standing += dt;
            return false;
        }
        self.flight += dt;
        let p = self.progress();
        let eased = 1.0 - (1.0 - p).powi(2); // fast out of the hand, slowing down before it lands
        self.rect.set_centerv(self.start.lerp(self.target, eased));
        self.landed()
    }
    /// Its window on the screen: vision × vision around it, kept on `area` (the monitor it stands on).
    pub fn vision_rect(&self, vision: f64, area: &Rect) -> Rect {
        let v = py_round(vision);
        let mut r = Rect::new(0.0, 0.0, v, v);
        r.set_center(py_round(self.rect.centerx() as f64), py_round(self.rect.centery() as f64));
        r.clamp(area).clip(area)
    }

    // ---------- how it looks ----------

    /// `rect`: the tower in window coordinates (on the ground – in flight it is drawn higher and bigger).
    pub fn draw(&self, surface: &mut Surface, offset: Vec2) {
        let rect = self.rect.moved(-offset.x, -offset.y).to_i();
        let body = if !self.landed() {
            surface.ellipse(SHADOW, rect.inflate(-rect.w as f64 * 0.2, -rect.h as f64 * 0.4), 0);
            let scale = 1.0 + 0.6 * self.height();
            let mut b = Rect::new(0.0, 0.0, rect.w as f64 * scale, rect.h as f64 * scale);
            b.set_center(rect.centerx(), rect.centery() as f64 - 30.0 * self.height());
            b
        } else {
            rect
        };
        let blink = self.landed() && self.time_left() < BLINK_FROM && (self.time_left() * BLINK_RATE) as i64 % 2 == 1;
        surface.rect_round(if blink { PIP_EMPTY } else { COLOR }, body, 0, Radii::all(4));
        let radius = body.w as f64 * 0.3;
        surface.circle(LENS, body.centerv(), radius, 0);
        if self.landed() {
            let angle = self.age * SWEEP_SPEED;
            let end = vec2(body.centerx() as f64 + angle.cos() * radius, body.centery() as f64 + angle.sin() * radius);
            surface.line(COLOR, body.centerv(), end, 2);
            // a bar above it: the part still filled is the time left
            let mut bar = Rect::new(0, 0, BAR_SIZE.0, BAR_SIZE.1);
            bar.set_midbottom(body.centerx(), body.top() - 4);
            let share = self.time_left() / self.value("lifetime");
            surface.rect(PIP_EMPTY, bar, 0);
            surface.rect(COLOR, Rect::new(bar.left(), bar.top(), py_round(BAR_SIZE.0 as f64 * share), BAR_SIZE.1), 0);
        }
    }
}

pub const COLOR: Color = rgb(120, 180, 255);
const LENS: Color = rgb(12, 24, 48);
/// How fast the radar line turns (radians per second).
const SWEEP_SPEED: f64 = 2.5;
const SHADOW: Color = rgb(30, 30, 36);
const BAR_SIZE: (i32, i32) = (30, 4);
const PIP_EMPTY: Color = rgb(70, 70, 70);
/// Seconds left from which it blinks …
const BLINK_FROM: f64 = 2.0;
/// … this often per second.
const BLINK_RATE: f64 = 6.0;
/// The outline of its area.
const AREA_ALPHA: u8 = 110;

/// Its area (`rect`, in window coordinates): a thin outline – enemies in it take more damage.
pub fn draw_area(surface: &mut Surface, rect: Rect) {
    let (w, h) = rect.size();
    let layer = sprite_cache::get(&format!("watchtower area {w} {h}"), || {
        let mut l = Surface::new_alpha(w, h);
        let r = l.get_rect();
        l.rect(COLOR.with_alpha(AREA_ALPHA), r, 1);
        Some(l)
    });
    if let Some(l) = layer {
        surface.blit(&l, rect.x, rect.y);
    }
}
