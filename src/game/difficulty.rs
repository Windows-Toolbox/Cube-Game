//! The levels (A1 … A6, B1 … B6, C1 … C6, Survival), chosen in the game menu before a run. Each one is a JSON file
//! in `content\difficulties\` – its goal (how long to survive to clear it), what unlocks it, arena, cores, how fast
//! and which enemies spawn (and how that changes over time), swarms, phases, evolutions (tier 2 enemies spawning
//! instead of tier 1), the enemies' own values on this level (`enemy_stats`) and the hints shown in the game menu
//! (`content\difficulties\README.md` explains every value). In the code they are still called "difficulties".
//!
//! Loading never crashes the game: a value that is missing or does not make sense falls back to the default below,
//! a broken file is skipped. Everything that went wrong ends up in `warnings()`, which the game menu shows. The files
//! are read again every time the game menu opens (`reload`), so changes show up without a restart.

use std::cell::RefCell;
use std::rc::Rc;

use serde_json::{Map, Value, json};

use super::tables::{self, f, is_int, is_number};
use super::{cards, enemies, roster, shop_data, skill_tree};
use crate::content;
use crate::pyfmt::dumps;
use crate::rng;

/// Seconds etc. where 0 would break the game.
const MUST_BE_POSITIVE: &[&str] = &["start_interval", "min_interval", "growth_time", "shrink_speed"];
/// Arena values a phase can change.
pub const ARENA_CHANGES: &[&str] = &["shrink_speed", "armor", "push_falloff"];
const PHASE_KEYS: &[&str] = &["weights", "enemy_stats", "announce", "arena", "spawn", "spawn_interval", "breathe", "points"];

/// How a value is changed: a number replaces it, "+1" / "-1" adds, "x1.5" multiplies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Set,
    Add,
    Multiply,
}

