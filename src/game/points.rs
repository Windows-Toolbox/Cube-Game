//! Points of interest (`points.py`): things that appear on the desktop, out of sight. Until someone has seen one it
//! is only a "?" – a marker at the window's edge shows where it is. Once it is in sight (the arena, a watchtower's
//! window, a radar scan), it is revealed.
//!
//! The ship cannot leave the arena, so a point is reached when the arena or a standing watchtower's window covers it.
//!
//! Finds (skill tree "Points of interest"): chest (stars), magnet (every star and core flies to the ship for a
//! while), repair (every edge springs out). Threats (a level phase's "points"): totem (every enemy faster and
//! harder-hitting while it stands – reach it to break it), nest (keeps sending out enemies), storm (while the arena
//! reaches into it, the walls move in faster; it cannot be broken). Every point vanishes after its lifetime.

use super::arena::Arena;
use super::effects::Explosion;
use super::enemies::{DAMAGE_STATS, EnemyRef, type_of};
use super::field::Field;
use super::pickups::Pickup;
use super::roster::{Buff, RosterRef};
use super::shapes::{STAR_COLOR, draw_star, shade};
use super::spawner::{Spawner, distance_to_rect};
use super::tables::points as stats;
use crate::engine::desktop::Desktop;
use crate::engine::geom::py_round;
use crate::engine::gfx::Radii;
use crate::engine::sprite_cache;
use crate::engine::ui::draw_text;
use crate::engine::{Anchor, Color, Rect, Surface, Vec2, rgb, rgba, vec2};
use crate::pyfmt::g;
use crate::rng;

pub const FINDS: [&str; 3] = ["chest", "magnet", "repair"];
pub const THREATS: [&str; 3] = ["totem", "nest", "storm"];
pub const KINDS: [&str; 6] = ["chest", "magnet", "repair", "totem", "nest", "storm"];

pub fn name(kind: &str) -> &'static str {
    match kind {
        "chest" => "Star chest",
        "magnet" => "Magnet",
        "repair" => "Repair kit",
        "totem" => "Totem",
        "nest" => "Nest",
        _ => "Storm",
    }
}

/// A point's square (pixels).
const SIZE: f64 = 30.0;
const PLACE_TRIES: usize = 30;
pub const FIND_COLOR: Color = rgb(110, 220, 140);
pub const THREAT_COLOR: Color = rgb(235, 70, 70);
const HIDDEN_COLOR: Color = rgb(235, 235, 235);
/// Also the outline of the enemies it buffs.
pub const TOTEM_COLOR: Color = rgb(170, 90, 255);
const STORM_FILL: Color = rgba(70, 110, 190, 70);
const STORM_RIM: Color = rgb(140, 185, 255);
/// The arrows at the window's edge: distance from it …
const MARKER_INSET: i32 = 10;
/// … and their size.
const ARROW: f64 = 9.0;

/// One point of interest in the world.
#[derive(Debug, Clone)]
pub struct Point {
    pub kind: &'static str,
    pub position: Vec2,
    pub lifetime: f64,
    pub age: f64,
    pub revealed: bool,
    pub done: bool,
    /// The nest's next enemy.
    pub timer: f64,
    /// The totem's, on every enemy.
    pub buffs: Vec<Buff>,
}

impl Point {
    pub fn new(kind: &str, position: Vec2) -> Point {
        let kind = KINDS.iter().find(|k| **k == kind).copied().unwrap_or("chest");
        let lifetime = if THREATS.contains(&kind) { stats::get(kind, "lifetime") } else { stats::get("finds", "lifetime") };
        Point { kind, position, lifetime, age: 0.0, revealed: false, done: false, timer: if kind == "nest" { stats::get("nest", "interval") } else { 0.0 }, buffs: vec![] }
    }
    pub fn threat(&self) -> bool {
        THREATS.contains(&self.kind)
    }
    pub fn rect(&self) -> Rect {
        let mut r = Rect::new(0.0, 0.0, SIZE, SIZE);
        r.set_centerv(self.position);
        r
    }
    /// Seen in one of `rects`: its middle – the storm as soon as its area reaches into one.
    pub fn in_sight(&self, rects: &[Rect]) -> bool {
        if self.kind == "storm" {
            return rects.iter().any(|r| distance_to_rect(self.position, r) < self.radius());
        }
        rects.iter().any(|r| r.collidev(self.position))
    }
    /// How far it reaches: the storm's area, else just its square.
    pub fn radius(&self) -> f64 {
        if self.kind == "storm" { stats::get("storm", "radius") } else { SIZE / 2.0 }
    }
    pub fn draw(&self, surface: &mut Surface, offset: Vec2) {
        let center = self.position - offset;
        let left = 1.0 - self.age / self.lifetime;
        if self.kind == "storm" {
            draw_storm_area(surface, center, self.radius(), self.age);
        }
        let color = if self.threat() { THREAT_COLOR } else { FIND_COLOR };
        let half_pi = std::f64::consts::FRAC_PI_2;
        surface.arc(color, Rect::new(center.x - SIZE * 0.8, center.y - SIZE * 0.8, SIZE * 1.6, SIZE * 1.6), half_pi, half_pi + std::f64::consts::TAU * left, 2);
        draw_point_icon(surface, center, self.kind, SIZE);
    }
    fn explosion(&self) -> Explosion {
        Explosion::new(self.position, SIZE * 2.0, if self.threat() { THREAT_COLOR } else { FIND_COLOR })
    }
}

