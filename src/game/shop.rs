//! The shop during a run (T) and the inventory (Tab) – `shop.py`, `shop_panel.py`, `inventory.py`.
//!
//! The shop shows only what the ship has: Ship, each equipped weapon (its own upgrades, in its colour), Dash, Rocket
//! and Watchtower once unlocked. Every upgrade of a stat costs price_step more than the last.

use super::cards::{self, Card, category_color};
use super::settings::{self, HINT_ALPHA, TEXT, TEXT_MUTED};
use super::shapes::{STAR_COLOR, draw_arrow, draw_ship, draw_star};
use super::ship::Ship;
use crate::engine::geom::py_round;
use crate::engine::gfx::Radii;
use crate::engine::ui::{draw_panel, draw_text, draw_text_a, text_size};
use crate::engine::window::Event;
use crate::engine::{Anchor, Color, Rect, Surface, Vec2, rgb, vec2};

/// One fires at a time (LMB), RMB switches.
const WEAPON_KEYS: [&str; 2] = ["Slot 1", "Slot 2"];

pub struct Section {
    /// "ship", "weapon1", "weapon2", "dash", "rocket", "watchtower" – where the panel puts it.
    pub key: String,
    pub title: String,
    pub color: Color,
    pub cards: Vec<Card>,
}

pub struct Shop {
    /// Card key → how often it was bought in this run.
    pub bought: Vec<(String, i64)>,
    /// The loadout does not change during a run.
    pub sections: Vec<Section>,
}

impl Shop {
    pub fn new(ship: &Ship) -> Shop {
        let all = cards::all();
        let mut sections = vec![Section { key: "ship".into(), title: "Ship".into(), color: category_color("Ship"), cards: all.iter().filter(|c| c.kind.category == "Ship").cloned().collect() }];
        for (slot, w) in ship.weapons.iter().enumerate() {
            if let Some(w) = w {
                let own = all.iter().filter(|c| c.weapon == Some(w.key)).cloned().collect();
                sections.push(Section { key: format!("weapon{}", slot + 1), title: format!("{} ({})", w.name(), WEAPON_KEYS[slot]), color: w.color(), cards: own });
            }
        }
        for (category, key_name) in [("Dash", settings::ABILITY_KEY_NAMES[0]), ("Rocket", settings::ABILITY_KEY_NAMES[1]), ("Watchtower", settings::BUILD_KEY_NAME)] {
            let owned: Vec<Card> = all.iter().filter(|c| c.kind.category == category && c.has_target(ship)).cloned().collect();
            if !owned.is_empty() {
                sections.push(Section { key: category.to_lowercase(), title: format!("{category} ({key_name})"), color: category_color(category), cards: owned });
            }
        }
        Shop { bought: vec![], sections }
    }
    pub fn level(&self, card: &Card) -> i64 {
        self.bought.iter().find(|(k, _)| *k == card.key).map(|(_, n)| *n).unwrap_or(0)
    }
    pub fn price(&self, card: &Card) -> i64 {
        let c = cards::get(&card.key).unwrap_or_else(|| card.clone());
        py_round(c.price + c.price_step * self.level(card) as f64) as i64
    }
    pub fn maxed(&self, card: &Card, ship: &Ship) -> bool {
        card.has_target(ship) && !self.fresh(card).available(ship)
    }
    pub fn can_afford(&self, card: &Card, ship: &Ship) -> bool {
        ship.stars >= self.price(card)
    }
    /// The card with the numbers of the data files as they are now.
    fn fresh(&self, card: &Card) -> Card {
        cards::get(&card.key).unwrap_or_else(|| card.clone())
    }
    /// One upgrade of the card's stat. False if the ship does not have the part, it is maxed or too expensive.
    pub fn buy(&mut self, card: &Card, ship: &mut Ship) -> bool {
        let c = self.fresh(card);
        if !c.available(ship) || !self.can_afford(card, ship) {
            return false;
        }
        ship.stars -= self.price(card);
        c.apply(ship, false);
        let level = self.level(card) + 1;
        match self.bought.iter_mut().find(|(k, _)| *k == card.key) {
            Some(slot) => slot.1 = level,
            None => self.bought.push((card.key.clone(), level)),
        }
        true
    }
}

