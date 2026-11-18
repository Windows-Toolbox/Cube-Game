//! The enemy roster of a run: every enemy type's values for this run and the buffs on all enemies (Python:
//! `game/enemies/roster.py` and `buff.py`).
//!
//! Where an enemy's values come from – each layer overrides the one before:
//!   1. the fallback stats of its type (`enemies::TYPES`) – only if `enemies.json` lacks a value or is broken
//!   2. `content\enemies.json` – the real standard values (explained in `content\README.md`)
//!   3. "enemy_stats" in the level file – a number replaces the value, "+1" / "-1" adds, "x1.5" multiplies; "all"
//!      applies to every type that has the stat
//! Buffs come on top during the run.
//!
//! Evolutions: some tier 1 types have a stronger tier 2 version (`EVOLUTIONS`). A level's "evolutions" say how
//! likely one of them spawns in its place – the spawner asks for that.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use serde_json::Value;

use super::difficulty::{Difficulty, Op, apply};
use super::enemies::{self, TYPES};
use super::tables::{self, f, is_number};
use crate::engine::Color;
use crate::pyfmt::{dumps, g, num};

/// tier 1 type → its evolution (tier 2).
pub const EVOLUTIONS: &[(&str, &str)] = &[("basic", "fat"), ("shooter", "cannoneer"), ("charger", "shield"), ("sniper", "laser"), ("artillery", "cluster")];
/// 0 would break things (division by 0, invisible enemies, …).
const MUST_BE_POSITIVE: &[&str] = &["base_hp", "size", "fire_interval", "reload_time", "aim_time", "spawn_interval", "blast_radius", "setup_time", "pack_time", "beam_time", "swing_time", "split_count", "split_time"];
/// Old stat names (older files may still have them) → their names now.
const RENAMED: &[(&str, &str)] = &[("shot_damage", "projectile_damage"), ("shot_speed", "projectile_speed"), ("shot_size", "projectile_size")];

pub fn evolution_of(key: &str) -> Option<&'static str> {
    EVOLUTIONS.iter().find(|(a, _)| *a == key).map(|(_, b)| *b)
}

/// The end of the warning about a stat that does not exist – with its new name if it was renamed.
pub fn unknown_stat(name: &str) -> String {
    match RENAMED.iter().find(|(a, _)| *a == name) {
        Some((_, now)) => format!("it's \"{now}\" now – ignored"),
        None => "ignored".into(),
    }
}

/// A type's fallback values: `armor` 0 for every type, then its own.
fn fallbacks(key: &str) -> Vec<(String, Value)> {
    let mut v = vec![("armor".to_string(), Value::from(0))];
    if let Some(t) = enemies::type_of(key) {
        for (name, value) in (t.stats)() {
            if let Some(slot) = v.iter_mut().find(|(n, _)| n == name) {
                slot.1 = value;
            } else {
                v.push((name.to_string(), value));
            }
        }
    }
    v
}

pub fn has_stat(key: &str, name: &str) -> bool {
    fallbacks(key).iter().any(|(n, _)| n == name)
}

fn valid_value(name: &str, value: &Value) -> bool {
    is_number(value) && f(value) >= 0.0 && (f(value) > 0.0 || !MUST_BE_POSITIVE.contains(&name))
}
fn valid_number(name: &str, value: f64) -> bool {
    value >= 0.0 && (value > 0.0 || !MUST_BE_POSITIVE.contains(&name))
}

/// One type's values: stat name → number.
pub type Stats = HashMap<String, f64>;

thread_local! {
    /// type key → its values: `enemies.json` over the fallbacks.
    static BASE_STATS: RefCell<HashMap<String, Stats>> = RefCell::new(HashMap::new());
}

fn base_stats() -> HashMap<String, Stats> {
    let empty = BASE_STATS.with(|b| b.borrow().is_empty());
    if empty {
        reload(&mut vec![]);
    }
    BASE_STATS.with(|b| b.borrow().clone())
}

