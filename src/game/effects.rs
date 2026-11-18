//! Effects and animations: only what you see – explosions, sparks, the artillery's blast and target marker, the buff
//! glow, burning ground and slow fields (Python: `game/effects/`), and the two enemy attacks that are no shots: the
//! Laser's beam (`laser_beam.py`) and the artillery's shells (`artillery_strike.py`). What they do to the game (damage,
//! wall push) happens in the play scene; an effect only draws itself – except burning ground, which tells the scene
//! when to hurt the enemies in it.

use crate::engine::geom::py_round;
use crate::engine::gfx::Radii;
use crate::engine::sprite_cache::{self, scratch};
use crate::engine::{Anchor, Color, FRect, Rect, Surface, Vec2, rgb, vec2};
use crate::rng;

use super::shapes::shade;

fn round_color(c: [f64; 3]) -> Color {
    Color::f(py_round(c[0]), py_round(c[1]), py_round(c[2]))
}

// ---------- target marker ----------

const MARKER_COLOR: Color = rgb(255, 70, 50);
/// How see-through the growing disc is.
const FILL_ALPHA: u8 = 70;
/// Progress after which the outline blinks.
const BLINK_FROM: f64 = 0.8;
const BLINK_SPEED: f64 = 30.0;

/// `progress`: 0 (just fired) to 1 (lands now). `age`: seconds since it was fired (for the blinking).
pub fn draw_target_marker(surface: &mut Surface, center: Vec2, radius: f64, progress: f64, age: f64) {
    let size = radius.ceil() as i32 * 2 + 2;
    scratch(size, size, |disc| {
        disc.circle(MARKER_COLOR.with_alpha(FILL_ALPHA), vec2(size as f64 / 2.0, size as f64 / 2.0), radius * progress, 0);
        let r = disc.get_rect().placed(Anchor::Center, py_round(center.x), py_round(center.y));
        surface.blit(disc, r.x, r.y);
    });
    if progress < BLINK_FROM || (age * BLINK_SPEED).sin() > 0.0 {
        surface.circle(MARKER_COLOR, center, radius, 2);
    }
    surface.circle(MARKER_COLOR, center, 3.0, 0); // the exact spot
}

/// Where a splitter shell comes down: only a thin ring and the spot – its fragments' circles show where they hurt.
pub fn draw_landing_marker(surface: &mut Surface, center: Vec2, radius: f64) {
    let dim = shade(MARKER_COLOR, 0.6);
    surface.circle(dim, center, radius, 1);
    surface.circle(dim, center, 3.0, 0);
}

// ---------- explosion ----------

/// Seconds until the ring has reached its full size and is gone.
const EXPLOSION_TIME: f64 = 0.35;

#[derive(Debug, Clone)]
pub struct Explosion {
    pub position: Vec2,
    pub radius: f64,
    pub color: Color,
    pub age: f64,
}

impl Explosion {
    pub fn new(position: Vec2, radius: f64, color: Color) -> Explosion {
        Explosion { position, radius, color, age: 0.0 }
    }
    pub fn done(&self) -> bool {
        self.age >= EXPLOSION_TIME
    }
    pub fn update(&mut self, dt: f64) {
        self.age += dt;
    }
    pub fn draw(&self, surface: &mut Surface, offset: Vec2) {
        let progress = (self.age / EXPLOSION_TIME).min(1.0);
        let radius = self.radius * (1.0 - (1.0 - progress).powi(3)); // fast at first, then slowing down
        let fade = 1.0 - progress; // the background is black, so darker = more faded
        let color = shade(self.color, fade);
        let width = (py_round(10.0 * fade) as i32).max(1);
        if radius > width as f64 {
            surface.circle(color, self.position - offset, radius, width);
        }
    }
}

// ---------- spark (a shot blocked by a shield) ----------

const SPARK_TIME: f64 = 0.2;
const SPARK_COLOR: Color = rgb(255, 240, 200);

#[derive(Debug, Clone)]
pub struct Spark {
    pub position: Vec2,
    streaks: Vec<Vec2>,
    pub age: f64,
}