/// What the points may look at and change in the run.
pub struct PointsCtx<'a> {
    pub arena: &'a mut Arena,
    pub spawner: &'a mut Spawner,
    pub roster: &'a RosterRef,
    pub field: &'a Field,
    pub desktop: &'a Desktop,
    /// The radar's open scan.
    pub scanned: Option<Rect>,
    /// The landed watchtowers' areas.
    pub tower_areas: Vec<Rect>,
    pub ship_center: Vec2,
    pub stars: &'a mut i64,
    pub pickups: &'a mut Vec<Pickup>,
    /// Texts for the bottom of the screen.
    pub notes: Vec<String>,
    pub explosions: Vec<Explosion>,
    pub new_enemies: Vec<EnemyRef>,
}

/// The run's points of interest: new finds now and then (from the skill tree), the level's threats, what they do,
/// and their markers.
pub struct Points {
    /// (kinds, interval) – `None` without the skill tree node.
    pub finds: Option<(Vec<String>, f64)>,
    pub points: Vec<Point>,
    timer: f64,
    /// Seconds the magnet still pulls.
    pub magnet: f64,
}

impl Points {
    pub fn new(finds: Option<(Vec<String>, f64)>) -> Points {
        Points { finds, points: vec![], timer: stats::get("finds", "first"), magnet: 0.0 }
    }

    pub fn update(&mut self, dt: f64, ctx: &mut PointsCtx) {
        if let Some((kinds, interval)) = self.finds.clone().filter(|(k, _)| !k.is_empty()) {
            self.timer -= dt;
            if self.timer <= 0.0 {
                self.timer = stats::get("finds", "min_interval").max(interval);
                let kind = kinds[rng::index(kinds.len()).unwrap()].clone();
                self.place(&kind, ctx);
            }
        }
        for kind in std::mem::take(&mut ctx.spawner.points) {
            self.place(&kind, ctx);
        }

        let arena_rect = ctx.arena.rect;
        let mut reach: Vec<Rect> = vec![arena_rect.to_i()];
        reach.extend(ctx.tower_areas.iter().copied());
        let mut storm = 0.0;
        for i in 0..self.points.len() {
            self.points[i].age += dt;
            let p = &self.points[i];
            if !p.revealed && (p.in_sight(&ctx.field.sight) || ctx.scanned.is_some_and(|s| p.in_sight(&[s]))) {
                self.points[i].revealed = true;
            }
            let p = self.points[i].clone();
            if p.kind == "storm" {
                if distance_to_rect(p.position, &arena_rect.to_i()) < p.radius() {
                    storm += stats::get("storm", "shrink_speed");
                }
            } else if reach_any(&reach, &arena_rect, p.position) {
                self.points[i].revealed = true;
                self.reach(i, ctx);
            } else if p.kind == "nest" {
                self.points[i].timer -= dt;
                if self.points[i].timer <= 0.0 {
                    self.points[i].timer = stats::get("nest", "interval");
                    hatch(&p, ctx);
                }
            }
            if self.points[i].age >= self.points[i].lifetime {
                self.points[i].done = true;
            }
        }
        ctx.arena.storm_speed = storm;
        for p in self.points.iter().filter(|p| p.done && !p.buffs.is_empty()) {
            ctx.roster.borrow_mut().buffs.retain(|b| !p.buffs.contains(b));
        }
        self.points.retain(|p| !p.done);

        self.magnet = (self.magnet - dt).max(0.0);
        if self.magnet > 0.0 {
            let step = stats::get("magnet", "speed") * dt;
            for pickup in ctx.pickups.iter_mut() {
                let to_ship = ctx.ship_center - pickup.rect.centerv();
                if to_ship.length() > 1.0 {
                    let d = to_ship.normalize() * step.min(to_ship.length());
                    pickup.rect.move_ip(d.x, d.y);
                }
            }
        }
    }