// ---------- the shop panel ----------

pub const SHOP_SIZE: (i32, i32) = (1000, 640);
/// Shop section keys per column.
const COLUMNS: [[&str; 2]; 3] = [["ship", "watchtower"], ["weapon1", "dash"], ["weapon2", "rocket"]];
const MARGIN: i32 = 24;
const COLUMN_GAP: i32 = 16;
const TOP: i32 = 70;
const HEADER_HEIGHT: i32 = 24;
const ROW_HEIGHT: i32 = 50;
const ROW_GAP: i32 = 4;
const ROW_FILL: Color = rgb(24, 24, 30);
const ROW_HOVER_FILL: Color = rgb(34, 34, 42);
const TOO_EXPENSIVE: Color = rgb(200, 70, 70);
/// Milliseconds a message ("Not enough stars") stays visible.
const MESSAGE_TIME: u64 = 1200;

pub struct ShopPanel {
    /// The key of the card under the mouse.
    hovered: Option<String>,
    message: String,
    message_until: u64,
}

impl ShopPanel {
    pub fn new() -> ShopPanel {
        ShopPanel { hovered: None, message: String::new(), message_until: 0 }
    }

    /// (section index, header position, [(card index, row rect)]) – where everything goes.
    fn layout(shop: &Shop) -> Vec<(usize, (i32, i32), Vec<(usize, Rect)>)> {
        let width = (SHOP_SIZE.0 - 2 * MARGIN - (COLUMNS.len() as i32 - 1) * COLUMN_GAP) / COLUMNS.len() as i32;
        let mut placed = vec![];
        for (column, keys) in COLUMNS.iter().enumerate() {
            let (x, mut y) = (MARGIN + column as i32 * (width + COLUMN_GAP), TOP);
            for key in keys {
                let Some(si) = shop.sections.iter().position(|s| s.key == *key) else { continue };
                let header = (x, y);
                y += HEADER_HEIGHT;
                let mut rows = vec![];
                for ci in 0..shop.sections[si].cards.len() {
                    rows.push((ci, Rect::new(x, y, width, ROW_HEIGHT)));
                    y += ROW_HEIGHT + ROW_GAP;
                }
                placed.push((si, header, rows));
                y += 10;
            }
        }
        placed
    }

    fn card_at(shop: &Shop, x: i32, y: i32) -> Option<Card> {
        for (si, _, rows) in ShopPanel::layout(shop) {
            for (ci, r) in rows {
                if r.collidepoint(x, y) {
                    return Some(shop.sections[si].cards[ci].clone());
                }
            }
        }
        None
    }

    pub fn handle_event(&mut self, event: &Event, shop: &mut Shop, ship: &mut Ship) {
        match *event {
            Event::MouseMotion { x, y } => self.hovered = ShopPanel::card_at(shop, x, y).map(|c| c.key),
            Event::MouseButtonUp { button: 1, x, y } => {
                if let Some(card) = ShopPanel::card_at(shop, x, y) {
                    if shop.maxed(&card, ship) {
                        self.show_message(format!("{} can't go any further", card.kind.name));
                    } else if !shop.buy(&card, ship) {
                        self.show_message("Not enough stars".into());
                    }
                }
            }
            _ => {}
        }
    }

    fn show_message(&mut self, text: String) {
        self.message = text;
        self.message_until = crate::engine::ticks() + MESSAGE_TIME;
    }

    pub fn draw(&self, surface: &mut Surface, shop: &Shop, ship: &Ship) {
        let panel = surface.get_rect();
        draw_panel(surface, panel, 0);
        draw_text(surface, "SHOP", 36, TEXT, Anchor::TopLeft, MARGIN, 20);
        draw_price(surface, ship.stars, (panel.right() - MARGIN, 34), 32, "right", TEXT);
        for (si, (x, y), rows) in ShopPanel::layout(shop) {
            let section = &shop.sections[si];
            draw_text(surface, &section.title.to_uppercase(), 20, section.color, Anchor::TopLeft, x, y + 4);
            for (ci, r) in rows {
                self.draw_row(surface, shop, ship, &section.cards[ci], r, section.color);
            }
        }
        let bottom = panel.bottom() - 14;
        if crate::engine::ticks() < self.message_until {
            draw_text(surface, &self.message, 24, TOO_EXPENSIVE, Anchor::MidBottom, panel.centerx(), bottom - 26);
        } else {
            draw_text_a(surface, "click a stat to upgrade it  ·  every upgrade of a stat costs more than the last", 20, TEXT, HINT_ALPHA * 2.0, Anchor::MidBottom, panel.centerx(), bottom - 26);
        }
        draw_text_a(surface, "T or ESC to close", 22, TEXT, HINT_ALPHA * 2.0, Anchor::BottomRight, panel.right() - 20, bottom);
    }

