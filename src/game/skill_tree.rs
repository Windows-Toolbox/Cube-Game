//! The core skill tree (`skill_tree.py`): permanent upgrades, bought with cores in the game menu between runs. A
//! node can have several levels (costs: one price per level) and needs other nodes first (requires: all of them).
//!
//! The tree is a graph: Shop sits in the middle, the branches grow outwards from it in every direction. Each node has
//! its own place (position, in grid units from the middle); the lines come from requires.
//!
//! What each node costs and does is in `skilltree.json`: its costs and its values – mostly "per_level", how much one
//! level adds. `reload` reads it again. The costs and values below are only the fallback.
//! What the bought nodes (and the active cards) do to a new run is in the functions at the bottom – the game menu
//! uses them, too, to show the next run's ship.

use std::cell::RefCell;
use std::rc::Rc;

use serde_json::{Map, Value, json};

use super::abilities::{Ability, Burning, Kind, Mines, SlowFieldValues};
use super::buildings::WatchtowerKit;
use super::cards;
use super::difficulty::Difficulty;
use super::profile::Profile;
use super::radar::Radar;
use super::ship::Ship;
use super::tables::{self, abilities as ability_data, f, is_int, is_number};
use super::weapons::{self, STARTER, Weapon};
use crate::engine::geom::py_round;
use crate::engine::{Color, Vec2, rgb};
use crate::pyfmt::{dumps, g, plus_g, plus_g_prec, round_to};

pub const SHOP: &str = "shop";
pub const DASH: &str = "dash";
pub const ROCKET: &str = "rocket";
pub const HEAD_START: &str = "head_start";
pub const BIGGER_ARENA: &str = "bigger_arena";
pub const CORE_LUCK: &str = "core_luck";
pub const WATCHTOWER: &str = "watchtower";
pub const RADAR: &str = "radar";
pub const RADAR_SIZE: &str = "radar_size";
pub const RADAR_COOLDOWN: &str = "radar_cooldown";
pub const RADAR_DURATION: &str = "radar_duration";
pub const POINTS: &str = "points";
pub const POINT_MAGNET: &str = "point_magnet";
pub const POINT_REPAIR: &str = "point_repair";
pub const POINT_RATE: &str = "point_rate";
pub const RADAR_FIND: &str = "radar_find";
pub const BURNING_GROUND: &str = "burning_ground";
pub const MINES: &str = "mines";
pub const SLOW_FIELD: &str = "slow_field";
pub const ARMOR: &str = "armor";
pub const RECOVERY: &str = "recovery";
/// The Cards tab of the game menu – without it, no cards (and active ones do not apply).
pub const CARD_SHOP: &str = "card_shop";
/// More cards active at once.
pub const CARD_SLOTS: &str = "card_slots";
const GUN_UPGRADES: &[&str] = &["gun_energy", "gun_attack_speed", "gun_damage"];

pub struct Branch {
    pub key: &'static str,
    pub name: &'static str,
    pub color: Color,
    /// Where its name is written (grid units) – `None`: its first node names it.
    pub label_at: Option<(f64, f64)>,
}

pub static BRANCHES: &[Branch] = &[
    Branch { key: "ship", name: "Ship", color: rgb(40, 110, 255), label_at: Some((0.0, 1.75)) },
    Branch { key: "energy", name: "Energy", color: rgb(255, 225, 90), label_at: Some((0.0, -1.1)) },
    Branch { key: "gun", name: "Gun", color: rgb(235, 235, 235), label_at: Some((-2.3, 0.0)) },
    Branch { key: "machine_gun", name: "Machine gun", color: rgb(255, 170, 60), label_at: None },
    Branch { key: "cannon", name: "Cannon", color: rgb(200, 140, 255), label_at: None },
    Branch { key: "laser", name: "Laser", color: rgb(90, 255, 160), label_at: None },
    Branch { key: "rocket", name: "Rocket", color: rgb(255, 70, 50), label_at: None },
    Branch { key: "dash", name: "Dash", color: rgb(80, 200, 255), label_at: None },
    Branch { key: "scouting", name: "Scouting", color: rgb(120, 180, 255), label_at: None },
];

pub fn branch(key: &str) -> &'static Branch {
    BRANCHES.iter().find(|b| b.key == key).unwrap_or(&BRANCHES[0])
}

