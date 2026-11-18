//! The playing field (`field.py`): the arena plus every extra window (watchtower, radar) connected to it –
//! overlapping or touching it, or touching another connected window. It is the standard answer to "where is the
//! field?" for everything:
//!
//!   - the wall is the field's whole outer edge: the arena's edges where no connected window covers them, and the
//!     windows' edges where neither the arena nor another window continues the field,
//!   - shots fly on into a connected window and push the edge they crossed when they leave it (`wall_hit`),
//!   - blasts push every edge they reach (`walls_within`), the mouse may go everywhere in it (`rects`),
//!   - what the player sees (`sight`, `sees`): the arena and every extra window, connected or not, plus lookouts.
//!
//! It is built anew for every question (`PlayScene::field`) – it is only a few rectangles.

use super::arena::{Arena, SIDES};
use super::projectile::Projectile;
use crate::engine::{FRect, Rect, Vec2, vec2};

/// Windows this close count as touching (rounded window positions leave no gap).
const REACH: i32 = 2;

pub struct Field {
    pub arena_rect: FRect,
    pub main: Rect,
    /// The arena, every extra window, the lookouts last.
    pub sight: Vec<Rect>,
    /// Windows that are only for seeing (a Pusher's).
    pub lookouts: Vec<Rect>,
    /// The field: where the mouse may go.
    pub rects: Vec<Rect>,
    /// The connected windows, grown by REACH.
    pub windows: Vec<Rect>,
}

impl Field {
    pub fn new(arena: &Arena, windows: &[Rect], lookouts: &[Rect]) -> Field {
        let main = arena.window_rect();
        let mut sight = vec![main];
        sight.extend(windows.iter().copied());
        let mut rest: Vec<Rect> = windows.to_vec();
        let mut connected: Vec<Rect> = vec![];
        sight.extend(lookouts.iter().copied()); // last in sight
        let mut grown = true;
        while grown {
            // windows connected through other windows count, too
            grown = false;
            let mut i = 0;
            while i < rest.len() {
                let w = rest[i];
                let near = w.inflate(REACH, REACH);
                if std::iter::once(&main).chain(connected.iter()).any(|o| near.colliderect(o)) {
                    connected.push(w);
                    rest.remove(i);
                    grown = true;
                } else {
                    i += 1;
                }
            }
        }
        let mut rects = vec![main];
        rects.extend(connected.iter().copied());
        let windows = connected.iter().map(|w| w.inflate(REACH, REACH)).collect();
        Field { arena_rect: arena.rect, main, sight, lookouts: lookouts.to_vec(), rects, windows }
    }

