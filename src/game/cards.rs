//! Cards (`game/cards/`): every card raises one stat of the ship, its weapons or its abilities by a fixed amount.
//! They are used two ways:
//!   - in the shop during a run (T): every card is a stat you can upgrade again and again, for stars – each time it
//!     costs price_step more, and adds `amount` (`shop.rs`); those numbers are in `shop.json` (`shop_data.rs`),
//!   - in the game menu: cards bought with cores go into the collection; the active ones apply at the start of every
//!     run, each adding card_amount (`upgrades.json`, together with the card shop's rules).
//!
//! Weapon cards belong to one weapon each ("damage:cannon") – in the shop during a run and as a card (there it does
//! nothing while its weapon is not in a slot). A card from the card shop works on a weapon times the weapon's
//! upgrade_factor for its type.

use std::cell::RefCell;

use serde_json::Value;

use super::abilities::Ability;
use super::settings;
use super::ship::Ship;
use super::tables::{self, abilities as ability_data, f, is_int, is_number};
use super::weapons::{self, WEAPON_TYPES};
use crate::engine::{Color, rgb};
use crate::pyfmt::{dumps, g_prec, num, plus_g, round_to};

/// Which part of the ship a card changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Ship,
    Weapon,
    Dash,
    Rocket,
    Watchtower,
}

/// A card type – what Python's card classes held.
#[derive(Debug)]
pub struct CardType {
    /// Its name in the data files and the save file – do not rename.
    pub key: &'static str,
    pub name: &'static str,
    /// "Ship", "Weapons", "Dash", "Rocket", "Watchtower" (also its colour).
    pub category: &'static str,
    /// The attribute that changes.
    pub stat: &'static str,
    pub amount: f64,
    pub card_amount: f64,
    pub unit: &'static str,
    /// The stat never goes above this.
    pub maximum: Option<f64>,
    /// Its entry in `shop.json`, if that is not its key.
    pub shop_name: Option<&'static str>,
    pub part: Part,
    pub price: f64,
    pub price_step: f64,
}

const fn card(key: &'static str, name: &'static str, category: &'static str, stat: &'static str, amount: f64, card_amount: f64, unit: &'static str, part: Part) -> CardType {
    CardType { key, name, category, stat, amount, card_amount, unit, maximum: None, shop_name: None, part, price: 10.0, price_step: 0.0 }
}

/// In this order – where the weapon card types stand, all of them for each weapon in turn.
pub static ALL_CARDS: &[CardType] = &[
    card("move_speed", "Move speed", "Ship", "speed", 20.0, 60.0, " px/s", Part::Ship),
    card("max_energy", "Max energy", "Ship", "max_energy", 10.0, 30.0, "", Part::Ship),
    card("energy_regen", "Energy regen", "Ship", "energy_regen", 2.0, 6.0, "/s", Part::Ship),
    card("armor", "Armor", "Ship", "armor", 2.0, 6.0, " px", Part::Ship),
    CardType { maximum: Some(settings::MAX_INVULNERABLE_TIME), ..card("recovery", "Recovery", "Ship", "invulnerable_time", 0.1, 0.3, " s", Part::Ship) },
    card("magnet", "Magnet", "Ship", "magnet_radius", 25.0, 70.0, " px", Part::Ship),
    card("damage", "Damage", "Weapons", "damage", 0.5, 2.0, "", Part::Weapon),
    card("attack_speed", "Attack speed", "Weapons", "fire_rate", 0.5, 1.5, "/s", Part::Weapon),
    card("wall_push", "Wall push", "Weapons", "wall_push", 3.0, 10.0, " px", Part::Weapon),
    card("projectile_speed", "Projectile speed", "Weapons", "projectile_speed", 100.0, 150.0, " px/s", Part::Weapon),
    card("dash_distance", "Dash distance", "Dash", "distance", 25.0, 80.0, " px", Part::Dash),
    card("dash_cooldown", "Dash cooldown", "Dash", "cooldown_time", -0.1, -0.3, " s", Part::Dash),
    card("rocket_damage", "Rocket damage", "Rocket", "damage", 2.0, 6.0, "", Part::Rocket),
    CardType { shop_name: Some("rocket_blast_radius"), ..card("blast_radius", "Blast radius", "Rocket", "blast_radius", 15.0, 50.0, " px", Part::Rocket) },
    card("rocket_wall_push", "Rocket wall push", "Rocket", "wall_push", 20.0, 60.0, " px", Part::Rocket),
    card("rocket_cooldown", "Rocket cooldown", "Rocket", "cooldown_time", -0.3, -1.0, " s", Part::Rocket),
    CardType { shop_name: Some("watchtower_vision"), ..card("vision", "Vision", "Watchtower", "vision", 40.0, 80.0, " px", Part::Watchtower) },
];