#[derive(Debug, Clone)]
pub struct Node {
    /// Saved in the profile – do not rename.
    pub key: &'static str,
    pub name: &'static str,
    pub description: String,
    /// Cores for each level – the number of levels is its length.
    pub costs: Vec<i64>,
    pub requires: Vec<&'static str>,
    pub branch: &'static str,
    /// (x, y) in grid units from the middle of the tree – y grows downwards.
    pub position: (f64, f64),
    /// What it does (`skilltree.json`).
    pub values: Map<String, Value>,
    /// True: the required nodes have to be at their maximum level, not just bought.
    pub requires_max: bool,
    /// The required nodes a line is drawn from (`None`: all).
    pub lines_from: Option<Vec<&'static str>>,
}

impl Node {
    pub fn max_level(&self) -> i64 {
        self.costs.len() as i64
    }
    pub fn value(&self, name: &str) -> f64 {
        self.values.get(name).map(f).unwrap_or(0.0)
    }
}

fn n(key: &'static str, name: &'static str, description: &str, costs: &[i64], requires: &[&'static str], branch: &'static str, position: (f64, f64)) -> Node {
    Node { key, name, description: description.into(), costs: costs.to_vec(), requires: requires.to_vec(), branch, position, values: Map::new(), requires_max: false, lines_from: None }
}

/// The tree as built in – `reload` makes the nodes from it with the costs and values of `skilltree.json`.
fn default_nodes() -> Vec<Node> {
    vec![
        n(SHOP, "Shop", "T during a run: buy upgrades with stars. Killed enemies drop stars.", &[1], &[], "ship", (0.0, 0.0)),
        // ship – downwards
        n(HEAD_START, "Head start", "Every run starts with {per_level:g} more stars.", &[2, 3, 5], &[SHOP], "ship", (-0.9, 1.1)),
        n(BIGGER_ARENA, "Bigger arena", "The window starts {width:g} × {height:g} px bigger.", &[2, 4, 6], &[HEAD_START], "ship", (-0.9, 2.4)),
        n(CORE_LUCK, "Core luck", "Cores drop more often: +{per_level:.1%} per level.", &[3, 6, 9], &[SHOP], "ship", (0.9, 1.1)),
        n(CARD_SHOP, "Card shop", "Unlocks the Cards tab: buy cards with cores – active cards apply at the start of every run.", &[4], &[CORE_LUCK], "ship", (0.9, 2.4)),
        n(CARD_SLOTS, "Card slots", "One more card can be active at once: +{per_level:g} per level.", &[4, 8], &[CARD_SHOP], "ship", (0.9, 3.3)),
        n(ARMOR, "Armor", "Every hit squeezes the window a few pixels less.", &[3, 6, 10], &[BIGGER_ARENA], "ship", (-0.9, 3.3)),
        n(RECOVERY, "Recovery", "After a hit you stay invulnerable for longer.", &[2, 4, 6], &[ARMOR], "ship", (0.0, 3.3)),
        // energy and the laser behind it – upwards
        n("max_energy", "Max energy", "More energy for your weapons.", &[2, 4, 6], &[SHOP], "energy", (-0.9, -1.1)),
        n("energy_regen", "Energy regen", "Energy refills faster.", &[2, 4, 6], &[SHOP], "energy", (0.9, -1.1)),
        Node { requires_max: true, ..n("laser", "Laser", "A weapon: a beam that barely hurts enemies, but pushes the walls hard.", &[6], &["max_energy", "energy_regen"], "laser", (0.0, -2.15)) },
        n("laser_damage", "Damage", "More damage.", &[2, 4, 6], &["laser"], "laser", (-1.0, -3.05)),
        n("laser_energy", "Energy use", "Uses less energy.", &[2, 4, 6], &["laser"], "laser", (0.0, -3.3)),
        n("laser_wall_push", "Wall push", "Pushes the walls further.", &[2, 4, 6], &["laser"], "laser", (1.0, -3.05)),
        // the gun – to the left, the machine gun and the cannon behind it
        n("gun_energy", "Energy use", "Uses less energy.", &[2, 3, 5], &[SHOP], "gun", (-1.35, 0.0)),
        n("gun_attack_speed", "Attack speed", "Shoots faster.", &[2, 3, 5], &["gun_energy"], "gun", (-2.3, -0.75)),
        n("gun_damage", "Damage", "More damage.", &[2, 3, 5], &["gun_energy"], "gun", (-2.3, 0.75)),
        Node {
            requires_max: true,
            lines_from: Some(vec!["gun_attack_speed"]),
            ..n("machine_gun", "Machine gun", "A weapon: very fast weak shots – great against unarmored enemies, bad against armor and walls.", &[5], GUN_UPGRADES, "machine_gun", (-3.4, -1.45))
        },
        n("mg_attack_speed", "Attack speed", "Shoots faster – and spreads more.", &[2, 4, 6], &["machine_gun"], "machine_gun", (-3.0, -2.55)),
        n("mg_damage", "Damage", "More damage – and spreads more.", &[2, 4, 6], &["machine_gun"], "machine_gun", (-4.35, -2.05)),
        n("mg_energy", "Energy use", "Uses less energy.", &[2, 4, 6], &["machine_gun"], "machine_gun", (-4.6, -0.95)),
        n("mg_precision", "Precision", "Its shots spread less.", &[3, 5, 8], &["mg_attack_speed"], "machine_gun", (-3.6, -3.4)),
        n("mg_wall_push", "Wall push", "Pushes the walls further.", &[2, 4, 6], &["mg_damage"], "machine_gun", (-5.45, -2.65)),
        n("mg_projectile_speed", "Projectile speed", "Faster projectiles.", &[2, 4, 6], &["mg_energy"], "machine_gun", (-5.6, -1.3)),
        Node {
            requires_max: true,
            lines_from: Some(vec!["gun_damage"]),
            ..n("cannon", "Cannon", "A weapon: slow heavy shots that pierce – great against armor and walls, wasted on weak enemies.", &[5], GUN_UPGRADES, "cannon", (-3.4, 1.45))
        },
        n("cannon_damage", "Damage", "More damage – and uses more energy.", &[2, 4, 6], &["cannon"], "cannon", (-3.0, 2.55)),
        n("cannon_attack_speed", "Attack speed", "Shoots faster.", &[2, 4, 6], &["cannon"], "cannon", (-4.35, 2.05)),
        n("cannon_energy", "Energy use", "Uses less energy.", &[2, 4, 6], &["cannon"], "cannon", (-4.6, 0.95)),
        n("cannon_piercing", "Piercing", "Each shot pierces one more enemy.", &[6, 10], &["cannon_damage"], "cannon", (-3.6, 3.4)),
        n("cannon_projectile_speed", "Projectile speed", "Faster projectiles.", &[2, 4, 6], &["cannon_attack_speed"], "cannon", (-5.45, 2.65)),
        n("cannon_wall_push", "Wall push", "Pushes the walls further.", &[2, 4, 6], &["cannon_energy"], "cannon", (-5.6, 1.3)),
        // abilities – to the right: the dash in the middle, the rocket below
        n(DASH, "Dash", "SHIFT: a quick burst in the direction you're moving.", &[3], &[SHOP], "dash", (1.6, 0.0)),
        n("dash_distance", "Distance", "Dashes further.", &[3, 5, 8], &[DASH], "dash", (2.75, 0.0)),
        n("dash_charges", "Charges", "One more dash stored: dash again right away, each refills on its own.", &[5, 10], &["dash_distance"], "dash", (3.85, -0.35)),
        n(MINES, "Mines", "Every dash leaves a mine where it started. It blows up when an enemy or the wall touches it.", &[25], &["dash_charges"], "dash", (5.0, -0.35)),
        n("mine_power", "Power", "Mines do more damage and blow up wider.", &[6, 12, 20], &[MINES], "dash", (4.5, -1.05)),
        n(SLOW_FIELD, "Slow field", "A mine's blast leaves a field behind that slows the enemies in it.", &[30], &[MINES], "dash", (6.0, 0.35)),
        n("slow_strength", "Strength", "The slow field slows more.", &[6, 12, 20], &[SLOW_FIELD], "dash", (6.1, -0.75)),
        n("slow_size", "Size", "The slow field gets bigger.", &[6, 12, 20], &[SLOW_FIELD], "dash", (6.15, 1.35)),
        n("slow_duration", "Duration", "The slow field lasts longer.", &[6, 12, 20], &[SLOW_FIELD], "dash", (5.3, 1.0)),
        n(ROCKET, "Rocket", "SPACE: a rocket that explodes at the mouse and pushes the walls out.", &[4], &[SHOP], "rocket", (2.6, 1.25)),
        n("rocket_cooldown", "Cooldown", "Ready again sooner.", &[2, 4, 6], &[ROCKET], "rocket", (3.95, 0.6)),
        n("rocket_radius", "Blast radius", "The blast reaches further.", &[2, 4, 6], &[ROCKET], "rocket", (4.1, 1.65)),
        n("rocket_damage", "Damage", "More damage.", &[2, 4, 6], &[ROCKET], "rocket", (3.3, 2.7)),
        n("rocket_wall_push", "Wall push", "Pushes the walls further.", &[2, 4, 6], &[ROCKET], "rocket", (2.0, 2.7)),
        Node { requires_max: true, ..n(BURNING_GROUND, "Burning ground", "The blast leaves burning ground behind that hurts enemies.", &[15], &["rocket_cooldown", "rocket_radius", "rocket_damage"], "rocket", (5.35, 2.35)) },
        // watchtower and radar – up and to the right
        n(WATCHTOWER, "Watchtower", "B: throw a tower (stars) – for a while its own window watches there, and enemies in it take more damage. +{per_level:g} tower per level.", &[3, 5, 8], &[SHOP], "scouting", (2.6, -1.25)),
        n(RADAR, "Radar", "Every now and then a light blue window shows the biggest enemy group out of sight.", &[4], &[WATCHTOWER], "scouting", (4.0, -2.05)),
        n(RADAR_SIZE, "Scan size", "The radar's window gets bigger.", &[2, 4, 6], &[RADAR], "scouting", (4.5, -3.25)),
        n(RADAR_COOLDOWN, "Scan rate", "The radar scans more often.", &[2, 4, 6], &[RADAR], "scouting", (5.4, -2.6)),
        n(RADAR_DURATION, "Scan time", "The radar's window stays open longer.", &[2, 4, 6], &[RADAR], "scouting", (5.4, -1.55)),
        n(POINTS, "Points of interest", "Now and then something appears out of sight – a ? at the window's edge points to it. Reach it with the window or a watchtower's window: a star chest (+{stars:g} stars).", &[6], &[WATCHTOWER], "scouting", (2.3, -2.5)),
        n(POINT_MAGNET, "Magnet", "A find can be a magnet: for a while every star and core flies to you.", &[5], &[POINTS], "scouting", (1.8, -3.5)),
        n(POINT_REPAIR, "Repair kit", "A find can be a repair kit: every wall springs out.", &[6], &[POINTS], "scouting", (2.75, -3.8)),
        n(POINT_RATE, "More finds", "Finds appear {per_level:g} s sooner per level.", &[4, 8, 12], &[POINTS], "scouting", (3.05, -2.95)),
        n(RADAR_FIND, "Find", "Now and then a scan goes to a point of interest nobody has seen yet and reveals it: {per_level:.0%} of the scans per level.", &[4, 8, 12], &[RADAR, POINTS], "scouting", (3.65, -3.75)),
    ]
}

/// What each node does – the fallback for `skilltree.json`.
fn default_values(key: &str) -> Value {
    match key {
        HEAD_START => json!({"per_level": 10}),
        BIGGER_ARENA => json!({"per_level": [60, 40]}),
        CORE_LUCK => json!({"per_level": 0.005}),
        CARD_SLOTS => json!({"per_level": 1}),
        ARMOR => json!({"per_level": 3}),
        RECOVERY => json!({"per_level": 0.15}),
        "max_energy" => json!({"per_level": 20}),
        "energy_regen" => json!({"per_level": 4}),
        "gun_energy" => json!({"per_level": -1}),
        "gun_attack_speed" => json!({"per_level": 0.3}),
        "gun_damage" => json!({"per_level": 0.4}),
        "mg_attack_speed" => json!({"per_level": 1, "spread_per_level": 3}),
        "mg_damage" => json!({"per_level": 0.1, "spread_per_level": 3}),
        "mg_energy" => json!({"per_level": -0.4}),
        "mg_precision" => json!({"per_level": -6}),
        "mg_wall_push" => json!({"per_level": 1.5}),
        "mg_projectile_speed" => json!({"per_level": 100}),
        "cannon_damage" => json!({"per_level": 1, "energy_per_level": 1}),
        "cannon_attack_speed" => json!({"per_level": 0.1}),
        "cannon_energy" => json!({"per_level": -2}),
        "cannon_piercing" => json!({"per_level": 1}),
        "cannon_projectile_speed" => json!({"per_level": 80}),
        "cannon_wall_push" => json!({"per_level": 6}),
        "laser_damage" => json!({"per_level": 0.01}),
        "laser_energy" => json!({"per_level": -0.12}),
        "laser_wall_push" => json!({"per_level": 0.4}),
        "dash_distance" => json!({"per_level": 25}),
        "dash_charges" => json!({"per_level": 1}),
        "mine_power" => json!({"per_level": 1, "blast_radius_per_level": 10}),
        "slow_strength" => json!({"per_level": 5}),
        "slow_size" => json!({"per_level": 15}),
        "slow_duration" => json!({"per_level": 1}),
        "rocket_cooldown" => json!({"per_level": -0.6}),
        "rocket_radius" => json!({"per_level": 15}),
        "rocket_damage" => json!({"per_level": 1}),
        "rocket_wall_push" => json!({"per_level": 20}),
        BURNING_GROUND => json!({"duration": 3, "damage_per_second": 1.5, "radius": 0.8}),
        WATCHTOWER => json!({"per_level": 1}),
        RADAR_SIZE => json!({"per_level": 30}),
        RADAR_COOLDOWN => json!({"per_level": -0.75}),
        RADAR_DURATION => json!({"per_level": 1}),
        POINT_RATE => json!({"per_level": 8}),
        RADAR_FIND => json!({"per_level": 0.15}),
        _ => json!({}),
    }
}
const MUST_NOT_BE_NEGATIVE: &[&str] = &[HEAD_START, BIGGER_ARENA, CORE_LUCK, WATCHTOWER, BURNING_GROUND, ARMOR, RECOVERY, CARD_SLOTS, POINT_RATE, RADAR_FIND];
/// Descriptions with their values in them.
const DESCRIBED: &[&str] = &[HEAD_START, BIGGER_ARENA, CORE_LUCK, WATCHTOWER, CARD_SLOTS, POINT_RATE, RADAR_FIND, POINTS];

thread_local! {
    static NODES: RefCell<Vec<Rc<Node>>> = const { RefCell::new(vec![]) };
}

/// The tree as it is now.
pub fn nodes() -> Vec<Rc<Node>> {
    if NODES.with(|n| n.borrow().is_empty()) {
        reload(&mut vec![]);
    }
    NODES.with(|n| n.borrow().clone())
}
pub fn node(key: &str) -> Option<Rc<Node>> {
    nodes().into_iter().find(|n| n.key == key)
}

/// Fills `{name:spec}` in a description – only the specs the descriptions use: g, .1%, .0%.
fn describe(text: &str, values: &[(&str, f64)]) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let end = rest[start..].find('}').map(|e| start + e).unwrap_or(rest.len() - 1);
        let field = &rest[start + 1..end];
        let (name, spec) = field.split_once(':').unwrap_or((field, ""));
        let v = values.iter().find(|(n, _)| *n == name).map(|(_, v)| *v).unwrap_or(0.0);
        out.push_str(&match spec {
            ".1%" => format!("{:.1}%", v * 100.0),
            ".0%" => format!("{:.0}%", v * 100.0),
            _ => g(v),
        });
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Reads `skilltree.json` again.
pub fn reload(warnings: &mut Vec<String>) {
    let data = tables::load("skilltree.json", warnings);
    let defaults = default_nodes();
    for key in data.keys() {
        if !defaults.iter().any(|n| n.key == key) {
            warnings.push(format!("skilltree.json: unknown node \"{key}\" – ignored"));
        }
    }
    let mut made = vec![];
    for node in defaults {
        let entry = match data.get(node.key) {
            None => Map::new(),
            Some(Value::Object(e)) => e.clone(),
            Some(_) => {
                warnings.push(format!("skilltree.json {}: should be {{ … }} – using built-in values", node.key));
                Map::new()
            }
        };
        let (costs, values) = read_node(&node, &entry, warnings);
        let mut description = node.description.clone();
        if DESCRIBED.contains(&node.key) {
            let stars = tables::points::get("chest", "stars");
            let mut texts: Vec<(&str, f64)> = vec![("stars", stars)];
            match values.get("per_level") {
                Some(Value::Array(pair)) => {
                    texts.push(("width", f(&pair[0])));
                    texts.push(("height", f(&pair[1])));
                }
                _ => {
                    for (k, v) in &values {
                        if let Some(x) = v.as_f64() {
                            texts.push((leak(k), x));
                        }
                    }
                }
            }
            description = describe(&description, &texts);
        }
        made.push(Rc::new(Node { costs, values, description, ..node }));
    }
    NODES.with(|n| *n.borrow_mut() = made);
}

/// The few value names that end up in descriptions – kept for the formatter.
fn leak(name: &str) -> &'static str {
    match name {
        "per_level" => "per_level",
        "spread_per_level" => "spread_per_level",
        "energy_per_level" => "energy_per_level",
        "blast_radius_per_level" => "blast_radius_per_level",
        "duration" => "duration",
        "damage_per_second" => "damage_per_second",
        "radius" => "radius",
        _ => "other",
    }
}