/// Reads `enemies.json` again.
pub fn reload(warnings: &mut Vec<String>) {
    let data = tables::load("enemies.json", warnings);
    let mut base = HashMap::new();
    for t in TYPES {
        let mut values = fallbacks(t.key);
        match data.get(t.key) {
            None => {}
            Some(Value::Object(entry)) => {
                for (name, value) in entry {
                    match values.iter_mut().find(|(n, _)| n == name) {
                        None => warnings.push(format!("enemies.json {}: unknown stat \"{name}\" – {}", t.key, unknown_stat(name))),
                        Some(slot) if valid_value(name, value) => slot.1 = value.clone(),
                        Some(slot) => warnings.push(format!("enemies.json {}.{name}: {} is not valid – using {}", t.key, dumps(value), num(&slot.1))),
                    }
                }
            }
            Some(_) => warnings.push(format!("enemies.json {}: should be {{ … }} – using built-in values", t.key)),
        }
        base.insert(t.key.to_string(), values.into_iter().map(|(n, v)| (n, f(&v))).collect::<Stats>());
    }
    for key in data.keys() {
        if enemies::type_of(key).is_none() {
            warnings.push(format!("enemies.json: unknown enemy type \"{key}\" – ignored"));
        }
    }
    BASE_STATS.with(|b| *b.borrow_mut() = base);
}

/// A temporary change to one of an enemy's stats during a run – flat (`add`) and/or as a `factor`. All buffs on a
/// stat are combined as (value + all adds) × all factors.
#[derive(Debug, Clone, PartialEq)]
pub struct Buff {
    pub stat: String,
    pub add: f64,
    pub factor: f64,
    /// Seconds – `None`: until it is removed.
    pub duration: Option<f64>,
    /// Only for the roster's buffs: an enemy type – `None`: every type.
    pub target: Option<String>,
    /// Who gave it ("commander") – such a buff is renewed instead of given twice.
    pub source: Option<String>,
    /// The outline it gives the enemy – `None`: the usual one.
    pub color: Option<Color>,
}

impl Buff {
    pub fn new(stat: &str) -> Buff {
        Buff { stat: stat.to_string(), add: 0.0, factor: 1.0, duration: None, target: None, source: None, color: None }
    }
    pub fn expired(&self) -> bool {
        self.duration.is_some_and(|d| d <= 0.0)
    }
    pub fn update(&mut self, dt: f64) {
        if let Some(d) = &mut self.duration {
            *d -= dt;
        }
    }
}

/// The roster of a run, shared by all its enemies.
pub struct Roster {
    pub stats: HashMap<String, Stats>,
    /// On all enemies of the run (`Buff::target`: one type, or every type).
    pub buffs: Vec<Buff>,
}

pub type RosterRef = Rc<RefCell<Roster>>;

impl Roster {
    /// `difficulty`: whose enemy_stats apply (`None`: just the standard values). `warnings`: to report overrides
    /// that end up invalid (e.g. "-5" on 2 HP) – such a value keeps its standard.
    pub fn new(difficulty: Option<&Difficulty>, mut warnings: Option<&mut Vec<String>>) -> Roster {
        let mut stats = base_stats();
        if let Some(d) = difficulty {
            for (target, name, op, number) in &d.enemy_stats {
                let keys: Vec<&str> = if target == "all" { TYPES.iter().map(|t| t.key).collect() } else { vec![target.as_str()] };
                for key in keys {
                    let Some(type_stats) = stats.get_mut(key) else { continue };
                    let Some(&current) = type_stats.get(name) else { continue }; // "all": only the types that have it
                    let value = apply(current, *op, *number);
                    if valid_number(name, value) {
                        type_stats.insert(name.clone(), value);
                    } else if let Some(w) = warnings.as_deref_mut() {
                        w.push(format!("{}.json enemy_stats: {key}.{name} would be {} – keeping {}", d.key, g(value), g(current)));
                    }
                }
            }
        }
        Roster { stats, buffs: vec![] }
    }
    pub fn shared(difficulty: Option<&Difficulty>) -> RosterRef {
        Rc::new(RefCell::new(Roster::new(difficulty, None)))
    }
    /// The roster's buffs that apply to type `key`.
    pub fn buffs_for(&self, key: &str) -> impl Iterator<Item = &Buff> {
        self.buffs.iter().filter(move |b| b.target.as_deref().is_none_or(|t| t == key))
    }
    pub fn update(&mut self, dt: f64) {
        for b in &mut self.buffs {
            b.update(dt);
        }
        self.buffs.retain(|b| !b.expired());
    }
}

/// `Op` is only used to build buffs here.
pub fn buff_from(stat: &str, op: Op, number: f64, duration: Option<f64>, target: Option<String>) -> Buff {
    let (add, factor) = if op == Op::Add { (number, 1.0) } else { (0.0, number) };
    Buff { add, factor, duration, target, ..Buff::new(stat) }
}
