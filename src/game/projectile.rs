//! Shots – `projectile.py`, `shot_stats.py`, `rocket_projectile.py`, `mine.py`, the laser's bolts
//! (`weapons/laser.py`) and the Cannoneer's ball (`enemies/cannoneer.py`).
//!
//! A shot flies until it leaves the arena (and pushes that edge out) – or, with a target, until it reaches it.
//! Shots with a blast radius explode at their end (`PlayScene::explode`). Everything comes from the weapon or
//! ability that fired it.
//!
//! Fast shots can jump past a small enemy within one tick, so hits are checked along the whole way it flew this tick
//! (previous → now, `touches`). Piercing shots remember who they went through (`pierced`).
//!
//! Two kinds of values: `stats` – the snapshot at the moment of firing, never changes; `direction`, `base_speed`,
//! `speed_factor`, `target` – the shot's own state in flight, which effects change (`redirect`, `slow` for the
//! current tick only).

use crate::engine::geom::py_round;
use crate::engine::{Color, FRect, Surface, Vec2, rgb, vec2};
use crate::rng;

/// Snapshot of a weapon's (or ability's) shot values at the moment a shot is fired.
#[derive(Debug, Clone, PartialEq)]
pub struct ShotStats {
    pub damage: f64,
    /// Pixels per second at the start.
    pub speed: f64,
    /// Pixels a hit edge moves out.
    pub wall_push: f64,
    /// Diameter in pixels.
    pub size: f64,
    pub color: Color,
    /// Pixels per second², the shot gets faster the longer it flies.
    pub acceleration: f64,
    /// > 0: explodes at its end – damage and wall push for everything within this radius.
    pub blast_radius: f64,
    /// Flies on through this many enemies (hurting each), stops at the next one.
    pub pierce: i64,
    /// Wall push × this for every enemy it went through.
    pub pierce_push: f64,
    /// > 0 (with a blast): leaves burning ground behind for this many seconds …
    pub burn_time: f64,
    /// … that does this much damage per second to enemies in it …
    pub burn_damage: f64,
    /// … within this radius.
    pub burn_radius: f64,
    /// > 0 (with a blast): leaves a slowing field behind – enemies in it this share slower …
    pub slow: f64,
    /// … within this radius …
    pub slow_radius: f64,
    /// … for this many seconds.
    pub slow_time: f64,
}

impl ShotStats {
    pub fn new(damage: f64, speed: f64, wall_push: f64, size: f64, color: Color) -> ShotStats {
        ShotStats { damage, speed, wall_push, size, color, acceleration: 0.0, blast_radius: 0.0, pierce: 0, pierce_push: 1.0, burn_time: 0.0, burn_damage: 0.0, burn_radius: 0.0, slow: 0.0, slow_radius: 0.0, slow_time: 0.0 }
    }
}

/// What kind of shot it is – only how it looks and a few extras differ.
#[derive(Debug, Clone, PartialEq)]
pub enum Look {
    /// A filled circle.
    Plain,
    /// The Cannoneer's: a big dark ball with a bright ring.
    CannonBall,
    /// A laser bolt: a line reaching back `trail` pixels – never behind `origin`, where it was fired.
    LaserBolt { origin: Vec2, trail: f64 },
    /// The rocket: a body with fins and a flickering flame.
    Rocket,
    /// A mine: lies still and blinks.
    Mine { age: f64 },
}

#[derive(Debug, Clone)]
pub struct Projectile {
    pub rect: FRect,
    pub stats: ShotStats,
    pub direction: Vec2,
    pub base_speed: f64,
    /// 1 = normal speed; reset after every tick.
    pub speed_factor: f64,
    /// Where the shot ends (a rocket's explosion) – or `None`.
    pub target: Option<Vec2>,
    /// True once it has reached its target.
    pub arrived: bool,
    /// Flying through a watchtower's window: the arena edge it crossed.
    pub beyond: Option<&'static str>,
    /// Where it was before this tick's move.
    pub previous: Vec2,
    /// Ids of the enemies it went through (piercing shots).
    pub pierced: Vec<u64>,
    pub look: Look,
}

impl Projectile {
    pub fn new(position: Vec2, direction: Vec2, stats: ShotStats) -> Projectile {
        Projectile::with(position, direction, stats, None, Look::Plain)
    }
    pub fn with(position: Vec2, direction: Vec2, stats: ShotStats, target: Option<Vec2>, look: Look) -> Projectile {
        let mut rect = FRect::new(0, 0, stats.size, stats.size);
        rect.set_centerv(position);
        let base_speed = stats.speed;
        Projectile { rect, direction: direction.normalize(), base_speed, speed_factor: 1.0, target, arrived: false, beyond: None, previous: rect.centerv(), pierced: vec![], look, stats }
    }
    /// The rocket, flying to `target`.
    pub fn rocket(position: Vec2, direction: Vec2, stats: ShotStats, target: Vec2) -> Projectile {
        Projectile::with(position, direction, stats, Some(target), Look::Rocket)
    }
    pub fn laser_bolt(position: Vec2, direction: Vec2, stats: ShotStats, trail: f64) -> Projectile {
        Projectile::with(position, direction, stats, None, Look::LaserBolt { origin: position, trail })
    }
    pub fn cannon_ball(position: Vec2, direction: Vec2, stats: ShotStats) -> Projectile {
        Projectile::with(position, direction, stats, None, Look::CannonBall)
    }
    /// A mine where a dash began. `values`: the mine's damage, blast radius, wall push, size; `slow_field`: the slow
    /// field's slow (percent), size, duration – with its upgrades – or `None`.
    pub fn mine(position: Vec2, damage: f64, blast_radius: f64, wall_push: f64, size: f64, slow_field: Option<(f64, f64, f64)>) -> Projectile {
        let mut stats = ShotStats { blast_radius, ..ShotStats::new(damage, 0.0, wall_push, size, MINE_COLOR) };
        if let Some((slow, size, duration)) = slow_field {
            stats.slow = slow / 100.0;
            stats.slow_radius = size / 2.0;
            stats.slow_time = duration;
        }
        Projectile::with(position, vec2(1.0, 0.0), stats, None, Look::Mine { age: 0.0 })
    }