/// (costs, values) of `node`: the valid ones from its entry in the file, else the built-in ones.
fn read_node(node: &Node, entry: &Map<String, Value>, warnings: &mut Vec<String>) -> (Vec<i64>, Map<String, Value>) {
    let mut costs = node.costs.clone();
    let mut values = match default_values(node.key) {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    for (name, value) in entry {
        let place = format!("skilltree.json {}.{name}", node.key);
        if name == "costs" {
            match value.as_array() {
                Some(l) if !l.is_empty() && l.iter().all(|c| is_int(c) && c.as_i64().unwrap_or(-1) >= 0) => costs = l.iter().map(|c| c.as_i64().unwrap()).collect(),
                _ => warnings.push(format!("{place}: {} is not valid (whole numbers, one per level) – using {}", dumps(value), dumps(&json!(costs)))),
            }
        } else if !values.contains_key(name) {
            let mut known = vec!["costs".to_string()];
            known.extend(values.keys().cloned());
            warnings.push(format!("skilltree.json {}: unknown value \"{name}\" – ignored (only {})", node.key, known.join(", ")));
        } else if values[name].is_array() {
            // a pair: Bigger arena's [wider, higher]
            match value.as_array() {
                Some(l) if l.len() == 2 && l.iter().all(|p| number(p, false)) => {
                    values.insert(name.clone(), value.clone());
                }
                _ => warnings.push(format!("{place}: {} is not valid ([wider, higher]) – using {}", dumps(value), dumps(&values[name]))),
            }
        } else if number(value, !MUST_NOT_BE_NEGATIVE.contains(&node.key)) {
            values.insert(name.clone(), value.clone());
        } else {
            warnings.push(format!("{place}: {} is not valid – using {}", dumps(value), dumps(&values[name])));
        }
    }
    (costs, values)
}

fn number(v: &Value, negative_allowed: bool) -> bool {
    is_number(v) && (negative_allowed || f(v) >= 0.0)
}

/// What the bought levels of node `key` add: its value `name` × its level (0 if not bought).
pub fn amount(profile: &Profile, key: &str, name: &str) -> f64 {
    node(key).map(|n| n.value(name)).unwrap_or(0.0) * profile.level(key) as f64
}
pub fn per_level(profile: &Profile, key: &str) -> f64 {
    amount(profile, key, "per_level")
}

/// Upgrade nodes: node key → (part, value) – the part is a weapon's key, "dash", "rocket", "energy" (the ship's),
/// "ship" (its hull), "mine" or "slow_field"; the value what one level raises by the node's per_level.
pub const EFFECTS: &[(&str, &str, &str)] = &[
    ("max_energy", "energy", "max"),
    ("energy_regen", "energy", "regen"),
    (ARMOR, "ship", "armor"),
    (RECOVERY, "ship", "invulnerable_time"),
    ("gun_energy", "gun", "energy_use"),
    ("gun_attack_speed", "gun", "attack_speed"),
    ("gun_damage", "gun", "damage"),
    ("mg_attack_speed", "machine_gun", "attack_speed"),
    ("mg_damage", "machine_gun", "damage"),
    ("mg_energy", "machine_gun", "energy_use"),
    ("mg_precision", "machine_gun", "spread"),
    ("mg_wall_push", "machine_gun", "wall_push"),
    ("mg_projectile_speed", "machine_gun", "projectile_speed"),
    ("cannon_damage", "cannon", "damage"),
    ("cannon_attack_speed", "cannon", "attack_speed"),
    ("cannon_energy", "cannon", "energy_use"),
    ("cannon_piercing", "cannon", "piercing"),
    ("cannon_projectile_speed", "cannon", "projectile_speed"),
    ("cannon_wall_push", "cannon", "wall_push"),
    ("laser_damage", "laser", "damage"),
    ("laser_energy", "laser", "energy_use"),
    ("laser_wall_push", "laser", "wall_push"),
    ("dash_distance", "dash", "distance"),
    ("dash_charges", "dash", "charges"),
    ("mine_power", "mine", "damage"),
    ("slow_strength", "slow_field", "slow"),
    ("slow_size", "slow_field", "size"),
    ("slow_duration", "slow_field", "duration"),
    ("rocket_cooldown", "rocket", "cooldown"),
    ("rocket_radius", "rocket", "blast_radius"),
    ("rocket_damage", "rocket", "damage"),
    ("rocket_wall_push", "rocket", "wall_push"),
];
pub fn effect_of(key: &str) -> Option<(&'static str, &'static str)> {
    EFFECTS.iter().find(|(k, _, _)| *k == key).map(|(_, p, v)| (*p, *v))
}
/// What some nodes change on top of their EFFECTS value: their value in the file → the part's value per level.
pub const SIDE_EFFECTS: &[(&str, &str)] = &[("spread_per_level", "spread"), ("energy_per_level", "energy_use"), ("blast_radius_per_level", "blast_radius")];
fn effect_unit(value: &str) -> &'static str {
    match value {
        "damage" => " damage",
        "attack_speed" => " shots/s",
        "energy_use" => " energy per shot",
        "wall_push" => " px wall push",
        "projectile_speed" => " px/s projectile speed",
        "spread" => "° spread",
        "piercing" => " enemy pierced",
        "max" => " max energy",
        "regen" => " energy/s",
        "cooldown" => " s cooldown",
        "blast_radius" => " px blast radius",
        "distance" => " px dash distance",
        "charges" => " dash stored",
        "slow" => " % slower",
        "size" => " px slow field size",
        "duration" => " s slow field",
        "armor" => " px less damage per hit",
        "invulnerable_time" => " s invulnerable after a hit",
        _ => "",
    }
}
/// The laser's values per second.
fn per_second(value: &str) -> Option<&'static str> {
    match value {
        "damage" => Some(" damage/s"),
        "energy_use" => Some(" energy/s"),
        "wall_push" => Some(" px/s wall push"),
        _ => None,
    }
}