impl Spark {
    /// `direction`: where the streaks fly (roughly – the shot bounces back off the shield).
    pub fn new(position: Vec2, direction: Vec2) -> Spark {
        let base = if direction.length_squared() > 0.0 { direction } else { vec2(1.0, 0.0) };
        let streaks = (0..6).map(|_| base.normalize().rotate(rng::uniform(-70.0, 70.0)) * rng::uniform(10.0, 22.0)).collect();
        Spark { position, streaks, age: 0.0 }
    }
    pub fn done(&self) -> bool {
        self.age >= SPARK_TIME
    }
    pub fn update(&mut self, dt: f64) {
        self.age += dt;
    }
    pub fn draw(&self, surface: &mut Surface, offset: Vec2) {
        let progress = (self.age / SPARK_TIME).min(1.0);
        let color = shade(SPARK_COLOR, 1.0 - progress);
        let center = self.position - offset;
        for s in &self.streaks {
            surface.line(color, center + *s * progress * 0.4, center + *s * progress, 2);
        }
    }
}

// ---------- the artillery's blast ----------

const BLAST_TIME: f64 = 0.7;
const FLASH_TIME: f64 = 0.06;
const FIRE_COLORS: [[f64; 3]; 5] = [[255.0, 255.0, 255.0], [255.0, 230.0, 120.0], [255.0, 140.0, 40.0], [190.0, 50.0, 20.0], [60.0, 20.0, 10.0]];
const SCORCH_COLOR: Color = rgb(90, 40, 20);

/// The colour at `progress` (0 = start, 1 = end), blended between FIRE_COLORS.
fn fire_color(progress: f64) -> Color {
    let position = progress.min(0.999) * (FIRE_COLORS.len() - 1) as f64;
    let (index, blend) = (position as usize, position - position.trunc());
    let (a, b) = (FIRE_COLORS[index], FIRE_COLORS[index + 1]);
    round_color([a[0] + (b[0] - a[0]) * blend, a[1] + (b[1] - a[1]) * blend, a[2] + (b[2] - a[2]) * blend])
}

#[derive(Debug, Clone)]
pub struct ArtilleryBlast {
    pub position: Vec2,
    pub radius: f64,
    pub age: f64,
    debris: Vec<(Vec2, f64)>,
}

impl ArtilleryBlast {
    pub fn new(position: Vec2, radius: f64) -> ArtilleryBlast {
        let debris = (0..14).map(|_| (vec2(radius, 0.0).rotate(rng::uniform(0.0, 360.0)) * rng::uniform(1.0, 2.2), rng::uniform(3.0, 6.0))).collect();
        ArtilleryBlast { position, radius, age: 0.0, debris }
    }
    pub fn done(&self) -> bool {
        self.age >= BLAST_TIME
    }
    pub fn update(&mut self, dt: f64) {
        self.age += dt;
    }
    pub fn draw(&self, surface: &mut Surface, offset: Vec2) {
        let center = self.position - offset;
        let progress = (self.age / BLAST_TIME).min(1.0);
        // scorch ring at the circle's edge, fading out over the whole time
        surface.circle(shade(SCORCH_COLOR, 1.0 - progress), center, self.radius, 3);
        if self.age < FLASH_TIME {
            surface.circle(rgb(255, 255, 255), center, self.radius, 0);
            return;
        }
        // fireball: starts as big as the circle, shrinks fast at first
        let fire = (self.age - FLASH_TIME) / (BLAST_TIME - FLASH_TIME);
        let size = self.radius * (1.0 - fire).powi(2);
        if size > 1.0 {
            surface.circle(fire_color(fire), center, size, 0);
            surface.circle(fire_color(fire * 0.5), center, size * 0.55, 0); // hotter core
        }
        // debris: flies out, slows down, fades
        let travel = 1.0 - (1.0 - progress).powi(2);
        for (reach, piece) in &self.debris {
            let spot = center + *reach * travel;
            surface.rect(fire_color(0.3 + 0.7 * progress), Rect::new(spot.x - piece / 2.0, spot.y - piece / 2.0, *piece, *piece), 0);
        }
    }
}

// ---------- burning ground ----------

/// Seconds between two pulses of damage.
const PULSE: f64 = 0.5;
const FADE_TIME: f64 = 0.6;
const GROUND: Color = rgb(255, 90, 30);
const EMBER: Color = rgb(255, 190, 80);

#[derive(Debug, Clone)]
pub struct BurningGround {
    pub position: Vec2,
    pub radius: f64,
    /// Per pulse.
    pub damage: f64,
    pub duration: f64,
    pub age: f64,
    next_pulse: f64,
    /// Pulses due this tick (read by the scene).
    pub pulses: u32,
    embers: Vec<(Vec2, f64)>,
}