    /// One stat: `color` is its section's (a weapon's own colour for its upgrades).
    fn draw_row(&self, surface: &mut Surface, shop: &Shop, ship: &Ship, card: &Card, r: Rect, color: Color) {
        let hovered = self.hovered.as_deref() == Some(card.key.as_str());
        surface.rect_round(if hovered { ROW_HOVER_FILL } else { ROW_FILL }, r, 0, Radii::all(6));
        if hovered {
            surface.rect_round(color, r, 2, Radii::all(6));
        }
        surface.rect_round(color, Rect::new(r.left(), r.top(), 4, r.h), 0, Radii { tl: 6, bl: 6, ..Radii::default() });
        let name = draw_text(surface, card.kind.name, 22, TEXT, Anchor::TopLeft, r.left() + 14, r.top() + 7);
        let level = shop.level(card);
        if level != 0 {
            draw_text(surface, &format!("×{level}"), 18, TEXT_MUTED, Anchor::MidLeft, name.right() + 8, name.centery());
        }
        let fresh = cards::get(&card.key).unwrap_or_else(|| card.clone());
        let (now, after) = fresh.values(ship, false);
        if shop.maxed(card, ship) {
            draw_text(surface, &now, 19, TEXT_MUTED, Anchor::TopLeft, r.left() + 14, r.top() + 28);
            draw_text(surface, "MAX", 24, TEXT_MUTED, Anchor::MidRight, r.right() - 14, r.centery());
            return;
        }
        let left = draw_text(surface, &now, 19, TEXT_MUTED, Anchor::TopLeft, r.left() + 14, r.top() + 28);
        let cy = left.centery() as f64;
        draw_arrow(surface, vec2((left.right() + 6) as f64, cy), vec2((left.right() + 22) as f64, cy), TEXT_MUTED, 2);
        draw_text(surface, &after, 19, TEXT, Anchor::MidLeft, left.right() + 28, left.centery());
        let affordable = shop.can_afford(card, ship);
        draw_price(surface, shop.price(card), (r.right() - 14, r.centery()), 26, "right", if affordable { TEXT } else { TOO_EXPENSIVE });
    }
}

/// A star icon followed by a number. `align`: where `anchor` is – "left", "center" or "right" of the whole.
pub fn draw_price(surface: &mut Surface, amount: i64, anchor: (i32, i32), size: i32, align: &str, color: Color) {
    let s = size as f64;
    let width = s * 0.6 + 6.0 + text_size(&amount.to_string(), size).0 as f64;
    let (x, y) = (anchor.0 as f64, anchor.1 as f64);
    let left = match align {
        "left" => x,
        "center" => x - width / 2.0,
        _ => x - width,
    };
    draw_star(surface, vec2(left + s * 0.3, y), s * 0.32, STAR_COLOR);
    draw_text(surface, &amount.to_string(), size, color, Anchor::MidLeft, left + s * 0.6 + 6.0, y);
}

// ---------- the inventory ----------

pub const INVENTORY_SIZE: (i32, i32) = (680, 560);
/// Square on the left with the ship in it.
const PREVIEW_SIZE: i32 = 190;
const SLOT_GAP: i32 = 14;
const SLOT_HEIGHT: i32 = 200;
const ABILITY_SLOT_HEIGHT: i32 = 170;
const SLOT_BORDER: Color = rgb(70, 70, 80);
const LINE: Color = rgb(50, 50, 58);

