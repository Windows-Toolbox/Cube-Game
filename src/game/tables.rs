//! The simple data files – each one a table of numbers the game reads at the start and again every time the game
//! menu opens (`difficulty::reload`). What is missing or invalid in a file keeps the built-in value, and a warning
//! goes to the game menu. Every value is explained in `content\README.md`.
//!
//! | Python module          | here          | file             |
//! |------------------------|---------------|------------------|
//! | `game/ship_data.py`    | `ships`       | `ships.json`     |
//! | `game/ability_data.py` | `abilities`   | `abilities.json` |
//! | `game/weapons/data.py` | `weapons`     | `weapons.json`   |
//! | `game/radar.py` (STATS)| `radar`       | `radar.json`     |
//! | `game/points.py` (STATS)| `points`     | `points.json`    |
//! | `game/buildings/`      | `buildings`   | `buildings.json` |
//!
//! The tables stay dynamic (name → JSON value, in file order), as the Python dicts were: the warnings name the
//! values the way the file does, and an int stays an int (`7`, not `7.0`).

use std::cell::RefCell;

use serde_json::{Map, Value, json};

use crate::content;
use crate::pyfmt::{dumps, num};

pub type Table = Map<String, Value>;

/// A number from a data file – `isinstance(value, (int, float))` without bools.
pub fn is_number(v: &Value) -> bool {
    v.is_number()
}
pub fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}
/// A whole number from a data file (`isinstance(value, int)`).
pub fn is_int(v: &Value) -> bool {
    v.is_i64() || v.is_u64()
}

/// A data file's top level, or `{}` with a warning.
pub fn load(file: &str, warnings: &mut Vec<String>) -> Table {
    match content::read_object(file) {
        Ok(map) => map,
        Err(e) => {
            warnings.push(format!("{file}: can't be read ({e}) – using built-in values"));
            Map::new()
        }
    }
}

