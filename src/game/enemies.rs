//! The enemies – `game/enemies/*.py` (what they do) and `game/skins/*.py` (how they look) in one place.
//!
//! Every enemy is one `Enemy`: what all of them share (rect, HP, buffs, facing, …) plus its type's own state
//! (`State`). Its numbers are read with `stat("speed")`: the value for this run (`enemies.json` and the level's
//! enemy_stats, put together by the roster) with the buffs on top. The fallbacks in `TYPES` are only used if
//! `enemies.json` lacks a value.
//!
//! Damage to the ship (contact_damage, projectile_damage, blast_damage) is in pixels: how far the arena's edges move
//! in on average (`Arena::hit`).
//!
//! Facing: every enemy has a direction it faces (`aim`). Ranged enemies turn it towards the ship at a limited speed
//! (`turn_towards`, stat "turn_speed") and only shoot while the ship is within their "fire_cone" in front of them
//! (`facing`). They spawn facing the ship.
//!
//! Armor (stat "armor"): every hit from the player does that much less damage – flat. Slowing (a mine's slow field):
//! `slow(factor)` for the current tick only, the strongest counts. Shots of the player first ask `blocks`: an enemy
//! can stop a shot without being hurt (the shield, the Pusher's body). Auras: the Commander buffs the others near it
//! (`command`). Swarms: every enemy of a swarm shares one `Swarm`. Tiers: some types have an evolution (tier 2) –
//! their bodies get a dark frame.
//!
//! The Python version kept references between objects (a Carrier's critters, a Laser's beam, the watchtower that lures
//! an enemy); here they are shared cells (`Rc<RefCell<…>>`), the same objects the scene holds.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use serde_json::Value;

use super::arena::Arena;
use super::buildings::TowerRef;
use super::effects::{self, ArtilleryStrike, LaserBeam, SHELL_COLOR, SHELL_RING, Split, arc_for, buff_glow};
use super::projectile::{Projectile, ShotStats};
use super::roster::{Buff, RosterRef, Stats};
use super::settings::{self, reduce};
use super::shapes::shade;
use super::spawner::Spawner;
use crate::engine::geom::{py_mod, py_round};
use crate::engine::gfx::{Blend, Radii};
use crate::engine::sprite_cache;
use crate::engine::{Anchor, Color, FRect, Rect, Surface, Vec2, ViewId, rgb, vec2};
use crate::rng;

/// Seconds an enemy flashes white after a hit.
pub const HIT_FLASH: f64 = 0.1;

// ---------- the types ----------

