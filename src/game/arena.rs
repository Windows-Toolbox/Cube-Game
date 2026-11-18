//! The arena – the game window itself. It keeps shrinking; shots against its edges push them back out (smoothly,
//! over the next ticks). Once it is as narrow (or as low) as the ship, the ship is crushed.
//!
//! The level sets how fast it shrinks (growing per minute, up to a maximum), its armor (every push is that many
//! pixels shorter) and push falloff (pushes get weaker the further it is out already); phases change them while
//! they last, breathing phases pull the edges in and let them spring back. The edges only go where there is screen.

use std::collections::BTreeMap;

use super::difficulty::{Op, Phase, apply};
use super::settings::{self, reduce};
use super::tables;
use crate::engine::collision::keep_inside;
use crate::engine::desktop::Desktop;
use crate::engine::geom::py_round;
use crate::engine::{FRect, Rect, Surface, Vec2};

pub const SIDES: [&str; 4] = ["left", "right", "top", "bottom"];
/// How exactly the free room in front of an edge is found (halving the step this often).
const ROOM_STEPS: u32 = 20;

pub struct Arena {
    pub desktop: Desktop,
    start_shrink_speed: f64,
    shrink_growth: f64,
    max_shrink_speed: Option<f64>,
    base_armor: f64,
    base_push_falloff: f64,
    /// (value, operation, number) from the active phases – set by the scene.
    pub changes: Vec<(String, Op, f64)>,
    /// The active phases that breathe (by their place in the spawner's list) – `breathe`.
    breathing: BTreeMap<usize, Phase>,
    /// Pixels per second more while it reaches into a storm (`points.rs`).
    pub storm_speed: f64,
    fixed_speed: bool,
    /// The edges stop there – set by the scene from its ship.
    pub ship_size: f64,
    /// Seconds this arena has been shrinking.
    pub elapsed: f64,
    pub rect: FRect,
    /// Pushes are measured against it (push falloff).
    pub start_size: (f32, f32),
    /// How far each edge still has to move out after hits (left, right, top, bottom).
    pub pending: [f64; 4],
}

fn side_index(side: &str) -> usize {
    SIDES.iter().position(|s| *s == side).unwrap_or(0)
}

impl Arena {
    /// It starts in the middle of the main monitor. `size`: at the start. `shrink_speed`: pixels per second each
    /// edge moves in at the start, plus `shrink_growth` per minute, up to `max_shrink_speed`.
    pub fn new(desktop: Desktop, size: (f64, f64), shrink_speed: f64, shrink_growth: f64, max_shrink_speed: Option<f64>, armor: f64, push_falloff: f64) -> Arena {
        let mut rect = FRect::new(0.0, 0.0, size.0, size.1);
        let (cx, cy) = desktop.main.center();
        rect.set_center(cx, cy);
        if !desktop.covers(rect.left() as f64, rect.top() as f64, rect.right() as f64, rect.bottom() as f64) {
            rect = rect.clip(&desktop.main.to_f()); // bigger than the main monitor
        }
        Arena {
            desktop,
            start_shrink_speed: shrink_speed,
            shrink_growth,
            max_shrink_speed,
            base_armor: armor,
            base_push_falloff: push_falloff,
            changes: vec![],
            breathing: BTreeMap::new(),
            storm_speed: 0.0,
            fixed_speed: false,
            ship_size: tables::ships::current().size,
            elapsed: 0.0,
            start_size: (rect.w, rect.h),
            rect,
            pending: [0.0; 4],
        }
    }

    /// The arena as a whole-pixel rectangle for the window. Every edge is rounded on its own, so the opposite edge
    /// does not wobble when one edge moves.
    pub fn window_rect(&self) -> Rect {
        let (left, top) = (py_round(self.rect.left() as f64) as i32, py_round(self.rect.top() as f64) as i32);
        Rect::new(left, top, py_round(self.rect.right() as f64) as i32 - left, py_round(self.rect.bottom() as f64) as i32 - top)
    }

    /// `value` with the active phases' changes to `name` applied, in their order.
    fn changed(&self, name: &str, mut value: f64) -> f64 {
        for (n, op, number) in &self.changes {
            if n == name {
                value = apply(value, *op, *number);
            }
        }
        value
    }

