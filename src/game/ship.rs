//! The player's ship (`ship.py`): moves with W A S D and always points at the mouse.
//!
//! It has 2 weapon slots (filled from the loadout) and 2 ability slots. Only one weapon fires at a time – the one in
//! `active_slot` (left mouse button); `switch_weapon` swaps to the other one (right mouse button).
//!
//! Energy: both weapons share it. Every shot costs its weapon's energy_cost; the ship gets energy_regen back per
//! second, up to max_energy. Abilities do not use energy, only their cooldown.
//!
//! Hull: armor takes a flat amount off every hit (`absorb`), invulnerable_time is how long it cannot be hit again
//! after one, and magnet_radius is how close stars and cores have to be to get pulled in.
//!
//! Knocked (`knock`): pushed back – fast at first, then slowing down – and stunned for a moment: it cannot move,
//! shoot, use abilities or throw towers, only aim. Small dots circle above it then.

use super::abilities::Ability;
use super::buildings::WatchtowerKit;
use super::projectile::Projectile;
use super::settings::{self, reduce};
use super::shapes::draw_ship;
use super::tables::ships;
use super::weapons::Weapon;
use crate::engine::window::{K_A, K_D, K_S, K_W};
use crate::engine::{Color, Engine, FRect, Surface, Vec2, rgb, vec2};

/// Per second: how fast a knockback slows down (it covers distance in total).
const KNOCKBACK_DECAY: f64 = 12.0;
const STUN_DOTS: usize = 3;
const STUN_COLOR: Color = rgb(255, 230, 120);

#[derive(Debug, Clone)]
pub struct Ship {
    pub rect: FRect,
    pub color: Color,
    pub size: f64,
    /// Direction the nose points in (length 1).
    pub aim: Vec2,
    pub speed: f64,
    pub weapons: [Option<Weapon>; 2],
    /// The weapon slot that fires.
    pub active_slot: usize,
    pub max_energy: f64,
    /// Per second.
    pub energy_regen: f64,
    pub energy: f64,
    pub invulnerable_time: f64,
    /// Pixels off every hit.
    pub armor: f64,
    pub magnet_radius: f64,
    pub abilities: [Option<Ability>; 2],
    /// Seconds left in which hits do not count (after a hit).
    pub invulnerable: f64,
    /// Currency for the shop, dropped by killed enemies.
    pub stars: i64,
    /// Pixels per second it is being pushed (slowing down).
    pub knockback: Vec2,
    /// Seconds left in which it cannot act.
    pub stun: f64,
    /// Seconds since the stun began (for the dots).
    pub stun_age: f64,
    /// The watchtower kit (skill tree) – `None`: cannot build any.
    pub watchtower: Option<WatchtowerKit>,
}

impl Ship {
    /// `key`: which ship of `ships.json` – `None`: the one the file says is flown.
    pub fn new(center: Vec2, key: Option<&str>) -> Ship {
        let stats = key.and_then(ships::get).unwrap_or_else(ships::current);
        let mut rect = FRect::new(0.0, 0.0, stats.size, stats.size);
        rect.set_centerv(center);
        Ship {
            rect,
            color: stats.color,
            size: stats.size,
            aim: vec2(0.0, -1.0),
            speed: stats.speed,
            weapons: [Some(Weapon::new("gun")), None],
            active_slot: 0,
            max_energy: stats.max_energy,
            energy_regen: stats.energy_regen,
            energy: stats.max_energy,
            invulnerable_time: stats.invulnerable_time,
            armor: stats.armor,
            magnet_radius: stats.magnet_radius,
            abilities: [Some(Ability::dash()), Some(Ability::rocket())],
            invulnerable: 0.0,
            stars: 0,
            knockback: Vec2::ZERO,
            stun: 0.0,
            stun_age: 0.0,
            watchtower: None,
        }
    }

    pub fn center(&self) -> Vec2 {
        self.rect.centerv()
    }

    /// Where W A S D steer right now: length 1, or (0, 0) when standing still.
    pub fn move_direction(e: &Engine) -> Vec2 {
        let k = |key| if e.key_held(key) { 1.0 } else { 0.0 };
        // right minus left, down minus up: opposite keys held together cancel each other out
        let d = vec2(k(K_D) - k(K_A), k(K_S) - k(K_W));
        if d.length_squared() > 0.0 { d.normalize() } else { d }
    }

    pub fn stunned(&self) -> bool {
        self.stun > 0.0
    }
    /// Pushes it `distance` pixels in `direction` (spread over a moment) and stuns it for `stun` seconds.
    pub fn knock(&mut self, direction: Vec2, distance: f64, stun: f64) {
        if direction.length_squared() > 0.0 {
            self.knockback = direction.normalize() * distance * KNOCKBACK_DECAY;
        }
        if stun > self.stun {
            self.stun = stun;
            self.stun_age = 0.0;
        }
    }