pub struct TypeInfo {
    /// Its name in the data files.
    pub key: &'static str,
    pub name: &'static str,
    /// 2: the evolution of a tier 1 type.
    pub tier: u8,
    /// Only fights by touching – a watchtower nearby can lure it away from the ship.
    pub melee: bool,
    /// Has a shield that can block the player's shots.
    pub shielded: bool,
    /// Buffs the enemies near it.
    pub commands: bool,
    /// Pushed apart from other enemies – false: walks through them.
    pub solid: bool,
    /// Leaves a star and, rarely, a core when killed.
    pub drops_loot: bool,
    /// The spawner puts it beyond the edge of the map.
    pub spawns_outside: bool,
    /// The skin's colour (death explosion, markers, …).
    pub color: Color,
    /// Fallback only – the real values are in `enemies.json`.
    pub stats: fn() -> Vec<(&'static str, Value)>,
}

macro_rules! stats {
    ($($name:literal: $v:expr),* $(,)?) => {
        || vec![$(($name, Value::from($v))),*]
    };
}

const fn t(key: &'static str, name: &'static str, color: Color, stats: fn() -> Vec<(&'static str, Value)>) -> TypeInfo {
    TypeInfo { key, name, tier: 1, melee: false, shielded: false, commands: false, solid: true, drops_loot: true, spawns_outside: false, color, stats }
}

pub static TYPES: &[TypeInfo] = &[
    TypeInfo { melee: true, ..t("basic", "Basic", rgb(230, 40, 40), stats!("base_hp": 2, "speed": 110, "size": 26, "contact_damage": 25)) },
    TypeInfo {
        melee: true,
        ..t("charger", "Charger", rgb(255, 140, 30), stats!(
            "base_hp": 3, "speed": 85, "size": 34, "contact_damage": 50,
            "charge_range": 280, "windup_time": 0.55, "charge_speed": 700, "charge_time": 0.5, "recover_time": 0.8, "charge_mass": 20,
        ))
    },
    t("shooter", "Shooter", rgb(160, 60, 230), stats!(
        "base_hp": 2, "speed": 95, "size": 28, "contact_damage": 25,
        "shooting_distance": 320, "fire_range": 520, "fire_interval": 1.5, "turn_speed": 140, "fire_cone": 20,
        "projectile_damage": 25, "projectile_speed": 380, "projectile_size": 10,
    )),
    t("sniper", "Sniper", rgb(235, 205, 50), stats!(
        "base_hp": 1, "speed": 80, "size": 22, "contact_damage": 25,
        "preferred_distance": 650, "back_off": 150, "close_in": 80, "fire_range": 900, "aim_time": 1.2, "lock_time": 0.3,
        "reload_time": 2.2, "turn_speed": 70, "fire_cone": 30, "projectile_damage": 50, "projectile_speed": 1400, "projectile_size": 6,
    )),
    t("carrier", "Carrier", rgb(40, 190, 150), stats!(
        "base_hp": 6, "speed": 75, "size": 40, "contact_damage": 25,
        "preferred_distance": 480, "back_off": 100, "close_in": 60, "spawn_range": 900, "spawn_interval": 0.9,
        "max_critters": 8, "first_wave": 5, "spawn_amount": 1,
    )),
    TypeInfo { melee: true, drops_loot: false, ..t("critter", "Critter", rgb(140, 245, 200), stats!("base_hp": 1, "speed": 175, "size": 12, "contact_damage": 25)) },
    t("artillery", "Artillery", rgb(125, 140, 60), stats!(
        "base_hp": 3, "speed": 60, "size": 32, "contact_damage": 25,
        "fire_range": 900, "min_range": 220, "setup_time": 1.0, "pack_time": 0.8, "reload_time": 3.5, "turn_speed": 60,
        "fire_cone": 16, "fill_time": 1.5, "aim_spread": 60, "blast_radius": 80, "blast_damage": 50, "swing_time": 0.3,
        "arc_height": 60, "arc_steepness": 0.15, "arc_peak": 0.5, "shell_size": 14,
    )),
    TypeInfo { tier: 2, melee: true, ..t("fat", "Fat Guy", rgb(185, 30, 40), stats!("base_hp": 8, "speed": 90, "size": 38, "contact_damage": 25)) },
    TypeInfo {
        tier: 2,
        ..t("cannoneer", "Cannoneer", rgb(120, 40, 180), stats!(
            "base_hp": 5, "speed": 45, "size": 36, "contact_damage": 25,
            "shooting_distance": 380, "fire_range": 650, "fire_interval": 4.0, "turn_speed": 60, "fire_cone": 14,
            "projectile_damage": 100, "projectile_speed": 140, "projectile_size": 44,
        ))
    },
    TypeInfo {
        tier: 2,
        melee: true,
        shielded: true,
        ..t("shield", "Shield", rgb(70, 110, 170), stats!(
            "base_hp": 4, "speed": 55, "size": 30, "contact_damage": 50,
            "turn_speed": 40, "shield_width": 54, "shield_arc": 150, "dash_range": 300, "dash_cone": 20, "windup_time": 0.7,
            "dash_speed": 650, "dash_time": 0.5, "recover_time": 1.2, "dash_mass": 30, "knockback": 70, "stun_time": 0.5,
        ))
    },
    TypeInfo {
        tier: 2,
        ..t("laser", "Laser", rgb(225, 150, 25), stats!(
            "base_hp": 2, "speed": 70, "size": 26, "contact_damage": 25,
            "preferred_distance": 650, "back_off": 150, "close_in": 80, "fire_range": 900, "aim_time": 1.5, "lock_time": 0.4,
            "reload_time": 3.0, "turn_speed": 60, "fire_cone": 30, "projectile_damage": 50, "beam_time": 0.5, "beam_width": 16,
        ))
    },
    TypeInfo {
        tier: 2,
        ..t("cluster", "Cluster", rgb(110, 100, 45), stats!(
            "base_hp": 5, "speed": 50, "size": 36, "contact_damage": 25,
            "fire_range": 950, "min_range": 240, "setup_time": 1.2, "pack_time": 0.9, "reload_time": 4.0, "turn_speed": 50,
            "fire_cone": 16, "fill_time": 1.4, "aim_spread": 40, "swing_time": 0.35, "arc_height": 80, "arc_steepness": 0.18,
            "arc_peak": 0.5, "shell_size": 18, "split_count": 3, "split_spread": 170, "split_min_spread": 100, "split_radius": 40,
            "split_damage": 35, "split_time": 0.8, "split_arc_height": 60,
        ))
    },
    TypeInfo {
        commands: true,
        ..t("commander", "Commander", rgb(235, 180, 30), stats!(
            "base_hp": 5, "speed": 85, "size": 30, "contact_damage": 25,
            "preferred_distance": 220, "back_off": 80, "close_in": 40, "aura_radius": 170, "aura_time": 3, "aura_speed": 1.2, "aura_damage": 1.25,
        ))
    },
    TypeInfo {
        solid: false,
        spawns_outside: true,
        ..t("pusher", "Pusher", rgb(45, 60, 130), stats!("base_hp": 10, "speed": 70, "size": 46, "contact_damage": 25, "push_speed": 30, "arm_length": 44, "arm_width": 18))
    },
    TypeInfo {
        solid: false,
        ..t("living_window", "Living Window", YOUNG, stats!(
            "base_hp": 1, "speed": 60, "size": 80, "contact_damage": 0,
            "min_size": 30, "max_size": 220, "grow_speed": 6, "shrink_per_damage": 6, "overlap": 30, "slide_speed": 25,
            "spawn_interval": 5, "burst_count": 8,
        ))
    },
];

pub fn type_of(key: &str) -> Option<&'static TypeInfo> {
    TYPES.iter().find(|t| t.key == key)
}
pub fn is_type(key: &str) -> bool {
    type_of(key).is_some()
}

// ---------- the enemy ----------

/// One swarm: what its enemies share – the type it is made of, for its marker.
#[derive(Debug)]
pub struct Swarm {
    pub kind: String,
}

pub type EnemyRef = Rc<RefCell<Enemy>>;
pub type BeamRef = Rc<RefCell<LaserBeam>>;

/// Charger and Shield go through these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Approach,
    Windup,
    /// Charging / dashing.
    Charge,
    Recover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Siege {
    Moving,
    SettingUp,
    Sieged,
    Packing,
}

pub enum State {
    Plain,
    Charger { step: Step, timer: f64, direction: Vec2 },
    Shield { step: Step, timer: f64 },
    /// The Shooter and the Cannoneer.
    Shooter { cooldown: f64 },
    /// The Sniper and the Laser (with its beam).
    Sniper { aiming: bool, timer: f64, beam: Option<BeamRef> },
    Carrier { timer: f64, critters: Vec<EnemyRef>, first_wave: bool },
    /// The Artillery and the Cluster.
    Artillery { siege: Siege, deploy: f64, timer: f64, arm: f64, loaded: bool, swing: Option<f64> },
    Pusher { body: FRect, holding: bool, side: Option<&'static str>, along: f64 },
    LivingWindow { docked: bool, side: Option<&'static str>, along: f64, turn: f64, timer: f64 },
}

/// What an enemy fires.
pub enum Attack {
    Shot(Projectile),
    Beam(BeamRef),
    Strike(ArtilleryStrike),
}

/// What an enemy may look at and change while it acts.
pub struct Ctx<'a> {
    pub ship_center: Vec2,
    pub arena: Option<&'a mut Arena>,
    pub spawner: Option<&'a mut Spawner>,
}

thread_local! {
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
}

pub struct Enemy {
    pub id: u64,
    pub info: &'static TypeInfo,
    pub roster: RosterRef,
    pub base: Stats,
    /// On this enemy only (buffs on all enemies are in the roster).
    pub buffs: Vec<Buff>,
    pub rect: FRect,
    pub hp: f64,
    pub flash: f64,
    /// Set by the scene once it has been completely inside a window (arena or extra).
    pub seen: bool,
    /// Enemies created this tick – the scene moves them into its list.
    pub spawned: Vec<EnemyRef>,
    /// A watchtower it goes for instead of the ship (set by the scene, melee enemies only).
    pub lure: Option<TowerRef>,
    /// The direction it faces (length 1).
    pub aim: Vec2,
    oriented: bool,
    /// 1 = normal speed; `slow` lowers it for one tick.
    pub speed_factor: f64,
    /// Damage it takes × this – more in a watchtower's area (set by the scene every tick).
    pub damage_factor: f64,
    pub swarm: Option<Rc<Swarm>>,
    /// The Living Window drops nothing when it bursts.
    pub drops_loot: bool,
    /// Its own window out there (Pusher, Living Window) – set by the scene.
    pub view: Option<ViewId>,
    pub state: State,
}

impl Enemy {
    pub fn new(key: &str, position: Vec2, roster: RosterRef) -> Enemy {
        let info = type_of(key).expect("a known enemy type");
        let base = roster.borrow().stats.get(key).cloned().unwrap_or_default();
        let id = NEXT_ID.with(|n| {
            let id = n.get();
            n.set(id + 1);
            id
        });
        let mut e = Enemy {
            id,
            info,
            roster,
            base,
            buffs: vec![],
            rect: FRect::default(),
            hp: 0.0,
            flash: 0.0,
            seen: false,
            spawned: vec![],
            lure: None,
            aim: vec2(1.0, 0.0),
            oriented: false,
            speed_factor: 1.0,
            damage_factor: 1.0,
            swarm: None,
            drops_loot: info.drops_loot,
            view: None,
            state: State::Plain,
        };
        let size = e.stat("size");
        e.rect = FRect::new(0.0, 0.0, size, size);
        e.rect.set_centerv(position);
        e.hp = e.stat("base_hp");
        e.state = match key {
            "charger" => State::Charger { step: Step::Approach, timer: 0.0, direction: vec2(1.0, 0.0) },
            "shield" => State::Shield { step: Step::Approach, timer: 0.0 },
            // so a group does not fire in sync
            "shooter" | "cannoneer" => State::Shooter { cooldown: rng::uniform(0.5, e.stat("fire_interval")) },
            "sniper" | "laser" => State::Sniper { aiming: false, timer: e.stat("reload_time") / 2.0, beam: None },
            "carrier" => State::Carrier { timer: e.stat("spawn_interval"), critters: vec![], first_wave: true },
            "artillery" | "cluster" => State::Artillery { siege: Siege::Moving, deploy: 0.0, timer: 0.0, arm: ARM_REST, loaded: false, swing: None },
            "pusher" => State::Pusher { body: e.rect, holding: false, side: None, along: 0.0 },
            "living_window" => State::LivingWindow { docked: false, side: None, along: 0.0, turn: if rng::chance(0.5) { 1.0 } else { -1.0 }, timer: e.stat("spawn_interval") },
            _ => State::Plain,
        };
        e
    }
    pub fn shared(key: &str, position: Vec2, roster: RosterRef) -> EnemyRef {
        Rc::new(RefCell::new(Enemy::new(key, position, roster)))
    }

