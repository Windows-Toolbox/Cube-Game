//! The abilities in the ship's two ability slots (`ability.py`, `dash.py`, `rocket.py`). An ability has a cooldown;
//! the player triggers it with its key (`settings::ABILITY_KEYS`).
//!
//! Charges: an ability can be stored up to `max_charges` times. Every use takes one; while it has fewer, the
//! cooldown runs and gives one back each time it is over.
//!
//! Dash (Shift): a quick burst in the direction you are moving – or towards the mouse if you stand still. Nothing
//! can hit the ship for invulnerable_time seconds from its start. With "Mines" every dash leaves a mine where it
//! started; with "Slow field" the mine's blast slows the enemies in it.
//!
//! Rocket (Space): flies to where the mouse was, getting faster, and explodes there: damage, and every arena edge
//! within the blast radius is pushed out a lot. With "Burning ground" the blast leaves burning ground behind.

use super::projectile::{Projectile, ShotStats};
use super::shapes::{draw_ship, shade};
use super::tables::abilities as data;
use crate::engine::geom::py_round;
use crate::engine::{Color, FRect, Surface, Vec2, rgb};
use crate::pyfmt::g;

/// Seconds an afterimage of the dash stays visible.
const TRAIL_TIME: f64 = 0.25;
const ROCKET_COLOR: Color = rgb(255, 70, 50);

