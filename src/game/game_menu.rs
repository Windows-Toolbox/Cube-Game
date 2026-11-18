//! The game menu between runs (`game_menu_scene.py`): the ship as the next run starts it, the chosen level, and the
//! tabs Levels, Loadout, Skill tree and Cards (the cards only once the card shop is bought). Every data file is read
//! again when it opens; problems in them are shown in red.

use std::any::Any;

use super::difficulty;
use super::hud::clock as format_time;
use super::panels::{CardPanel, CardShop, CardThing, LevelPanel, LoadoutPanel, LoadoutThing, Message, SUCCESS, SkillTreePanel};
use super::profile::Profile;
use super::settings::{self, BACKGROUND, HINT_ALPHA, TEXT, TEXT_MUTED};
use super::shapes::{CORE_COLOR, draw_core, draw_ship};
use super::skill_tree;
use crate::engine::desktop::work_area;
use crate::engine::gfx::Radii;
use crate::engine::menu::{ContextMenu, Used};
use crate::engine::ui::{draw_panel, draw_text, draw_text_a};
use crate::engine::window::{Event, K_A, K_D, K_ESCAPE, K_KP_ENTER, K_LEFT, K_RETURN, K_RIGHT, K_SPACE, K_TAB};
use crate::engine::{Anchor, Color, Engine, Rect, Scene, Surface, Vec2, rgb, vec2};
use crate::pyfmt::{g, g_prec};

const MARGIN: i32 = 24;
/// Ship column.
const LEFT_WIDTH: i32 = 260;
/// Level and tabs.
const RIGHT_LEFT: i32 = 320;
const PREVIEW_HEIGHT: i32 = 170;
/// The chosen level, above the tabs.
const LEVEL_TOP: i32 = 94;
const TABS: [(&str, &str); 4] = [("levels", "Levels"), ("loadout", "Loadout"), ("tree", "Skill tree"), ("cards", "Cards")];
const TAB_TOP: i32 = 168;
const TAB_SIZE: (i32, i32) = (150, 32);
const TAB_GAP: i32 = 10;
/// The tabs' content starts here …
const TREE_TOP: i32 = 214;
/// … and ends here.
const TREE_BOTTOM: i32 = 770;
/// Messages, hints and warnings: below the tabs' content.
const INFO_TOP: i32 = TREE_BOTTOM;
const WARNING_LINES: usize = 2;
const WARNING_LENGTH: usize = 80;
const BACK_SIZE: (i32, i32) = (140, 44);
const START_SIZE: (i32, i32) = (240, 52);
/// Milliseconds a message ("Not enough cores") stays visible.
const MESSAGE_TIME: u64 = 1500;
const HOVER_FILL: Color = rgb(34, 34, 42);
const LOCKED: Color = rgb(70, 70, 80);
const TOO_EXPENSIVE: Color = rgb(200, 70, 70);
const SELECTED_FILL: Color = rgb(255, 255, 255);
const LINE: Color = rgb(50, 50, 58);