    pub fn center(&self) -> Vec2 {
        self.rect.centerv()
    }
    /// Current speed in pixels per second (base speed × effects this tick).
    pub fn speed(&self) -> f64 {
        self.base_speed * self.speed_factor
    }
    pub fn can_pierce(&self) -> bool {
        (self.pierced.len() as i64) < self.stats.pierce
    }
    /// Its push when it reaches the wall – weaker for every enemy it went through.
    pub fn wall_push(&self) -> f64 {
        self.stats.wall_push * self.stats.pierce_push.powi(self.pierced.len() as i32)
    }
    /// The rectangle around this tick's whole way (previous → now) – for a quick first check.
    pub fn sweep(&self) -> FRect {
        let d = self.previous - self.center();
        self.rect.union(&self.rect.moved(d.x, d.y))
    }
    /// True if it touched `rect` anywhere on this tick's way.
    pub fn touches(&self, rect: &FRect) -> bool {
        rect.inflate(self.rect.w, self.rect.h).clipline(self.previous, self.center())
    }

    pub fn update(&mut self, dt: f64) {
        self.previous = self.center();
        let step = self.speed() * dt;
        match self.target {
            Some(t) if t.distance_to(self.center()) <= step => {
                self.rect.set_centerv(t); // don't fly past the target
                self.arrived = true;
            }
            _ => {
                let d = self.direction * step;
                self.rect.move_ip(d.x, d.y);
            }
        }
        self.base_speed += self.stats.acceleration * dt;
        self.speed_factor = 1.0; // effects have to be applied again next tick
        if let Look::Mine { age } = &mut self.look {
            *age += dt;
        }
    }

    pub fn draw(&self, surface: &mut Surface, offset: Vec2) {
        let center = self.center() - offset;
        let color = self.stats.color;
        match &self.look {
            Look::Plain => surface.circle(color, center, self.stats.size / 2.0, 0),
            Look::CannonBall => {
                let radius = self.stats.size / 2.0;
                surface.circle(color, center, radius, 0);
                surface.circle(BALL_RING, center, radius, 3);
            }
            Look::LaserBolt { origin, trail } => {
                let head = self.center();
                let tail = head - self.direction * trail.min(head.distance_to(*origin));
                let glow = Color::f(py_round(color.r as f64 * 0.35), py_round(color.g as f64 * 0.35), py_round(color.b as f64 * 0.35));
                let width = self.stats.size;
                surface.line(glow, head - offset, tail - offset, (py_round(width * GLOW_WIDTH) as i32).max(1));
                surface.line(color, head - offset, tail - offset, (py_round(width) as i32).max(1));
                surface.line(LASER_CORE, head - offset, tail - offset, (py_round(width / 3.0) as i32).max(1));
            }
            Look::Rocket => {
                let forward = self.direction;
                let side = forward.rotate(90.0);
                let width = self.stats.size;
                let length = width * 2.8;
                let tip = center + forward * length / 2.0;
                let back = center - forward * length / 2.0;
                let flame = width * rng::uniform(0.35, 0.6);
                surface.circle(FLAME, back - forward * flame, flame, 0);
                for sign in [1.0, -1.0] {
                    // two fins at the back
                    surface.polygon(FINS, &[back + forward * width * 0.8 + side * sign * width * 0.4, back + side * sign * width * 0.9, back + side * sign * width * 0.3], 0);
                }
                surface.polygon(color, &[tip, center + forward * length * 0.2 + side * width / 2.0, back + side * width / 2.0, back - side * width / 2.0, center + forward * length * 0.2 - side * width / 2.0], 0);
            }
            Look::Mine { age } => {
                let radius = self.stats.size / 2.0;
                surface.circle(MINE_BODY, center, radius, 0);
                if (age / MINE_BLINK * std::f64::consts::TAU).sin() > 0.0 {
                    // blinks: on half the time
                    surface.circle(MINE_LIGHT, center, (radius * 0.45).max(1.0), 0);
                }
            }
        }
    }
}

const BALL_RING: Color = rgb(235, 170, 255);
/// The soft glow around the laser's beam, in bolt sizes.
const GLOW_WIDTH: f64 = 2.5;
const LASER_CORE: Color = rgb(225, 255, 235);
const FLAME: Color = rgb(255, 170, 40);
const FINS: Color = rgb(150, 40, 30);
/// The mine's blast.
pub const MINE_COLOR: Color = rgb(255, 90, 60);
const MINE_BODY: Color = rgb(110, 110, 120);
const MINE_LIGHT: Color = rgb(255, 60, 40);
/// Seconds per blink of a mine's light.
const MINE_BLINK: f64 = 0.8;