/// What one level of an upgrade node does, e.g. "+0.4 damage per level" – `None` for nodes without a number.
pub fn effect_text(node: &Node) -> Option<String> {
    let (part, value) = effect_of(node.key)?;
    let per = node.value("per_level");
    let mut text = match (part == "laser", per_second(value)) {
        (true, Some(unit)) => format!("{}{unit}", plus_g_prec(per * tables::weapons::get(part, "attack_speed"), 3)),
        _ => format!("{}{}", plus_g(per), effect_unit(value)),
    };
    for (name, side) in SIDE_EFFECTS {
        if node.values.contains_key(*name) {
            text += &format!(", {}{}", plus_g(node.value(name)), effect_unit(side));
        }
    }
    Some(text + " per level")
}

// ---------- what the bought nodes do to a new run ----------

/// The ship a run starts with: the weapons of the loadout (at least the gun), its energy and hull, the unlocked
/// abilities, the watchtower kit, the head start stars and the active cards (once the card shop is unlocked) – with
/// every bought upgrade.
pub fn start_ship(profile: &Profile, center: Vec2) -> Ship {
    let mut ship = Ship::new(center, None);
    ship.weapons = [0, 1].map(|i| profile.loadout[i].as_deref().filter(|k| weapons::unlocked(profile, k)).map(|k| build_weapon(profile, k)));
    if ship.weapons.iter().all(|w| w.is_none()) {
        ship.weapons[0] = Some(build_weapon(profile, STARTER));
    }
    for (key, part, value) in EFFECTS {
        if (*part == "energy" || *part == "ship") && profile.owns(key) {
            ship.upgrade(value, per_level(profile, key));
        }
    }
    ship.abilities = [profile.owns(DASH).then(|| build_dash(profile)), profile.owns(ROCKET).then(|| build_rocket(profile))];
    ship.stars = py_round(per_level(profile, HEAD_START)) as i64;
    if profile.owns(WATCHTOWER) {
        ship.watchtower = Some(WatchtowerKit::new(py_round(per_level(profile, WATCHTOWER)) as i64));
    }
    if profile.owns(CARD_SHOP) {
        for key in profile.active_cards() {
            if let Some(card) = cards::get(&key) {
                card.apply(&mut ship, true); // a dash card without the dash simply changes nothing
            }
        }
    }
    ship
}