/// What the mouse is on.
#[derive(Debug, Clone, PartialEq)]
enum Thing {
    Difficulty(String),
    Tab(&'static str),
    Node(&'static str),
    Cards(CardThing),
    Loadout(LoadoutThing),
    Start,
    Back,
}

pub struct GameMenuScene {
    profile: Profile,
    rect: Rect,
    tab: &'static str,
    card_panel: CardPanel,
    tree_panel: SkillTreePanel,
    loadout_panel: LoadoutPanel,
    level_panel: LevelPanel,
    hovered: Option<Thing>,
    /// The open context menu (right click on a card): its card's index.
    menu: Option<ContextMenu<usize>>,
    message: String,
    message_color: Color,
    message_until: u64,
}

impl GameMenuScene {
    pub fn new(profile: Option<Profile>) -> GameMenuScene {
        difficulty::reload();
        let mut profile = profile.unwrap_or_else(Profile::load_default);
        if !difficulty::exists(&profile.difficulty) || !profile.difficulty_unlocked(&profile.difficulty.clone()) {
            profile.difficulty = difficulty::default_key(); // its file was removed or changed
        }
        let mut rect = Rect::new(0, 0, settings::GAME_MENU_WINDOW_SIZE.0, settings::GAME_MENU_WINDOW_SIZE.1);
        rect.set_centerv(work_area().centerv());
        let area = Rect::new(RIGHT_LEFT, TREE_TOP, rect.w - MARGIN - RIGHT_LEFT, TREE_BOTTOM - TREE_TOP);
        let shop = CardShop::new(&mut profile);
        GameMenuScene {
            card_panel: CardPanel::new(shop, RIGHT_LEFT, TREE_TOP, rect.w - MARGIN - RIGHT_LEFT),
            tree_panel: SkillTreePanel::new(area),
            loadout_panel: LoadoutPanel::new(area),
            level_panel: LevelPanel::new(area),
            profile,
            rect,
            tab: "levels",
            hovered: None,
            menu: None,
            message: String::new(),
            message_color: TEXT,
            message_until: 0,
        }
    }

    /// The tabs to show – the cards only once the card shop is bought.
    fn tabs(&self) -> Vec<(&'static str, &'static str)> {
        TABS.iter().copied().filter(|(k, _)| *k != "cards" || self.profile.owns(skill_tree::CARD_SHOP)).collect()
    }
    fn tab_rects(&self) -> Vec<(&'static str, Rect)> {
        self.tabs().iter().enumerate().map(|(i, (k, _))| (*k, Rect::new(RIGHT_LEFT + i as i32 * (TAB_SIZE.0 + TAB_GAP), TAB_TOP, TAB_SIZE.0, TAB_SIZE.1))).collect()
    }
    fn back_rect(&self) -> Rect {
        Rect::new(MARGIN, self.rect.h - MARGIN - BACK_SIZE.1, BACK_SIZE.0, BACK_SIZE.1)
    }
    fn start_rect(&self) -> Rect {
        let mut r = Rect::new(0, 0, START_SIZE.0, START_SIZE.1);
        r.set_bottomright(self.rect.w - MARGIN, self.rect.h - MARGIN + 4);
        r
    }

    fn thing_at(&self, x: i32, y: i32) -> Option<Thing> {
        if let Some((k, _)) = self.tab_rects().into_iter().find(|(_, r)| r.collidepoint(x, y)) {
            return Some(Thing::Tab(k));
        }
        let found = match self.tab {
            "levels" => self.level_panel.thing_at(x, y).map(Thing::Difficulty),
            "tree" => self.tree_panel.thing_at(&self.profile, x, y).map(Thing::Node),
            "loadout" => self.loadout_panel.thing_at(x, y).map(Thing::Loadout),
            "cards" => self.card_panel.thing_at(&self.profile, x, y).map(Thing::Cards),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
        if self.start_rect().collidepoint(x, y) {
            return Some(Thing::Start);
        }
        if self.back_rect().collidepoint(x, y) {
            return Some(Thing::Back);
        }
        None
    }

    fn start(&mut self, e: &mut Engine) {
        e.change_scene(Box::new(super::play_scene::PlayScene::new(self.profile.clone())));
    }
    fn back(&mut self, e: &mut Engine) {
        e.change_scene(Box::new(super::scenes::MainMenuScene::new()));
    }

    fn choose_difficulty(&mut self, key: &str) {
        if !self.profile.difficulty_unlocked(key) {
            let hint = unlock_hint(key);
            self.show_message(hint, TOO_EXPENSIVE);
        } else if key != self.profile.difficulty {
            self.profile.difficulty = key.to_string();
            self.profile.save();
        }
    }
    fn step_difficulty(&mut self, step: i64) {
        let keys: Vec<String> = difficulty::all().iter().map(|d| d.key.clone()).filter(|k| self.profile.difficulty_unlocked(k)).collect();
        let i = keys.iter().position(|k| *k == self.profile.difficulty).unwrap_or(0) as i64;
        let n = keys.len() as i64;
        let next = keys[((i + step) % n + n) as usize % n as usize].clone();
        self.choose_difficulty(&next);
    }

    fn buy(&mut self, key: &str) {
        let Some(node) = skill_tree::node(key) else { return };
        let p = &mut self.profile;
        let (text, color) = if p.maxed(&node) {
            ("Already at its maximum".to_string(), TEXT_MUTED)
        } else if !p.unlocked(&node) {
            let missing: Vec<&str> = p.missing(&node).iter().filter_map(|k| skill_tree::node(k)).map(|n| n.name).collect();
            (format!("{} {} first", if node.requires_max { "Max out" } else { "Unlock" }, missing.join(" and ")), TOO_EXPENSIVE)
        } else if !p.buy(&node) {
            ("Not enough cores".to_string(), TOO_EXPENSIVE)
        } else if node.max_level() > 1 {
            (format!("{}: level {}", node.name, p.level(key)), SUCCESS)
        } else {
            (format!("{} unlocked!", node.name), SUCCESS)
        };
        self.show_message(text, color);
    }

    fn show_message(&mut self, text: String, color: Color) {
        self.message = text;
        self.message_color = color;
        self.message_until = crate::engine::ticks() + MESSAGE_TIME;
    }
    fn show(&mut self, m: Message) {
        if let Some((t, c)) = m {
            self.show_message(t, c);
        }
    }

    // ---------- drawing ----------

    /// The ship as the next run starts it – read from the same functions the run uses.
    fn draw_ship_column(&self, e: &Engine, surface: &mut Surface) {
        let p = &self.profile;
        let ship = skill_tree::start_ship(p, Vec2::ZERO);
        draw_text(surface, "YOUR SHIP", 22, TEXT_MUTED, Anchor::TopLeft, MARGIN, 70);
        let preview = Rect::new(MARGIN, 94, LEFT_WIDTH, PREVIEW_HEIGHT);
        surface.rect_round(settings::BACKGROUND, preview, 0, Radii::all(6));
        surface.rect_round(TEXT_MUTED, preview, 1, Radii::all(6));
        let m = e.mouse_pos();
        let aim = vec2(m.0 as f64, m.1 as f64) - preview.centerv();
        draw_ship(surface, preview.centerv(), if aim.length_squared() > 1.0 { aim.normalize() } else { vec2(0.0, -1.0) }, 80.0, ship.color);
        let [dash, rocket] = &ship.abilities;
        let radar = skill_tree::start_radar(p);
        let level = difficulty::get(&p.difficulty).unwrap_or_else(|| difficulty::all()[0].clone());
        let (w, h) = skill_tree::start_arena_size(p, &level);
        let best = p.best_time(&p.difficulty);
        let rows: Vec<(String, Option<String>)> = vec![
            ("Weapon 1".into(), Some(ship.weapons[0].as_ref().map(|w| w.name().to_string()).unwrap_or("empty".into()))),
            ("Weapon 2".into(), Some(ship.weapons[1].as_ref().map(|w| w.name().to_string()).unwrap_or("empty".into()))),
            ("Energy".into(), Some(format!("{}  ·  +{}/s", g(ship.max_energy), g(ship.energy_regen)))),
            ("Armor".into(), Some(format!("{} px  ·  {} s", g(ship.armor), g(ship.invulnerable_time)))),
            (settings::ABILITY_KEY_NAMES[0].into(), dash.as_ref().map(|d| format!("{}{}", d.name(), if d.max_charges > 1.0 { format!(" ×{}", d.max_charges as i64) } else { String::new() }))),
            (settings::ABILITY_KEY_NAMES[1].into(), rocket.as_ref().map(|r| r.name().to_string())),
            (settings::BUILD_KEY_NAME.into(), ship.watchtower.as_ref().map(|k| format!("Watchtower ×{}", k.max_count))),
            ("Radar".into(), radar.map(|r| format!("every {} s", g(r.cooldown)))),
            ("Shop".into(), p.owns(skill_tree::SHOP).then(|| "T".to_string())),
            ("Arena".into(), Some(format!("{} × {}", w as i64, h as i64))),
            ("Start stars".into(), Some(ship.stars.to_string())),
            ("Active cards".into(), p.owns(skill_tree::CARD_SHOP).then(|| format!("{}/{}", p.active_cards().len(), p.max_active_cards()))),
            (format!("Best ({})", level.name), Some(if best > 0.0 { format_time(best) } else { "–".into() })),
        ];
        let mut y = preview.bottom() + 16;
        for (name, value) in rows {
            draw_text(surface, &name, 22, TEXT_MUTED, Anchor::TopLeft, MARGIN, y);
            match value {
                None => draw_text(surface, "locked", 22, LOCKED, Anchor::TopRight, MARGIN + LEFT_WIDTH, y),
                Some(v) => draw_text(surface, &v, 22, TEXT, Anchor::TopRight, MARGIN + LEFT_WIDTH, y),
            };
            surface.line(LINE, vec2(MARGIN as f64, (y + 20) as f64), vec2((MARGIN + LEFT_WIDTH) as f64, (y + 20) as f64), 1);
            y += 27;
        }
    }

    /// The chosen level above the tabs: its name, description, goal and core chance.
    fn draw_difficulty(&self, surface: &mut Surface) {
        draw_text(surface, "LEVEL", 22, TEXT_MUTED, Anchor::TopLeft, RIGHT_LEFT, 70);
        let Some(level) = difficulty::get(&self.profile.difficulty) else { return };
        let name = draw_text(surface, &level.name, 44, TEXT, Anchor::TopLeft, RIGHT_LEFT, LEVEL_TOP);
        draw_text(surface, &level.description, 24, TEXT_MUTED, Anchor::MidLeft, name.right() + 16, name.centery() + 2);
        let chance = skill_tree::core_chance(&self.profile, &level);
        let mut goal = match level.goal {
            None => "endless".to_string(),
            Some(g) => format!("goal {}", format_time(g)),
        };
        if self.profile.cleared(&level.key) {
            goal = "cleared".into();
        }
        draw_text(surface, &format!("{goal}  ·  core chance {} %", g_prec(chance * 100.0, 3)), 22, TEXT, Anchor::MidRight, self.rect.w - MARGIN, name.centery() + 2);
    }

    /// The description of the upgrade under the mouse, or the last message.
    fn draw_info(&self, surface: &mut Surface) {
        let center = (RIGHT_LEFT + self.rect.w - MARGIN) / 2;
        let top = INFO_TOP + 12;
        if crate::engine::ticks() < self.message_until {
            draw_text(surface, &self.message, 24, self.message_color, Anchor::MidTop, center, top);
            return;
        }
        if let Some(Thing::Cards(t)) = &self.hovered {
            if let Some(info) = self.card_panel.info(&self.profile, t) {
                draw_text(surface, &info, 22, TEXT, Anchor::MidTop, center, top);
                return;
            }
        }
        let hint = |surface: &mut Surface, text: &str| {
            draw_text_a(surface, text, 20, TEXT, HINT_ALPHA * 2.0, Anchor::MidTop, center, top);
        };
        match self.tab {
            "levels" => match &self.hovered {
                Some(Thing::Difficulty(k)) if !self.profile.difficulty_unlocked(k) => {
                    draw_text(surface, &unlock_hint(k), 22, TEXT, Anchor::MidTop, center, top);
                }
                _ => hint(surface, "click a level to choose it  ·  survive its goal to unlock the next  ·  the run goes on after the goal"),
            },
            "loadout" => hint(surface, "drag a weapon onto a slot  ·  drag it off its slot to empty it  ·  one fires at a time (LMB), RMB switches  ·  both share the energy"),
            "cards" => {
                let limit = self.profile.max_active_cards();
                let apply = if limit == 1 { "card applies" } else { "cards apply" };
                hint(surface, &format!("buy cards with cores  ·  up to {limit} active {apply} at the start of every run"));
            }
            _ => hint(surface, "click an upgrade to buy it  ·  point at one for details  ·  cores drop rarely from killed enemies"),
        }
    }

    /// Problems found in the data files, in red, so a typo there does not go unnoticed.
    fn draw_warnings(&self, surface: &mut Surface) {
        let warnings = difficulty::warnings();
        if warnings.is_empty() {
            return;
        }
        let mut lines: Vec<String> = warnings
            .iter()
            .take(WARNING_LINES)
            .map(|l| if l.chars().count() <= WARNING_LENGTH { l.clone() } else { l.chars().take(WARNING_LENGTH - 1).collect::<String>() + "…" })
            .collect();
        if warnings.len() > WARNING_LINES {
            let last = lines.len() - 1;
            lines[last] += &format!("   (+{} more)", warnings.len() - WARNING_LINES);
        }
        let center = (RIGHT_LEFT + self.rect.w - MARGIN) / 2;
        for (i, l) in lines.iter().enumerate() {
            draw_text(surface, l, 18, TOO_EXPENSIVE, Anchor::MidTop, center, INFO_TOP + 40 + i as i32 * 18);
        }
    }
}

/// E.g. "Survive 02:30 on A3 to unlock A4".
fn unlock_hint(key: &str) -> String {
    let Some(level) = difficulty::get(key) else { return String::new() };
    let Some(req) = level.requires.as_deref().and_then(difficulty::get) else { return String::new() };
    format!("Survive {} on {} to unlock {}", format_time(req.goal.unwrap_or(0.0)), req.name, level.name)
}

/// Like the menu buttons: white when selected, an outline otherwise (brighter fill under the mouse).
fn draw_button(surface: &mut Surface, r: Rect, label: &str, size: i32, selected: bool, hovered: bool) {
    let c = r.center();
    if selected {
        surface.rect_round(SELECTED_FILL, r, 0, Radii::all(6));
        draw_text(surface, label, size, BACKGROUND, Anchor::Center, c.0, c.1);
        if hovered {
            surface.rect_round(CORE_COLOR, r, 3, Radii::all(6));
        }
        return;
    }
    if hovered {
        surface.rect_round(HOVER_FILL, r, 0, Radii::all(6));
    }
    surface.rect_round(TEXT_MUTED, r, 2, Radii::all(6));
    draw_text(surface, label, size, TEXT, Anchor::Center, c.0, c.1);
}

impl Scene for GameMenuScene {
    fn enter(&mut self, e: &mut Engine) {
        e.grab_mouse(false);
        e.move_window(self.rect);
    }

    fn handle_event(&mut self, e: &mut Engine, event: &Event) {
        if let Some(menu) = &mut self.menu {
            let used = menu.handle_event(event);
            let open = menu.open;
            if !open {
                self.menu = None;
                if let Event::MouseButtonUp { x, y, .. } = *event {
                    self.hovered = self.thing_at(x, y); // the card under the mouse may be gone (sold)
                }
            }
            match used {
                Used::Chosen(index) => {
                    let m = self.card_panel.sell(&mut self.profile, index);
                    self.show(m);
                    return;
                }
                Used::Yes => return,
                Used::No => {}
            }
        }
        if self.tab == "loadout" {
            let (used, m) = self.loadout_panel.handle_event(&mut self.profile, event);
            self.show(m);
            if used {
                return;
            }
        }
        match *event {
            Event::MouseMotion { x, y } => self.hovered = self.thing_at(x, y),
            Event::MouseButtonUp { button, x, y } if button == 1 || button == 3 => {
                let thing = self.thing_at(x, y);
                match thing {
                    Some(Thing::Cards(t)) => {
                        if button == 1 {
                            let m = self.card_panel.click(&mut self.profile, &t);
                            self.show(m);
                            self.hovered = self.thing_at(x, y); // the card under the mouse may have changed
                        } else if let (Some((title, label)), CardThing::Owned(i)) = (self.card_panel.menu_items(&self.profile, &t), &t) {
                            self.menu = Some(ContextMenu::new(x, y, vec![(label, *i)], Rect::new(0, 0, self.rect.w, self.rect.h), Some(title)));
                        }
                    }
                    _ if button != 1 => {}
                    Some(Thing::Start) => self.start(e),
                    Some(Thing::Back) => self.back(e),
                    Some(Thing::Difficulty(k)) => self.choose_difficulty(&k),
                    Some(Thing::Tab(k)) => self.tab = k,
                    Some(Thing::Node(k)) => self.buy(k),
                    _ => {}
                }
            }
            Event::KeyDown(k) => {
                if k == K_ESCAPE {
                    self.back(e);
                } else if [K_RETURN, K_KP_ENTER, K_SPACE].contains(&k) {
                    self.start(e);
                } else if k == K_TAB {
                    let keys: Vec<&'static str> = self.tabs().iter().map(|(k, _)| *k).collect();
                    self.tab = match keys.iter().position(|t| *t == self.tab) {
                        Some(i) => keys[(i + 1) % keys.len()],
                        None => keys[0],
                    };
                    self.hovered = None;
                    self.menu = None;
                } else if k == K_A || k == K_LEFT {
                    self.step_difficulty(-1);
                } else if k == K_D || k == K_RIGHT {
                    self.step_difficulty(1);
                }
            }
            _ => {}
        }
    }

    fn draw(&mut self, e: &mut Engine, surface: &mut Surface) {
        let panel = surface.get_rect();
        draw_panel(surface, panel, 0);
        draw_text(surface, "GAME MENU", 36, TEXT, Anchor::TopLeft, MARGIN, 20);
        let count = draw_text(surface, &self.profile.cores.to_string(), 34, TEXT, Anchor::TopRight, panel.right() - MARGIN, 22);
        draw_core(surface, vec2((count.left() - 16) as f64, (count.centery() - 1) as f64), 11.0, CORE_COLOR);
        self.draw_ship_column(e, surface);
        self.draw_difficulty(surface);
        for (k, r) in self.tab_rects() {
            let label = TABS.iter().find(|(t, _)| *t == k).map(|(_, l)| *l).unwrap_or(k);
            draw_button(surface, r, label, 24, k == self.tab, self.hovered == Some(Thing::Tab(k)));
        }
        match self.tab {
            "levels" => {
                let h = match &self.hovered {
                    Some(Thing::Difficulty(k)) => Some(k.as_str()),
                    _ => None,
                };
                self.level_panel.draw(surface, &self.profile, h);
            }
            "tree" => {
                let h = match &self.hovered {
                    Some(Thing::Node(k)) => Some(*k),
                    _ => None,
                };
                self.tree_panel.draw(surface, &self.profile, h);
            }
            "loadout" => {
                let h = match &self.hovered {
                    Some(Thing::Loadout(t)) => Some(t),
                    _ => None,
                };
                self.loadout_panel.draw(surface, &self.profile, h);
            }
            _ => {
                let h = match &self.hovered {
                    Some(Thing::Cards(t)) => Some(t),
                    _ => None,
                };
                self.card_panel.draw(surface, &self.profile, h);
            }
        }
        self.draw_info(surface);
        self.draw_warnings(surface);
        draw_button(surface, self.back_rect(), "Back", 28, self.hovered == Some(Thing::Back), false);
        draw_button(surface, self.start_rect(), "Start", 34, true, self.hovered == Some(Thing::Start));
        draw_text_a(surface, "ENTER start  ·  ESC back  ·  A / D level  ·  TAB switch tab", 20, TEXT, HINT_ALPHA * 2.0, Anchor::MidBottom, panel.centerx(), panel.bottom() - 20);
        if let Some(m) = &self.menu {
            m.draw(surface);
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}