impl BurningGround {
    pub fn new(position: Vec2, radius: f64, damage_per_second: f64, duration: f64) -> BurningGround {
        let embers = (0..18).map(|_| (vec2(radius * rng::random().sqrt(), 0.0).rotate(rng::uniform(0.0, 360.0)), rng::uniform(0.0, std::f64::consts::TAU))).collect();
        BurningGround { position, radius, damage: damage_per_second * PULSE, duration, age: 0.0, next_pulse: PULSE, pulses: 0, embers }
    }
    pub fn done(&self) -> bool {
        self.age >= self.duration
    }
    pub fn update(&mut self, dt: f64) {
        self.age += dt;
        self.pulses = 0;
        while self.age >= self.next_pulse && self.next_pulse <= self.duration {
            self.pulses += 1;
            self.next_pulse += PULSE;
        }
    }
    pub fn draw(&self, surface: &mut Surface, offset: Vec2) {
        let fade = ((self.duration - self.age) / FADE_TIME).min(1.0);
        let size = (self.radius * 2.0).ceil() as i32;
        let c = vec2(size as f64 / 2.0, size as f64 / 2.0);
        let radius = self.radius;
        let ground = sprite_cache::get(&format!("burning ground {size} {radius}"), || {
            let mut g = Surface::new_alpha(size, size);
            g.circle(GROUND.with_alpha(55), c, radius, 0);
            g.circle(GROUND.with_alpha(140), c, radius, 2);
            Some(g)
        });
        scratch(size, size, |layer| {
            if let Some(g) = &ground {
                layer.blit(g, 0, 0);
            }
            for (spot, phase) in &self.embers {
                let flicker = 0.5 + 0.5 * (self.age * 9.0 + phase).sin();
                layer.circle(EMBER.with_alpha(py_round(220.0 * flicker)), *spot + c, 2.0, 0);
            }
            layer.set_alpha(py_round(255.0 * fade));
            let p = self.position - offset - c;
            surface.blit(layer, p.x as i32, p.y as i32);
        });
    }
}

// ---------- slow field ----------

const SLOW_COLOR: Color = rgb(90, 170, 255);
const RINGS: u32 = 3;
/// Seconds a ring takes from the edge to the middle.
const RING_TIME: f64 = 1.2;

#[derive(Debug, Clone)]
pub struct SlowField {
    pub position: Vec2,
    pub radius: f64,
    /// What the enemies' speed is multiplied with.
    pub factor: f64,
    pub duration: f64,
    pub age: f64,
}

impl SlowField {
    /// `slow`: share slower, e.g. 0.2 = 20 % slower.
    pub fn new(position: Vec2, radius: f64, slow: f64, duration: f64) -> SlowField {
        SlowField { position, radius, factor: (1.0 - slow).max(0.0), duration, age: 0.0 }
    }
    pub fn done(&self) -> bool {
        self.age >= self.duration
    }
    pub fn update(&mut self, dt: f64) {
        self.age += dt;
    }
    /// Does it reach an enemy at `center` of `size`?
    pub fn covers(&self, center: Vec2, size: f64) -> bool {
        center.distance_to(self.position) <= self.radius + size / 2.0
    }
    pub fn draw(&self, surface: &mut Surface, offset: Vec2) {
        let fade = ((self.duration - self.age) / FADE_TIME).clamp(0.0, 1.0);
        let size = (self.radius * 2.0).ceil() as i32 + 2;
        let c = vec2(size as f64 / 2.0, size as f64 / 2.0);
        let radius = self.radius;
        let field = sprite_cache::get(&format!("slow field {size} {radius}"), || {
            let mut g = Surface::new_alpha(size, size);
            g.circle(SLOW_COLOR.with_alpha(45), c, radius, 0);
            g.circle(SLOW_COLOR.with_alpha(150), c, radius, 2);
            Some(g)
        });
        scratch(size, size, |layer| {
            if let Some(g) = &field {
                layer.blit(g, 0, 0);
            }
            for ring in 0..RINGS {
                let share = 1.0 - crate::engine::geom::py_mod(self.age / RING_TIME + ring as f64 / RINGS as f64, 1.0); // 1 at the edge → 0 in the middle
                if share * self.radius >= 2.0 {
                    layer.circle(SLOW_COLOR.with_alpha(py_round(90.0 * share)), c, share * self.radius, 1);
                }
            }
            layer.set_alpha(py_round(255.0 * fade));
            let p = self.position - offset - c;
            surface.blit(layer, p.x as i32, p.y as i32);
        });
    }
}