    /// The value for this run with every active buff: (value + adds) × factors.
    pub fn stat(&self, name: &str) -> f64 {
        let (mut add, mut factor) = (0.0, 1.0);
        for b in self.buffs.iter().filter(|b| b.stat == name) {
            add += b.add;
            factor *= b.factor;
        }
        let roster = self.roster.borrow();
        for b in roster.buffs_for(self.info.key).filter(|b| b.stat == name) {
            add += b.add;
            factor *= b.factor;
        }
        (self.base.get(name).copied().unwrap_or(0.0) + add) * factor
    }
    /// The colour of the first coloured active buff, or the usual glow – `None` without buffs.
    fn glow_color(&self) -> Option<Color> {
        let roster = self.roster.borrow();
        let all: Vec<&Buff> = self.buffs.iter().chain(roster.buffs_for(self.info.key)).collect();
        if all.is_empty() {
            return None;
        }
        Some(all.iter().find_map(|b| b.color).unwrap_or(effects::GLOW_COLOR))
    }
    /// Gives it buffs (all from one source, e.g. a Commander's): buffs it already has from that source are replaced.
    pub fn renew_buffs(&mut self, buffs: Vec<Buff>) {
        let source = buffs[0].source.clone();
        self.buffs.retain(|b| b.source != source);
        self.buffs.extend(buffs);
    }

    pub fn alive(&self) -> bool {
        self.hp > 0.0
    }
    pub fn size(&self) -> f64 {
        self.rect.w as f64
    }
    pub fn center(&self) -> Vec2 {
        self.rect.centerv()
    }
    /// How hard it is to push: bigger enemies shove smaller ones out of the way.
    pub fn mass(&self) -> f64 {
        let base = self.size().powi(2);
        match &self.state {
            State::Charger { step: Step::Charge, .. } => base * self.stat("charge_mass"),
            State::Shield { step: Step::Charge, .. } => base * self.stat("dash_mass"),
            State::Artillery { siege, .. } if *siege != Siege::Moving => f64::INFINITY, // rooted: go around it
            _ => base,
        }
    }

    /// A hit from the player: `amount` minus the armor (flat), at least MIN_DAMAGE_SHARE – times damage_factor.
    pub fn take_damage(&mut self, amount: f64) {
        let hit = reduce(amount, self.stat("armor"), settings::MIN_DAMAGE_SHARE) * self.damage_factor;
        if let State::LivingWindow { .. } = self.state {
            // a hit shrinks it – below min_size it is dead
            let size = self.rect.w as f64 - hit * self.stat("shrink_per_damage");
            self.resize(size);
            self.flash = HIT_FLASH;
            if self.rect.w as f64 <= self.stat("min_size") {
                self.hp = 0.0;
            }
            return;
        }
        self.hp -= hit;
        self.flash = HIT_FLASH;
    }

    // ---------- helpers ----------

    pub fn distance_to(&self, point: Vec2) -> f64 {
        self.center().distance_to(point)
    }
    /// Unit vector towards `point` (or (0, 0) if it is right here).
    pub fn direction_to(&self, point: Vec2) -> Vec2 {
        let d = point - self.center();
        if d.length_squared() > 0.0 { d.normalize() } else { Vec2::ZERO }
    }
    /// Slower for this tick, e.g. 0.8 = 20 % slower. Only the strongest slow counts.
    pub fn slow(&mut self, factor: f64) {
        self.speed_factor = self.speed_factor.min(factor);
    }
    fn walk(&mut self, direction: Vec2, speed: f64, dt: f64) {
        let d = direction * speed * self.speed_factor * dt;
        self.rect.move_ip(d.x, d.y);
    }
    /// Turns aim towards `point` by at most turn_speed degrees per second.
    fn turn_towards(&mut self, point: Vec2, dt: f64) {
        let wanted = self.direction_to(point);
        if wanted.length_squared() == 0.0 {
            return;
        }
        let angle = py_mod(self.aim.angle_to(wanted) + 180.0, 360.0) - 180.0; // the shorter way round
        let step = self.stat("turn_speed") * dt;
        self.aim = self.aim.rotate(angle.clamp(-step, step));
    }
    /// True if `point` lies within the cone (degrees, all of it – `None`: the "fire_cone" stat) in front of it.
    pub fn facing(&self, point: Vec2, cone: Option<f64>) -> bool {
        let wanted = self.direction_to(point);
        if wanted.length_squared() == 0.0 {
            return true;
        }
        let cone = cone.unwrap_or_else(|| self.stat("fire_cone"));
        (py_mod(self.aim.angle_to(wanted) + 180.0, 360.0) - 180.0).abs() <= cone / 2.0
    }
    /// Walks closer when further than preferred + close_in, backs off when closer than preferred - back_off.
    fn keep_distance(&mut self, target: Vec2, dt: f64) {
        let (preferred, back_off, close_in) = (self.stat("preferred_distance"), self.stat("back_off"), self.stat("close_in"));
        let distance = self.distance_to(target);
        let direction = self.direction_to(target);
        if distance > preferred + close_in {
            self.walk(direction, self.stat("speed"), dt);
        } else if distance < preferred - back_off {
            self.walk(-direction, self.stat("speed"), dt);
        }
    }
    /// The values of its shots.
    fn shot_stats(&self, color: Color) -> ShotStats {
        ShotStats::new(self.stat("projectile_damage"), self.stat("projectile_speed"), 0.0, self.stat("projectile_size"), color)
    }
    /// Where it is heading: the watchtower luring it, if there is one, else the ship.
    fn target(&self, ship: Vec2) -> Vec2 {
        if let Some(l) = &self.lure {
            let l = l.borrow();
            if l.alive() {
                return l.center();
            }
        }
        ship
    }

    /// It just hurt the ship by touching it (the hit counted): the Shield's dash knocks it back.
    pub fn on_hit_ship(&self) -> Option<(Vec2, f64, f64)> {
        if let State::Shield { step: Step::Charge, .. } = self.state {
            return Some((self.aim, self.stat("knockback"), self.stat("stun_time")));
        }
        None
    }

    /// True if it stops the player's shot without being hurt: a shield, or the Pusher's armored body.
    pub fn blocks(&self, shot: &Projectile) -> bool {
        match &self.state {
            State::Shield { .. } => {
                let (start, end) = self.shield_line();
                if shot.rect.clipline(start, end) {
                    return true;
                }
                if !shot.rect.colliderect(&self.rect) {
                    return false;
                }
                let came_from = -shot.direction;
                (py_mod(self.aim.angle_to(came_from) + 180.0, 360.0) - 180.0).abs() <= self.stat("shield_arc") / 2.0
            }
            State::Pusher { holding, .. } => !holding,
            _ => false,
        }
    }
    /// The shield as a line (two points) in front of the body, across the direction it faces.
    pub fn shield_line(&self) -> (Vec2, Vec2) {
        let middle = self.center() + self.aim * (self.size() / 2.0 + SHIELD_GAP);
        let across = self.aim.rotate(90.0) * self.stat("shield_width") / 2.0;
        (middle - across, middle + across)
    }

    /// An aura: buff the enemies near it (only the Commander does something here).
    pub fn command(&self, enemies: &[EnemyRef]) {
        if !self.info.commands {
            return;
        }
        let (radius, time) = (self.stat("aura_radius"), self.stat("aura_time"));
        let (speed, damage) = (self.stat("aura_speed"), self.stat("aura_damage"));
        let center = self.center();
        for other in enemies {
            let Ok(mut other) = other.try_borrow_mut() else { continue }; // itself
            if other.info.commands || !other.alive() || center.distance_to(other.center()) > radius {
                continue;
            }
            let mut buffs = vec![Buff { factor: speed, duration: Some(time), source: Some(self.info.key.into()), color: Some(AURA_COLOR), ..Buff::new("speed") }];
            for stat in DAMAGE_STATS {
                buffs.push(Buff { factor: damage, duration: Some(time), source: Some(self.info.key.into()), ..Buff::new(stat) });
            }
            other.renew_buffs(buffs);
        }
    }

    // ---------- every tick ----------

    pub fn update(&mut self, dt: f64, ctx: &mut Ctx) -> Vec<Attack> {
        if !self.oriented {
            // it comes in already facing the ship – turning is for keeping up with it
            self.oriented = true;
            let d = self.direction_to(ctx.ship_center);
            if d.length_squared() > 0.0 {
                self.aim = d;
            }
        }
        self.flash = (self.flash - dt).max(0.0);
        for b in &mut self.buffs {
            b.update(dt);
        }
        self.buffs.retain(|b| !b.expired());
        let attacks = self.act(dt, ctx);
        self.speed_factor = 1.0; // slows have to be applied again next tick
        attacks
    }