    /// Pixels per second each edge moves in right now.
    pub fn shrink_speed(&self) -> f64 {
        let mut speed = self.start_shrink_speed + self.shrink_growth * self.elapsed / 60.0;
        if let Some(max) = self.max_shrink_speed {
            speed = speed.min(max);
        }
        if self.fixed_speed { speed } else { self.changed("shrink_speed", speed).max(0.0) }
    }
    /// Pixels every push of an edge is shorter right now.
    pub fn armor(&self) -> f64 {
        self.changed("armor", self.base_armor).max(0.0)
    }
    pub fn push_falloff(&self) -> f64 {
        self.changed("push_falloff", self.base_push_falloff).max(0.0)
    }
    /// What a push on `side` is multiplied by: (start size ÷ size now) ^ push_falloff on that side's axis.
    pub fn push_factor(&self, side: &str) -> f64 {
        let (start, now) = if side == "left" || side == "right" { (self.start_size.0, self.rect.w) } else { (self.start_size.1, self.rect.h) };
        (start as f64 / (now as f64).max(1.0)).powf(self.push_falloff())
    }

    pub fn update(&mut self, dt: f64) {
        self.elapsed += dt;
        let breath: f64 = self.breathing.values().map(|p| p.breathe / (p.end.unwrap_or(p.start + 1.0) - p.start)).sum();
        self.squeeze((self.shrink_speed() + breath + self.storm_speed) * dt);
        self.expand(dt);
    }

    /// `phases`: the active phases (by their place in the spawner's list, set by the scene every tick). Those with
    /// breathe pull the edges in while they last; one that has ended gives its pixels back.
    pub fn breathe(&mut self, phases: Vec<(usize, Phase)>) {
        let now: BTreeMap<usize, Phase> = phases.into_iter().filter(|(_, p)| p.breathe != 0.0).collect();
        for (key, phase) in &self.breathing {
            if !now.contains_key(key) {
                for p in &mut self.pending {
                    *p += phase.breathe;
                }
            }
        }
        self.breathing = now;
    }

    /// Moves every edge inwards by `distance`. It stops at the ship's size: then the ship is crushed.
    pub fn squeeze(&mut self, distance: f64) {
        let smallest = self.ship_size;
        let step_x = distance.min(((self.rect.w as f64 - smallest) / 2.0).max(0.0));
        let step_y = distance.min(((self.rect.h as f64 - smallest) / 2.0).max(0.0));
        self.rect.inflate_ip(-2.0 * step_x, -2.0 * step_y); // inflate keeps the centre in place
    }

    /// The ship (at `ship`) was hit: the arena gets 2 × distance narrower and lower – the edges away from it move
    /// in more than the near ones. Stops at the ship's size, like `squeeze`.
    pub fn hit(&mut self, distance: f64, ship: Vec2) {
        let smallest = self.ship_size;
        for horizontal in [true, false] {
            let (size, position) = if horizontal { (self.rect.w as f64, ship.x - self.rect.left() as f64) } else { (self.rect.h as f64, ship.y - self.rect.top() as f64) };
            let total = (2.0 * distance).min((size - smallest).max(0.0));
            let share = if size != 0.0 { (position / size).clamp(0.0, 1.0) } else { 0.5 }; // 0 at the low edge … 1 at the high one
            let near_low = 0.5 + settings::HIT_SIDE_BIAS * (share - 0.5); // the low edge's part of total
            let r = self.rect;
            self.rect = if horizontal {
                FRect::new(r.left() as f64 + total * near_low, r.top(), r.w as f64 - total, r.h)
            } else {
                FRect::new(r.left(), r.top() as f64 + total * near_low, r.w, r.h as f64 - total)
            };
        }
    }

    fn covers(&self, r: &FRect) -> bool {
        self.desktop.covers(r.left() as f64, r.top() as f64, r.right() as f64, r.bottom() as f64)
    }

    fn expand(&mut self, dt: f64) {
        for (i, side) in SIDES.iter().enumerate() {
            let mut step = self.pending[i].min(settings::PUSH_SPEED * dt);
            if step <= 0.0 {
                continue;
            }
            self.pending[i] -= step;
            if !self.covers(&self.widened(side, step)) {
                step = self.room(side, step); // only up to the end of the screen
            }
            self.rect = self.widened(side, step);
        }
    }