pub fn apply(value: f64, op: Op, number: f64) -> f64 {
    match op {
        Op::Set => number,
        Op::Add => value + number,
        Op::Multiply => value * number,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arena {
    /// Width, height (the skill tree's "Bigger arena" adds to it).
    pub start_size: (f64, f64),
    /// Pixels per second each edge moves in at the start …
    pub shrink_speed: f64,
    /// … plus this much per minute played …
    pub shrink_growth: f64,
    /// … up to this.
    pub max_shrink_speed: f64,
    /// Pixels every push of an edge is shorter (phases can change it).
    pub armor: f64,
    /// Pushes get weaker the further the arena is out already – 0: off (`arena.rs`).
    pub push_falloff: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spawning {
    pub first_spawn: f64,
    pub start_interval: f64,
    pub min_interval: f64,
    pub interval_drop: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnemyEntry {
    pub kind: String,
    pub weight: f64,
    pub after: f64,
    pub growth: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Swarms {
    pub kind: String,
    pub after: f64,
    pub start_chance: f64,
    pub chance_growth: f64,
    pub max_chance: f64,
    pub start_size: f64,
    pub growth_time: f64,
    pub max_size: f64,
    pub min_gap: f64,
    pub leader: Option<String>,
    pub leader_chance: f64,
}

/// How likely a tier 1 type spawns as its evolution (tier 2) instead.
#[derive(Debug, Clone, PartialEq)]
pub struct Evolution {
    pub kind: String,
    pub chance: f64,
    pub after: f64,
    pub growth: f64,
    pub max_chance: f64,
}

/// A stretch of the run (from start to end seconds) with different spawn weights and/or buffs on the enemies.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Phase {
    pub start: f64,
    /// `None`: until the run ends.
    pub end: Option<f64>,
    /// (type, operation, number) – on the spawn weights while it lasts.
    pub weights: Vec<(String, Op, f64)>,
    /// (target, stat, operation, number) – buffs while it lasts: add or multiply only.
    pub enemy_stats: Vec<(String, String, Op, f64)>,
    pub announce: Option<String>,
    /// (value, operation, number) – the arena's ARENA_CHANGES while it lasts.
    pub arena: Vec<(String, Op, f64)>,
    /// (type, count) – extra enemies once, when it starts.
    pub spawn: Vec<(String, i64)>,
    /// On the seconds between enemies while it lasts.
    pub spawn_interval: Option<(Op, f64)>,
    /// Pixels every edge moves in over the phase (on top), given back when it ends.
    pub breathe: f64,
    /// (kind, count) – points of interest placed once, when it starts.
    pub points: Vec<(String, i64)>,
    /// (weight, Phase) – "one_of": every run picks one and adds it (`chosen`).
    pub options: Vec<(f64, Phase)>,
    /// > 0: starts again every this many seconds.
    pub every: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Difficulty {
    /// The file name without `.json` – saved in the profile.
    pub key: String,
    pub name: String,
    pub description: String,
    pub order: f64,
    /// Seconds to survive to clear it – `None`: endless (Survival).
    pub goal: Option<f64>,
    pub core_chance: f64,
    /// File name (without .json) of the level to clear first – `None`: open.
    pub requires: Option<String>,
    pub arena: Arena,
    pub spawning: Spawning,
    pub enemies: Vec<EnemyEntry>,
    pub swarms: Vec<Swarms>,
    pub phases: Vec<Phase>,
    pub evolutions: Vec<Evolution>,
    /// (target, stat, operation, number) – target: "all" or a type.
    pub enemy_stats: Vec<(String, String, Op, f64)>,
    pub hints: Vec<String>,
}

// ---------- the fields and their defaults (the Python dataclasses) ----------

fn unlock_fields() -> Vec<(&'static str, Value)> {
    vec![("requires", Value::Null)]
}
fn arena_fields() -> Vec<(&'static str, Value)> {
    vec![("start_size", json!([800, 560])), ("shrink_speed", json!(20)), ("shrink_growth", json!(0)), ("max_shrink_speed", json!(60)), ("armor", json!(0)), ("push_falloff", json!(0))]
}
fn spawning_fields() -> Vec<(&'static str, Value)> {
    vec![("first_spawn", json!(2.0)), ("start_interval", json!(2.6)), ("min_interval", json!(0.9)), ("interval_drop", json!(0.012))]
}
fn entry_fields() -> Vec<(&'static str, Value)> {
    vec![("type", json!("basic")), ("weight", json!(1)), ("after", json!(0)), ("growth", json!(0))]
}
fn swarm_fields() -> Vec<(&'static str, Value)> {
    vec![
        ("type", json!("basic")),
        ("after", json!(0)),
        ("start_chance", json!(0.02)),
        ("chance_growth", json!(0.0008)),
        ("max_chance", json!(0.15)),
        ("start_size", json!(6)),
        ("growth_time", json!(10)),
        ("max_size", json!(40)),
        ("min_gap", json!(6)),
        ("leader", Value::Null),
        ("leader_chance", json!(1)),
    ]
}
fn evolution_fields() -> Vec<(&'static str, Value)> {
    vec![("type", json!("basic")), ("chance", json!(0.1)), ("after", json!(0)), ("growth", json!(0)), ("max_chance", json!(1))]
}
/// The plain values at the top of a level file.
fn top_fields() -> Vec<(&'static str, Value)> {
    vec![("name", json!("")), ("description", json!("")), ("order", json!(0)), ("goal", Value::Null), ("core_chance", json!(0.01)), ("hints", json!([]))]
}

thread_local! {
    static DIFFICULTIES: RefCell<Vec<Rc<Difficulty>>> = const { RefCell::new(vec![]) };
    static WARNINGS: RefCell<Vec<String>> = const { RefCell::new(vec![]) };
}

/// Every level, in menu order.
pub fn all() -> Vec<Rc<Difficulty>> {
    ensure_loaded();
    DIFFICULTIES.with(|d| d.borrow().clone())
}
pub fn get(key: &str) -> Option<Rc<Difficulty>> {
    ensure_loaded();
    DIFFICULTIES.with(|d| d.borrow().iter().find(|x| x.key == key).cloned())
}
pub fn exists(key: &str) -> bool {
    get(key).is_some()
}
/// What went wrong while loading, for the game menu.
pub fn warnings() -> Vec<String> {
    WARNINGS.with(|w| w.borrow().clone())
}
/// The first difficulty – where a new profile starts.
pub fn default_key() -> String {
    all()[0].key.clone()
}
/// The level that clearing `key` unlocks – or `None` for the last.
pub fn next_difficulty(key: &str) -> Option<String> {
    all().iter().find(|d| d.requires.as_deref() == Some(key)).map(|d| d.key.clone())
}

fn ensure_loaded() {
    if DIFFICULTIES.with(|d| d.borrow().is_empty()) {
        reload();
    }
}

/// `phase` as it plays in this run: without options as it is, else with one of them picked (weighted) and added.
pub fn chosen(phase: &Phase) -> Phase {
    if phase.options.is_empty() {
        return phase.clone();
    }
    let weights: Vec<f64> = phase.options.iter().map(|(w, _)| *w).collect();
    let i = rng::weighted(&weights).unwrap_or(0);
    with_option(phase, &phase.options[i].1)
}

/// `phase` with `option` (one of its options) added – the lists after the phase's own, a single value of the
/// option instead of the phase's.
pub fn with_option(phase: &Phase, option: &Phase) -> Phase {
    Phase {
        options: vec![],
        weights: cat(&phase.weights, &option.weights),
        enemy_stats: cat(&phase.enemy_stats, &option.enemy_stats),
        arena: cat(&phase.arena, &option.arena),
        spawn: cat(&phase.spawn, &option.spawn),
        points: cat(&phase.points, &option.points),
        announce: option.announce.clone().or_else(|| phase.announce.clone()),
        spawn_interval: option.spawn_interval.or(phase.spawn_interval),
        breathe: if option.breathe != 0.0 { option.breathe } else { phase.breathe },
        ..phase.clone()
    }
}

fn cat<T: Clone>(a: &[T], b: &[T]) -> Vec<T> {
    a.iter().chain(b.iter()).cloned().collect()
}

/// How likely `entry`'s type spawns as its evolution `elapsed` seconds into the run (0 before `entry.after`).
pub fn evolution_chance(entry: &Evolution, elapsed: f64) -> f64 {
    if elapsed < entry.after {
        return 0.0;
    }
    entry.max_chance.min(entry.chance + entry.growth * (elapsed - entry.after) / 60.0)
}

// ---------- loading ----------

/// Does `value` fit where `default` is? Numbers ≥ 0 (> 0 for MUST_BE_POSITIVE), text, a pair of sizes, …
fn valid(name: &str, value: &Value, default: &Value) -> bool {
    if name == "goal" {
        return value.is_null() || (is_number(value) && f(value) > 0.0);
    }
    if name == "hints" {
        return value.as_array().is_some_and(|l| l.iter().all(|t| t.is_string()));
    }
    match default {
        Value::Number(_) => is_number(value) && f(value) >= 0.0 && (f(value) > 0.0 || !MUST_BE_POSITIVE.contains(&name)),
        Value::String(_) => value.as_str().is_some_and(|s| name != "type" || enemies::is_type(s)),
        Value::Array(d) => value.as_array().is_some_and(|l| l.len() == d.len() && l.iter().all(|n| is_number(n) && f(n) > 0.0)),
        _ => value.is_null() || value.is_string(),
    }
}

/// The valid values for `fields` from `data` – what is missing or invalid is left out (so the default is used) and
/// noted in the warnings.
fn read_values(fields: &[(&str, Value)], data: &Value, file: &str, section: &str, w: &mut Vec<String>) -> Map<String, Value> {
    let Value::Object(data) = data else {
        w.push(format!("{file} {section}: should be {{ … }} – using the defaults"));
        return defaults_of(fields);
    };
    let mut values = defaults_of(fields);
    for (name, default) in fields {
        let Some(value) = data.get(*name) else { continue };
        if valid(name, value, default) {
            values.insert(name.to_string(), value.clone());
        } else {
            let place = if section.is_empty() { name.to_string() } else { format!("{section}.{name}") };
            w.push(format!("{file} {place}: {} is not valid – using {}", dumps(value), dumps(default)));
        }
    }
    for key in data.keys() {
        if !fields.iter().any(|(n, _)| n == key) {
            w.push(format!("{file} {}: unknown key \"{key}\" – ignored", if section.is_empty() { "top level" } else { section }));
        }
    }
    values
}

fn defaults_of(fields: &[(&str, Value)]) -> Map<String, Value> {
    fields.iter().map(|(n, v)| (n.to_string(), v.clone())).collect()
}

fn num(m: &Map<String, Value>, name: &str) -> f64 {
    m.get(name).map(f).unwrap_or(0.0)
}
fn text(m: &Map<String, Value>, name: &str) -> Option<String> {
    m.get(name).and_then(|v| v.as_str()).map(String::from)
}

fn to_arena(m: &Map<String, Value>) -> Arena {
    let size = m["start_size"].as_array().map(|l| (f(&l[0]), f(&l[1]))).unwrap_or((800.0, 560.0));
    Arena { start_size: size, shrink_speed: num(m, "shrink_speed"), shrink_growth: num(m, "shrink_growth"), max_shrink_speed: num(m, "max_shrink_speed"), armor: num(m, "armor"), push_falloff: num(m, "push_falloff") }
}
fn to_spawning(m: &Map<String, Value>) -> Spawning {
    Spawning { first_spawn: num(m, "first_spawn"), start_interval: num(m, "start_interval"), min_interval: num(m, "min_interval"), interval_drop: num(m, "interval_drop") }
}
fn to_entry(m: &Map<String, Value>) -> EnemyEntry {
    EnemyEntry { kind: text(m, "type").unwrap_or_else(|| "basic".into()), weight: num(m, "weight"), after: num(m, "after"), growth: num(m, "growth") }
}
fn to_swarms(m: &Map<String, Value>) -> Swarms {
    Swarms {
        kind: text(m, "type").unwrap_or_else(|| "basic".into()),
        after: num(m, "after"),
        start_chance: num(m, "start_chance"),
        chance_growth: num(m, "chance_growth"),
        max_chance: num(m, "max_chance"),
        start_size: num(m, "start_size"),
        growth_time: num(m, "growth_time"),
        max_size: num(m, "max_size"),
        min_gap: num(m, "min_gap"),
        leader: text(m, "leader"),
        leader_chance: num(m, "leader_chance"),
    }
}
fn to_evolution(m: &Map<String, Value>) -> Evolution {
    Evolution { kind: text(m, "type").unwrap_or_else(|| "basic".into()), chance: num(m, "chance"), after: num(m, "after"), growth: num(m, "growth"), max_chance: num(m, "max_chance") }
}

fn load_file(name: &str, w: &mut Vec<String>) -> Result<Difficulty, String> {
    let file = name.to_string();
    let data = content::read_object(&format!("difficulties/{name}"))?;
    let stem = name.trim_end_matches(".json").to_string();
    let get = |k: &str| data.get(k).cloned().unwrap_or(json!({}));
    let unlock = read_values(&unlock_fields(), &get("unlock"), &file, "unlock", w);
    let arena = read_values(&arena_fields(), &get("arena"), &file, "arena", w);
    let spawning = read_values(&spawning_fields(), &get("spawning"), &file, "spawning", w);
    let mut entries = vec![];
    let list = match data.get("enemies") {
        None => vec![],
        Some(Value::Array(l)) => l.clone(),
        Some(_) => {
            w.push(format!("{file} enemies: should be a list [ … ]"));
            vec![]
        }
    };
    for (i, entry) in list.iter().enumerate() {
        if let Value::Object(e) = entry {
            let kind = e.get("type").cloned().unwrap_or(json!("basic"));
            if !kind.as_str().is_some_and(enemies::is_type) {
                // a typo in the type would otherwise quietly turn into a basic enemy
                w.push(format!("{file} enemies[{i}]: unknown type {} – skipped", dumps(&kind)));
                continue;
            }
        }
        entries.push(to_entry(&read_values(&entry_fields(), entry, &file, &format!("enemies[{i}]"), w)));
    }
    if entries.is_empty() {
        w.push(format!("{file} enemies: none given – only basic enemies"));
        entries.push(to_entry(&defaults_of(&entry_fields())));
    }
    let special = ["unlock", "arena", "spawning", "enemies", "swarms", "phases", "enemy_stats", "evolutions"];
    let plain: Map<String, Value> = data.iter().filter(|(k, _)| !special.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();
    let top = read_values(&top_fields(), &Value::Object(plain.clone()), &file, "", w);
    // a valid "name", else the file name with a capital letter
    let shown_name = plain.get("name").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| capitalize(&stem));
    Ok(Difficulty {
        key: stem.clone(),
        name: shown_name,
        description: text(&top, "description").unwrap_or_default(),
        order: num(&top, "order"),
        goal: top.get("goal").and_then(|v| v.as_f64()),
        core_chance: num(&top, "core_chance"),
        requires: text(&unlock, "requires"),
        arena: to_arena(&arena),
        spawning: to_spawning(&spawning),
        enemies: entries,
        swarms: read_swarms(&data, &file, w),
        phases: read_phases(&data, &file, w),
        evolutions: read_evolutions(&data, &file, w),
        enemy_stats: read_enemy_stats(data.get("enemy_stats").unwrap_or(&json!({})), &file, "enemy_stats", w),
        hints: top.get("hints").and_then(|v| v.as_array()).map(|l| l.iter().filter_map(|t| t.as_str().map(String::from)).collect()).unwrap_or_default(),
    })
}

/// Python's `str.capitalize()`.
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

fn read_evolutions(data: &Map<String, Value>, file: &str, w: &mut Vec<String>) -> Vec<Evolution> {
    let section = match data.get("evolutions") {
        None => return vec![],
        Some(Value::Array(l)) => l.clone(),
        Some(_) => {
            w.push(format!("{file} evolutions: should be a list [ … ] – ignored"));
            return vec![];
        }
    };
    let mut out = vec![];
    for (i, entry) in section.iter().enumerate() {
        let kind = entry.get("type").cloned().unwrap_or(json!("basic"));
        if !entry.is_object() || !kind.as_str().is_some_and(|k| roster::evolution_of(k).is_some()) {
            let shown = if entry.is_object() { dumps(&entry.get("type").cloned().unwrap_or(Value::Null)) } else { "it".into() };
            w.push(format!("{file} evolutions[{i}]: {shown} has no evolution (only {}) – skipped", roster::EVOLUTIONS.iter().map(|(a, _)| *a).collect::<Vec<_>>().join(", ")));
            continue;
        }
        out.push(to_evolution(&read_values(&evolution_fields(), entry, file, &format!("evolutions[{i}]"), w)));
    }
    out
}

/// "swarms": one { … } or a list of them (one per kind of swarm), [] for none.
fn read_swarms(data: &Map<String, Value>, file: &str, w: &mut Vec<String>) -> Vec<Swarms> {
    match data.get("swarms") {
        None => vec![read_swarm(&json!({}), file, "swarms", w)],
        Some(v @ Value::Object(_)) => vec![read_swarm(v, file, "swarms", w)],
        Some(Value::Array(l)) => {
            let mut out = vec![];
            for (i, entry) in l.iter().enumerate() {
                if let Value::Object(e) = entry {
                    let kind = e.get("type").cloned().unwrap_or(json!("basic"));
                    if !kind.as_str().is_some_and(enemies::is_type) {
                        w.push(format!("{file} swarms[{i}]: unknown type {} – skipped", dumps(&kind)));
                        continue;
                    }
                }
                out.push(read_swarm(entry, file, &format!("swarms[{i}]"), w));
            }
            out
        }
        Some(_) => {
            w.push(format!("{file} swarms: should be {{ … }} or a list [ … ] – using the defaults"));
            vec![to_swarms(&defaults_of(&swarm_fields()))]
        }
    }
}

/// One kind of swarm – its leader only if it is an enemy type.
fn read_swarm(entry: &Value, file: &str, place: &str, w: &mut Vec<String>) -> Swarms {
    let mut s = to_swarms(&read_values(&swarm_fields(), entry, file, place, w));
    if let Some(leader) = &s.leader {
        if !enemies::is_type(leader) {
            w.push(format!("{file} {place}.leader: unknown type {} – no leader", dumps(&json!(leader))));
            s.leader = None;
        }
    }
    s
}

/// "phases": [{"from": 180, "to": 220, "every": 90, "weights": {"sniper": "x3"}, "enemy_stats": {…}, "announce":
/// "…", "one_of": [{…}, {…}]}, …]
fn read_phases(data: &Map<String, Value>, file: &str, w: &mut Vec<String>) -> Vec<Phase> {
    let section = match data.get("phases") {
        None => return vec![],
        Some(Value::Array(l)) => l.clone(),
        Some(_) => {
            w.push(format!("{file} phases: should be a list [ … ] – ignored"));
            return vec![];
        }
    };
    let mut phases = vec![];
    for (i, entry) in section.iter().enumerate() {
        let place = format!("phases[{i}]");
        let Value::Object(e) = entry else {
            w.push(format!("{file} {place}: should be {{ … }} – skipped"));
            continue;
        };
        for key in e.keys() {
            if !["from", "to", "every", "one_of"].contains(&key.as_str()) && !PHASE_KEYS.contains(&key.as_str()) {
                w.push(format!("{file} {place}: unknown key \"{key}\" – ignored"));
            }
        }
        let start = e.get("from").cloned().unwrap_or(json!(0));
        let end = e.get("to").cloned().unwrap_or(Value::Null);
        if !valid("from", &start, &json!(0.0)) {
            w.push(format!("{file} {place}.from: {} is not valid (seconds) – skipped", dumps(&start)));
            continue;
        }
        if !end.is_null() && !(valid("to", &end, &json!(0.0)) && f(&end) > f(&start)) {
            w.push(format!("{file} {place}.to: {} is not valid (seconds after \"from\") – skipped", dumps(&end)));
            continue;
        }
        let end = end.as_f64();
        let mut every = e.get("every").cloned().unwrap_or(json!(0));
        if !valid("every", &every, &json!(0.0)) {
            w.push(format!("{file} {place}.every: {} is not valid (seconds) – only once", dumps(&every)));
            every = json!(0);
        }
        let mut phase = read_phase_values(e, end, file, &place, w);
        phase.start = f(&start);
        phase.end = end;
        phase.options = read_options(e, end, file, &place, w);
        phase.every = f(&every);
        phases.push(phase);
    }
    phases
}

/// What a phase (or one of its options) does. `end`: when the phase ends (`None`: never – then it cannot breathe).
fn read_phase_values(e: &Map<String, Value>, end: Option<f64>, file: &str, place: &str, w: &mut Vec<String>) -> Phase {
    let mut announce = e.get("announce").cloned().unwrap_or(Value::Null);
    if !announce.is_null() && !announce.is_string() {
        w.push(format!("{file} {place}.announce: should be a text – ignored"));
        announce = Value::Null;
    }
    let mut stats = vec![];
    for change in read_enemy_stats(e.get("enemy_stats").unwrap_or(&json!({})), file, &format!("{place}.enemy_stats"), w) {
        if change.2 == Op::Set {
            w.push(format!("{file} {place}.enemy_stats.{}.{}: only \"+1\", \"-1\" or \"x1.5\" in a phase – ignored", change.0, change.1));
        } else {
            stats.push(change);
        }
    }
    let mut interval = None;
    if let Some(v) = e.get("spawn_interval") {
        interval = parse_change(v);
        if interval.is_none_or(|(op, n)| op != Op::Add && n <= 0.0) {
            w.push(format!("{file} {place}.spawn_interval: {} is not valid – ignored (seconds, \"-1\", \"+1\" or \"x0.5\")", dumps(v)));
            interval = None;
        }
    }
    let mut breathe = e.get("breathe").cloned().unwrap_or(json!(0));
    let truthy = !(breathe.is_null() || breathe == json!(0) || breathe == json!(0.0) || breathe == json!(false) || breathe == json!(""));
    if truthy && end.is_none() {
        w.push(format!("{file} {place}.breathe: only in a phase with \"to\" (the edges come back then) – ignored"));
        breathe = json!(0);
    } else if !valid("breathe", &breathe, &json!(0.0)) {
        w.push(format!("{file} {place}.breathe: {} is not valid – ignored (pixels)", dumps(&breathe)));
        breathe = json!(0);
    }
    Phase {
        weights: read_weights(e.get("weights").unwrap_or(&json!({})), file, &format!("{place}.weights"), w),
        enemy_stats: stats,
        announce: announce.as_str().map(String::from),
        arena: read_arena_changes(e.get("arena").unwrap_or(&json!({})), file, &format!("{place}.arena"), w),
        spawn: read_counts(e.get("spawn").unwrap_or(&json!({})), file, &format!("{place}.spawn"), w, true),
        spawn_interval: interval,
        breathe: f(&breathe),
        points: read_counts(e.get("points").unwrap_or(&json!({})), file, &format!("{place}.points"), w, false),
        ..Phase::default()
    }
}

/// "spawn" ({"shooter": 4}) or "points" ({"totem": 1}) as (key, count).
fn read_counts(section: &Value, file: &str, place: &str, w: &mut Vec<String>, enemies_: bool) -> Vec<(String, i64)> {
    let Value::Object(m) = section else {
        w.push(format!("{file} {place}: should be {{ … }} – ignored"));
        return vec![];
    };
    let mut out = vec![];
    for (key, count) in m {
        let known = if enemies_ { enemies::is_type(key) } else { super::points::KINDS.contains(&key.as_str()) };
        if !known {
            if enemies_ {
                w.push(format!("{file} {place}: unknown enemy type \"{key}\" – ignored"));
            } else {
                w.push(format!("{file} {place}: unknown point \"{key}\" – ignored (only {})", super::points::KINDS.join(", ")));
            }
        } else if !is_int(count) || count.as_i64().unwrap_or(0) < 1 {
            w.push(format!("{file} {place}.{key}: {} is not valid – ignored (a whole number, 1 or more)", dumps(count)));
        } else {
            out.push((key.clone(), count.as_i64().unwrap()));
        }
    }
    out
}

/// "one_of": [{…}, {…, "weight": 2}, …] as (weight, Phase). Weight 0 switches an option off.
fn read_options(e: &Map<String, Value>, end: Option<f64>, file: &str, place: &str, w: &mut Vec<String>) -> Vec<(f64, Phase)> {
    let section = match e.get("one_of") {
        None => return vec![],
        Some(Value::Array(l)) => l.clone(),
        Some(_) => {
            w.push(format!("{file} {place}.one_of: should be a list [ … ] – ignored"));
            return vec![];
        }
    };
    let mut options = vec![];
    for (i, option) in section.iter().enumerate() {
        let p = format!("{place}.one_of[{i}]");
        let Value::Object(o) = option else {
            w.push(format!("{file} {p}: should be {{ … }} – skipped"));
            continue;
        };
        for key in o.keys() {
            if key != "weight" && !PHASE_KEYS.contains(&key.as_str()) {
                w.push(format!("{file} {p}: unknown key \"{key}\" – ignored"));
            }
        }
        let mut weight = o.get("weight").cloned().unwrap_or(json!(1));
        if !valid("weight", &weight, &json!(0.0)) {
            w.push(format!("{file} {p}.weight: {} is not valid – using 1", dumps(&weight)));
            weight = json!(1);
        }
        if f(&weight) == 0.0 {
            continue;
        }
        options.push((f(&weight), read_phase_values(o, end, file, &p, w)));
    }
    options
}

fn read_arena_changes(section: &Value, file: &str, place: &str, w: &mut Vec<String>) -> Vec<(String, Op, f64)> {
    let Value::Object(m) = section else {
        w.push(format!("{file} {place}: should be {{ … }} – ignored"));
        return vec![];
    };
    let mut out = vec![];
    for (name, value) in m {
        let change = parse_change(value);
        if !ARENA_CHANGES.contains(&name.as_str()) {
            w.push(format!("{file} {place}: unknown value \"{name}\" – ignored (only {})", ARENA_CHANGES.join(", ")));
        } else if let Some((op, n)) = change {
            out.push((name.clone(), op, n));
        } else {
            w.push(format!("{file} {place}.{name}: {} is not valid – ignored (a number, \"+3\", \"-3\" or \"x1.5\")", dumps(value)));
        }
    }
    out
}

fn read_weights(section: &Value, file: &str, place: &str, w: &mut Vec<String>) -> Vec<(String, Op, f64)> {
    let Value::Object(m) = section else {
        w.push(format!("{file} {place}: should be {{ … }} – ignored"));
        return vec![];
    };
    let mut out = vec![];
    for (key, value) in m {
        let change = parse_change(value);
        if !enemies::is_type(key) {
            w.push(format!("{file} {place}: unknown enemy type \"{key}\" – ignored"));
        } else if let Some((op, n)) = change {
            out.push((key.clone(), op, n));
        } else {
            w.push(format!("{file} {place}.{key}: {} is not valid – ignored (a number, \"+10\", \"-10\" or \"x3\")", dumps(value)));
        }
    }
    out
}

/// {"all": {"base_hp": "+1"}, "charger": {"speed": 100}} as (target, stat, operation, number) – in file order,
/// "all" first, so a type's own entry comes after it.
pub fn read_enemy_stats(section: &Value, file: &str, place: &str, w: &mut Vec<String>) -> Vec<(String, String, Op, f64)> {
    let Value::Object(m) = section else {
        w.push(format!("{file} {place}: should be {{ … }} – ignored"));
        return vec![];
    };
    let mut targets: Vec<&String> = m.keys().collect();
    targets.sort_by_key(|t| *t != "all"); // stable: file order otherwise
    let mut out = vec![];
    for target in targets {
        let values = &m[target];
        if target != "all" && !enemies::is_type(target) {
            w.push(format!("{file} {place}: unknown enemy type \"{target}\" – ignored"));
            continue;
        }
        let Value::Object(values) = values else {
            w.push(format!("{file} {place}.{target}: should be {{ … }} – ignored"));
            continue;
        };
        for (name, value) in values {
            let known = if target == "all" { enemies::TYPES.iter().any(|t| roster::has_stat(t.key, name)) } else { roster::has_stat(target, name) };
            let change = parse_change(value);
            if !known {
                w.push(format!("{file} {place}.{target}: unknown stat \"{name}\" – {}", roster::unknown_stat(name)));
            } else if let Some((op, n)) = change {
                out.push((target.clone(), name.clone(), op, n));
            } else {
                w.push(format!("{file} {place}.{target}.{name}: {} is not valid – ignored (a number, \"+1\", \"-1\" or \"x1.5\")", dumps(value)));
            }
        }
    }
    out
}

/// 3 → (Set, 3), "+1" → (Add, 1), "-2" → (Add, -2), "x1.5" / "*1.5" → (Multiply, 1.5); else `None`.
pub fn parse_change(value: &Value) -> Option<(Op, f64)> {
    if is_number(value) {
        return Some((Op::Set, f(value)));
    }
    let s = value.as_str()?.trim();
    let mut chars = s.chars();
    let sign = chars.next()?;
    if !"+-x*".contains(sign) {
        return None;
    }
    let rest = chars.as_str().trim();
    // \d+(\.\d+)?
    let (whole, frac) = rest.split_once('.').map(|(a, b)| (a, Some(b))).unwrap_or((rest, None));
    if whole.is_empty() || !whole.chars().all(|c| c.is_ascii_digit()) || frac.is_some_and(|f| f.is_empty() || !f.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    let number: f64 = rest.parse().ok()?;
    Some(match sign {
        'x' | '*' => (Op::Multiply, number),
        '+' => (Op::Add, number),
        _ => (Op::Add, -number),
    })
}

/// Reads every data file and every level again.
pub fn reload() {
    let mut w = vec![];
    roster::reload(&mut w);
    cards::reload(&mut w);
    tables::weapons::reload(&mut w);
    shop_data::reload(&mut w); // after the weapons: their upgrade_factor is the fallback
    tables::abilities::reload(&mut w);
    tables::buildings::reload(&mut w);
    tables::radar::reload(&mut w);
    tables::points::reload(&mut w);
    tables::ships::reload(&mut w);
    skill_tree::reload(&mut w);
    let mut loaded = vec![];
    for name in content::list("difficulties") {
        match load_file(&name, &mut w) {
            Ok(d) => loaded.push(d),
            Err(e) => w.push(format!("{name}: can't be read ({e}) – skipped")),
        }
    }
    if loaded.is_empty() {
        w.push("no difficulty files in content\\difficulties – using built-in values".into());
        loaded.push(built_in());
    }
    loaded.sort_by(|a, b| a.order.total_cmp(&b.order));
    let keys: Vec<String> = loaded.iter().map(|d| d.key.clone()).collect();
    let goals: Vec<(String, Option<f64>)> = loaded.iter().map(|d| (d.key.clone(), d.goal)).collect();
    for d in loaded.iter_mut() {
        let Some(req) = d.requires.clone() else { continue };
        if !keys.contains(&req) {
            w.push(format!("{}.json unlock.requires: \"{req}\" doesn't exist – open", d.key));
            d.requires = None;
        } else if goals.iter().find(|(k, _)| *k == req).is_some_and(|(_, g)| g.is_none()) {
            w.push(format!("{}.json unlock.requires: \"{req}\" has no goal (can't be cleared) – open", d.key));
            d.requires = None;
        }
    }
    for d in &loaded {
        roster::Roster::new(Some(d), Some(&mut w)); // only to report enemy_stats that end up invalid
    }
    DIFFICULTIES.with(|x| *x.borrow_mut() = loaded.into_iter().map(Rc::new).collect());
    WARNINGS.with(|x| *x.borrow_mut() = w);
}

fn built_in() -> Difficulty {
    Difficulty {
        key: "normal".into(),
        name: "Normal".into(),
        description: String::new(),
        order: 0.0,
        goal: None,
        core_chance: 0.01,
        requires: None,
        arena: to_arena(&defaults_of(&arena_fields())),
        spawning: to_spawning(&defaults_of(&spawning_fields())),
        enemies: vec![to_entry(&defaults_of(&entry_fields()))],
        swarms: vec![to_swarms(&defaults_of(&swarm_fields()))],
        phases: vec![],
        evolutions: vec![],
        enemy_stats: vec![],
        hints: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_are_read_like_python() {
        assert_eq!(parse_change(&json!(3)), Some((Op::Set, 3.0)));
        assert_eq!(parse_change(&json!("+1")), Some((Op::Add, 1.0)));
        assert_eq!(parse_change(&json!(" -2.5 ")), Some((Op::Add, -2.5)));
        assert_eq!(parse_change(&json!("x1.5")), Some((Op::Multiply, 1.5)));
        assert_eq!(parse_change(&json!("x1.")), None);
        assert_eq!(parse_change(&json!("1.5")), None);
    }
}

#[cfg(test)]
mod load_tests {
    use super::*;

    #[test]
    fn every_data_file_loads_without_warnings() {
        reload();
        let w = warnings();
        assert!(w.is_empty(), "{w:#?}");
        let all = all();
        assert!(all.len() >= 19, "{} levels", all.len());
        assert_eq!(all[0].key, "a1");
        assert!(all.iter().any(|d| d.key == "survival" && d.goal.is_none()));
    }
}