/// The weapon `key` with its bought skill tree upgrades.
pub fn build_weapon(profile: &Profile, key: &str) -> Weapon {
    let mut weapon = Weapon::new(key);
    for (node_key, part, value) in EFFECTS {
        if *part == key && profile.owns(node_key) {
            weapon.upgrade(value, per_level(profile, node_key));
            let node = node(node_key).unwrap();
            for (name, side) in SIDE_EFFECTS {
                // MG: more spread, cannon: more energy
                if node.values.contains_key(*name) {
                    weapon.upgrade(side, amount(profile, node_key, name));
                }
            }
        }
    }
    weapon
}

/// The dash with its bought skill tree upgrades.
pub fn build_dash(profile: &Profile) -> Ability {
    let mut dash = Ability::dash();
    for (node_key, part, value) in EFFECTS {
        if *part == "dash" && profile.owns(node_key) {
            dash.upgrade(value, per_level(profile, node_key));
        }
    }
    let mine = profile.owns(MINES).then(|| upgraded(profile, "mine", ability_data::section("mine")));
    let slow = profile.owns(SLOW_FIELD).then(|| upgraded(profile, "slow_field", ability_data::section("slow_field")));
    if let Kind::Dash { mines, slow_field, .. } = &mut dash.kind {
        *mines = mine.map(|m| Mines { damage: m["damage"], blast_radius: m["blast_radius"], wall_push: m["wall_push"], size: m["size"] });
        *slow_field = slow.map(|s| SlowFieldValues { slow: s["slow"], size: s["size"], duration: s["duration"] });
    }
    dash
}

