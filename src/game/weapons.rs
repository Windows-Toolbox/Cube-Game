//! The weapons for the ship's two weapon slots (`game/weapons/`). Each decides everything about its shots: damage,
//! attack speed, projectile speed and size, wall push, colour – and how much of the ship's energy a shot uses. Each
//! has its own cooldown; the energy is shared (`Ship::fire` checks it).
//!
//! Its values come from `weapons.json`. They are changed two ways: the skill tree (`upgrade(name, amount)`, never
//! below its minimum) and shop upgrades and cards (they set the attributes directly – `get` / `set` by the
//! attribute's name, e.g. "fire_rate").
//!
//! - the gun: steady single shots, the starter
//! - the machine gun: very fast weak shots within a spread (degrees, the whole cone)
//! - the cannon: slow heavy shots that pierce `pierce` enemies; after piercing its wall push is multiplied by
//!   pierced_push once per enemy
//! - the laser: a stream of very fast, tiny bolts drawn as one beam

use serde_json::Value;

use super::projectile::{Projectile, ShotStats};
use super::tables::weapons as data;
use crate::engine::geom::py_round;
use crate::engine::{Color, Vec2, rgb};
use crate::pyfmt::{g, g_prec};
use crate::rng;

/// Cooldowns at or below this count as 0 – 1/6 s minus 10 × 1/60 s leaves a tiny float remainder.
const READY: f64 = 1e-9;

/// Every weapon type: key (in `weapons.json`, the profile and the skill tree), name, colour, one line for the loadout.
pub const WEAPON_TYPES: &[(&str, &str, Color, &str)] = &[
    ("gun", "Gun", rgb(255, 255, 255), "The all-rounder: steady shots, fair on energy."),
    ("machine_gun", "Machine gun", rgb(255, 190, 90), "Very fast, weak shots: shreds the unarmored, barely scratches armor and walls."),
    ("cannon", "Cannon", rgb(200, 140, 255), "Slow, heavy shots that pierce: breaks armor and pushes walls hard, wasted on weak enemies."),
    ("laser", "Laser", rgb(90, 255, 160), "A beam: barely hurts enemies, pushes walls hard."),
];
pub const STARTER: &str = "gun";

pub fn is_weapon(key: &str) -> bool {
    WEAPON_TYPES.iter().any(|w| w.0 == key)
}
/// True if the player may put this weapon into a slot: the gun always, the others once their node is bought.
pub fn unlocked(profile: &super::profile::Profile, key: &str) -> bool {
    key == STARTER || profile.owns(key)
}
pub fn name_of(key: &str) -> &'static str {
    WEAPON_TYPES.iter().find(|w| w.0 == key).map(|w| w.1).unwrap_or("Weapon")
}
pub fn color_of(key: &str) -> Color {
    WEAPON_TYPES.iter().find(|w| w.0 == key).map(|w| w.2).unwrap_or(rgb(255, 255, 255))
}
pub fn description_of(key: &str) -> &'static str {
    WEAPON_TYPES.iter().find(|w| w.0 == key).map(|w| w.3).unwrap_or("")
}

