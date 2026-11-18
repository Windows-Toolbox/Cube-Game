//! The radar (`radar.py`, skill tree "Radar"): every `cooldown` seconds it looks for the biggest group of enemies
//! outside what you can see (the arena and the extra windows) and opens a light blue window over it for `duration`
//! seconds – size × size pixels, without a frame. If there is no enemy out of sight (on a monitor), it looks again
//! every RETRY seconds.
//!
//! Finding the group: the enemies out of sight are sorted into a grid of size × size cells; the fullest cell wins,
//! the window goes over the middle of the enemies around it.
//!
//! With "Find" (`find_chance`), a scan goes to a point of interest nobody has seen yet now and then instead.

use std::collections::HashMap;

use super::tables::radar as data;
use crate::engine::desktop::Desktop;
use crate::engine::geom::py_round;
use crate::engine::{Rect, Vec2, ViewId};
use crate::rng;

/// Seconds until it looks again when there was nothing to scan.
const RETRY: f64 = 1.0;

/// What `update` reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scan {
    Open,
    Close,
}

#[derive(Debug, Clone)]
pub struct Radar {
    pub find_chance: f64,
    pub size: i32,
    pub cooldown: f64,
    pub duration: f64,
    /// Seconds until the next scan.
    pub timer: f64,
    /// The scanned area while a scan is open (screen pixels).
    pub rect: Option<Rect>,
    pub remaining: f64,
    /// Its window (set by the scene).
    pub view: Option<ViewId>,
}

impl Radar {
    /// What the skill tree's radar upgrades add to the base values, and how likely a scan goes to a hidden point.
    pub fn new(more_size: f64, more_cooldown: f64, more_duration: f64, find_chance: f64) -> Radar {
        let cooldown = data::get("min_cooldown").max(data::get("cooldown") + more_cooldown);
        Radar {
            find_chance,
            size: (py_round(data::get("size") + more_size) as i32).max(1),
            cooldown,
            duration: (data::get("duration") + more_duration).max(0.1),
            timer: cooldown,
            rect: None,
            remaining: 0.0,
            view: None,
        }
    }

    /// `enemies`: their centres. `seen`: the rectangles the player already sees. `hidden_points`: where the points of
    /// interest nobody has seen yet are.
    pub fn update(&mut self, dt: f64, enemies: &[Vec2], seen: &[Rect], desktop: &Desktop, hidden_points: &[Vec2]) -> Option<Scan> {
        if self.rect.is_some() {
            self.remaining -= dt;
            if self.remaining > 0.0 {
                return None;
            }
            self.rect = None;
            self.timer = self.cooldown;
            return Some(Scan::Close);
        }
        self.timer -= dt;
        if self.timer > 0.0 {
            return None;
        }
        self.rect = if !hidden_points.is_empty() && rng::random() < self.find_chance {
            let i = rng::index(hidden_points.len()).unwrap();
            Some(self.over(hidden_points[i], desktop))
        } else {
            self.find_group(enemies, seen, desktop)
        };
        if self.rect.is_none() {
            self.timer = RETRY;
            return None;
        }
        self.remaining = self.duration;
        Some(Scan::Open)
    }

    /// The window centred on `p` (kept on its monitor).
    fn over(&self, p: Vec2, desktop: &Desktop) -> Rect {
        let mut r = Rect::new(0, 0, self.size, self.size);
        r.set_center(py_round(p.x), py_round(p.y));
        let area = desktop.area_at(r.centerv());
        r.clamp(&area).clip(&area)
    }

    /// The window over the biggest group of enemies out of sight – `None` if there is none.
    fn find_group(&self, enemies: &[Vec2], seen: &[Rect], desktop: &Desktop) -> Option<Rect> {
        let hidden: Vec<Vec2> = enemies.iter().copied().filter(|c| desktop.contains(c.x, c.y) && !seen.iter().any(|r| r.collidev(*c))).collect();
        if hidden.is_empty() {
            return None;
        }
        let size = self.size as f64;
        let mut order: Vec<(i64, i64)> = vec![];
        let mut cells: HashMap<(i64, i64), Vec<Vec2>> = HashMap::new();
        for p in &hidden {
            let cell = ((p.x / size).floor() as i64, (p.y / size).floor() as i64);
            cells.entry(cell).or_insert_with(|| {
                order.push(cell);
                vec![]
            }).push(*p);
        }
        // the fullest cell – the first of equally full ones, as Python's max() does
        let mut best = &cells[&order[0]];
        for c in &order[1..] {
            if cells[c].len() > best.len() {
                best = &cells[c];
            }
        }
        let middle = |g: &[Vec2]| g.iter().fold(Vec2::ZERO, |a, p| a + *p) / g.len() as f64;
        let m = middle(best);
        let mut r = Rect::new(0, 0, self.size, self.size);
        r.set_center(py_round(m.x), py_round(m.y));
        let around: Vec<Vec2> = hidden.iter().copied().filter(|p| r.collidev(*p)).collect(); // also those across the cell edge
        let group = if around.is_empty() { best.clone() } else { around };
        let m = middle(&group);
        r.set_center(py_round(m.x), py_round(m.y));
        let area = desktop.area_at(r.centerv());
        Some(r.clamp(&area).clip(&area))
    }
}