/// A copy of `values` (e.g. the mine's from `abilities.json`) with the bought upgrades of `part` added, never below 0.
fn upgraded(profile: &Profile, part: &str, values: Map<String, Value>) -> std::collections::HashMap<String, f64> {
    let mut out: std::collections::HashMap<String, f64> = values.iter().map(|(k, v)| (k.clone(), f(v))).collect();
    for (node_key, node_part, value) in EFFECTS {
        if *node_part == part && profile.owns(node_key) {
            *out.entry(value.to_string()).or_insert(0.0) += per_level(profile, node_key);
            let node = node(node_key).unwrap();
            for (name, side) in SIDE_EFFECTS {
                if node.values.contains_key(*name) {
                    *out.entry(side.to_string()).or_insert(0.0) += amount(profile, node_key, name);
                }
            }
        }
    }
    out.into_iter().map(|(k, v)| (k, round_to(v, 6).max(0.0))).collect()
}

/// The rocket with its bought skill tree upgrades.
pub fn build_rocket(profile: &Profile) -> Ability {
    let mut rocket = Ability::rocket();
    for (node_key, part, value) in EFFECTS {
        if *part == "rocket" && profile.owns(node_key) {
            rocket.upgrade(value, per_level(profile, node_key));
        }
    }
    if let Kind::Rocket { burning, .. } = &mut rocket.kind {
        if profile.owns(BURNING_GROUND) {
            let n = node(BURNING_GROUND).unwrap();
            *burning = Some(Burning { duration: n.value("duration"), damage_per_second: n.value("damage_per_second"), radius: n.value("radius") });
        }
    }
    rocket
}