    /// An attribute by name (cards and the skill tree: "speed", "max_energy", "energy_regen", …).
    pub fn get(&self, attr: &str) -> f64 {
        match attr {
            "speed" => self.speed,
            "max_energy" => self.max_energy,
            "energy_regen" => self.energy_regen,
            "invulnerable_time" => self.invulnerable_time,
            "armor" => self.armor,
            "magnet_radius" => self.magnet_radius,
            _ => 0.0,
        }
    }
    pub fn set(&mut self, attr: &str, v: f64) {
        match attr {
            "speed" => self.speed = v,
            "max_energy" => self.max_energy = v,
            "energy_regen" => self.energy_regen = v,
            "invulnerable_time" => self.invulnerable_time = v,
            "armor" => self.armor = v,
            "magnet_radius" => self.magnet_radius = v,
            _ => {}
        }
    }
    /// The skill tree's ship upgrades: "max" or "regen" (the energy) or one of the hull's values.
    pub fn upgrade(&mut self, name: &str, amount: f64) {
        let attr = match name {
            "max" => "max_energy",
            "regen" => "energy_regen",
            other => other,
        };
        self.set(attr, crate::pyfmt::round_to(self.get(attr) + amount, 6));
        self.energy = self.max_energy;
    }

    /// What is left of a hit's damage after the armor: armor pixels less, at least MIN_HIT_SHARE.
    pub fn absorb(&self, damage: f64) -> f64 {
        reduce(damage, self.armor, settings::MIN_HIT_SHARE)
    }

    /// `target`: the point to aim at (the mouse, in world coordinates); `moving`: where W A S D steer.
    pub fn update(&mut self, dt: f64, target: Vec2, moving: Vec2) {
        self.energy = self.max_energy.min(self.energy + self.energy_regen * dt);
        self.stun = (self.stun - dt).max(0.0);
        self.stun_age += dt;
        if !self.stunned() {
            let d = moving * self.speed * dt;
            self.rect.move_ip(d.x, d.y);
        }
        if self.knockback.length_squared() > 1.0 {
            let slowing = (-KNOCKBACK_DECAY * dt).exp();
            let d = self.knockback * (1.0 - slowing) / KNOCKBACK_DECAY; // exactly the way it slows down
            self.rect.move_ip(d.x, d.y);
            self.knockback *= slowing;
        } else {
            self.knockback = Vec2::ZERO;
        }
        let to_target = target - self.center();
        if to_target.length_squared() > 1.0 {
            // mouse right on the ship: keep the last direction
            self.aim = to_target.normalize();
        }
        self.invulnerable = (self.invulnerable - dt).max(0.0);
        for w in self.weapons.iter_mut().flatten() {
            w.update(dt);
        }
        let aim = self.aim;
        for a in self.abilities.iter_mut().flatten() {
            a.update(dt, &mut self.rect, aim);
        }
    }

    /// Something hit the ship. True if it counts – false while still invulnerable from the last hit or while an
    /// ability protects it (the dash).
    pub fn hit(&mut self) -> bool {
        if self.invulnerable > 0.0 || self.abilities.iter().flatten().any(|a| a.protects()) {
            return false;
        }
        self.invulnerable = self.invulnerable_time;
        true
    }

    /// Where shots come out (world coordinates).
    pub fn nose(&self) -> Vec2 {
        self.center() + self.aim * self.size * 0.6
    }

    /// The slot of the weapon that fires: the active one – or the other one if that slot is empty.
    pub fn weapon_slot(&self) -> Option<usize> {
        if self.weapons[self.active_slot].is_some() {
            Some(self.active_slot)
        } else if self.weapons[1 - self.active_slot].is_some() {
            Some(1 - self.active_slot)
        } else {
            None
        }
    }
    pub fn weapon(&self) -> Option<&Weapon> {
        self.weapon_slot().and_then(|s| self.weapons[s].as_ref())
    }
    /// Makes the other weapon slot the one that fires – if there is a weapon in it.
    pub fn switch_weapon(&mut self) {
        let other = 1 - self.active_slot;
        if self.weapons[other].is_some() {
            self.active_slot = other;
        }
    }

    /// Fires the active weapon from the nose: the new projectiles – none while it cools down or without energy.
    pub fn fire(&mut self) -> Vec<Projectile> {
        let (nose, aim) = (self.nose(), self.aim);
        let Some(slot) = self.weapon_slot() else { return vec![] };
        let energy = self.energy;
        let w = self.weapons[slot].as_mut().unwrap();
        if !w.ready() || energy < w.energy_cost {
            return vec![];
        }
        self.energy -= w.energy_cost;
        w.fire(nose, aim)
    }

    /// Triggers the ability in `slot` (0 or 1). `target`: the mouse in world coordinates.
    pub fn use_ability(&mut self, slot: usize, target: Vec2, moving: Vec2) -> Vec<Projectile> {
        let (rect, aim, nose) = (self.rect, self.aim, self.nose());
        match &mut self.abilities[slot] {
            Some(a) => a.use_at(rect, aim, nose, moving, target),
            None => vec![],
        }
    }

    pub fn draw(&self, surface: &mut Surface, offset: Vec2) {
        for a in self.abilities.iter().flatten() {
            a.draw(surface, offset, self.rect.w as f64, self.color);
        }
        let center = self.center() - offset;
        if self.stunned() {
            // little dots circling above it (also while it blinks)
            for dot in 0..STUN_DOTS {
                let angle = self.stun_age * 8.0 + dot as f64 * std::f64::consts::TAU / STUN_DOTS as f64;
                let spot = center + vec2(angle.cos() * 12.0, -self.size * 0.9 + angle.sin() * 4.0);
                surface.circle(STUN_COLOR, spot, 2.0, 0);
            }
        }
        if self.invulnerable > 0.0 && (self.invulnerable * 10.0) as i64 % 2 == 1 {
            return; // blinks while invulnerable
        }
        draw_ship(surface, center, self.aim, self.size, self.color);
    }
}
