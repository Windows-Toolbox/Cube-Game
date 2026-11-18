//! Brings the enemies of a run (`enemies/spawner.py`).
//!
//! Every `interval` seconds one enemy appears around the window, its type picked by weight: each entry of the
//! level's "enemies" once its time has come ("after"), its weight growing by "growth" every minute after that.
//!
//! Phases: stretches of the run that change the weights and/or the time between two enemies, buff the enemies (as
//! roster buffs that last as long as the phase) and/or bring a few extra enemies or points of interest once, when
//! they start. A phase can announce itself on screen (`announcements`, which the scene shows and empties). A phase
//! with options gets one of them, picked when the run starts; a phase with "every" starts again and again.
//!
//! Swarms: once per second there is a small chance that a whole group appears at one spot – on the edge of the map,
//! at least SWARM_GAP away from the window and every extra window. A kind of swarm can have a leader (a Commander) in
//! its middle. Every enemy of a swarm shares one `Swarm`.
//!
//! Evolutions: every enemy may come as its evolution instead, with the chance the level gives for its type right
//! now. The extra windows (watchtowers, radar) count as part of what the player sees: no enemy appears inside one.

use std::collections::HashSet;
use std::rc::Rc;

use super::difficulty::{Difficulty, EnemyEntry, Evolution, Phase, Spawning, Swarms, apply, chosen, evolution_chance};
use super::enemies::{Enemy, EnemyRef, Swarm, type_of};
use super::roster::{RosterRef, buff_from, evolution_of};
use crate::engine::desktop::Desktop;
use crate::engine::{Rect, Vec2, vec2};
use crate::rng;

/// Pixels beyond the window's corners.
const SPAWN_DISTANCE: (f64, f64) = (250.0, 500.0);
/// How tightly packed a swarm is (pixels).
const SWARM_SPACING: f64 = 14.0;
/// Pixels at least between a swarm's nearest enemy and the window.
pub const SWARM_GAP: f64 = 300.0;
const SWARM_TRIES: usize = 24;
/// Seconds between enemies, whatever the phases say.
const MIN_INTERVAL: f64 = 0.1;
/// Pixels beyond the edge of the map where a Pusher appears.
const OUTSIDE_DISTANCE: f64 = 60.0;

pub struct Spawner {
    pub desktop: Desktop,
    pub roster: RosterRef,
    spawning: Spawning,
    entries: Vec<EnemyEntry>,
    swarms: Vec<Swarms>,
    /// The phases of this run (options picked) – repeating ones are added as they come.
    pub phases: Vec<Phase>,
    /// (phase, next start).
    repeating: Vec<(Phase, f64)>,
    evolutions: Vec<Evolution>,
    timer: f64,
    pub elapsed: f64,
    swarm_timer: f64,
    since_swarm: Vec<f64>,
    started: HashSet<usize>,
    /// Texts of phases that just began – the scene shows and empties it.
    pub announcements: Vec<String>,
    /// Kinds of points of interest phases just placed.
    pub points: Vec<String>,
    /// The extra windows' rectangles (set by `update`).
    pub windows: Vec<Rect>,
}

impl Spawner {
    pub fn new(desktop: Desktop, difficulty: &Difficulty, roster: RosterRef) -> Spawner {
        Spawner {
            desktop,
            roster,
            spawning: difficulty.spawning.clone(),
            entries: difficulty.enemies.clone(),
            swarms: difficulty.swarms.clone(),
            phases: difficulty.phases.iter().filter(|p| p.every == 0.0).map(chosen).collect(), // one_of: picked now
            repeating: difficulty.phases.iter().filter(|p| p.every != 0.0).map(|p| (p.clone(), p.start)).collect(),
            evolutions: difficulty.evolutions.clone(),
            timer: difficulty.spawning.first_spawn,
            elapsed: 0.0,
            swarm_timer: 1.0,
            since_swarm: difficulty.swarms.iter().map(|s| s.min_gap).collect(),
            started: HashSet::new(),
            announcements: vec![],
            points: vec![],
            windows: vec![],
        }
    }

    /// Seconds between two enemies right now: the level's (getting shorter over time), changed by the active phases'
    /// spawn_interval (in file order) – a phase may go below min_interval.
    pub fn interval(&self) -> f64 {
        let s = &self.spawning;
        let mut interval = s.min_interval.max(s.start_interval - s.interval_drop * self.elapsed);
        for (_, p) in self.active_phases() {
            if let Some((op, n)) = p.spawn_interval {
                interval = apply(interval, op, n);
            }
        }
        interval.max(MIN_INTERVAL)
    }
    fn swarm_chance(&self, s: &Swarms) -> f64 {
        s.max_chance.min(s.start_chance + s.chance_growth * self.elapsed)
    }
    fn swarm_size(&self, s: &Swarms) -> f64 {
        s.max_size.min(s.start_size + (self.elapsed / s.growth_time).trunc())
    }
    /// The phases running now, with their place in `phases`.
    pub fn active_phases(&self) -> Vec<(usize, &Phase)> {
        self.phases.iter().enumerate().filter(|(_, p)| p.start <= self.elapsed && p.end.is_none_or(|e| self.elapsed < e)).collect()
    }