    fn act(&mut self, dt: f64, ctx: &mut Ctx) -> Vec<Attack> {
        let ship = ctx.ship_center;
        match self.info.key {
            "charger" => self.act_charger(dt, ship),
            "shield" => self.act_shield(dt, ship),
            "shooter" | "cannoneer" => self.act_shooter(dt, ship),
            "sniper" | "laser" => self.act_sniper(dt, ship),
            "carrier" => self.act_carrier(dt, ship),
            "artillery" | "cluster" => self.act_artillery(dt, ship),
            "commander" => {
                self.keep_distance(ship, dt);
                vec![]
            }
            "pusher" => self.act_pusher(dt, ctx),
            "living_window" => self.act_living_window(dt, ctx),
            _ => {
                // run straight at the ship (or the watchtower luring it), never shoot
                let target = self.target(ship);
                let d = self.direction_to(target);
                self.walk(d, self.stat("speed"), dt);
                vec![]
            }
        }
    }

    fn act_charger(&mut self, dt: f64, ship: Vec2) -> Vec<Attack> {
        let target = self.target(ship);
        let State::Charger { mut step, mut timer, mut direction } = self.state else { return vec![] };
        match step {
            Step::Approach => {
                let d = self.direction_to(target);
                self.walk(d, self.stat("speed"), dt);
                if self.distance_to(target) <= self.stat("charge_range") {
                    (step, timer) = (Step::Windup, self.stat("windup_time"));
                }
            }
            Step::Windup => {
                let d = self.direction_to(target);
                if d.length_squared() > 0.0 {
                    direction = d;
                }
                timer -= dt;
                if timer <= 0.0 {
                    (step, timer) = (Step::Charge, self.stat("charge_time")); // direction is locked from here on
                }
            }
            Step::Charge => {
                self.walk(direction, self.stat("charge_speed"), dt);
                timer -= dt;
                if timer <= 0.0 {
                    (step, timer) = (Step::Recover, self.stat("recover_time"));
                }
            }
            Step::Recover => {
                timer -= dt;
                if timer <= 0.0 {
                    step = Step::Approach;
                }
            }
        }
        self.state = State::Charger { step, timer, direction };
        vec![]
    }

    fn act_shield(&mut self, dt: f64, ship: Vec2) -> Vec<Attack> {
        let target = self.target(ship);
        let State::Shield { mut step, mut timer } = self.state else { return vec![] };
        match step {
            Step::Approach => {
                self.turn_towards(target, dt);
                self.walk(self.aim, self.stat("speed"), dt); // only ever where the shield points
                if self.distance_to(target) <= self.stat("dash_range") && self.facing(target, Some(self.stat("dash_cone"))) {
                    (step, timer) = (Step::Windup, self.stat("windup_time"));
                }
            }
            Step::Windup => {
                self.turn_towards(target, dt); // still turning (slowly) while it winds up
                timer -= dt;
                if timer <= 0.0 {
                    (step, timer) = (Step::Charge, self.stat("dash_time")); // the direction is locked from here on
                }
            }
            Step::Charge => {
                self.walk(self.aim, self.stat("dash_speed"), dt);
                timer -= dt;
                if timer <= 0.0 {
                    (step, timer) = (Step::Recover, self.stat("recover_time"));
                }
            }
            Step::Recover => {
                timer -= dt;
                if timer <= 0.0 {
                    step = Step::Approach;
                }
            }
        }
        self.state = State::Shield { step, timer };
        vec![]
    }

    fn act_shooter(&mut self, dt: f64, target: Vec2) -> Vec<Attack> {
        let distance = self.distance_to(target);
        self.turn_towards(target, dt);
        if distance > self.stat("shooting_distance") {
            let d = self.direction_to(target);
            self.walk(d, self.stat("speed"), dt);
        }
        let State::Shooter { cooldown } = &mut self.state else { return vec![] };
        *cooldown = (*cooldown - dt).max(0.0);
        let ready = *cooldown <= 0.0;
        if ready && distance <= self.stat("fire_range") && self.facing(target, None) {
            let interval = self.stat("fire_interval");
            if let State::Shooter { cooldown } = &mut self.state {
                *cooldown = interval;
            }
            let cannon = self.info.key == "cannoneer";
            let stats = self.shot_stats(if cannon { CANNON_SHOT } else { SHOOTER_SHOT });
            let shot = if cannon { Projectile::cannon_ball(self.center(), self.aim, stats) } else { Projectile::new(self.center(), self.aim, stats) };
            return vec![Attack::Shot(shot)];
        }
        vec![]
    }

    /// The Laser is firing while its beam is on (it stands still and does not turn).
    pub fn firing(&self) -> bool {
        match &self.state {
            State::Sniper { beam: Some(b), .. } => {
                let b = b.borrow();
                b.age < b.duration
            }
            _ => false,
        }
    }
    pub fn aiming(&self) -> bool {
        matches!(self.state, State::Sniper { aiming: true, .. })
    }
    /// The last part of aiming: the laser stops following – the moment to dodge.
    pub fn locked(&self) -> bool {
        match self.state {
            State::Sniper { aiming, timer, .. } => aiming && timer <= self.stat("lock_time"),
            _ => false,
        }
    }

    fn act_sniper(&mut self, dt: f64, target: Vec2) -> Vec<Attack> {
        if self.info.key == "laser" {
            if self.firing() {
                return vec![]; // stands still, does not turn – reloading starts once the beam is off
            }
            if let State::Sniper { beam, .. } = &mut self.state {
                *beam = None;
            }
        }
        if !self.locked() {
            self.turn_towards(target, dt);
        }
        if !self.aiming() {
            // only walks while not aiming
            self.keep_distance(target, dt);
        }
        let State::Sniper { aiming, timer, .. } = &mut self.state else { return vec![] };
        *timer -= dt;
        if *timer > 0.0 {
            return vec![];
        }
        if !*aiming {
            let can = self.distance_to(target) <= self.stat("fire_range") && self.facing(target, None);
            if can {
                let aim_time = self.stat("aim_time");
                if let State::Sniper { aiming, timer, .. } = &mut self.state {
                    (*aiming, *timer) = (true, aim_time);
                }
            }
            return vec![];
        }
        let reload = self.stat("reload_time");
        if let State::Sniper { aiming, timer, .. } = &mut self.state {
            (*aiming, *timer) = (false, reload);
        }
        if self.info.key == "laser" {
            let beam = Rc::new(RefCell::new(LaserBeam {
                source: self.id,
                start: self.center(),
                source_alive: true,
                direction: self.aim,
                width: self.stat("beam_width"),
                duration: self.stat("beam_time"),
                damage: self.stat("projectile_damage"),
                color: BEAM_COLOR,
                age: 0.0,
                hurt_ship: false,
            }));
            if let State::Sniper { beam: b, .. } = &mut self.state {
                *b = Some(beam.clone());
            }
            return vec![Attack::Beam(beam)];
        }
        // the shot at the end of aiming: one fast bullet where it faces
        vec![Attack::Shot(Projectile::new(self.center(), self.aim, self.shot_stats(SNIPER_SHOT)))]
    }

    /// 0 right after a critter came out, 1 when the next one is ready.
    pub fn hangar_progress(&self) -> f64 {
        match self.state {
            State::Carrier { timer, .. } => 1.0 - (timer / self.stat("spawn_interval")).min(1.0),
            _ => 0.0,
        }
    }

    fn act_carrier(&mut self, dt: f64, target: Vec2) -> Vec<Attack> {
        self.keep_distance(target, dt);
        let max = self.stat("max_critters") as i64;
        let in_range = self.distance_to(target) <= self.stat("spawn_range");
        let (interval, first, amount) = (self.stat("spawn_interval"), self.stat("first_wave") as i64, py_round(self.stat("spawn_amount")) as i64);
        let State::Carrier { timer, critters, first_wave } = &mut self.state else { return vec![] };
        critters.retain(|c| c.try_borrow().map(|c| c.alive()).unwrap_or(true));
        *timer = (*timer - dt).max(0.0); // 0: hangar full – the next critter comes out once there is room
        if *timer == 0.0 && (critters.len() as i64) < max && in_range {
            *timer = interval;
            let room = max - critters.len() as i64;
            let count = if *first_wave { first } else { room.min(amount) };
            *first_wave = false;
            for _ in 0..count {
                self.release_critter();
            }
        }
        vec![]
    }

