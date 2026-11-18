//! What stays between runs and after closing the game (`profile.py`): cores, bought skill tree nodes, the loadout
//! (which weapon is in which slot), the card collection and the card shop's offers, the chosen level and the best
//! time on each level (which also decides which levels are cleared and so which are unlocked).
//!
//! Saved as `data\profile.json` next to the exe. A missing or broken file simply means a fresh start; values that do
//! not make sense are ignored. The two settings of the Toolbox page (key display, all monitors) are in
//! `data\settings.json` – the kit's file; the game only reads them (`settings`).

use std::path::PathBuf;

use serde_json::{Map, Value, json};

use super::cards;
use super::difficulty;
use super::skill_tree::{self, Node};
use super::weapons::{self, STARTER};

/// Old node keys → their keys now.
const RENAMED_NODES: &[(&str, &str)] = &[("mg_shot_speed", "mg_projectile_speed"), ("cannon_shot_speed", "cannon_projectile_speed")];

/// The Toolbox page's settings, as the game reads them.
#[derive(Debug, Clone, PartialEq)]
pub struct GameSettings {
    /// The key display in the bottom right at the start of a run (F3 toggles it).
    pub key_display: bool,
    /// The arena may spread over all monitors (only where there really is screen).
    pub all_monitors: bool,
}
impl Default for GameSettings {
    fn default() -> Self {
        GameSettings { key_display: true, all_monitors: false }
    }
}

#[derive(Debug, Clone)]
pub struct Profile {
    /// Where it is saved – `None`: nowhere (tests, the bot).
    pub file: Option<PathBuf>,
    pub cores: i64,
    /// Node key → level bought (in the order they were bought).
    pub levels: Vec<(String, i64)>,
    /// Weapon key in each weapon slot (left / right mouse button), `None`: empty.
    pub loadout: [Option<String>; 2],
    pub difficulty: String,
    /// Level key → seconds survived.
    pub best_times: Vec<(String, f64)>,
    /// The card collection: (key, active).
    pub cards: Vec<(String, bool)>,
    /// The card shop's offers: card keys, `None` for a sold slot.
    pub card_offers: Vec<Option<String>>,
    pub settings: GameSettings,
}

impl Profile {
    pub fn new(file: Option<PathBuf>) -> Profile {
        Profile {
            file,
            cores: 0,
            levels: vec![],
            loadout: [Some(STARTER.to_string()), None],
            difficulty: difficulty::default_key(),
            best_times: vec![],
            cards: vec![],
            card_offers: vec![],
            settings: GameSettings::default(),
        }
    }