fn as_table(v: Value) -> Table {
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

/// A number ≥ 0 – and > 0 for the names in `positive`.
fn ok(name: &str, value: &Value, positive: &[&str]) -> bool {
    is_number(value) && f(value) >= 0.0 && (f(value) > 0.0 || !positive.contains(&name))
}

/// Sections of values (`{"dash": {"cooldown": 1.2, …}, …}`): the file's valid values over the defaults.
/// `unknown_section` is how a section that does not exist is called in the warning ("ability", "entry").
fn read_sections(file: &str, defaults: &Value, positive: &[&str], unknown_section: &str, warnings: &mut Vec<String>) -> Table {
    let data = load(file, warnings);
    let mut stats = as_table(defaults.clone());
    for key in data.keys() {
        if !stats.contains_key(key) {
            warnings.push(format!("{file}: unknown {unknown_section} \"{key}\" – ignored"));
        }
    }
    for (key, values) in stats.iter_mut() {
        let Some(entry) = data.get(key) else { continue };
        let Value::Object(entry) = entry else {
            warnings.push(format!("{file} {key}: should be {{ … }} – using built-in values"));
            continue;
        };
        let values = values.as_object_mut().unwrap();
        for (name, value) in entry {
            if !values.contains_key(name) {
                warnings.push(format!("{file} {key}: unknown value \"{name}\" – ignored"));
            } else if !ok(name, value, positive) {
                warnings.push(format!("{file} {key}.{name}: {} is not valid – using {}", dumps(value), num(&values[name])));
            } else {
                values.insert(name.clone(), value.clone());
            }
        }
    }
    stats
}

macro_rules! table_access {
    ($cell:ident) => {
        /// `STATS[section][name]` as a number.
        pub fn get(section: &str, name: &str) -> f64 {
            $cell.with(|t| t.borrow().get(section).and_then(|s| s.get(name)).map(super::f).unwrap_or(0.0))
        }
        /// One section (`STATS[section]`) – not every table needs it.
        #[allow(dead_code)]
        pub fn section(section: &str) -> super::Table {
            $cell.with(|t| t.borrow().get(section).and_then(|s| s.as_object()).cloned().unwrap_or_default())
        }
    };
}

// ---------- abilities.json ----------

pub mod abilities {
    use super::*;

    pub fn defaults() -> Value {
        json!({
            "dash": {
                "cooldown": 1.2,            // seconds until it's ready again …
                "min_cooldown": 0.3,        // … never shorter than this, whatever upgrades and cards take off
                "distance": 220,            // pixels
                "duration": 0.12,           // seconds the dash takes
                "charges": 1,               // dashes stored at once (the skill tree's Charges add more)
                "invulnerable_time": 0.2,   // seconds from the start of a dash in which nothing can hit the ship
            },
            "rocket": {
                "cooldown": 7, "min_cooldown": 1.5, "damage": 3, "blast_radius": 110, "wall_push": 120,
                "speed": 120,               // pixels per second at launch …
                "acceleration": 2200,       // … getting faster by this much every second
                "size": 12,                 // diameter of the rocket (pixels)
            },
            "mine": { "damage": 3, "blast_radius": 40, "wall_push": 100, "size": 8 },   // left by every dash with "Mines"
            "slow_field": { "slow": 20, "size": 60, "duration": 3 },  // left by a mine's blast with "Slow field"
        })
    }
    const POSITIVE: &[&str] = &["cooldown", "duration", "speed", "size", "charges"];

    thread_local! {
        static STATS: RefCell<Table> = RefCell::new(as_table(defaults()));
    }
    table_access!(STATS);

    pub fn reload(warnings: &mut Vec<String>) {
        let t = read_sections("abilities.json", &defaults(), POSITIVE, "ability", warnings);
        STATS.with(|s| *s.borrow_mut() = t);
    }
}

// ---------- points.json ----------

pub mod points {
    use super::*;

    pub fn defaults() -> Value {
        json!({
            "finds": {"first": 25, "interval": 45, "min_interval": 15, "lifetime": 40},    // the skill tree's finds
            "place": {"min_distance": 200, "max_distance": 650},     // pixels beyond the arena where a point appears
            "chest": {"stars": 20},
            "magnet": {"time": 8, "speed": 900},
            "repair": {"push": 60},
            "totem": {"lifetime": 50, "speed": 1.15, "damage": 1.2},
            "nest": {"lifetime": 40, "interval": 2.5},
            "storm": {"lifetime": 30, "radius": 220, "shrink_speed": 10},
        })
    }
    const POSITIVE: &[&str] = &["interval", "min_interval", "lifetime", "time", "speed", "radius", "max_distance"];

    thread_local! {
        static STATS: RefCell<Table> = RefCell::new(as_table(defaults()));
    }
    table_access!(STATS);

    pub fn reload(warnings: &mut Vec<String>) {
        let t = read_sections("points.json", &defaults(), POSITIVE, "entry", warnings);
        STATS.with(|s| *s.borrow_mut() = t);
    }
}

// ---------- radar.json ----------

pub mod radar {
    use super::*;

    pub fn defaults() -> Value {
        json!({
            "size": 150,                    // side length of the window (pixels)
            "cooldown": 4,                  // seconds from the end of one scan to the next …
            "duration": 2.5,                // seconds a scan stays open
            "min_cooldown": 1,              // … never shorter than this, whatever the skill tree adds
        })
    }
    const POSITIVE: &[&str] = &["size", "cooldown", "duration", "min_cooldown"];

    thread_local! {
        static STATS: RefCell<Table> = RefCell::new(as_table(defaults()));
    }

    pub fn get(name: &str) -> f64 {
        STATS.with(|t| t.borrow().get(name).map(f).unwrap_or(0.0))
    }

    pub fn reload(warnings: &mut Vec<String>) {
        let data = load("radar.json", warnings);
        let defaults = as_table(defaults());
        let mut stats = defaults.clone();
        for (name, value) in &data {
            if !defaults.contains_key(name) {
                warnings.push(format!("radar.json: unknown value \"{name}\" – ignored"));
            } else if !ok(name, value, POSITIVE) {
                warnings.push(format!("radar.json {name}: {} is not valid – using {}", dumps(value), num(&defaults[name])));
            } else {
                stats.insert(name.clone(), value.clone());
            }
        }
        STATS.with(|s| *s.borrow_mut() = stats);
    }
}

// ---------- buildings.json ----------

pub mod buildings {
    use super::*;

    /// Fallback only – the real values are in `buildings.json`.
    pub fn defaults() -> Value {
        json!({
            "watchtower": {
                "lifetime": 10,         // seconds it stands after landing
                "damage_bonus": 20,     // percent more damage to enemies in its area (its window)
                "size": 24,             // pixels (square)
                "vision": 320,          // side length of its window
                "throw_range": 420,     // how far it's thrown from the ship
                "throw_time": 0.5,      // seconds in the air
                "lure_range": 220,      // melee enemies this close (and closer to it than to the ship) go for it
            }
        })
    }
    const POSITIVE: &[&str] = &["lifetime", "size", "vision", "throw_time"];

    thread_local! {
        static STATS: RefCell<Table> = RefCell::new(as_table(defaults()));
    }
    table_access!(STATS);

    pub fn reload(warnings: &mut Vec<String>) {
        let data = load("buildings.json", warnings);
        let mut stats = as_table(defaults());
        for (key, values) in stats.iter_mut() {
            let values = values.as_object_mut().unwrap();
            let entry = match data.get(key) {
                None => continue,
                Some(Value::Object(e)) => e.clone(),
                Some(_) => {
                    warnings.push(format!("buildings.json {key}: should be {{ … }} – using built-in values"));
                    continue;
                }
            };
            for (name, value) in &entry {
                if name == "price" || name == "price_step" {
                    warnings.push(format!("buildings.json {key}.{name}: what towers cost is in shop.json now – ignored"));
                } else if name == "base_hp" || name == "hit_cooldown" {
                    warnings.push(format!("buildings.json {key}.{name}: towers can't be hurt any more, they have a lifetime now – ignored"));
                } else if !values.contains_key(name) {
                    warnings.push(format!("buildings.json {key}: unknown value \"{name}\" – ignored"));
                } else if ok(name, value, POSITIVE) {
                    values.insert(name.clone(), value.clone());
                } else {
                    warnings.push(format!("buildings.json {key}.{name}: {} is not valid – using {}", dumps(value), num(&values[name])));
                }
            }
        }
        for key in data.keys() {
            if !stats.contains_key(key) {
                warnings.push(format!("buildings.json: unknown building \"{key}\" – ignored"));
            }
        }
        STATS.with(|s| *s.borrow_mut() = stats);
    }
}

// ---------- weapons.json ----------

pub mod weapons {
    use super::*;

    /// 1 = like the gun.
    fn upgrade_factor(over: Value) -> Value {
        let mut base = json!({"damage": 1, "attack_speed": 1, "wall_push": 1, "projectile_speed": 1});
        for (k, v) in over.as_object().unwrap() {
            base[k] = v.clone();
        }
        base
    }

    pub fn defaults() -> Value {
        json!({
            "gun": {
                "damage": 1.5, "attack_speed": 2, "projectile_speed": 880, "projectile_size": 7, "wall_push": 18,
                "energy_use": 8,
                "upgrade_factor": upgrade_factor(json!({})),
            },
            "machine_gun": {
                "damage": 0.5, "attack_speed": 8, "projectile_speed": 950, "projectile_size": 5, "wall_push": 7,
                "energy_use": 3, "spread": 30,
                "upgrade_factor": upgrade_factor(json!({"damage": 0.3, "attack_speed": 2, "wall_push": 0.5})),
            },
            "cannon": {
                "damage": 4.5, "attack_speed": 0.8, "projectile_speed": 650, "projectile_size": 14, "wall_push": 35,
                "energy_use": 22, "piercing": 1, "pierced_push": 0.5,
                "upgrade_factor": upgrade_factor(json!({"damage": 2, "attack_speed": 0.2, "wall_push": 2})),
            },
            "laser": {
                "damage": 0.04, "attack_speed": 30, "projectile_speed": 1500, "projectile_size": 4, "wall_push": 2,
                "energy_use": 1,
                "upgrade_factor": upgrade_factor(json!({"damage": 0.02, "attack_speed": 4, "wall_push": 0.15})),
            },
        })
    }
    const POSITIVE: &[&str] = &["attack_speed", "projectile_speed", "projectile_size"];

    thread_local! {
        static STATS: RefCell<Table> = RefCell::new(as_table(defaults()));
    }
    table_access!(STATS);

    /// How strongly the cards of type `kind` work on `weapon` (1 unless the file says otherwise).
    pub fn factor(weapon: &str, kind: &str) -> f64 {
        STATS.with(|t| t.borrow().get(weapon).and_then(|w| w.get("upgrade_factor")).and_then(|u| u.get(kind)).map(f).unwrap_or(1.0))
    }

    pub fn reload(warnings: &mut Vec<String>) {
        let data = load("weapons.json", warnings);
        let defaults = as_table(defaults());
        let mut stats = defaults.clone();
        merge(&mut stats, &data, &defaults, "", warnings);
        STATS.with(|s| *s.borrow_mut() = stats);
    }

    /// Copies the valid values of `data` into `target` – `defaults` says which keys exist and what they hold.
    fn merge(target: &mut Table, data: &Table, defaults: &Table, place_of: &str, warnings: &mut Vec<String>) {
        for (key, value) in data {
            let place = if place_of.is_empty() { key.clone() } else { format!("{place_of}.{key}") };
            match defaults.get(key) {
                None => warnings.push(format!("weapons.json {}: unknown key \"{key}\" – ignored", if place_of.is_empty() { "top level" } else { place_of })),
                Some(Value::Object(d)) => match value {
                    Value::Object(v) => {
                        let t = target.get_mut(key).and_then(|t| t.as_object_mut()).unwrap();
                        merge(t, v, d, &place, warnings);
                    }
                    _ => warnings.push(format!("weapons.json {place}: should be {{ … }} – using the defaults")),
                },
                Some(d) => {
                    if ok(key, value, POSITIVE) {
                        target.insert(key.clone(), value.clone());
                    } else {
                        warnings.push(format!("weapons.json {place}: {} is not valid – using {}", dumps(value), num(d)));
                    }
                }
            }
        }
    }
}

// ---------- ships.json ----------

pub mod ships {
    use super::*;
    use crate::engine::Color;

    pub const DEFAULT_KEY: &str = "standard";

    pub fn defaults() -> Table {
        as_table(json!({
            "name": "Standard",             // shown in the game
            "size": 30,                     // hitbox (square) and size of the ship
            "speed": 280,                   // pixels per second
            "max_energy": 100,              // energy for the weapons (full at the start) …
            "energy_regen": 20,             // … and how much comes back per second
            "invulnerable_time": 0.5,       // seconds after a hit in which it can't be hit again (it blinks)
            "armor": 0,                     // pixels every hit squeezes the arena less (flat, game/armor.rs)
            "magnet_radius": 130,           // stars and cores this close get pulled in
            "color": [40, 110, 255],
        }))
    }
    const POSITIVE: &[&str] = &["size", "speed", "max_energy"];

    /// One ship's values.
    #[derive(Debug, Clone, PartialEq)]
    pub struct ShipStats {
        pub name: String,
        pub size: f64,
        pub speed: f64,
        pub max_energy: f64,
        pub energy_regen: f64,
        pub invulnerable_time: f64,
        pub armor: f64,
        pub magnet_radius: f64,
        pub color: Color,
    }

    fn stats_of(t: &Table) -> ShipStats {
        ShipStats {
            name: t["name"].as_str().unwrap_or("Standard").to_string(),
            size: f(&t["size"]),
            speed: f(&t["speed"]),
            max_energy: f(&t["max_energy"]),
            energy_regen: f(&t["energy_regen"]),
            invulnerable_time: f(&t["invulnerable_time"]),
            armor: f(&t["armor"]),
            magnet_radius: f(&t["magnet_radius"]),
            color: Color::from_json(&t["color"]).unwrap_or(crate::engine::rgb(40, 110, 255)),
        }
    }

    thread_local! {
        /// ship key → its values, in file order; and the key of the one that is flown.
        static SHIPS: RefCell<(Vec<(String, ShipStats)>, String)> = RefCell::new((vec![(DEFAULT_KEY.to_string(), stats_of(&defaults()))], DEFAULT_KEY.to_string()));
    }

    /// The values of the ship that's flown.
    pub fn current() -> ShipStats {
        SHIPS.with(|s| {
            let s = s.borrow();
            s.0.iter().find(|(k, _)| *k == s.1).map(|(_, v)| v.clone()).unwrap_or_else(|| stats_of(&defaults()))
        })
    }
    pub fn get(key: &str) -> Option<ShipStats> {
        SHIPS.with(|s| s.borrow().0.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()))
    }

    pub fn reload(warnings: &mut Vec<String>) {
        let data = load("ships.json", warnings);
        for key in data.keys() {
            if key != "ship" && key != "ships" {
                warnings.push(format!("ships.json: unknown key \"{key}\" – ignored"));
            }
        }
        let entries = match data.get("ships") {
            None => {
                let mut m = Map::new();
                m.insert(DEFAULT_KEY.into(), json!({}));
                m
            }
            Some(Value::Object(m)) if !m.is_empty() => m.clone(),
            Some(_) => {
                warnings.push("ships.json ships: should be { \"name\": { … }, … } with at least one ship – using built-in values".into());
                let mut m = Map::new();
                m.insert(DEFAULT_KEY.into(), json!({}));
                m
            }
        };
        let mut ships = vec![];
        for (key, entry) in &entries {
            let entry = match entry {
                Value::Object(e) => e.clone(),
                _ => {
                    warnings.push(format!("ships.json {key}: should be {{ … }} – using built-in values"));
                    Map::new()
                }
            };
            ships.push((key.clone(), stats_of(&read_ship(key, &entry, warnings))));
        }
        let mut selected = data.get("ship").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| ships[0].0.clone());
        if !ships.iter().any(|(k, _)| *k == selected) {
            let fallback = ships[0].0.clone();
            let shown = data.get("ship").map(dumps).unwrap_or_else(|| format!("\"{selected}\""));
            warnings.push(format!("ships.json ship: {shown} isn't one of the ships – flying \"{fallback}\""));
            selected = fallback;
        }
        SHIPS.with(|s| *s.borrow_mut() = (ships, selected));
    }

    fn read_ship(key: &str, entry: &Table, warnings: &mut Vec<String>) -> Table {
        let defaults = defaults();
        let mut values = defaults.clone();
        for (name, value) in entry {
            if !defaults.contains_key(name) {
                warnings.push(format!("ships.json {key}: unknown value \"{name}\" – ignored"));
            } else if valid(name, value) {
                values.insert(name.clone(), value.clone());
            } else {
                warnings.push(format!("ships.json {key}.{name}: {} is not valid – using {}", dumps(value), dumps(&defaults[name])));
            }
        }
        values
    }

    fn valid(name: &str, value: &Value) -> bool {
        match name {
            "name" => value.as_str().is_some_and(|s| !s.trim().is_empty()),
            // [red, green, blue], each 0 … 255
            "color" => value.as_array().is_some_and(|l| l.len() == 3 && l.iter().all(|p| is_int(p) && (0..=255).contains(&p.as_i64().unwrap_or(-1)))),
            _ => is_number(value) && if POSITIVE.contains(&name) { f(value) > 0.0 } else { f(value) >= 0.0 },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_files_load_without_warnings() {
        let mut w = vec![];
        abilities::reload(&mut w);
        points::reload(&mut w);
        radar::reload(&mut w);
        buildings::reload(&mut w);
        weapons::reload(&mut w);
        ships::reload(&mut w);
        assert!(w.is_empty(), "{w:?}");
        assert!(abilities::get("dash", "distance") > 0.0);
        assert!(weapons::factor("laser", "damage") < 1.0);
    }
}