    /// A new critter right next to the carrier, on a random side (not on top of it, so they do not shove).
    fn release_critter(&mut self) {
        let critter = Enemy::shared("critter", self.center(), self.roster.clone());
        let side = vec2((self.size() + critter.borrow().size()) / 2.0 + 4.0, 0.0).rotate(rng::uniform(0.0, 360.0));
        critter.borrow_mut().rect.set_centerv(self.center() + side);
        if let State::Carrier { critters, .. } = &mut self.state {
            critters.push(critter.clone());
        }
        self.spawned.push(critter);
    }

    fn act_artillery(&mut self, dt: f64, target: Vec2) -> Vec<Attack> {
        let distance = self.distance_to(target);
        self.turn_towards(target, dt);
        let State::Artillery { mut siege, mut deploy, mut timer, mut arm, mut loaded, mut swing } = self.state else { return vec![] };
        if siege != Siege::Sieged {
            // walking or packing: fold the arm back, no throw
            swing = None;
            arm = (arm - PACK_ARM_SPEED * dt).max(ARM_REST);
        }
        let (min_range, fire_range) = (self.stat("min_range"), self.stat("fire_range"));
        let mut thrown = vec![];
        'act: {
            if siege == Siege::Moving {
                if self.seen && distance < min_range + RANGE_MARGIN {
                    let d = -self.direction_to(target);
                    self.walk(d, self.stat("speed"), dt); // too close: back off
                } else if self.seen && distance <= fire_range {
                    siege = Siege::SettingUp;
                } else {
                    let d = self.direction_to(target);
                    self.walk(d, self.stat("speed"), dt);
                }
                break 'act;
            }
            if (siege == Siege::SettingUp || siege == Siege::Sieged) && (distance < min_range || distance > fire_range + RANGE_MARGIN) {
                siege = Siege::Packing;
            }
            if siege == Siege::SettingUp {
                deploy = (deploy + dt / self.stat("setup_time")).min(1.0);
                if deploy >= 1.0 {
                    siege = Siege::Sieged;
                    loaded = loaded || arm <= ARM_REST;
                }
                break 'act;
            }
            if siege == Siege::Packing {
                deploy = (deploy - dt / self.stat("pack_time")).max(0.0);
                if deploy <= 0.0 {
                    siege = Siege::Moving;
                }
                break 'act;
            }
            if let Some(s) = swing {
                // sieged and swinging forward – lets go at the top; the arm speeds up like a catapult's
                let s = s + dt;
                let part = (s / self.stat("swing_time").max(0.01)).min(1.0);
                arm = ARM_REST + (ARM_FORWARD - ARM_REST) * part * part;
                swing = Some(s);
                if loaded && arm >= ARM_RELEASE {
                    loaded = false;
                    let spot = target + self.spread();
                    thrown.push(Attack::Strike(self.launch(spot)));
                }
                if part >= 1.0 {
                    swing = None;
                }
                break 'act;
            }
            timer -= dt;
            let reload = self.stat("reload_time");
            if !loaded {
                // cranking back, then the next shell goes in
                let back = (ARM_FORWARD - ARM_REST) * dt / (reload * CRANK_SHARE).max(0.05);
                arm = (arm - back).max(ARM_REST);
                loaded = arm <= ARM_REST;
            }
            if timer > 0.0 || !loaded || !self.facing(target, None) {
                break 'act;
            }
            timer = reload;
            swing = Some(0.0);
        }
        self.state = State::Artillery { siege, deploy, timer, arm, loaded, swing };
        thrown
    }

    /// The shell for `spot`, thrown from the bucket at the top of the swing (the Cluster's bursts into fragments).
    fn launch(&self, spot: Vec2) -> ArtilleryStrike {
        let arc = arc_for(self.center(), spot, self.stat("arc_height"), self.stat("arc_steepness"), self.stat("arc_peak"), self.size() * ARM_LENGTH);
        if self.info.key == "cluster" {
            let split = Split {
                count: self.stat("split_count"),
                spread: self.stat("split_spread"),
                min_spread: self.stat("split_min_spread"),
                radius: self.stat("split_radius"),
                damage: self.stat("split_damage"),
                time: self.stat("split_time"),
                arc_height: self.stat("split_arc_height"),
            };
            return ArtilleryStrike::new(self.center(), spot, self.stat("split_radius"), self.stat("fill_time"), 0.0, arc, Some(split), self.stat("shell_size"), CLUSTER_SHELL);
        }
        ArtilleryStrike::new(self.center(), spot, self.stat("blast_radius"), self.stat("fill_time"), self.stat("blast_damage"), arc, None, self.stat("shell_size"), SHELL_COLOR)
    }
    /// A random offset up to aim_spread long, evenly over the whole disc.
    fn spread(&self) -> Vec2 {
        let distance = self.stat("aim_spread") * rng::random().sqrt();
        vec2(distance, 0.0).rotate(rng::uniform(0.0, 360.0))
    }

    // ---------- the Pusher ----------

    pub fn holding(&self) -> bool {
        matches!(self.state, State::Pusher { holding: true, .. })
    }
    /// What its own window shows: the Pusher's body, the Living Window as a whole.
    pub fn body(&self) -> FRect {
        match self.state {
            State::Pusher { body, .. } => body,
            _ => self.rect,
        }
    }
    pub fn side(&self) -> Option<&'static str> {
        match self.state {
            State::Pusher { side, .. } | State::LivingWindow { side, .. } => side,
            _ => None,
        }
    }

    fn act_pusher(&mut self, dt: f64, ctx: &mut Ctx) -> Vec<Attack> {
        let Some(arena) = ctx.arena.as_deref_mut() else { return vec![] };
        if !self.holding() {
            self.pusher_walk(dt, arena);
        }
        if let State::Pusher { holding: true, side: Some(side), .. } = self.state {
            arena.shove(opposite(side), self.stat("push_speed") * self.speed_factor * dt);
            self.hold_on(arena);
        }
        vec![]
    }

    /// Towards the nearest point just outside the arena's edge; there it grabs the edge.
    fn pusher_walk(&mut self, dt: f64, arena: &Arena) {
        let State::Pusher { body, .. } = self.state else { return };
        let center = body.centerv();
        let side = arena.side_hit(center).unwrap_or_else(|| nearest_side(&arena.rect, center));
        let spot = self.pusher_spot(arena, &body, side, along_of(center, side));
        let offset = spot - center;
        let step = self.stat("speed") * self.speed_factor * dt;
        if offset.length() <= step {
            self.state = State::Pusher { body, holding: true, side: Some(side), along: along_of(spot, side) };
            return;
        }
        let mut body = body;
        body.set_centerv(center + offset.normalize() * step);
        if arena.rect.colliderect(&body) {
            // came at a corner: step straight out of the arena again
            body.set_centerv(self.pusher_spot(arena, &body, side, along_of(center, side)));
        }
        self.rect = body;
        if let State::Pusher { body: b, .. } = &mut self.state {
            *b = body;
        }
    }

    /// Where its centre is when it holds `side` at `along` (kept within the edge, so the arm fits).
    fn pusher_spot(&self, arena: &Arena, body: &FRect, side: &str, along: f64) -> Vec2 {
        let a = arena.rect;
        let half = body.w as f64 / 2.0 + GAP;
        let reach = self.stat("arm_width") / 2.0;
        if side == "left" || side == "right" {
            let y = if a.h as f64 > 2.0 * reach { along.max(a.top() as f64 + reach).min(a.bottom() as f64 - reach) } else { a.centery() as f64 };
            return vec2(if side == "left" { a.left() as f64 - half } else { a.right() as f64 + half }, y);
        }
        let x = if a.w as f64 > 2.0 * reach { along.max(a.left() as f64 + reach).min(a.right() as f64 - reach) } else { a.centerx() as f64 };
        vec2(x, if side == "top" { a.top() as f64 - half } else { a.bottom() as f64 + half })
    }