    /// A new point of `kind` somewhere out of sight, min_distance … max_distance beyond the arena, on a monitor.
    pub fn place(&mut self, kind: &str, ctx: &mut PointsCtx) -> bool {
        let arena = ctx.arena.rect;
        let center = arena.centerv();
        let reach = vec2(arena.w as f64, arena.h as f64).length() / 2.0;
        let low = stats::get("place", "min_distance");
        let high = low.max(stats::get("place", "max_distance"));
        for _ in 0..PLACE_TRIES {
            let spot = center + vec2(reach + rng::uniform(low, high), 0.0).rotate(rng::uniform(0.0, 360.0));
            let mut area = Rect::new(0.0, 0.0, SIZE, SIZE);
            area.set_centerv(spot);
            if ctx.desktop.covers_rect(&area) && !ctx.field.sight.iter().any(|r| r.colliderect(&area)) {
                let mut point = Point::new(kind, spot);
                if kind == "totem" {
                    let (speed, damage) = (stats::get("totem", "speed"), stats::get("totem", "damage"));
                    point.buffs.push(Buff { factor: speed, source: Some("totem".into()), color: Some(TOTEM_COLOR), ..Buff::new("speed") });
                    for stat in DAMAGE_STATS {
                        point.buffs.push(Buff { factor: damage, source: Some("totem".into()), ..Buff::new(stat) });
                    }
                    ctx.roster.borrow_mut().buffs.extend(point.buffs.iter().cloned());
                }
                self.points.push(point);
                return true;
            }
        }
        false // no room out of sight (the arena fills the screen) – no point this time
    }

    /// The arena or a watchtower got to it: a find does its thing, a totem or nest breaks.
    fn reach(&mut self, i: usize, ctx: &mut PointsCtx) {
        self.points[i].done = true;
        let p = self.points[i].clone();
        let name = name(p.kind);
        match p.kind {
            "chest" => {
                let stars = stats::get("chest", "stars");
                *ctx.stars += stars as i64;
                ctx.notes.push(format!("{name}: +{} stars", g(stars)));
            }
            "magnet" => {
                self.magnet = stats::get("magnet", "time");
                ctx.notes.push(format!("{name}: everything flies to you"));
            }
            "repair" => {
                ctx.arena.spring_out(stats::get("repair", "push"));
                ctx.notes.push(format!("{name}: the walls spring out"));
            }
            _ => ctx.notes.push(format!("{name} destroyed")),
        }
        ctx.explosions.push(p.explosion());
    }

    pub fn draw(&self, surface: &mut Surface, offset: Vec2, area: Rect) {
        for p in &self.points {
            let r = p.radius();
            if p.rect().inflate(2.0 * r, 2.0 * r).colliderect(&area) {
                p.draw(surface, offset);
            }
        }
    }

    /// An arrow at the window's edge for every point out of sight, pointing at it – with a "?" until it is revealed,
    /// then its icon (green: a find, red: a threat). A storm reaching into the window needs none.
    pub fn draw_markers(&self, surface: &mut Surface, offset: Vec2, field: &Field) {
        let inside = surface.get_rect().inflate(-2 * MARKER_INSET, -2 * MARKER_INSET);
        let middle = inside.centerv();
        for p in &self.points {
            if p.in_sight(&field.sight) {
                continue;
            }
            let position = p.position - offset;
            let spot = vec2(position.x.clamp(inside.left() as f64, inside.right() as f64), position.y.clamp(inside.top() as f64, inside.bottom() as f64));
            let d = position - spot;
            let direction = if d.length_squared() > 0.0 { d.normalize() } else { (spot - middle).normalize() };
            let color = if !p.revealed { HIDDEN_COLOR } else if p.threat() { THREAT_COLOR } else { FIND_COLOR };
            let tip = spot + direction * ARROW;
            let side = direction.rotate(90.0) * ARROW * 0.7;
            surface.polygon(color, &[tip, spot - side, spot + side], 0);
            let label = spot - direction * (ARROW + 8.0);
            if p.revealed {
                draw_point_icon(surface, label, p.kind, 16.0);
            } else {
                draw_text(surface, "?", 24, color, Anchor::Center, label.x, label.y);
            }
        }
    }
}

/// Is `point` covered by the arena (as floats) or one of the watchtowers' areas?
fn reach_any(reach: &[Rect], arena: &crate::engine::FRect, point: Vec2) -> bool {
    arena.collidev(point) || reach[1..].iter().any(|r| r.collidev(point))
}