    fn side_hit(&self, p: Vec2) -> Option<&'static str> {
        let r = self.arena_rect;
        let overshoot = [r.left() as f64 - p.x, p.x - r.right() as f64, r.top() as f64 - p.y, p.y - r.bottom() as f64];
        let mut best = 0;
        for i in 1..4 {
            if overshoot[i] > overshoot[best] {
                best = i;
            }
        }
        (overshoot[best] > 0.0).then_some(SIDES[best])
    }

    pub fn in_window(&self, p: Vec2) -> bool {
        self.windows.iter().any(|w| w.collidev(p))
    }
    pub fn contains(&self, p: Vec2) -> bool {
        self.side_hit(p).is_none() || self.in_window(p)
    }
    /// True if `rect` lies completely in the arena or in one extra window (connected or not).
    pub fn sees(&self, rect: &FRect) -> bool {
        self.arena_rect.contains(rect) || self.sight[1..].iter().any(|w| w.to_f().contains(rect))
    }
    pub fn sees_point(&self, p: Vec2) -> bool {
        self.sight.iter().any(|w| w.collidev(p))
    }
    /// `p` itself if it is in the field, else the nearest point that is.
    pub fn clamp(&self, p: Vec2) -> Vec2 {
        if self.contains(p) {
            return p;
        }
        self.rects
            .iter()
            .map(|r| vec2(p.x.clamp(r.left() as f64, (r.right() - 1) as f64), p.y.clamp(r.top() as f64, (r.bottom() - 1) as f64)))
            .min_by(|a, b| a.distance_squared_to(p).total_cmp(&b.distance_squared_to(p)))
            .unwrap_or(p)
    }

    /// For a shot that has just moved: the arena edge it pushes now – `None` while it is still in the field. Past an
    /// edge into a connected window it flies on (`beyond` remembers which edge it crossed); leaving the field there,
    /// it pushes that edge.
    pub fn wall_hit(&self, shot: &mut Projectile) -> Option<&'static str> {
        let p = shot.center();
        let Some(side) = self.side_hit(p) else {
            shot.beyond = None; // (back) inside the arena
            return None;
        };
        if self.in_window(p) {
            shot.beyond = shot.beyond.or(Some(side));
            return None;
        }
        Some(shot.beyond.unwrap_or(side))
    }

    /// The arena edges a blast at `point` (`radius`) pushes: every edge whose open part it reaches – and `crossed` if
    /// it reaches the wall along a connected window, just like a bullet hitting that wall would.
    pub fn walls_within(&self, point: Vec2, radius: f64, crossed: Option<&'static str>) -> Vec<&'static str> {
        let reaches = |segments: &[(Vec2, Vec2)]| segments.iter().any(|s| distance_to_segment(point, s) <= radius);
        let mut sides: Vec<&'static str> = SIDES.iter().copied().filter(|s| reaches(&self.open_parts(s))).collect();
        if let Some(c) = crossed {
            if !sides.contains(&c) && reaches(&self.window_walls()) {
                sides.push(c);
            }
        }
        sides
    }
    /// The parts of an arena edge that are the field's outer wall.
    fn open_parts(&self, side: &str) -> Vec<(Vec2, Vec2)> {
        let r = self.arena_rect;
        edge_parts((r.left() as f64, r.top() as f64, r.right() as f64, r.bottom() as f64), side, &self.windows)
    }
    /// The field's outer wall along the connected windows.
    fn window_walls(&self) -> Vec<(Vec2, Vec2)> {
        let mut walls = vec![];
        for (i, w) in self.rects.iter().enumerate().skip(1) {
            let others: Vec<Rect> = self.rects.iter().enumerate().filter(|(n, _)| *n != i).map(|(_, r)| r.inflate(REACH, REACH)).collect();
            for side in SIDES {
                walls.extend(edge_parts((w.left() as f64, w.top() as f64, w.right() as f64, w.bottom() as f64), side, &others));
            }
        }
        walls
    }
}

/// One edge of a rectangle (left, top, right, bottom) without the parts lying inside one of `covers`.
fn edge_parts(r: (f64, f64, f64, f64), side: &str, covers: &[Rect]) -> Vec<(Vec2, Vec2)> {
    let (left, top, right, bottom) = r;
    if side == "left" || side == "right" {
        let x = if side == "left" { left } else { right };
        let covered: Vec<(f64, f64)> = covers.iter().filter(|c| (c.left() as f64) < x && x < c.right() as f64).map(|c| (c.top() as f64, c.bottom() as f64)).collect();
        return subtract((top, bottom), &covered).into_iter().map(|(a, b)| (vec2(x, a), vec2(x, b))).collect();
    }
    let y = if side == "top" { top } else { bottom };
    let covered: Vec<(f64, f64)> = covers.iter().filter(|c| (c.top() as f64) < y && y < c.bottom() as f64).map(|c| (c.left() as f64, c.right() as f64)).collect();
    subtract((left, right), &covered).into_iter().map(|(a, b)| (vec2(a, y), vec2(b, y))).collect()
}

/// `interval` without the `cuts` – what is left, as a list of intervals.
fn subtract(interval: (f64, f64), cuts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut parts = vec![interval];
    for &(cs, ce) in cuts {
        parts = parts.into_iter().flat_map(|(s, e)| [(s, e.min(cs)), (s.max(ce), e)]).filter(|p| p.1 > p.0).collect();
    }
    parts
}

/// Distance from `p` to an axis-parallel segment.
fn distance_to_segment(p: Vec2, s: &(Vec2, Vec2)) -> f64 {
    let (a, b) = s;
    let dx = (a.x - p.x).max(0.0).max(p.x - b.x);
    let dy = (a.y - p.y).max(0.0).max(p.y - b.y);
    dx.hypot(dy)
}