    /// Its body stays glued to the edge it holds; the arm reaches into the arena from there (its hitbox).
    fn hold_on(&mut self, arena: &Arena) {
        let State::Pusher { mut body, side: Some(side), along, .. } = self.state else { return };
        let a = arena.rect;
        body.set_centerv(self.pusher_spot(arena, &body, side, along));
        let (length, width) = (self.stat("arm_length"), self.stat("arm_width"));
        let arm = if side == "left" || side == "right" {
            let mut arm = FRect::new(0.0, 0.0, length.min(a.w as f64), width);
            arm.set_centery(body.centery());
            if side == "left" { arm.set_left(a.left()) } else { arm.set_right(a.right()) }
            arm
        } else {
            let mut arm = FRect::new(0.0, 0.0, width, length.min(a.h as f64));
            arm.set_centerx(body.centerx());
            if side == "top" { arm.set_top(a.top()) } else { arm.set_bottom(a.bottom()) }
            arm
        };
        self.rect = arm;
        if let State::Pusher { body: b, .. } = &mut self.state {
            *b = body;
        }
    }

    // ---------- the Living Window ----------

    fn resize(&mut self, size: f64) {
        let c = self.rect.centerv();
        self.rect.set_size(size.max(1.0), size.max(1.0));
        self.rect.set_centerv(c);
    }

    fn act_living_window(&mut self, dt: f64, ctx: &mut Ctx) -> Vec<Attack> {
        if ctx.arena.is_none() {
            return vec![];
        }
        let d = self.direction_to(ctx.ship_center); // its eye follows the ship
        if d.length_squared() > 0.0 {
            self.aim = d;
        }
        let grown = self.rect.w as f64 + self.stat("grow_speed") * dt;
        self.resize(grown);
        if self.rect.w as f64 >= self.stat("max_size") {
            // grown too big: a whole group comes out, and it is gone (no loot)
            let n = py_round(self.stat("burst_count")) as usize;
            self.release(n, ctx);
            self.hp = 0.0;
            self.drops_loot = false;
            return vec![];
        }
        let docked = matches!(self.state, State::LivingWindow { docked: true, .. });
        if !docked {
            self.drift(dt, ctx.arena.as_deref().unwrap());
        } else {
            self.glide(dt, ctx.arena.as_deref().unwrap());
            let interval = self.stat("spawn_interval");
            let mut release = false;
            if let State::LivingWindow { timer, .. } = &mut self.state {
                *timer -= dt;
                if *timer <= 0.0 {
                    *timer = interval;
                    release = true;
                }
            }
            if release {
                self.release(1, ctx);
            }
        }
        vec![]
    }

    /// Towards the nearest edge of the arena; once there, it docks.
    fn drift(&mut self, dt: f64, arena: &Arena) {
        let center = self.rect.centerv();
        let side = arena.side_hit(center).unwrap_or("top");
        let along = along_of(center, side);
        let spot = self.window_spot(arena, side, along);
        let offset = spot - center;
        let step = self.stat("speed") * self.speed_factor * dt;
        if offset.length() <= step {
            if let State::LivingWindow { docked, side: s, along: a, .. } = &mut self.state {
                (*docked, *s, *a) = (true, Some(side), along);
            }
            self.rect.set_centerv(spot);
            return;
        }
        self.rect.set_centerv(center + offset.normalize() * step);
    }

    /// On along the edges – past a corner onto the next one.
    fn glide(&mut self, dt: f64, arena: &Arena) {
        let State::LivingWindow { side: Some(side), along, turn, .. } = self.state else { return };
        let a = arena.rect;
        let (l, r, t, b) = (a.left() as f64, a.right() as f64, a.top() as f64, a.bottom() as f64);
        let (width, height) = (a.w as f64, a.h as f64);
        let order = ["top", "right", "bottom", "left"];
        let lengths = [width, height, width, height];
        // where it is, counted clockwise from the top left corner
        let into = match side {
            "top" => along.clamp(l, r) - l,
            "right" => along.clamp(t, b) - t,
            "bottom" => r - along.clamp(l, r),
            _ => b - along.clamp(t, b),
        };
        let index = order.iter().position(|s| *s == side).unwrap();
        let mut way = lengths[..index].iter().sum::<f64>() + into;
        way = py_mod(way + turn * self.stat("slide_speed") * self.speed_factor * dt, 2.0 * (width + height));
        let mut now = order[3];
        for (i, s) in order.iter().enumerate() {
            now = s;
            if way <= lengths[i] {
                break;
            }
            way -= lengths[i];
        }
        let along = match now {
            "top" => l + way,
            "right" => t + way,
            "bottom" => r - way,
            _ => b - way,
        };
        if let State::LivingWindow { side: s, along: al, .. } = &mut self.state {
            (*s, *al) = (Some(now), along);
        }
        let spot = self.window_spot(arena, now, along);
        self.rect.set_centerv(spot);
    }

    /// Its centre when docked at `side`, at `along`: reaching overlap pixels into the arena.
    fn window_spot(&self, arena: &Arena, side: &str, along: f64) -> Vec2 {
        let a = arena.rect;
        let half = self.rect.w as f64 / 2.0;
        let reach = self.stat("overlap");
        if side == "left" || side == "right" {
            let y = along.max(a.top() as f64).min(a.bottom() as f64);
            return vec2(if side == "left" { a.left() as f64 - half + reach } else { a.right() as f64 + half - reach }, y);
        }
        let x = along.max(a.left() as f64).min(a.right() as f64);
        vec2(x, if side == "top" { a.top() as f64 - half + reach } else { a.bottom() as f64 + half - reach })
    }

    /// `count` enemies out of it – the types the level brings right now (via the spawner).
    fn release(&mut self, count: usize, ctx: &mut Ctx) {
        let Some(spawner) = ctx.spawner.as_deref_mut() else { return };
        let weights: Vec<(String, f64)> = spawner.weights().into_iter().filter(|(k, _)| k != self.info.key && !type_of(k).is_some_and(|t| t.spawns_outside)).collect();
        if weights.is_empty() {
            return;
        }
        let w: Vec<f64> = weights.iter().map(|(_, w)| *w).collect();
        for _ in 0..count {
            let key = &weights[rng::weighted(&w).unwrap_or(0)].0;
            let spot = self.rect.centerv() + vec2(rng::uniform(0.0, self.rect.w as f64 / 3.0), 0.0).rotate(rng::uniform(0.0, 360.0));
            let enemy = spawner.create(key, spot);
            self.spawned.push(enemy);
        }
    }

    // ---------- how it looks (the skins) ----------

    pub fn draw(&self, surface: &mut Surface, offset: Vec2) {
        match self.info.key {
            "pusher" => {
                let r = self.body().moved(-offset.x, -offset.y);
                self.draw_skin(surface, r);
            }
            "living_window" => self.draw_window(surface, self.rect.moved(-offset.x, -offset.y)),
            _ => self.draw_skin(surface, self.rect.moved(-offset.x, -offset.y)),
        }
    }

    /// `rect`: the enemy's body in window coordinates.
    fn draw_skin(&self, surface: &mut Surface, rect: FRect) {
        self.draw_below(surface, rect);
        let (w, h) = (rect.w as i32, rect.h as i32);
        if let Some(color) = self.glow_color() {
            if let Some(glow) = buff_glow(w, h, color) {
                surface.blit(&glow, rect.x as i32 - effects::GLOW_GAP, rect.y as i32 - effects::GLOW_GAP);
            }
        }
        let flash = self.flash > 0.0;
        match picture_file(self.info.key) {
            None => {
                let (color, tier) = (self.info.color, self.info.tier);
                let body = sprite_cache::get(&format!("body {} {w} {h} {tier} {flash}", self.info.key), || {
                    let mut b = Surface::new_alpha(w, h);
                    b.fill(if flash { FLASH_COLOR } else { color });
                    if tier == 2 {
                        let r = b.get_rect();
                        draw_tier_frame(&mut b, r, color);
                    }
                    Some(b)
                });
                if let Some(b) = body {
                    surface.blit(&b, rect.x as i32, rect.y as i32);
                }
            }
            Some(file) => {
                let body = turned_file(self.info.key, &file, (w, h), flash, self.aim);
                let c = rect.to_i().center();
                surface.blit_at(&body, Anchor::Center, c.0, c.1);
            }
        }
        self.draw_above(surface, rect);
        let max_hp = (py_round(self.stat("base_hp")) as i64).max(1);
        if self.hp < max_hp as f64 {
            self.draw_health(surface, rect, max_hp);
        }
    }