/// The nest sends out one enemy – one of the types the level brings right now.
fn hatch(p: &Point, ctx: &mut PointsCtx) {
    let weights = ctx.spawner.weights();
    if weights.is_empty() {
        return;
    }
    let w: Vec<f64> = weights.iter().map(|(_, v)| *v).collect();
    let mut key = weights[rng::weighted(&w).unwrap_or(0)].0.clone();
    if type_of(&key).is_some_and(|t| t.spawns_outside) {
        key = "basic".into(); // a Pusher has to come from beyond the map
    }
    ctx.new_enemies.push(ctx.spawner.create(&key, p.position));
}

// ---------- looks ----------

/// The storm: a blue-grey disc with a pulsing rim and turning arcs.
fn draw_storm_area(surface: &mut Surface, center: Vec2, radius: f64, age: f64) {
    let r = py_round(radius) as i32;
    if let Some(disc) = sprite_cache::get(&format!("storm {r}"), || {
        let mut d = Surface::new_alpha(2 * r, 2 * r);
        d.circle(STORM_FILL, vec2(r as f64, r as f64), r as f64, 0);
        Some(d)
    }) {
        surface.blit(&disc, (center.x - disc.width() as f64 / 2.0) as i32, (center.y - disc.height() as f64 / 2.0) as i32);
    }
    let pulse = 0.6 + 0.4 * (age * 4.0).sin();
    surface.circle(shade(STORM_RIM, pulse), center, radius, 3);
    for i in 0..5 {
        let start = age * (1.2 + 0.25 * i as f64) + i as f64 * 1.3;
        let reach = radius * (0.3 + 0.14 * i as f64);
        surface.arc(STORM_RIM, Rect::new(center.x - reach, center.y - reach, 2.0 * reach, 2.0 * reach), start, start + 1.4, 2);
    }
}

/// A point's icon, `size` pixels across.
pub fn draw_point_icon(surface: &mut Surface, center: Vec2, kind: &str, size: f64) {
    let (x, y, half) = (center.x, center.y, size / 2.0);
    match kind {
        "chest" => {
            let mut b = Rect::new(0.0, 0.0, size, size * 0.7);
            b.set_center(x, y + size * 0.1);
            surface.rect_round(rgb(150, 95, 45), b, 0, Radii::all(3));
            let ly = b.top() as f64 + b.h as f64 * 0.35;
            surface.line(rgb(90, 55, 25), vec2(b.left() as f64, ly), vec2((b.right() - 1) as f64, ly), 2);
            draw_star(surface, vec2(x, y + size * 0.1), size * 0.25, STAR_COLOR);
        }
        "magnet" => {
            let width = (py_round(size * 0.22) as i32).max(3);
            let bow = Rect::new(x - half * 0.8, y - half * 0.8, size * 0.8, size * 0.8);
            surface.arc(rgb(220, 50, 50), bow, std::f64::consts::PI, std::f64::consts::TAU, width);
            for edge in [bow.left() as f64 + width as f64 / 2.0, bow.right() as f64 - width as f64 / 2.0] {
                surface.line(rgb(220, 50, 50), vec2(edge, y), vec2(edge, y - half * 0.6), width);
                surface.line(rgb(230, 230, 230), vec2(edge, y - half * 0.6), vec2(edge, y - half * 0.85), width);
            }
        }
        "repair" => {
            let thick = py_round(size * 0.28).max(3.0);
            surface.rect(FIND_COLOR, Rect::new(x - half * 0.8, y - thick / 2.0, size * 0.8, thick), 0);
            surface.rect(FIND_COLOR, Rect::new(x - thick / 2.0, y - half * 0.8, thick, size * 0.8), 0);
        }
        "totem" => {
            let mut pole = Rect::new(0.0, 0.0, size * 0.45, size);
            pole.set_centerv(center);
            surface.rect_round(TOTEM_COLOR, pole, 0, Radii::all(3));
            surface.circle(rgb(30, 10, 50), vec2(x, y - size * 0.18), size * 0.12, 0);
            surface.circle(rgb(255, 255, 255), vec2(x, y - size * 0.18), (size * 0.05).max(1.0), 0);
        }
        "nest" => {
            surface.circle(rgb(120, 30, 30), center, half * 0.9, 0);
            for i in 0..5 {
                let spot = center + vec2(half * 0.45, 0.0).rotate(i as f64 * 72.0);
                surface.circle(THREAT_COLOR, spot, (size * 0.09).max(1.5), 0);
            }
        }
        _ => {
            for i in 0..3 {
                let reach = half * (0.35 + 0.3 * i as f64);
                surface.arc(rgb(170, 180, 205), Rect::new(x - reach, y - reach, 2.0 * reach, 2.0 * reach), i as f64 * 2.1, i as f64 * 2.1 + 3.6, 2);
            }
        }
    }
}