    pub fn load(file: Option<PathBuf>, settings: GameSettings) -> Profile {
        let mut p = Profile::new(file.clone());
        p.settings = settings;
        let Some(path) = file else { return p };
        let Some(data) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok()) else { return p };
        let Value::Object(data) = data else { return p };
        if let Some(c) = data.get("cores").and_then(Value::as_i64).filter(|c| *c >= 0 && data["cores"].is_i64()) {
            p.cores = c;
        }
        if let Some(Value::Object(levels)) = data.get("levels") {
            for (key, level) in levels {
                let key = RENAMED_NODES.iter().find(|(a, _)| a == key).map(|(_, b)| b.to_string()).unwrap_or_else(|| key.clone());
                if skill_tree::node(&key).is_some() && level.is_i64() && level.as_i64().unwrap() > 0 {
                    p.levels.push((key, level.as_i64().unwrap())); // more than the node has now: level() caps it
                }
            }
        }
        if let Some(Value::Array(l)) = data.get("loadout") {
            let keys: Vec<Option<String>> = l.iter().map(|v| v.as_str().map(String::from)).collect();
            let valid = l.len() == 2
                && l.iter().any(|v| !v.is_null() && v.as_str() != Some(""))
                && l.iter().all(|v| v.is_null() || v.as_str().is_some_and(weapons::is_weapon))
                && l[0] != l[1];
            if valid {
                p.loadout = [keys[0].clone(), keys[1].clone()];
            }
        }
        if let Some(d) = data.get("difficulty").and_then(Value::as_str).filter(|d| difficulty::exists(d)) {
            p.difficulty = d.to_string(); // a level whose file is gone: back to the first
        }
        if let Some(Value::Object(times)) = data.get("best_times") {
            for (key, seconds) in times {
                if difficulty::exists(key) && seconds.is_number() && seconds.as_f64().unwrap() > 0.0 {
                    p.best_times.push((key.clone(), crate::pyfmt::round_to(seconds.as_f64().unwrap(), 2)));
                }
            }
        }
        if !p.difficulty_unlocked(&p.difficulty.clone()) {
            p.difficulty = difficulty::default_key();
        }
        let rules = cards::rules();
        let weapon = p.loadout[0].clone().unwrap_or_else(|| STARTER.to_string()); // old weapon cards become this weapon's
        if let Some(Value::Array(saved)) = data.get("cards") {
            let valid: Vec<(String, bool)> = saved
                .iter()
                .filter_map(|c| c.as_object())
                .filter_map(|c| cards::saved_key(c.get("key").unwrap_or(&Value::Null), &weapon).map(|k| (k, c.get("active") == Some(&Value::Bool(true)))))
                .collect();
            let mut active = 0;
            for (key, wants) in valid.into_iter().take(rules.max_owned as usize) {
                // never more than allowed (a limit was lowered): the first count
                let is_active = wants && active < p.max_active_cards();
                active += is_active as i64;
                p.cards.push((key, is_active));
            }
        }
        if let Some(Value::Array(offers)) = data.get("card_offers") {
            p.card_offers = offers.iter().map(|k| cards::saved_key(k, &weapon)).collect();
        }
        p
    }

    /// Writes the file. False if that did not work (the game goes on, the progress just is not kept). What is in the
    /// file and not the game's stays as it is there.
    pub fn save(&self) -> bool {
        let Some(path) = &self.file else { return true };
        let mut data: Map<String, Value> = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        data.insert("cores".into(), json!(self.cores));
        data.insert("levels".into(), Value::Object(self.levels.iter().map(|(k, v)| (k.clone(), json!(v))).collect()));
        data.insert("loadout".into(), json!(self.loadout));
        data.insert("difficulty".into(), json!(self.difficulty));
        data.insert("best_times".into(), Value::Object(self.best_times.iter().map(|(k, v)| (k.clone(), json!(v))).collect()));
        data.insert("cards".into(), Value::Array(self.cards.iter().map(|(k, a)| json!({"key": k, "active": a})).collect()));
        data.insert("card_offers".into(), json!(self.card_offers));
        let text = serde_json::to_string_pretty(&Value::Object(data)).unwrap_or_default();
        path.parent().map(std::fs::create_dir_all);
        std::fs::write(path, text).is_ok()
    }

    // ---------- skill tree ----------

    /// The bought level of node `key` – at most its levels now.
    pub fn level(&self, key: &str) -> i64 {
        let bought = self.levels.iter().find(|(k, _)| k == key).map(|(_, l)| *l).unwrap_or(0);
        match skill_tree::node(key) {
            Some(n) => bought.min(n.max_level()),
            None => bought,
        }
    }
    pub fn owns(&self, key: &str) -> bool {
        self.level(key) > 0
    }
    /// True if every node it requires has been bought – maxed, if it says so.
    pub fn unlocked(&self, node: &Node) -> bool {
        self.missing(node).is_empty()
    }
    /// The keys of the nodes it requires that are not bought yet (or not maxed yet, for requires_max).
    pub fn missing(&self, node: &Node) -> Vec<&'static str> {
        node.requires
            .iter()
            .copied()
            .filter(|k| if node.requires_max { skill_tree::node(k).is_none_or(|n| !self.maxed(&n)) } else { !self.owns(k) })
            .collect()
    }
    /// Shown in the skill tree: bought, or every node it requires is bought (maxed or not).
    pub fn visible(&self, node: &Node) -> bool {
        self.owns(node.key) || node.requires.iter().all(|k| self.owns(k))
    }
    pub fn maxed(&self, node: &Node) -> bool {
        self.level(node.key) >= node.max_level()
    }
    /// Cores for the next level – `None` if it is already at its maximum.
    pub fn next_cost(&self, node: &Node) -> Option<i64> {
        if self.maxed(node) { None } else { node.costs.get(self.level(node.key) as usize).copied() }
    }
    pub fn can_buy(&self, node: &Node) -> bool {
        self.unlocked(node) && !self.maxed(node) && self.next_cost(node).is_some_and(|c| self.cores >= c)
    }
    /// Buys the next level and saves. False if that is not possible.
    pub fn buy(&mut self, node: &Node) -> bool {
        if !self.can_buy(node) {
            return false;
        }
        self.cores -= self.next_cost(node).unwrap();
        let level = self.level(node.key) + 1;
        match self.levels.iter_mut().find(|(k, _)| k == node.key) {
            Some(slot) => slot.1 = level,
            None => self.levels.push((node.key.to_string(), level)),
        }
        self.save();
        true
    }

    /// How many cards can be active at once: max_active plus the skill tree's Card slots.
    pub fn max_active_cards(&self) -> i64 {
        cards::rules().max_active + crate::engine::geom::py_round(skill_tree::per_level(self, skill_tree::CARD_SLOTS)) as i64
    }
    /// The keys of the active cards – never more than `max_active_cards`.
    pub fn active_cards(&self) -> Vec<String> {
        self.cards.iter().filter(|(_, a)| *a).map(|(k, _)| k.clone()).take(self.max_active_cards().max(0) as usize).collect()
    }

    // ---------- levels and runs ----------

    pub fn difficulty_unlocked(&self, key: &str) -> bool {
        match difficulty::get(key).and_then(|d| d.requires.clone()) {
            None => true,
            Some(req) => self.cleared(&req),
        }
    }
    pub fn best_time(&self, key: &str) -> f64 {
        self.best_times.iter().find(|(k, _)| k == key).map(|(_, s)| *s).unwrap_or(0.0)
    }
    /// True once a run on level `key` lasted its goal (never for an endless level).
    pub fn cleared(&self, key: &str) -> bool {
        difficulty::get(key).and_then(|d| d.goal).is_some_and(|goal| self.best_time(key) >= goal)
    }
    /// A run on the current level ended after `seconds`. True if that is a new best time (saved to 1/100 s).
    pub fn record(&mut self, seconds: f64) -> bool {
        let seconds = crate::pyfmt::round_to(seconds, 2);
        if seconds <= self.best_time(&self.difficulty) {
            return false;
        }
        let key = self.difficulty.clone();
        match self.best_times.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = seconds,
            None => self.best_times.push((key, seconds)),
        }
        true
    }
}

thread_local! {
    /// Where the game's profile is and the Toolbox page's settings – set once when the game starts (`main.rs`).
    static SOURCE: std::cell::RefCell<(Option<PathBuf>, GameSettings)> = std::cell::RefCell::new((None, GameSettings::default()));
}

/// Where `load_default` reads from, and the settings it hands on.
pub fn set_source(file: Option<PathBuf>, settings: GameSettings) {
    SOURCE.with(|s| *s.borrow_mut() = (file, settings));
}

impl Profile {
    /// The player's profile (the game menu opened from the main menu).
    pub fn load_default() -> Profile {
        let (file, settings) = SOURCE.with(|s| s.borrow().clone());
        Profile::load(file, settings)
    }
}