    /// type → how likely it is right now (only types above 0), in the order they first appear.
    pub fn weights(&self) -> Vec<(String, f64)> {
        let mut w: Vec<(String, f64)> = vec![];
        let add = |w: &mut Vec<(String, f64)>, key: &str, f: &dyn Fn(f64) -> f64| match w.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = f(slot.1),
            None => w.push((key.to_string(), f(0.0))),
        };
        for e in &self.entries {
            if self.elapsed >= e.after {
                let minutes = (self.elapsed - e.after) / 60.0;
                add(&mut w, &e.kind, &|v| v + e.weight + e.growth * minutes);
            }
        }
        for (_, p) in self.active_phases() {
            for (key, op, n) in &p.weights {
                add(&mut w, key, &|v| apply(v, *op, *n));
            }
        }
        w.retain(|(_, v)| *v > 0.0);
        w
    }

    /// The new enemies (usually none). `arena`: the window's rectangle, `windows`: the extra windows' rectangles.
    pub fn update(&mut self, dt: f64, arena: Rect, windows: Vec<Rect>) -> Vec<EnemyRef> {
        self.windows = windows;
        self.elapsed += dt;
        self.repeat_phases();
        let mut new = self.start_phases(arena);
        new.extend(self.swarm(arena, dt));
        self.timer -= dt;
        if self.timer <= 0.0 {
            self.timer = self.interval();
            let w = self.weights();
            if !w.is_empty() {
                let weights: Vec<f64> = w.iter().map(|(_, v)| *v).collect();
                let key = w[rng::weighted(&weights).unwrap_or(0)].0.clone();
                let point = self.point_for(&key, arena);
                new.push(self.create(&key, point));
            }
        }
        new
    }

    /// An enemy of type `key` – or, with the level's chance for that type right now, its evolution.
    pub fn create(&self, key: &str, position: Vec2) -> EnemyRef {
        let chance = self.evolutions.iter().filter(|e| e.kind == key).map(|e| evolution_chance(e, self.elapsed)).fold(0.0, f64::max);
        let mut key = key.to_string();
        if chance > 0.0 && rng::random() < chance {
            if let Some(evo) = evolution_of(&key) {
                key = evo.to_string();
            }
        }
        Enemy::shared(&key, position, self.roster.clone())
    }

    /// Phases with "every" whose next time has come: a copy of each goes into the phases, starting now.
    fn repeat_phases(&mut self) {
        for (phase, next) in &mut self.repeating {
            while *next <= self.elapsed {
                let end = phase.end.map(|e| *next + e - phase.start);
                let mut copy = chosen(phase);
                copy.start = *next;
                copy.end = end;
                copy.every = 0.0;
                self.phases.push(copy);
                *next += phase.every;
            }
        }
    }

    /// Phases whose time has come: their buffs go on all enemies (until the phase ends), their text is shown, a
    /// faster spawn_interval counts right away. Returns their extra enemies.
    fn start_phases(&mut self, arena: Rect) -> Vec<EnemyRef> {
        let mut new = vec![];
        for i in 0..self.phases.len() {
            if self.started.contains(&i) || self.elapsed < self.phases[i].start {
                continue;
            }
            self.started.insert(i);
            let phase = self.phases[i].clone();
            if phase.end.is_some_and(|e| self.elapsed >= e) {
                continue; // already over (only if a single tick skipped all of it)
            }
            let duration = phase.end.map(|e| e - self.elapsed);
            for (target, stat, op, n) in &phase.enemy_stats {
                let target = if target == "all" { None } else { Some(target.clone()) };
                self.roster.borrow_mut().buffs.push(buff_from(stat, *op, *n, duration, target));
            }
            if let Some(a) = &phase.announce {
                self.announcements.push(a.clone());
            }
            for (key, count) in &phase.spawn {
                for _ in 0..*count {
                    let point = self.point_for(key, arena);
                    new.push(self.create(key, point));
                }
            }
            for (kind, count) in &phase.points {
                for _ in 0..*count {
                    self.points.push(kind.clone());
                }
            }
            if phase.spawn_interval.is_some() {
                self.timer = self.timer.min(self.interval()); // do not wait out the old, longer gap
            }
        }
        new
    }

    /// Once per second, for each kind of swarm: maybe one – a group of enemies packed around one spawn point.
    fn swarm(&mut self, arena: Rect, dt: f64) -> Vec<EnemyRef> {
        for s in &mut self.since_swarm {
            *s += dt;
        }
        self.swarm_timer -= dt;
        if self.swarm_timer > 0.0 {
            return vec![];
        }
        self.swarm_timer += 1.0;
        let mut new = vec![];
        for i in 0..self.swarms.len() {
            let s = self.swarms[i].clone();
            if self.elapsed < s.after || self.since_swarm[i] < s.min_gap || rng::random() >= self.swarm_chance(&s) {
                continue;
            }
            self.since_swarm[i] = 0.0;
            let size = self.swarm_size(&s);
            let radius = SWARM_SPACING * size.sqrt();
            let center = self.swarm_point(arena, radius);
            let group = Rc::new(Swarm { kind: s.kind.clone() });
            let mut hole = 0.0; // the leader's place in the middle stays free
            if let Some(leader) = &s.leader {
                if rng::random() < s.leader_chance {
                    let l = Enemy::shared(leader, center, self.roster.clone());
                    l.borrow_mut().swarm = Some(group.clone());
                    hole = l.borrow().size();
                    new.push(l);
                }
            }
            for _ in 0..size as i64 {
                let point = center + vec2(rng::uniform(hole, radius + hole), 0.0).rotate(rng::uniform(0.0, 360.0));
                let e = self.create(&s.kind, point); // beyond the map's edge is fine
                e.borrow_mut().swarm = Some(group.clone());
                new.push(e);
            }
        }
        new
    }

    /// Where a swarm's centre goes: on the edge of the map, in a random direction from the window – at least
    /// SWARM_GAP + radius away from it and every extra window. Without such an edge, the farthest one found, moved out
    /// beyond it as far as needed.
    fn swarm_point(&self, arena: Rect, radius: f64) -> Vec2 {
        let center = arena.centerv();
        let needed = SWARM_GAP + radius;
        let (mut farthest, mut farthest_room, mut farthest_direction) = (center, -1.0, vec2(1.0, 0.0));
        for _ in 0..SWARM_TRIES {
            let direction = vec2(1.0, 0.0).rotate(rng::uniform(0.0, 360.0));
            let point = center + direction * self.distance_to_edge(center, direction);
            let room = self.room(point, arena);
            if room >= needed {
                return point;
            }
            if room > farthest_room {
                (farthest, farthest_room, farthest_direction) = (point, room, direction);
            }
        }
        let mut point = farthest;
        for _ in 0..5 {
            let room = self.room(point, arena);
            if room >= needed {
                break;
            }
            point += farthest_direction * (needed - room);
        }
        point
    }

    /// How far `point` is from what the player sees: the window and the extra windows.
    fn room(&self, point: Vec2, arena: Rect) -> f64 {
        std::iter::once(&arena).chain(self.windows.iter()).map(|r| distance_to_rect(point, r)).fold(f64::INFINITY, f64::min)
    }

    /// How far from `start` (inside the map) along `direction` until the edge of the map.
    fn distance_to_edge(&self, start: Vec2, direction: Vec2) -> f64 {
        let b = self.desktop.bounds;
        let mut steps = vec![];
        if direction.x != 0.0 {
            steps.push(((if direction.x > 0.0 { b.right() } else { b.left() }) as f64 - start.x) / direction.x);
        }
        if direction.y != 0.0 {
            steps.push(((if direction.y > 0.0 { b.bottom() } else { b.top() }) as f64 - start.y) / direction.y);
        }
        steps.into_iter().fold(f64::INFINITY, f64::min).max(0.0)
    }

    /// Where an enemy of type `key` appears: the Pusher beyond the edge of the map, the others around the window.
    pub fn point_for(&self, key: &str, arena: Rect) -> Vec2 {
        if type_of(key).is_some_and(|t| t.spawns_outside) {
            let center = arena.centerv();
            let direction = vec2(1.0, 0.0).rotate(rng::uniform(0.0, 360.0));
            return center + direction * (self.distance_to_edge(center, direction) + OUTSIDE_DISTANCE);
        }
        self.spawn_point(arena)
    }

    /// A random point around the window, SPAWN_DISTANCE beyond its corners, on a monitor – not in an extra window.
    fn spawn_point(&self, arena: Rect) -> Vec2 {
        let center = arena.centerv();
        let reach = vec2(arena.w as f64, arena.h as f64).length() / 2.0; // centre to corner
        let mut point = center;
        for _ in 0..20 {
            point = center + vec2(reach + rng::uniform(SPAWN_DISTANCE.0, SPAWN_DISTANCE.1), 0.0).rotate(rng::uniform(0.0, 360.0));
            if self.desktop.contains(point.x, point.y) && !self.windows.iter().any(|w| w.collidev(point)) {
                return point;
            }
        }
        // the window fills almost the whole desktop: put it on the nearest monitor's edge
        self.desktop.clamp_point(point)
    }
}

/// 0 inside `r`, else the distance to its nearest point.
pub fn distance_to_rect(p: Vec2, r: &Rect) -> f64 {
    let dx = (r.left() as f64 - p.x).max(0.0).max(p.x - r.right() as f64);
    let dy = (r.top() as f64 - p.y).max(0.0).max(p.y - r.bottom() as f64);
    dx.hypot(dy)
}