// ---------- buff glow ----------

pub const GLOW_COLOR: Color = rgb(255, 70, 150);
/// Pixels between the enemy and the outline.
pub const GLOW_GAP: i32 = 4;

/// The outline for an enemy of `w` × `h`, as a picture – its top left corner GLOW_GAP up and left of the enemy's.
pub fn buff_glow(w: i32, h: i32, color: Color) -> Option<std::rc::Rc<Surface>> {
    sprite_cache::get(&format!("buff glow {w} {h} {color:?}"), || {
        let mut glow = Surface::new_alpha(w + 2 * GLOW_GAP, h + 2 * GLOW_GAP);
        let r = glow.get_rect();
        glow.rect_round(color, r, 2, Radii::all(3));
        Some(glow)
    })
}

// ---------- the Laser's beam ----------

/// Pixels – far enough to cross any screen.
const BEAM_LENGTH: f64 = 3000.0;
const BEAM_CORE: Color = rgb(255, 255, 235);
/// The last part of its time it gets thinner.
const FADE_FROM: f64 = 0.6;

#[derive(Debug, Clone)]
pub struct LaserBeam {
    /// The enemy firing it (the beam starts at its centre, and ends when it dies).
    pub source: u64,
    /// Where it starts – the source's centre, updated by the scene every tick.
    pub start: Vec2,
    pub source_alive: bool,
    pub direction: Vec2,
    pub width: f64,
    pub duration: f64,
    pub damage: f64,
    pub color: Color,
    pub age: f64,
    /// It has already hurt the ship (once per beam).
    pub hurt_ship: bool,
}

impl LaserBeam {
    pub fn done(&self) -> bool {
        self.age >= self.duration || !self.source_alive
    }
    pub fn line(&self) -> (Vec2, Vec2) {
        (self.start, self.start + self.direction * BEAM_LENGTH)
    }
    pub fn update(&mut self, dt: f64) {
        self.age += dt;
    }
    /// True if the beam touches `rect`.
    pub fn hits(&self, rect: &FRect) -> bool {
        let (a, b) = self.line();
        rect.to_i().inflate(self.width, self.width).clipline(a, b)
    }
    pub fn draw(&self, surface: &mut Surface, offset: Vec2) {
        let (a, b) = self.line();
        let progress = if self.duration > 0.0 { self.age / self.duration } else { 1.0 };
        let fade = if progress < FADE_FROM { 1.0 } else { ((1.0 - progress) / (1.0 - FADE_FROM)).max(0.2) };
        surface.line(self.color, a - offset, b - offset, (py_round(self.width * fade) as i32).max(2));
        surface.line(BEAM_CORE, a - offset, b - offset, (py_round(self.width * fade / 3.0) as i32).max(1));
    }
}

// ---------- the artillery's shells ----------

const SHADOW_COLOR: Color = rgb(40, 40, 40);
pub const SHELL_COLOR: Color = rgb(70, 80, 35);
pub const SHELL_RING: Color = rgb(200, 210, 150);
/// Pixels of height at which a shell looks twice as big (closer to the viewer).
const GROW_HEIGHT: f64 = 400.0;

/// The shell's way up and down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arc {
    /// Pixels above the ground at the highest point (on top of the start height) …
    pub height: f64,
    /// … reached at this part of the way (0.5: halfway, less: rises steeply, falls long).
    pub peak: f64,
    /// The height it is thrown from (the catapult's bucket).
    pub start_height: f64,
}

impl Arc {
    pub fn flat(height: f64) -> Arc {
        Arc { height, peak: 0.5, start_height: 0.0 }
    }
    /// Height at `progress` (0 thrown … 1 landed): a curve through the peak, plus the start height sinking to 0.
    pub fn height_at(&self, progress: f64) -> f64 {
        let peak = self.peak.clamp(0.05, 0.95);
        let part = if progress < peak { (peak - progress) / peak } else { (progress - peak) / (1.0 - peak) };
        self.height * (1.0 - part * part) + self.start_height * (1.0 - progress)
    }
}

/// The arc for a throw from `start` to `target`: height plus steepness pixels per pixel of distance.
pub fn arc_for(start: Vec2, target: Vec2, height: f64, steepness: f64, peak: f64, start_height: f64) -> Arc {
    Arc { height: height + steepness * start.distance_to(target), peak, start_height }
}