pub fn draw_inventory(surface: &mut Surface, ship: &Ship) {
    let panel = surface.get_rect();
    draw_panel(surface, panel, 0);
    draw_text(surface, "INVENTORY", 36, TEXT, Anchor::TopLeft, panel.left() + 24, panel.top() + 20);
    // left: the ship and its own stats
    let preview = Rect::new(panel.left() + 24, panel.top() + 64, PREVIEW_SIZE, PREVIEW_SIZE);
    surface.rect_round(settings::BACKGROUND, preview, 0, Radii::all(6));
    surface.rect_round(TEXT_MUTED, preview, 1, Radii::all(6));
    draw_ship(surface, preview.centerv(), Vec2::new(1.0, -1.0).normalize(), PREVIEW_SIZE as f64 * 0.55, ship.color);
    use crate::pyfmt::g;
    let rows = vec![
        ("Move speed".to_string(), format!("{} px/s", g(ship.speed))),
        ("Energy".to_string(), format!("{:.0}/{}", ship.energy, g(ship.max_energy))),
        ("Energy regen".to_string(), format!("{}/s", g(ship.energy_regen))),
        ("Armor".to_string(), format!("{} px", g(ship.armor))),
        ("Recovery".to_string(), format!("{} s", g(ship.invulnerable_time))),
        ("Magnet".to_string(), format!("{} px", g(ship.magnet_radius))),
    ];
    draw_rows(surface, &rows, preview.left(), preview.right(), preview.bottom() + 16, 24, 30);
    // right: 2 weapon slots, 2 ability slots
    let left = preview.right() + 28;
    let width = (panel.right() - 24 - left - SLOT_GAP) / 2;
    let mut y = preview.top();
    draw_text(surface, "WEAPONS", 22, TEXT_MUTED, Anchor::TopLeft, left, y);
    for (i, w) in ship.weapons.iter().enumerate() {
        let b = Rect::new(left + i as i32 * (width + SLOT_GAP), y + 22, width, SLOT_HEIGHT);
        draw_slot(surface, b, w.as_ref().map(|w| (w.name().to_string(), w.stat_lines())), WEAPON_KEYS[i]);
    }
    y += 22 + SLOT_HEIGHT + 16;
    draw_text(surface, "ABILITIES", 22, TEXT_MUTED, Anchor::TopLeft, left, y);
    for (i, a) in ship.abilities.iter().enumerate() {
        let b = Rect::new(left + i as i32 * (width + SLOT_GAP), y + 22, width, ABILITY_SLOT_HEIGHT);
        draw_slot(surface, b, a.as_ref().map(|a| (a.name().to_string(), a.stat_lines())), settings::ABILITY_KEY_NAMES[i]);
    }
    draw_text_a(surface, "TAB or ESC to close", 22, TEXT, HINT_ALPHA * 2.0, Anchor::BottomRight, panel.right() - 20, panel.bottom() - 14);
}

/// One slot: a frame, the name of what is in it and its values – or "empty".
pub fn draw_slot(surface: &mut Surface, b: Rect, item: Option<(String, Vec<(String, String)>)>, key: &str) {
    surface.rect_round(SLOT_BORDER, b, 1, Radii::all(6));
    if !key.is_empty() {
        draw_text_a(surface, key, 18, TEXT, HINT_ALPHA * 2.0, Anchor::TopRight, b.right() - 10, b.top() + 10);
    }
    let Some((name, lines)) = item else {
        let c = b.center();
        draw_text_a(surface, "empty", 24, TEXT, HINT_ALPHA, Anchor::Center, c.0, c.1);
        return;
    };
    draw_text(surface, &name, 26, TEXT, Anchor::TopLeft, b.left() + 12, b.top() + 10);
    draw_rows(surface, &lines, b.left() + 12, b.right() - 12, b.top() + 40, 20, 25);
}

/// (name, value) rows: name faded on the left, value on the right, a thin line below each.
pub fn draw_rows(surface: &mut Surface, rows: &[(String, String)], left: i32, right: i32, mut top: i32, size: i32, step: i32) {
    for (name, value) in rows {
        draw_text(surface, name, size, TEXT_MUTED, Anchor::TopLeft, left, top);
        draw_text(surface, value, size, TEXT, Anchor::TopRight, right, top);
        surface.line(LINE, vec2(left as f64, (top + size - 2) as f64), vec2(right as f64, (top + size - 2) as f64), 1);
        top += step;
    }
}