pub fn card_type(key: &str) -> Option<&'static CardType> {
    ALL_CARDS.iter().find(|c| c.key == key)
}

/// Weapon key → its upgrades' prefix in `shop.json` (as in the skill tree).
fn shop_prefix(weapon: &str) -> &str {
    if weapon == "machine_gun" { "mg" } else { weapon }
}

pub fn category_color(category: &str) -> Color {
    match category {
        "Ship" => rgb(40, 110, 255),
        "Weapons" => rgb(235, 235, 235),
        "Dash" => rgb(80, 200, 255),
        "Rocket" => rgb(255, 70, 50),
        "Watchtower" => rgb(120, 180, 255),
        _ => rgb(255, 255, 255),
    }
}

/// One card: its type, for a weapon card its weapon, and its numbers in the shop during a run.
#[derive(Debug, Clone)]
pub struct Card {
    pub kind: &'static CardType,
    pub weapon: Option<&'static str>,
    /// "damage:cannon" for a weapon card, else the type's key.
    pub key: String,
    pub amount: f64,
    pub price: f64,
    pub price_step: f64,
}

/// The card shop's rules (`upgrades.json` card_shop).
#[derive(Debug, Clone, PartialEq)]
pub struct Rules {
    /// Cards offered at once.
    pub offers: i64,
    /// Cores for a card while the collection is empty …
    pub first_price: i64,
    /// … and this many more for each card already in it.
    pub price_step: i64,
    /// Cores for new offers.
    pub reroll_price: i64,
    /// Cores back for selling a card.
    pub sell_refund: i64,
    pub max_owned: i64,
    /// Active at once – the skill tree's Card slots add more.
    pub max_active: i64,
}
impl Default for Rules {
    fn default() -> Self {
        Rules { offers: 3, first_price: 2, price_step: 0, reroll_price: 1, sell_refund: 1, max_owned: 12, max_active: 1 }
    }
}