/// Value name in `weapons.json` → attribute.
fn attribute(name: &str) -> &str {
    match name {
        "attack_speed" => "fire_rate",
        "energy_use" => "energy_cost",
        "piercing" => "pierce",
        other => other,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Weapon {
    pub key: &'static str,
    pub damage: f64,
    pub fire_rate: f64,
    pub projectile_speed: f64,
    pub projectile_size: f64,
    pub wall_push: f64,
    pub energy_cost: f64,
    /// The machine gun's spread (degrees, the whole cone).
    pub spread: f64,
    /// The cannon's: enemies a shot pierces.
    pub pierce: f64,
    /// Card type key → how strongly cards work on it.
    pub upgrade_factors: Vec<(String, f64)>,
    /// Seconds until the next shot is possible.
    pub cooldown: f64,
}

impl Weapon {
    pub fn new(key: &str) -> Weapon {
        let key = WEAPON_TYPES.iter().find(|w| w.0 == key).map(|w| w.0).unwrap_or(STARTER);
        let stats = data::section(key);
        let n = |name: &str| stats.get(name).and_then(Value::as_f64).unwrap_or(0.0);
        let factors = stats.get("upgrade_factor").and_then(Value::as_object).map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_f64().unwrap_or(1.0))).collect()).unwrap_or_default();
        Weapon {
            key,
            damage: n("damage"),
            fire_rate: n("attack_speed"),
            projectile_speed: n("projectile_speed"),
            projectile_size: n("projectile_size"),
            wall_push: n("wall_push"),
            energy_cost: n("energy_use"),
            spread: n("spread"),
            pierce: n("piercing"),
            upgrade_factors: factors,
            cooldown: 0.0,
        }
    }
    pub fn name(&self) -> &'static str {
        name_of(self.key)
    }
    pub fn color(&self) -> Color {
        color_of(self.key)
    }
    /// How strongly a card of type `kind` works on it (1 unless the file says otherwise).
    pub fn factor(&self, kind: &str) -> f64 {
        self.upgrade_factors.iter().find(|(k, _)| k == kind).map(|(_, f)| *f).unwrap_or(1.0)
    }

    /// An attribute by name ("damage", "fire_rate", …).
    pub fn get(&self, attr: &str) -> f64 {
        match attr {
            "damage" => self.damage,
            "fire_rate" => self.fire_rate,
            "projectile_speed" => self.projectile_speed,
            "projectile_size" => self.projectile_size,
            "wall_push" => self.wall_push,
            "energy_cost" => self.energy_cost,
            "spread" => self.spread,
            "pierce" => self.pierce,
            _ => 0.0,
        }
    }
    pub fn set(&mut self, attr: &str, v: f64) {
        match attr {
            "damage" => self.damage = v,
            "fire_rate" => self.fire_rate = v,
            "projectile_speed" => self.projectile_speed = v,
            "projectile_size" => self.projectile_size = v,
            "wall_push" => self.wall_push = v,
            "energy_cost" => self.energy_cost = v,
            "spread" => self.spread = v,
            "pierce" => self.pierce = v,
            _ => {}
        }
    }

    /// The skill tree: adds `amount` to the value `name` (as in `weapons.json`), never below its minimum.
    pub fn upgrade(&mut self, name: &str, amount: f64) {
        let attr = attribute(name);
        let minimum = match attr {
            "fire_rate" => 0.1,
            "projectile_speed" => 50.0,
            _ => 0.0,
        };
        let value = crate::pyfmt::round_to(self.get(attr) + amount, 6);
        self.set(attr, value.max(minimum));
    }

    pub fn ready(&self) -> bool {
        self.cooldown <= READY
    }
    pub fn update(&mut self, dt: f64) {
        // may go up to one tick below 0: the leftover time carries over to the next shot
        self.cooldown = (self.cooldown - dt).max(-dt);
    }

    /// Snapshot of the current values – every shot gets its own copy when it is fired.
    pub fn shot_stats(&self) -> ShotStats {
        let mut s = ShotStats::new(self.damage, self.projectile_speed, self.wall_push, self.projectile_size, self.color());
        if self.key == "cannon" {
            s.pierce = py_round(self.pierce) as i64;
            s.pierce_push = data::get("cannon", "pierced_push");
        }
        s
    }

    /// New projectiles – empty while cooling down. The ship has already checked (and taken) the energy.
    pub fn fire(&mut self, origin: Vec2, direction: Vec2) -> Vec<Projectile> {
        if !self.ready() {
            return vec![];
        }
        self.cooldown += 1.0 / self.fire_rate;
        let shot = match self.key {
            "machine_gun" => {
                let angle = rng::uniform(-self.spread / 2.0, self.spread / 2.0);
                Projectile::new(origin, direction.rotate(angle), self.shot_stats())
            }
            "laser" => {
                let gap = self.projectile_speed / self.fire_rate; // how far apart two bolts fly
                Projectile::laser_bolt(origin, direction, self.shot_stats(), gap * 1.1)
            }
            _ => Projectile::new(origin, direction, self.shot_stats()),
        };
        vec![shot]
    }

    /// (name, value) rows for the inventory and the loadout.
    pub fn stat_lines(&self) -> Vec<(String, String)> {
        if self.key == "laser" {
            let rate = self.fire_rate;
            return vec![
                ("Damage".into(), format!("{}/s", g_prec(self.damage * rate, 3))),
                ("Wall push".into(), format!("{} px/s", g_prec(self.wall_push * rate, 3))),
                ("Beam speed".into(), format!("{} px/s", g(self.projectile_speed))),
                ("Energy use".into(), format!("{}/s", g_prec(self.energy_cost * rate, 3))),
            ];
        }
        let mut lines = vec![
            ("Damage".into(), g(self.damage)),
            ("Attack speed".into(), format!("{}/s", g(self.fire_rate))),
            ("Wall push".into(), format!("{} px", g(self.wall_push))),
            ("Projectile speed".into(), format!("{} px/s", g(self.projectile_speed))),
            ("Energy use".into(), format!("{} per shot", g(self.energy_cost))),
        ];
        match self.key {
            "machine_gun" => lines.push(("Spread".into(), format!("{}°", g(self.spread)))),
            "cannon" => {
                let p = py_round(self.pierce) as i64;
                lines.push(("Piercing".into(), format!("{p} {}", if p == 1 { "enemy" } else { "enemies" })));
            }
            _ => {}
        }
        lines
    }
}