    fn draw_below(&self, surface: &mut Surface, rect: FRect) {
        let start = rect.centerv();
        match &self.state {
            State::Charger { step: Step::Windup, direction, .. } => surface.line(rgb(130, 70, 15), start, start + *direction * 160.0, 4),
            State::Shield { step, .. } if *step == Step::Windup => surface.line(rgb(40, 70, 120), start, start + self.aim * 200.0, 4),
            State::Shooter { .. } => {
                let (color, length, width) = if self.info.key == "cannoneer" { (rgb(190, 120, 240), 0.95, 16) } else { (rgb(220, 160, 255), 1.15, 6) };
                surface.line(color, start, start + self.aim * self.size() * length, width);
            }
            State::Sniper { .. } => {
                let laser = self.info.key == "laser";
                if self.aiming() {
                    let (color, width) = match (laser, self.locked()) {
                        (false, true) => (rgb(255, 235, 90), 2),
                        (false, false) => (rgb(110, 95, 30), 1),
                        (true, true) => (rgb(255, 190, 70), 4),
                        (true, false) => (rgb(110, 75, 20), 3),
                    };
                    surface.line(color, start, start + self.aim * 3000.0, width);
                }
                let (color, width) = if laser { (rgb(255, 215, 140), 7) } else { (rgb(255, 245, 170), 4) };
                surface.line(color, start, start + self.aim * self.size() * 1.1, width);
            }
            State::Artillery { deploy, .. } if *deploy > 0.0 => {
                let side = self.size() * FOOT_SIZE;
                let out = self.size() * FOOT_OUT * deploy; // at 0 just inside the corner: hidden under the body
                for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                    let mut foot = Rect::new(0.0, 0.0, side, side);
                    foot.set_center(rect.centerx() as f64 + x * (rect.w as f64 / 2.0 + out - side / 2.0), rect.centery() as f64 + y * (rect.h as f64 / 2.0 + out - side / 2.0));
                    surface.rect(FOOT_COLOR, foot, 0);
                }
            }
            State::Pusher { holding: true, side: Some(side), body, .. } => {
                // the arm – moved the same way as the body
                let shift = rect.topleftv() - body.topleftv();
                let arm = self.rect.moved(shift.x, shift.y);
                let color = if self.flash > 0.0 { FLASH_COLOR } else { PUSHER_ARM };
                surface.rect(color, arm.to_i(), 0);
                let mut fist = FRect::new(0.0, 0.0, arm.w as f64 + 8.0, arm.h as f64 + 8.0);
                if *side == "left" || *side == "right" {
                    fist.w = arm.h + 8.0;
                    fist.set_center(if *side == "left" { arm.right() - fist.w / 2.0 } else { arm.left() + fist.w / 2.0 }, arm.centery());
                } else {
                    fist.h = arm.w + 8.0;
                    fist.set_center(arm.centerx(), if *side == "top" { arm.bottom() - fist.h / 2.0 } else { arm.top() + fist.h / 2.0 });
                }
                surface.rect_round(color, fist.to_i(), 0, Radii::all(4));
            }
            State::Plain if self.info.commands => surface.circle(rgb(110, 85, 20), rect.centerv(), self.stat("aura_radius"), 1),
            _ => {}
        }
    }

    fn draw_above(&self, surface: &mut Surface, rect: FRect) {
        match &self.state {
            State::Carrier { .. } => {
                let side = self.size() * 0.6 * self.hangar_progress();
                let mut hangar = Rect::new(0.0, 0.0, side, side);
                hangar.set_centerv(rect.centerv());
                surface.rect(rgb(15, 70, 55), hangar, 0);
            }
            State::Shield { .. } => {
                let offset = rect.centerv() - self.center(); // world → window
                let (a, b) = self.shield_line();
                surface.line(SHIELD_EDGE, a + offset, b + offset, SHIELD_THICKNESS + 2);
                surface.line(SHIELD_COLOR, a + offset, b + offset, SHIELD_THICKNESS);
            }
            State::Artillery { arm, loaded, .. } => {
                let (center, aim) = (rect.centerv(), self.aim);
                let across = aim.rotate(90.0) * self.size() * CROSSBAR_WIDTH / 2.0;
                let bar = center + aim * self.size() * CROSSBAR_AT;
                surface.line(FRAME_COLOR, bar - across, bar + across, 4);
                // the arm turns about the middle, in the upright plane through aim: -cos along the ground, sin upwards
                let length = self.size() * ARM_LENGTH;
                let angle = arm.to_radians();
                let up = angle.sin();
                let bucket = center + aim * length * -angle.cos() - vec2(0.0, length * up);
                surface.line(ARM_COLOR, center, bucket, 5);
                surface.circle(FRAME_COLOR, center, 4.0, 0); // the axle
                let radius = self.size() * BUCKET_SIZE * (1.0 + 0.3 * up);
                surface.circle(ARM_COLOR, bucket, radius, 0);
                if *loaded {
                    let shell = radius * 0.75;
                    let color = if self.info.key == "cluster" { CLUSTER_SHELL } else { SHELL_COLOR };
                    surface.circle(color, bucket, shell, 0);
                    surface.circle(SHELL_RING, bucket, shell, 1);
                }
            }
            State::Pusher { side, .. } => {
                // two light stripes across the body, along the way it shoves
                let horizontal = matches!(side, Some("left") | Some("right") | None);
                for share in [0.33, 0.67] {
                    if horizontal {
                        let y = rect.top() as f64 + rect.h as f64 * share;
                        surface.line(STRIPE_COLOR, vec2(rect.left() as f64 + 5.0, y), vec2(rect.right() as f64 - 6.0, y), 3);
                    } else {
                        let x = rect.left() as f64 + rect.w as f64 * share;
                        surface.line(STRIPE_COLOR, vec2(x, rect.top() as f64 + 5.0), vec2(x, rect.bottom() as f64 - 6.0), 3);
                    }
                }
            }
            State::Plain if self.info.commands => {
                // a small crown: three points on a band, in the body's dark shade
                let dark = shade(self.info.color, 0.35);
                let (w, h) = (rect.w as f64 * 0.6, rect.h as f64 * 0.35);
                let (left, bottom) = (rect.centerx() as f64 - w / 2.0, rect.centery() as f64 + h / 2.0);
                let points = [
                    vec2(left, bottom),
                    vec2(left, bottom - h),
                    vec2(left + w * 0.25, bottom - h * 0.45),
                    vec2(left + w * 0.5, bottom - h),
                    vec2(left + w * 0.75, bottom - h * 0.45),
                    vec2(left + w, bottom - h),
                    vec2(left + w, bottom),
                ];
                surface.polygon(dark, &points, 0);
            }
            _ => {}
        }
    }

    /// One pip per hit point above the enemy – filled ones are left.
    fn draw_health(&self, surface: &mut Surface, rect: FRect, max_hp: i64) {
        let filled = self.hp.ceil() as i64;
        let color = self.info.color;
        let pips = sprite_cache::get(&format!("pips {color:?} {max_hp} {filled}"), || {
            let (pw, ph) = PIP_SIZE;
            let mut p = Surface::new_alpha(max_hp as i32 * pw + (max_hp as i32 - 1) * 2, ph);
            for i in 0..max_hp {
                p.rect(if i < filled { color } else { PIP_EMPTY }, Rect::new(i as i32 * (pw + 2), 0, pw, ph), 0);
            }
            Some(p)
        });
        if let Some(p) = pips {
            surface.blit(&p, (rect.centerx() as f64 - p.width() as f64 / 2.0) as i32, (rect.top() as f64 - PIP_SIZE.1 as f64 - 4.0) as i32);
        }
    }

    /// The Living Window: a frame from young (turquoise) to ripe (red), a faint tint inside, and an eye.
    fn draw_window(&self, surface: &mut Surface, rect: FRect) {
        let (low, high) = (self.stat("min_size"), self.stat("max_size"));
        let ripe = ((rect.w as f64 - low) / (high - low).max(1.0)).clamp(0.0, 1.0);
        let color = if self.flash > 0.0 { FLASH_COLOR } else { mix(YOUNG, RIPE, ripe) };
        let r = rect.to_i();
        let mut tint = Surface::new(r.w.max(1), r.h.max(1));
        tint.fill(shade(color, TINT));
        surface.blit_mode(&tint, r.x, r.y, Blend::Add);
        surface.rect(color, r, 4);
        let inner = rect.inflate(-12.0, -12.0);
        if inner.w > 4.0 {
            surface.rect(shade(color, 0.4), inner.to_i(), 1);
        }
        let mut radius = (rect.w as f64 * 0.11).max(5.0);
        let mut center = rect.centerv();
        if let Some(side) = self.side() {
            // docked: into the part that reaches into the arena
            let overlap = self.stat("overlap");
            radius = radius.min(overlap * 0.32);
            let inward = match side {
                "left" => vec2(1.0, 0.0),
                "right" => vec2(-1.0, 0.0),
                "top" => vec2(0.0, 1.0),
                _ => vec2(0.0, -1.0),
            };
            center += inward * (rect.w as f64 / 2.0 - overlap / 2.0);
        }
        surface.ellipse(EYE, FRect::new(center.x - radius * 1.4, center.y - radius, radius * 2.8, radius * 2.0).to_i(), 0);
        surface.circle(PUPIL, center + self.aim * radius * 0.6, radius * 0.5, 0);
    }
}

