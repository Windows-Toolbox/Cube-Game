//! Stars and cores lying in the world (`star_pickup.py`, `core_pickup.py`). Killed enemies drop them; within the
//! ship's magnet radius they get pulled in, faster the closer they are.

use super::shapes::{CORE_COLOR, STAR_COLOR, draw_core, draw_star, shade};
use crate::engine::{FRect, Surface, Vec2, vec2};

/// Hitbox.
const SIZE: f64 = 16.0;
/// Pulled in at this speed at the edge of the ship's magnet_radius, three times as fast at it.
const MAGNET_SPEED: f64 = 260.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Currency {
    Stars,
    Cores,
}

#[derive(Debug, Clone)]
pub struct Pickup {
    pub rect: FRect,
    pub currency: Currency,
    pub age: f64,
}

impl Pickup {
    pub fn new(position: Vec2, currency: Currency) -> Pickup {
        let mut rect = FRect::new(0.0, 0.0, SIZE, SIZE);
        rect.set_centerv(position);
        Pickup { rect, currency, age: 0.0 }
    }

    pub fn update(&mut self, dt: f64, ship: Vec2, magnet_radius: f64) {
        self.age += dt;
        let to_ship = ship - self.rect.centerv();
        let distance = to_ship.length();
        if 0.0 < distance && distance < magnet_radius {
            let pull = MAGNET_SPEED * (1.0 + 2.0 * (1.0 - distance / magnet_radius));
            let d = to_ship / distance * (pull * dt).min(distance);
            self.rect.move_ip(d.x, d.y);
        }
    }

    pub fn draw(&self, surface: &mut Surface, offset: Vec2) {
        let bob = (self.age * 5.0).sin() * 2.0; // floats up and down a little
        let center = self.rect.centerv() - offset + vec2(0.0, bob);
        match self.currency {
            Currency::Stars => draw_star(surface, center, SIZE / 2.0 + 1.0, STAR_COLOR),
            Currency::Cores => {
                // a pulsing ring, so it stands out among the stars
                let glow = 0.5 + 0.5 * (self.age * 8.0).sin();
                surface.circle(shade(CORE_COLOR, 0.25 + 0.35 * glow), center, SIZE * 0.95, 2);
                draw_core(surface, center, SIZE / 2.0 + 1.0, CORE_COLOR);
            }
        }
    }
}