/// How a splitter shell bursts (the Cluster's stats split_…).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Split {
    pub count: f64,
    pub spread: f64,
    pub min_spread: f64,
    pub radius: f64,
    pub damage: f64,
    pub time: f64,
    pub arc_height: f64,
}

#[derive(Debug, Clone)]
pub struct ArtilleryStrike {
    pub start: Vec2,
    pub center: Vec2,
    pub radius: f64,
    /// Seconds until it lands.
    pub delay: f64,
    pub damage: f64,
    pub arc: Arc,
    pub split: Option<Split>,
    pub size: f64,
    pub color: Color,
    pub age: f64,
    pub landed: bool,
}

impl ArtilleryStrike {
    /// `start`: where it is thrown from, `center`: where it lands, `delay`: seconds of flight. With `split` it bursts
    /// into fragments instead of hurting anything itself.
    pub fn new(start: Vec2, center: Vec2, radius: f64, delay: f64, damage: f64, arc: Arc, split: Option<Split>, size: f64, color: Color) -> ArtilleryStrike {
        let damage = if split.is_some() { 0.0 } else { damage };
        ArtilleryStrike { start, center, radius, delay, damage, arc, split, size, color, age: 0.0, landed: false }
    }
    /// 0 just fired, 1 lands now.
    pub fn progress(&self) -> f64 {
        if self.delay > 0.0 { (self.age / self.delay).min(1.0) } else { 1.0 }
    }
    /// True in the tick it lands (only once).
    pub fn update(&mut self, dt: f64) -> bool {
        self.age += dt;
        if self.landed || self.age < self.delay {
            return false;
        }
        self.landed = true;
        true
    }
    /// True if any part of `rect` is inside the circle.
    pub fn hits(&self, rect: &FRect) -> bool {
        let nearest = vec2(self.center.x.clamp(rect.left() as f64, rect.right() as f64), self.center.y.clamp(rect.top() as f64, rect.bottom() as f64));
        self.center.distance_to(nearest) <= self.radius
    }
    /// A splitter shell that just landed: its fragments, evenly round the spot (a little off). Other shells: none.
    pub fn fragments(&self) -> Vec<ArtilleryStrike> {
        let Some(split) = self.split else { return vec![] };
        let count = (py_round(split.count) as i64).max(1);
        let turn = rng::uniform(0.0, 360.0);
        (0..count)
            .map(|i| {
                let angle = turn + i as f64 * 360.0 / count as f64 + rng::uniform(-0.15, 0.15) * 360.0 / count as f64;
                let distance = rng::uniform(split.min_spread.min(split.spread), split.spread);
                let spot = self.center + vec2(distance, 0.0).rotate(angle);
                ArtilleryStrike::new(self.center, spot, split.radius, split.time, split.damage, Arc::flat(split.arc_height), None, self.size * 0.6, self.color)
            })
            .collect()
    }
    /// (over which spot on the ground it is, how high) right now.
    pub fn position(&self) -> (Vec2, f64) {
        let p = self.progress();
        (self.start.lerp(self.center, p), self.arc.height_at(p))
    }
    /// On the ground: the circle (a thin ring for a splitter) and the shell's shadow.
    pub fn draw(&self, surface: &mut Surface, offset: Vec2) {
        if self.split.is_some() {
            draw_landing_marker(surface, self.center - offset, self.radius);
        } else {
            draw_target_marker(surface, self.center - offset, self.radius, self.progress(), self.age);
        }
        let (ground, height) = self.position();
        let shadow = self.size / 2.0 * (1.0 - 0.5 * (height / GROW_HEIGHT).min(1.0));
        surface.circle(SHADOW_COLOR, ground - offset, shadow.max(2.0), 0);
    }
    /// The shell in the air (above the enemies) – higher up on the screen the higher it is, a bit bigger too.
    pub fn draw_air(&self, surface: &mut Surface, offset: Vec2) {
        let (ground, height) = self.position();
        let center = ground - offset - vec2(0.0, height);
        let radius = self.size / 2.0 * (1.0 + height / GROW_HEIGHT);
        surface.circle(self.color, center, radius, 0);
        surface.circle(SHELL_RING, center, radius, 2);
        if let Some(split) = self.split {
            // the fragments inside, so it looks different
            let n = (py_round(split.count) as i64).max(1);
            for i in 0..n {
                let dot = center + vec2(radius * 0.45, 0.0).rotate(i as f64 * 360.0 / split.count + self.age * 400.0);
                surface.circle(SHELL_RING, dot, (radius * 0.2).max(1.5), 0);
            }
        }
    }
}