fn opposite(side: &str) -> &'static str {
    match side {
        "left" => "right",
        "right" => "left",
        "top" => "bottom",
        _ => "top",
    }
}
fn along_of(center: Vec2, side: &str) -> f64 {
    if side == "left" || side == "right" { center.y } else { center.x }
}
fn nearest_side(arena: &FRect, c: Vec2) -> &'static str {
    let gaps = [("left", c.x - arena.left() as f64), ("right", arena.right() as f64 - c.x), ("top", c.y - arena.top() as f64), ("bottom", arena.bottom() as f64 - c.y)];
    gaps.iter().min_by(|a, b| a.1.total_cmp(&b.1)).unwrap().0
}

fn mix(low: Color, high: Color, share: f64) -> Color {
    let m = |a: u8, b: u8| py_round(a as f64 + (b as f64 - a as f64) * share);
    Color::f(m(low.r, high.r), m(low.g, high.g), m(low.b, high.b))
}

// ---------- pictures ----------

/// The folder with the player's own enemy pictures: `data\images\` next to the exe.
pub fn images_dir() -> std::path::PathBuf {
    kit::app::data_dir().join("images")
}

/// The enemy's picture file (`<key>.png`) – `None` if there is none. Read only once.
fn picture_file(key: &str) -> Option<Rc<Surface>> {
    if cfg!(test) {
        return None;
    }
    sprite_cache::get(&format!("file {key}"), || sprite_cache::load_picture(&images_dir().join(format!("{key}.png"))))
}

/// The picture file as the body: scaled to fit, white for a hit, turned towards `aim` (in TURN_STEP steps).
fn turned_file(key: &str, file: &Surface, size: (i32, i32), flash: bool, aim: Vec2) -> Rc<Surface> {
    // screen y points down: turn the other way
    let angle = py_mod(py_round(-aim.angle() / TURN_STEP) * TURN_STEP, 360.0);
    let scaled = sprite_cache::get(&format!("file {key} {size:?} {flash}"), || {
        let scale = (size.0 as f64 / file.width() as f64).min(size.1 as f64 / file.height() as f64);
        let s = file.smoothscaled((py_round(file.width() as f64 * scale) as i32).max(1), (py_round(file.height() as f64 * scale) as i32).max(1));
        Some(if flash { s.tinted(FLASH_COLOR) } else { s })
    })
    .expect("a scaled picture");
    sprite_cache::get(&format!("file {key} {size:?} {flash} {angle}"), || Some(scaled.rotated(angle))).expect("a turned picture")
}

/// The mark of an evolution: a dark frame inside the body (also used for the icons in the game menu).
pub fn draw_tier_frame(surface: &mut Surface, rect: Rect, color: Color) {
    let inset = py_round(rect.w as f64 * TIER_FRAME_INSET);
    let frame = rect.inflate(-2.0 * inset, -2.0 * inset);
    let width = (py_round(rect.w as f64 * TIER_FRAME_WIDTH) as i32).max(2);
    surface.rect(shade(color, TIER_FRAME_SHADE), frame, width);
}

// ---------- constants ----------

const FLASH_COLOR: Color = rgb(255, 255, 255);
/// Health pips above a damaged enemy.
const PIP_SIZE: (i32, i32) = (6, 4);
const PIP_EMPTY: Color = rgb(70, 70, 70);
const TIER_FRAME_SHADE: f64 = 0.45;
const TIER_FRAME_INSET: f64 = 0.18;
const TIER_FRAME_WIDTH: f64 = 0.1;
/// Degrees: a picture file is prepared turned in steps this big.
const TURN_STEP: f64 = 5.0;

const SHOOTER_SHOT: Color = rgb(200, 90, 255);
const CANNON_SHOT: Color = rgb(95, 30, 140);
const SNIPER_SHOT: Color = rgb(255, 235, 90);
const BEAM_COLOR: Color = rgb(255, 200, 80);
/// The outline of the enemies a Commander buffs.
pub const AURA_COLOR: Color = rgb(255, 200, 40);
pub const DAMAGE_STATS: [&str; 3] = ["contact_damage", "projectile_damage", "blast_damage"];

/// Pixels between the body and the shield.
const SHIELD_GAP: f64 = 4.0;
const SHIELD_COLOR: Color = rgb(200, 220, 245);
const SHIELD_EDGE: Color = rgb(120, 150, 190);
const SHIELD_THICKNESS: i32 = 7;

/// Degrees: the catapult's arm lying back …
const ARM_REST: f64 = 15.0;
/// … straight up: here it lets go of the shell …
const ARM_RELEASE: f64 = 90.0;
/// … and here it stops, thrown forward.
const ARM_FORWARD: f64 = 120.0;
/// Part of the reload it takes to crank the arm back.
const CRANK_SHARE: f64 = 0.7;
/// Degrees per second the arm folds back while it packs up or walks.
const PACK_ARM_SPEED: f64 = 200.0;
/// It sets up a bit inside its range and packs up a bit outside – no flickering.
const RANGE_MARGIN: f64 = 40.0;
/// The arm's length, as part of the size (the enemy throws its shells from that high).
const ARM_LENGTH: f64 = 0.9;
const ARM_COLOR: Color = rgb(200, 210, 150);
const FRAME_COLOR: Color = rgb(75, 85, 35);
const FOOT_COLOR: Color = rgb(90, 100, 45);
const FOOT_SIZE: f64 = 0.35;
const FOOT_OUT: f64 = 0.12;
const CROSSBAR_AT: f64 = 0.3;
const CROSSBAR_WIDTH: f64 = 0.9;
const BUCKET_SIZE: f64 = 0.17;
const CLUSTER_SHELL: Color = rgb(150, 60, 30);

/// Pixels between the Pusher's body and the arena's edge while it holds on.
const GAP: f64 = 2.0;
const PUSHER_ARM: Color = rgb(90, 120, 200);
const STRIPE_COLOR: Color = rgb(140, 165, 230);

/// The Living Window's frame at min_size …
const YOUNG: Color = rgb(60, 220, 200);
/// … and right before it bursts.
const RIPE: Color = rgb(240, 60, 60);
const EYE: Color = rgb(235, 245, 240);
const PUPIL: Color = rgb(20, 30, 30);
/// The area inside: the frame's colour this bright, added to what is there.
const TINT: f64 = 0.12;