/// The run's radar with the bought upgrades – `None` if it is not unlocked.
pub fn start_radar(profile: &Profile) -> Option<Radar> {
    if !profile.owns(RADAR) {
        return None;
    }
    let find = if profile.owns(POINTS) { per_level(profile, RADAR_FIND) } else { 0.0 };
    Some(Radar::new(per_level(profile, RADAR_SIZE), per_level(profile, RADAR_COOLDOWN), per_level(profile, RADAR_DURATION), find))
}

/// The run's finds (points of interest for the player): which kinds and how often – `None` without the node.
pub fn start_finds(profile: &Profile) -> Option<(Vec<String>, f64)> {
    if !profile.owns(POINTS) {
        return None;
    }
    let mut kinds = vec!["chest".to_string()];
    for (key, kind) in [(POINT_MAGNET, "magnet"), (POINT_REPAIR, "repair")] {
        if profile.owns(key) {
            kinds.push(kind.into());
        }
    }
    Some((kinds, tables::points::get("finds", "interval") - per_level(profile, POINT_RATE)))
}

pub fn start_arena_size(profile: &Profile, difficulty: &Difficulty) -> (f64, f64) {
    let (w, h) = difficulty.arena.start_size;
    let level = profile.level(BIGGER_ARENA) as f64;
    let pair = node(BIGGER_ARENA).and_then(|n| n.values.get("per_level").and_then(|v| v.as_array()).map(|l| (f(&l[0]), f(&l[1])))).unwrap_or((60.0, 40.0));
    (py_round(w + pair.0 * level), py_round(h + pair.1 * level))
}

/// Chance that a killed enemy drops a core: the level's plus "Core luck".
pub fn core_chance(profile: &Profile, difficulty: &Difficulty) -> f64 {
    difficulty.core_chance + per_level(profile, CORE_LUCK)
}