thread_local! {
    static CARDS: RefCell<Vec<Card>> = RefCell::new(make_cards());
    /// card type key → card_amount (from `upgrades.json`).
    static CARD_AMOUNTS: RefCell<Vec<(&'static str, f64)>> = RefCell::new(ALL_CARDS.iter().map(|c| (c.key, c.card_amount)).collect());
    static RULES: RefCell<Rules> = RefCell::new(Rules::default());
}

fn make_cards() -> Vec<Card> {
    let mut made = vec![];
    let first_weapon_kind = ALL_CARDS.iter().position(|c| c.part == Part::Weapon).unwrap();
    for (i, t) in ALL_CARDS.iter().enumerate() {
        if i == first_weapon_kind {
            for (weapon, ..) in WEAPON_TYPES {
                for kind in ALL_CARDS.iter().filter(|c| c.part == Part::Weapon) {
                    made.push(Card { kind, weapon: Some(weapon), key: format!("{}:{weapon}", kind.key), amount: kind.amount, price: kind.price, price_step: kind.price_step });
                }
            }
        } else if t.part != Part::Weapon {
            made.push(Card { kind: t, weapon: None, key: t.key.to_string(), amount: t.amount, price: t.price, price_step: t.price_step });
        }
    }
    made
}

/// Every card, in order.
pub fn all() -> Vec<Card> {
    CARDS.with(|c| c.borrow().clone())
}
pub fn get(key: &str) -> Option<Card> {
    CARDS.with(|c| c.borrow().iter().find(|x| x.key == key).cloned())
}
pub fn exists(key: &str) -> bool {
    get(key).is_some()
}
pub fn rules() -> Rules {
    RULES.with(|r| r.borrow().clone())
}
pub fn card_amount(kind: &str) -> f64 {
    CARD_AMOUNTS.with(|c| c.borrow().iter().find(|(k, _)| *k == kind).map(|(_, v)| *v).unwrap_or(0.0))
}
/// Sets a card's shop numbers (`shop_data.rs`).
pub fn set_shop_numbers(key: &str, amount: f64, price: f64, price_step: f64) {
    CARDS.with(|c| {
        if let Some(card) = c.borrow_mut().iter_mut().find(|x| x.key == key) {
            (card.amount, card.price, card.price_step) = (amount, price, price_step);
        }
    });
}

/// Old card type key (in save files) → its key now.
const RENAMED: &[(&str, &str)] = &[("shot_speed", "projectile_speed")];

/// A card key from a save file as it is now: weapon cards used to work on every weapon ("damage") – those become
/// the card of `weapon` (the one in the first slot); renamed card types get their new key. `None` if there is no
/// such card any more.
pub fn saved_key(key: &Value, weapon: &str) -> Option<String> {
    let key = key.as_str()?;
    let (kind, rest) = match key.split_once(':') {
        Some((k, r)) => (k, Some(r)),
        None => (key, None),
    };
    let kind = RENAMED.iter().find(|(a, _)| *a == kind).map(|(_, b)| *b).unwrap_or(kind);
    let key = match rest {
        Some(r) => format!("{kind}:{r}"),
        None => kind.to_string(),
    };
    if exists(&key) {
        return Some(key);
    }
    if card_type(&key).is_some_and(|t| t.part == Part::Weapon) {
        return Some(format!("{key}:{weapon}"));
    }
    None
}

pub fn valid_number(name: &str, value: &Value) -> bool {
    is_number(value) && (name == "amount" || name == "card_amount" || f(value) >= 0.0) // amounts can go down (cooldowns)
}

/// Reads `upgrades.json` again: the cards' card_amount and the card shop's rules.
pub fn reload(warnings: &mut Vec<String>) {
    let data = tables::load("upgrades.json", warnings);
    let mut amounts: Vec<(&'static str, f64)> = ALL_CARDS.iter().map(|c| (c.key, c.card_amount)).collect();
    if let Some(upgrades) = data.get("upgrades") {
        if let Value::Object(upgrades) = upgrades {
            for (key, entry) in upgrades {
                let Some(t) = card_type(key) else {
                    warnings.push(format!("upgrades.json upgrades: unknown card \"{key}\" – ignored"));
                    continue;
                };
                let Value::Object(entry) = entry else { continue };
                for (name, value) in entry {
                    let slot = amounts.iter_mut().find(|(k, _)| *k == t.key).unwrap();
                    if ["amount", "price", "price_step"].contains(&name.as_str()) {
                        warnings.push(format!("upgrades.json {key}.{name}: belongs in shop.json now – ignored"));
                    } else if name != "card_amount" {
                        warnings.push(format!("upgrades.json {key}: unknown value \"{name}\" – ignored"));
                    } else if !valid_number(name, value) {
                        warnings.push(format!("upgrades.json {key}.{name}: {} is not valid – using {}", dumps(value), num(&Value::from(slot.1))));
                    } else {
                        slot.1 = f(value);
                    }
                }
            }
        }
    }
    CARD_AMOUNTS.with(|c| *c.borrow_mut() = amounts);

    let mut rules = Rules::default();
    if let Some(Value::Object(section)) = data.get("card_shop") {
        for (key, value) in section {
            let slot = match key.as_str() {
                "offers" => &mut rules.offers,
                "first_price" => &mut rules.first_price,
                "price_step" => &mut rules.price_step,
                "reroll_price" => &mut rules.reroll_price,
                "sell_refund" => &mut rules.sell_refund,
                "max_owned" => &mut rules.max_owned,
                "max_active" => &mut rules.max_active,
                _ => {
                    warnings.push(format!("upgrades.json card_shop: unknown value \"{key}\" – ignored"));
                    continue;
                }
            };
            if !is_int(value) || value.as_i64().unwrap_or(-1) < 0 || (key == "offers" && value.as_i64().unwrap_or(0) < 1) {
                warnings.push(format!("upgrades.json card_shop.{key}: {} is not valid (a whole number)", dumps(value)));
            } else {
                *slot = value.as_i64().unwrap();
            }
        }
    }
    RULES.with(|r| *r.borrow_mut() = rules);
}

// ---------- what a card does ----------

impl Card {
    pub fn color(&self) -> Color {
        match self.weapon {
            Some(w) => weapons::color_of(w),
            None => category_color(self.kind.category),
        }
    }
    /// Its name in `shop.json`.
    pub fn shop_key(&self) -> String {
        match self.weapon {
            Some(w) => format!("{}_{}", shop_prefix(w), self.kind.key),
            None => self.kind.shop_name.unwrap_or(self.kind.key).to_string(),
        }
    }
    /// The band on top of the card: what it belongs to.
    pub fn label(&self) -> &'static str {
        match self.weapon {
            Some(w) => weapons::name_of(w),
            None => self.kind.category,
        }
    }
    /// Its name where it stands alone, e.g. "Cannon: Damage".
    pub fn full_name(&self) -> String {
        match self.weapon {
            Some(w) => format!("{}: {}", weapons::name_of(w), self.kind.name),
            None => self.kind.name.to_string(),
        }
    }
    /// The stat never goes below this (cooldowns).
    pub fn minimum(&self) -> Option<f64> {
        match self.kind.key {
            "dash_cooldown" => Some(ability_data::get("dash", "min_cooldown")),
            "rocket_cooldown" => Some(ability_data::get("rocket", "min_cooldown")),
            _ => None,
        }
    }

    /// The values of the first target, or `None` if the ship does not have that part.
    fn first_value(&self, ship: &Ship) -> Option<(f64, f64)> {
        let stat = self.kind.stat;
        match self.kind.part {
            Part::Ship => Some((ship.get(stat), 1.0)),
            Part::Weapon => ship.weapons.iter().flatten().find(|w| Some(w.key) == self.weapon).map(|w| (w.get(stat), w.factor(self.kind.key))),
            Part::Dash => ship.abilities.iter().flatten().find(|a| a.is_dash()).map(|a| (a.get(stat), 1.0)),
            Part::Rocket => ship.abilities.iter().flatten().find(|a| a.is_rocket()).map(|a| (a.get(stat), 1.0)),
            Part::Watchtower => ship.watchtower.as_ref().map(|k| (k.vision, 1.0)),
        }
    }
    /// True if the ship has the part this card changes.
    pub fn has_target(&self, ship: &Ship) -> bool {
        self.first_value(ship).is_some()
    }

    /// True if buying it would change something: the ship has the part, and the stat is not at its minimum (or
    /// maximum).
    pub fn available(&self, ship: &Ship) -> bool {
        let Some((value, _)) = self.first_value(ship) else { return false };
        if let (Some(min), true) = (self.minimum(), self.amount < 0.0) {
            return value > min;
        }
        if let (Some(max), true) = (self.kind.maximum, self.amount > 0.0) {
            return value < max;
        }
        true
    }

    /// The big text on the card (what it really adds), e.g. "+2" or "-0.5 s".
    pub fn effect(&self) -> String {
        let amount = card_amount(self.kind.key);
        match self.weapon {
            Some(w) => format!("{}{}", plus_g(round_to(amount * tables::weapons::factor(w, self.kind.key), 6)), self.kind.unit),
            None => format!("{}{}", plus_g(amount), self.kind.unit),
        }
    }

    /// (now, after buying) as text, e.g. ("6/s", "7/s") – read from the first target. `card`: a card from the card
    /// shop (its card_amount × factor) instead of a shop upgrade.
    pub fn values(&self, ship: &Ship, card: bool) -> (String, String) {
        let (now, factor) = self.first_value(ship).unwrap_or((0.0, 1.0));
        let (amount, factor) = if card { (card_amount(self.kind.key), factor) } else { (self.amount, 1.0) };
        let after = self.changed(now, amount, factor);
        (format!("{}{}", g_prec(now, 4), self.kind.unit), format!("{}{}", g_prec(after, 4), self.kind.unit))
    }

    /// `value` with amount × factor added, never below the minimum or above the maximum.
    fn changed(&self, value: f64, amount: f64, factor: f64) -> f64 {
        let mut v = round_to(value + amount * factor, 6); // 1.2 - 0.15 would be 1.0499999…
        if let Some(min) = self.minimum() {
            v = v.max(min);
        }
        if let Some(max) = self.kind.maximum {
            v = v.min(max);
        }
        v
    }

    /// Adds to the stat on every target: a shop upgrade's amount (`card` false), or the card shop's card_amount.
    pub fn apply(&self, ship: &mut Ship, card: bool) {
        let stat = self.kind.stat;
        let amount = if card { card_amount(self.kind.key) } else { self.amount };
        match self.kind.part {
            Part::Ship => {
                let before = ship.max_energy;
                let v = self.changed(ship.get(stat), amount, 1.0);
                ship.set(stat, v);
                if stat == "max_energy" {
                    ship.energy += ship.max_energy - before; // the new part comes filled up
                }
            }
            Part::Weapon => {
                for w in ship.weapons.iter_mut().flatten().filter(|w| Some(w.key) == self.weapon) {
                    let factor = if card { w.factor(self.kind.key) } else { 1.0 };
                    let v = self.changed(w.get(stat), amount, factor);
                    w.set(stat, v);
                }
            }
            Part::Dash | Part::Rocket => {
                let dash = self.kind.part == Part::Dash;
                for a in ship.abilities.iter_mut().flatten().filter(|a: &&mut Ability| a.is_dash() == dash) {
                    let v = self.changed(a.get(stat), amount, 1.0);
                    a.set(stat, v);
                }
            }
            Part::Watchtower => {
                if let Some(k) = &mut ship.watchtower {
                    k.vision = self.changed(k.vision, amount, 1.0);
                }
            }
        }
    }
}

