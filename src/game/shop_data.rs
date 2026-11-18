//! What you buy with stars during a run: `shop.json` (`shop_data.py`) – the upgrades in the shop (T: what one adds
//! and what it costs), one entry per row of the shop ("gun_damage", "mg_damage", …), and the watchtowers (B). What is
//! missing or invalid keeps the value in the code, and a warning goes to the game menu.

use std::cell::Cell;

use serde_json::Value;

use super::cards::{self, Part, card_type};
use super::tables::{self, f, is_number};
use crate::pyfmt::{dumps, round_to};

thread_local! {
    /// Stars for the first watchtower of a run, and more for each further.
    static TOWER: Cell<(f64, f64)> = const { Cell::new((15.0, 0.0)) };
}

/// (price, price_step) of the watchtowers.
pub fn tower() -> (f64, f64) {
    TOWER.with(|t| t.get())
}

pub fn reload(warnings: &mut Vec<String>) {
    let data = tables::load("shop.json", warnings);
    for key in data.keys() {
        if key != "upgrades" && key != "watchtower" {
            warnings.push(format!("shop.json: unknown key \"{key}\" – ignored"));
        }
    }
    read_upgrades(data.get("upgrades").unwrap_or(&Value::Object(Default::default())), warnings);

    let mut tower = (15.0, 0.0);
    if let Some(Value::Object(t)) = data.get("watchtower") {
        for (name, value) in t {
            let slot = match name.as_str() {
                "price" => &mut tower.0,
                "price_step" => &mut tower.1,
                _ => {
                    warnings.push(format!("shop.json watchtower: unknown value \"{name}\" – ignored"));
                    continue;
                }
            };
            if !is_number(value) || f(value) < 0.0 {
                warnings.push(format!("shop.json watchtower.{name}: {} is not valid – using {}", dumps(value), crate::pyfmt::g(*slot)));
            } else {
                *slot = f(value);
            }
        }
    }
    TOWER.with(|t| t.set(tower));
}

/// Sets amount, price and price_step on every card from its entry in `upgrades` (by its shop key) – without an
/// entry: its type's numbers, for a weapon card the amount times the weapon's upgrade_factor.
fn read_upgrades(upgrades: &Value, warnings: &mut Vec<String>) {
    let all = cards::all();
    for c in &all {
        let factor = c.weapon.map(|w| tables::weapons::factor(w, c.kind.key)).unwrap_or(1.0);
        cards::set_shop_numbers(&c.key, round_to(c.kind.amount * factor, 6), c.kind.price, c.kind.price_step);
    }
    let Value::Object(upgrades) = upgrades else { return };
    for (key, entry) in upgrades {
        let Some(c) = all.iter().find(|c| c.shop_key() == *key) else {
            // a type's key that has a shop key of its own now
            if let Some(t) = card_type(key).filter(|t| all.iter().any(|c| c.kind.key == t.key && c.shop_key() != t.key)) {
                let hint = if t.part == Part::Weapon {
                    let own: Vec<String> = all.iter().filter(|c| c.kind.key == t.key).map(|c| c.shop_key()).collect();
                    format!("every weapon has its own now ({})", own.join(", "))
                } else {
                    format!("it's \"{}\" now", t.shop_name.unwrap_or(t.key))
                };
                warnings.push(format!("shop.json upgrades: \"{key}\" – {hint} – ignored"));
            } else {
                warnings.push(format!("shop.json upgrades: unknown upgrade \"{key}\" – ignored"));
            }
            continue;
        };
        let Value::Object(entry) = entry else { continue };
        let current = cards::get(&c.key).unwrap();
        let (mut amount, mut price, mut step) = (current.amount, current.price, current.price_step);
        for (name, value) in entry {
            let slot = match name.as_str() {
                "amount" => &mut amount,
                "price" => &mut price,
                "price_step" => &mut step,
                _ => {
                    warnings.push(format!("shop.json {key}: unknown value \"{name}\" – ignored"));
                    continue;
                }
            };
            if !cards::valid_number(name, value) {
                warnings.push(format!("shop.json {key}.{name}: {} is not valid – using {}", dumps(value), crate::pyfmt::g(*slot)));
            } else {
                *slot = f(value);
            }
        }
        cards::set_shop_numbers(&c.key, amount, price, step);
    }
}