#[derive(Debug, Clone, PartialEq)]
pub struct Mines {
    pub damage: f64,
    pub blast_radius: f64,
    pub wall_push: f64,
    pub size: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlowFieldValues {
    /// Percent slower.
    pub slow: f64,
    pub size: f64,
    pub duration: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Burning {
    pub duration: f64,
    pub damage_per_second: f64,
    /// Share of the blast radius.
    pub radius: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Dash {
        distance: f64,
        duration: f64,
        invulnerable_time: f64,
        /// > 0: seconds left in which nothing can hit the ship.
        shield: f64,
        /// > 0 while dashing.
        time_left: f64,
        direction: Vec2,
        /// Afterimages: position, aim, age.
        trail: Vec<(Vec2, Vec2, f64)>,
        mines: Option<Mines>,
        slow_field: Option<SlowFieldValues>,
    },
    Rocket {
        damage: f64,
        blast_radius: f64,
        wall_push: f64,
        min_cooldown: f64,
        speed: f64,
        acceleration: f64,
        size: f64,
        burning: Option<Burning>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ability {
    /// Seconds after use until it can be used again (until one charge is back).
    pub cooldown_time: f64,
    pub max_charges: f64,
    /// Seconds left until the next charge is back (0 while all are there).
    pub cooldown: f64,
    pub charges: f64,
    pub kind: Kind,
}

impl Ability {
    pub fn dash() -> Ability {
        let charges = py_round(data::get("dash", "charges"));
        Ability {
            cooldown_time: data::get("dash", "cooldown"),
            max_charges: charges,
            cooldown: 0.0,
            charges,
            kind: Kind::Dash {
                distance: data::get("dash", "distance"),
                duration: data::get("dash", "duration"),
                invulnerable_time: data::get("dash", "invulnerable_time"),
                shield: 0.0,
                time_left: 0.0,
                direction: Vec2::ZERO,
                trail: vec![],
                mines: None,
                slow_field: None,
            },
        }
    }
    pub fn rocket() -> Ability {
        let n = |k: &str| data::get("rocket", k);
        Ability {
            cooldown_time: n("cooldown"),
            max_charges: 1.0,
            cooldown: 0.0,
            charges: 1.0,
            kind: Kind::Rocket { damage: n("damage"), blast_radius: n("blast_radius"), wall_push: n("wall_push"), min_cooldown: n("min_cooldown"), speed: n("speed"), acceleration: n("acceleration"), size: n("size"), burning: None },
        }
    }
    pub fn is_dash(&self) -> bool {
        matches!(self.kind, Kind::Dash { .. })
    }
    pub fn is_rocket(&self) -> bool {
        matches!(self.kind, Kind::Rocket { .. })
    }
    pub fn name(&self) -> &'static str {
        if self.is_dash() { "Dash" } else { "Rocket" }
    }

    /// True while it keeps the ship from being hit (the dash).
    pub fn protects(&self) -> bool {
        matches!(self.kind, Kind::Dash { shield, .. } if shield > 0.0)
    }
    pub fn ready(&self) -> bool {
        // not again in the middle of a dash
        self.charges > 0.0 && !matches!(self.kind, Kind::Dash { time_left, .. } if time_left > 0.0)
    }

    /// An attribute by name (cards: "distance", "cooldown_time", "damage", "blast_radius", "wall_push").
    pub fn get(&self, attr: &str) -> f64 {
        match (&self.kind, attr) {
            (_, "cooldown_time") => self.cooldown_time,
            (_, "max_charges") => self.max_charges,
            (Kind::Dash { distance, .. }, "distance") => *distance,
            (Kind::Rocket { damage, .. }, "damage") => *damage,
            (Kind::Rocket { blast_radius, .. }, "blast_radius") => *blast_radius,
            (Kind::Rocket { wall_push, .. }, "wall_push") => *wall_push,
            _ => 0.0,
        }
    }
    pub fn set(&mut self, attr: &str, v: f64) {
        match (&mut self.kind, attr) {
            (_, "cooldown_time") => self.cooldown_time = v,
            (_, "max_charges") => self.max_charges = v,
            (Kind::Dash { distance, .. }, "distance") => *distance = v,
            (Kind::Rocket { damage, .. }, "damage") => *damage = v,
            (Kind::Rocket { blast_radius, .. }, "blast_radius") => *blast_radius = v,
            (Kind::Rocket { wall_push, .. }, "wall_push") => *wall_push = v,
            _ => {}
        }
    }

    /// The skill tree's upgrades: adds `amount` to the value `name` (as in `skilltree.json`'s EFFECTS).
    pub fn upgrade(&mut self, name: &str, amount: f64) {
        match &self.kind {
            Kind::Dash { .. } => {
                let attr = if name == "charges" { "max_charges" } else { "distance" };
                let v = crate::pyfmt::round_to(self.get(attr) + amount, 6).max(0.0);
                self.set(attr, v);
                self.max_charges = py_round(self.max_charges).max(1.0);
                self.charges = self.max_charges;
            }
            Kind::Rocket { min_cooldown, .. } => {
                let attr = if name == "cooldown" { "cooldown_time" } else { name };
                let floor = if name == "cooldown" { *min_cooldown } else { 0.0 };
                let v = crate::pyfmt::round_to(self.get(attr) + amount, 6).max(floor);
                self.set(attr, v);
            }
        }
    }

    /// The player pressed the key. Returns new projectiles (a mine, the rocket).
    pub fn use_at(&mut self, ship_rect: FRect, aim: Vec2, nose: Vec2, moving: Vec2, target: Vec2) -> Vec<Projectile> {
        if !self.ready() {
            return vec![];
        }
        if self.charges >= self.max_charges {
            self.cooldown = self.cooldown_time; // all were there: the cooldown starts now
        }
        self.charges -= 1.0;
        match &mut self.kind {
            Kind::Dash { duration, invulnerable_time, shield, time_left, direction, mines, slow_field, .. } => {
                // read now, so Shift together with a direction key goes that way
                *direction = if moving.length_squared() > 0.0 { moving } else { aim };
                *time_left = *duration;
                *shield = *invulnerable_time;
                match mines {
                    Some(m) => vec![Projectile::mine(ship_rect.centerv(), m.damage, m.blast_radius, m.wall_push, m.size, slow_field.as_ref().map(|s| (s.slow, s.size, s.duration)))],
                    None => vec![],
                }
            }
            Kind::Rocket { .. } => {
                let mut direction = target - nose;
                if direction.length_squared() < 1.0 {
                    direction = aim;
                }
                vec![Projectile::rocket(nose, direction, self.shot_stats(), target)]
            }
        }
    }

    pub fn shot_stats(&self) -> ShotStats {
        let Kind::Rocket { damage, blast_radius, wall_push, speed, acceleration, size, burning, .. } = &self.kind else {
            return ShotStats::new(0.0, 0.0, 0.0, 1.0, ROCKET_COLOR);
        };
        let mut s = ShotStats { acceleration: *acceleration, blast_radius: *blast_radius, ..ShotStats::new(*damage, *speed, *wall_push, *size, ROCKET_COLOR) };
        if let Some(b) = burning {
            s.burn_time = b.duration;
            s.burn_damage = b.damage_per_second;
            s.burn_radius = blast_radius * b.radius;
        }
        s
    }

    /// Every tick. `ship`: its rect, moved by a running dash.
    pub fn update(&mut self, dt: f64, ship: &mut FRect, aim: Vec2) {
        if self.charges >= self.max_charges {
            self.charges = self.max_charges;
            self.cooldown = 0.0;
        } else {
            self.cooldown -= dt;
            if self.cooldown <= 0.0 {
                // one back – and the next one starts filling if any is missing
                self.charges += 1.0;
                self.cooldown = if self.charges < self.max_charges { self.cooldown_time + self.cooldown } else { 0.0 };
            }
        }
        if let Kind::Dash { distance, duration, shield, time_left, direction, trail, .. } = &mut self.kind {
            *shield = (*shield - dt).max(0.0);
            if *time_left > 0.0 {
                let step = dt.min(*time_left);
                let d = *direction * (*distance / *duration) * step;
                ship.move_ip(d.x, d.y);
                *time_left -= step;
                trail.push((ship.centerv(), aim, 0.0));
            }
            for t in trail.iter_mut() {
                t.2 += dt;
            }
            trail.retain(|t| t.2 < TRAIL_TIME);
        }
    }

    /// Drawn below the ship: the dash's afterimages.
    pub fn draw(&self, surface: &mut Surface, offset: Vec2, ship_size: f64, ship_color: Color) {
        if let Kind::Dash { trail, .. } = &self.kind {
            for (position, aim, age) in trail {
                let fade = 0.5 * (1.0 - age / TRAIL_TIME); // the background is black, so darker = more faded
                draw_ship(surface, *position - offset, *aim, ship_size, shade(ship_color, fade));
            }
        }
    }

    /// (name, value) rows for the inventory.
    pub fn stat_lines(&self) -> Vec<(String, String)> {
        let cooldown = ("Cooldown".to_string(), format!("{} s", g(self.cooldown_time)));
        match &self.kind {
            Kind::Dash { distance, mines, slow_field, .. } => {
                let mut lines = vec![("Distance".to_string(), format!("{} px", g(*distance))), ("Charges".to_string(), format!("{}", self.max_charges as i64)), cooldown];
                if let Some(m) = mines {
                    lines.push(("Mines".into(), format!("{} damage", g(m.damage))));
                    if let Some(s) = slow_field {
                        lines.push(("Slow field".into(), format!("{} % · {} px · {} s", g(s.slow), g(s.size), g(s.duration))));
                    }
                }
                lines
            }
            Kind::Rocket { damage, blast_radius, wall_push, burning, .. } => {
                let mut lines = vec![("Damage".to_string(), g(*damage)), ("Blast radius".to_string(), format!("{} px", g(*blast_radius))), ("Wall push".to_string(), format!("{} px", g(*wall_push))), cooldown];
                if burning.is_some() {
                    lines.push(("Burning ground".into(), "yes".into()));
                }
                lines
            }
        }
    }
}