    /// Moves the whole window `distance` pixels towards `side`. As far as the screen goes – the rest squeezes: the
    /// edge at the end of the screen stays, the opposite one moves in (not below the ship).
    pub fn shove(&mut self, side: &str, distance: f64) {
        let d = match side {
            "left" => (-1.0, 0.0),
            "right" => (1.0, 0.0),
            "top" => (0.0, -1.0),
            _ => (0.0, 1.0),
        };
        let moved = |r: &FRect, step: f64| r.moved(d.0 * step, d.1 * step);
        let mut step = distance;
        if !self.covers(&moved(&self.rect, step)) {
            let (mut fits, mut too_far) = (0.0, step);
            for _ in 0..ROOM_STEPS {
                let middle = (fits + too_far) / 2.0;
                if self.covers(&moved(&self.rect, middle)) {
                    fits = middle;
                } else {
                    too_far = middle;
                }
            }
            step = fits;
        }
        self.rect = moved(&self.rect, step);
        let rest = distance - step;
        if rest <= 0.0 {
            return;
        }
        let horizontal = side == "left" || side == "right";
        let rest = rest.min(((if horizontal { self.rect.w } else { self.rect.h }) as f64 - self.ship_size).max(0.0));
        let r = self.rect;
        match side {
            "right" => self.rect = FRect::new(r.left() as f64 + rest, r.top(), r.w as f64 - rest, r.h),
            "left" => self.rect.w = (r.w as f64 - rest) as f32,
            "bottom" => self.rect = FRect::new(r.left(), r.top() as f64 + rest, r.w, r.h as f64 - rest),
            _ => self.rect.h = (r.h as f64 - rest) as f32,
        }
    }

    /// The arena with one edge moved out by `step`.
    pub fn widened(&self, side: &str, step: f64) -> FRect {
        let r = self.rect;
        match side {
            "left" => FRect::new(r.left() as f64 - step, r.top(), r.w as f64 + step, r.h),
            "right" => FRect { w: (r.w as f64 + step) as f32, ..r },
            "top" => FRect::new(r.left(), r.top() as f64 - step, r.w, r.h as f64 + step),
            _ => FRect { h: (r.h as f64 + step) as f32, ..r },
        }
    }

    /// How far (at most `step`) the edge can move out and still be completely on screen – found by halving.
    fn room(&self, side: &str, step: f64) -> f64 {
        let (mut fits, mut too_far) = (0.0, step);
        for _ in 0..ROOM_STEPS {
            let middle = (fits + too_far) / 2.0;
            if self.covers(&self.widened(side, middle)) {
                fits = middle;
            } else {
                too_far = middle;
            }
        }
        fits
    }

    /// Hit on an edge: it moves out by `distance` times the push factor, minus the armor (smoothly).
    pub fn push(&mut self, side: &str, distance: f64) {
        if distance > 0.0 {
            let distance = distance * self.push_factor(side);
            self.pending[side_index(side)] += reduce(distance, self.armor(), settings::MIN_PUSH_SHARE);
        }
    }
    /// Every edge springs out by `distance` (no armor, no falloff) – the repair kit.
    pub fn spring_out(&mut self, distance: f64) {
        for p in &mut self.pending {
            *p += distance;
        }
    }

    /// Which edge `point` has crossed – or `None` while it is inside. Past a corner, the edge it is further beyond
    /// wins.
    pub fn side_hit(&self, point: Vec2) -> Option<&'static str> {
        let r = self.rect;
        let overshoot = [r.left() as f64 - point.x, point.x - r.right() as f64, r.top() as f64 - point.y, point.y - r.bottom() as f64];
        let mut best = 0;
        for i in 1..4 {
            if overshoot[i] > overshoot[best] {
                best = i;
            }
        }
        (overshoot[best] > 0.0).then_some(SIDES[best])
    }

    /// True if two opposite edges touch `rect` at once – the arena is no wider or no higher than it.
    pub fn crushed(&self, rect: &FRect) -> bool {
        self.rect.w as f64 <= rect.w as f64 + 0.01 || self.rect.h as f64 <= rect.h as f64 + 0.01
    }
    /// Keeps `rect` inside the arena (shrinking edges push it along). True if it touched an edge.
    pub fn contain(&self, rect: &mut FRect) -> bool {
        keep_inside(rect, &self.rect)
    }

    /// Background and an outline along the edges (the arena fills the whole window).
    pub fn draw(&self, surface: &mut Surface, offset: Vec2) {
        surface.fill(settings::BACKGROUND);
        let r = self.window_rect().moved(-offset.x, -offset.y);
        surface.rect(settings::BORDER, r, settings::BORDER_WIDTH);
    }
}
