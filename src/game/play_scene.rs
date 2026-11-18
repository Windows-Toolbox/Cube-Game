//! The scene where the game is played (`play_scene.py`): the window keeps shrinking, the ship shoots its edges to
//! push them back out, enemies come in from the desktop. Getting hit squeezes the window. Once it is squeezed down
//! to the ship, the run is over (game over screen).
//!
//! What the run starts with comes from the profile: the level and the skill tree. Killed enemies drop stars (only
//! with the shop unlocked) and, rarely, a core; cores are saved right away. Surviving the level's goal clears it and
//! unlocks the next level – saved right then, with a notice on screen; the run goes on until the ship is crushed.
//!
//! Watchtowers (B): thrown out from the ship, each one gets its own window that shows the world around it
//! (`draw_view`). The radar opens a window of its own over enemies out of sight now and then. The field (`field.rs`):
//! the arena plus the extra windows connected to it – shots, blasts, the mouse, what counts as seen, where enemies
//! appear all ask it.
//!
//! States: playing, paused (Esc), inventory (Tab) and shop (T). Outside playing nothing moves, the mouse is free and
//! the window shows only the panel, centred on the arena. Leaving the window pauses the game, too.

use std::any::Any;
use std::rc::Rc;
use std::cell::RefCell;

use super::arena::Arena;
use super::buildings::{self, TowerRef, Watchtower};
use super::difficulty::{self, Difficulty};
use super::effects::{ArtilleryBlast, ArtilleryStrike, BurningGround, Explosion, SlowField, Spark};
use super::enemies::{Attack, BeamRef, Ctx, EnemyRef, type_of};
use super::field::Field;
use super::game_over::{GameOverScene, RunResult};
use super::hud::{draw_ability_bar, draw_hud};
use super::pickups::{Currency, Pickup};
use super::points::{Points, PointsCtx};
use super::profile::Profile;
use super::projectile::Projectile;
use super::radar::{Radar, Scan};
use super::roster::{Roster, RosterRef};
use super::scenes::{PauseAction, PauseMenu, PAUSE_SIZE};
use super::settings::{self, TEXT};
use super::shapes::draw_swarm_icon;
use super::ship::Ship;
use super::shop::{INVENTORY_SIZE, SHOP_SIZE, Shop, ShopPanel, draw_inventory};
use super::skill_tree;
use super::spawner::{Spawner, distance_to_rect};
use super::tables;
use crate::engine::collision::{close, nearby_pairs, push_apart};
use crate::engine::desktop::Desktop;
use crate::engine::geom::py_round;
use crate::engine::key_overlay::{Cap, KeyOverlay};
use crate::engine::sprite_cache;
use crate::engine::ui::draw_text_a;
use crate::engine::window::{Event, K_A, K_D, K_ESCAPE, K_S, K_T, K_TAB, K_W};
use crate::engine::{Anchor, Color, Engine, FRect, Rect, Scene, Surface, Vec2, ViewId, rgb, rgba, vec2};
use crate::rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Playing,
    Paused,
    Inventory,
    Shop,
}

/// Off-screen markers: distance from the window's edge …
const MARKER_INSET: i32 = 8;
/// … and size.
const MARKER_RADIUS: f64 = 4.0;
const SWARM_MARKER_INSET: i32 = 16;
const SWARM_MARKER_SIZES: (f64, f64) = (12.0, 30.0);
const SWARM_MARKER_FAR: f64 = 900.0;
/// Seconds a notice ("… unlocked", a phase's announcement) stays on screen.
const NOTICE_TIME: f64 = 4.0;
const NOTICE_COLOR: Color = rgb(255, 215, 60);
const PHASE_COLOR: Color = rgb(255, 110, 80);
/// Seconds for small notices ("A watchtower costs 25 stars").
const SHORT_NOTICE_TIME: f64 = 1.5;
/// A Pusher's window: this much around its body …
const LOOKOUT_MARGIN: f64 = 14.0;
/// … with a red frame, so it is clearly the enemy's.
const LOOKOUT_BORDER: Color = rgb(150, 50, 50);
/// A watchtower's window outside the arena: almost black, a hint of grey …
const TOWER_BACKGROUND: Color = rgb(12, 12, 14);
/// … the radar's: a hint of blue.
const RADAR_BACKGROUND: Color = rgb(5, 11, 17);
/// How often per tick touching enemies are pushed apart …
const SEPARATION_PASSES: (usize, usize) = (2, 8);
/// … as often as fits into this many pair checks.
const SEPARATION_BUDGET: usize = 4000;
/// Pixels between touching enemies.
const ENEMY_GAP: f64 = 3.0;
const SEPARATION_REACH: f64 = 6.0;
/// The left mouse button fires the active weapon …
const FIRE_BUTTON: usize = 0;
/// … the right one switches weapons (shown lit).
const SWITCH_BUTTON: usize = 2;

/// The key display: W A S D, below them the keys of the abilities the ship has (SHIFT, SPACE, B).
fn key_layout(ship: &Ship) -> Vec<Cap> {
    let cap = |label: &str, keys: &[u32], column, row, width| Cap { label: label.into(), keys: keys.to_vec(), column, row, width };
    let mut layout = vec![cap("W", &[K_W], 1, 0, 1), cap("A", &[K_A], 0, 1, 1), cap("S", &[K_S], 1, 1, 1), cap("D", &[K_D], 2, 1, 1)];
    for (slot, column) in [(0usize, 0), (1, 2)] {
        if ship.abilities[slot].is_some() {
            layout.push(cap(settings::ABILITY_KEY_NAMES[slot], settings::ABILITY_KEYS[slot], column, 2, 2));
        }
    }
    if ship.watchtower.is_some() {
        layout.push(cap(settings::BUILD_KEY_NAME, settings::BUILD_KEYS, 4, 2, 1));
    }
    layout
}

/// A notice at the top: text, colour, seconds left.
struct Notice(String, Color, f64);

pub struct PlayScene {
    pub profile: Profile,
    difficulty: Rc<Difficulty>,
    pub desktop: Desktop,
    pub arena: Arena,
    pub ship: Ship,
    shop_unlocked: bool,
    core_chance: f64,
    pub radar: Option<Radar>,
    /// The player's shots.
    pub projectiles: Vec<Projectile>,
    pub enemies: Vec<EnemyRef>,
    pub enemy_projectiles: Vec<Projectile>,
    /// Artillery shells in the air.
    pub strikes: Vec<ArtilleryStrike>,
    /// Laser beams while they are on.
    pub beams: Vec<BeamRef>,
    pub roster: RosterRef,
    pub spawner: Spawner,
    /// Effects: explosions, blasts, sparks (only drawn, they do nothing).
    explosions: Vec<Explosion>,
    blasts: Vec<ArtilleryBlast>,
    sparks: Vec<Spark>,
    burning: Vec<BurningGround>,
    slow_fields: Vec<SlowField>,
    pub pickups: Vec<Pickup>,
    pub towers: Vec<TowerRef>,
    pub points: Points,
    /// This frame's HUD etc. for the extra windows: (main window, picture).
    ui_layer: Option<(Rect, Surface)>,
    /// Seconds played (not counting pauses).
    pub elapsed: f64,
    pub kills: i64,
    /// Cores collected in this run (already added to the profile).
    pub cores: i64,
    /// The level this run will unlock once it has lasted the goal – `None` if there is none (left).
    unlocks: Option<String>,
    unlocked: Option<String>,
    cleared: bool,
    first_clear: bool,
    notices: Vec<Notice>,
    shop: Shop,
    shop_panel: ShopPanel,
    key_overlay: KeyOverlay,
    pause_menu: PauseMenu,
    state: State,
    /// For tests and the bot: the run is over (the game over scene was asked for).
    pub over: bool,
}

impl PlayScene {
    pub fn new(profile: Profile) -> PlayScene {
        let difficulty = difficulty::get(&profile.difficulty).unwrap_or_else(|| difficulty::all()[0].clone());
        let desktop = Desktop::current(profile.settings.all_monitors);
        let rules = &difficulty.arena;
        let mut arena = Arena::new(desktop.clone(), skill_tree::start_arena_size(&profile, &difficulty), rules.shrink_speed, rules.shrink_growth, Some(rules.max_shrink_speed), rules.armor, rules.push_falloff);
        let ship = skill_tree::start_ship(&profile, arena.rect.centerv());
        arena.ship_size = ship.size;
        let roster = Rc::new(RefCell::new(Roster::new(Some(&difficulty), None)));
        let spawner = Spawner::new(desktop.clone(), &difficulty, roster.clone());
        let upcoming = difficulty::next_difficulty(&profile.difficulty);
        let unlocks = upcoming.filter(|u| !profile.difficulty_unlocked(u));
        let both = ship.weapons.iter().all(|w| w.is_some());
        let key_overlay = KeyOverlay::new(key_layout(&ship), settings::KEY_OVERLAY_INSET, profile.settings.key_display, if both { vec![FIRE_BUTTON, SWITCH_BUTTON] } else { vec![FIRE_BUTTON] });
        PlayScene {
            shop_unlocked: profile.owns(skill_tree::SHOP),
            core_chance: skill_tree::core_chance(&profile, &difficulty),
            radar: skill_tree::start_radar(&profile),
            points: Points::new(skill_tree::start_finds(&profile)),
            first_clear: !profile.cleared(&profile.difficulty),
            shop: Shop::new(&ship),
            shop_panel: ShopPanel::new(),
            key_overlay,
            pause_menu: PauseMenu::new(),
            projectiles: vec![],
            enemies: vec![],
            enemy_projectiles: vec![],
            strikes: vec![],
            beams: vec![],
            roster,
            spawner,
            explosions: vec![],
            blasts: vec![],
            sparks: vec![],
            burning: vec![],
            slow_fields: vec![],
            pickups: vec![],
            towers: vec![],
            ui_layer: None,
            elapsed: 0.0,
            kills: 0,
            cores: 0,
            unlocks,
            unlocked: None,
            cleared: false,
            notices: vec![],
            state: State::Playing,
            over: false,
            difficulty,
            desktop,
            arena,
            ship,
            profile,
        }
    }

    fn set_state(&mut self, e: &mut Engine, state: State) {
        self.state = state;
        if state == State::Paused {
            self.pause_menu.open();
        }
        for v in self.extra_views() {
            e.set_view_hidden(v, state != State::Playing); // the extra windows only show while playing
        }
        e.grab_mouse(state == State::Playing);
        e.move_window(self.window_rect());
    }

    /// Playing: exactly the arena. Otherwise just the panel, centred on the arena (on its monitor).
    fn window_rect(&self) -> Rect {
        let arena = self.arena.window_rect();
        let size = match self.state {
            State::Playing => return arena,
            State::Paused => PAUSE_SIZE,
            State::Inventory => INVENTORY_SIZE,
            State::Shop => SHOP_SIZE,
        };
        let mut panel = Rect::new(0, 0, size.0, size.1);
        panel.set_centerv(arena.centerv());
        panel.clamp(&self.desktop.area_at(arena.centerv()))
    }

    fn to_game_menu(&mut self, e: &mut Engine) {
        e.change_scene(Box::new(super::game_menu::GameMenuScene::new(Some(self.profile.clone()))));
    }

    /// The ship is crushed: remember the time, then the game over screen – centred where the arena was.
    fn game_over(&mut self, e: &mut Engine) {
        let new_best = self.profile.record(self.elapsed);
        self.profile.save();
        let result = RunResult {
            time: self.elapsed,
            kills: self.kills,
            cores: self.cores,
            difficulty: self.profile.difficulty.clone(),
            new_best,
            unlocked: self.unlocked.clone(),
            cleared: self.cleared && self.first_clear,
        };
        self.over = true;
        e.change_scene(Box::new(GameOverScene::new(self.profile.clone(), result, self.arena.rect.centerv())));
    }

    /// The goal is reached: the time counts right now (saved), so leaving the run afterwards keeps the unlock.
    fn clear_level(&mut self) {
        self.profile.record(self.elapsed);
        self.profile.save();
        self.cleared = true;
        let mut parts = vec![];
        if self.first_clear {
            parts.push(format!("{} CLEARED", self.difficulty.name.to_uppercase()));
        }
        if let Some(u) = self.unlocks.take() {
            let name = difficulty::get(&u).map(|d| d.name.to_uppercase()).unwrap_or_default();
            parts.push(format!("{name} UNLOCKED"));
            self.unlocked = Some(u);
        }
        if !parts.is_empty() {
            self.notices.push(Notice(parts.join("  ·  "), NOTICE_COLOR, NOTICE_TIME));
        }
    }

    /// A small notice for a moment – the same text again only restarts it.
    fn notify(&mut self, text: String) {
        if let Some(n) = self.notices.iter_mut().find(|n| n.0 == text) {
            n.2 = SHORT_NOTICE_TIME;
            return;
        }
        self.notices.push(Notice(text, settings::TEXT_MUTED, SHORT_NOTICE_TIME));
    }

    /// B: a watchtower flies from the ship towards the mouse, throw_range far (kept on a monitor).
    fn throw_watchtower(&mut self, e: &Engine) {
        let Some(kit) = &self.ship.watchtower else { return };
        let (max, price) = (kit.max_count, kit.price());
        if self.towers.len() as i64 >= max {
            self.notify(format!("All watchtowers are standing ({}/{max})", self.towers.len()));
            return;
        }
        if self.ship.stars < price {
            self.notify(format!("A watchtower costs {price} stars"));
            return;
        }
        self.ship.stars -= price;
        if let Some(k) = &mut self.ship.watchtower {
            k.bought += 1;
        }
        let start = self.ship.center();
        let d = e.mouse_position() - start;
        let direction = if d.length_squared() > 1.0 { d.normalize() } else { self.ship.aim };
        let size = tables::buildings::get("watchtower", "size");
        let target = start + direction * tables::buildings::get("watchtower", "throw_range");
        let inner = self.desktop.area_at(target).inflate(-size, -size); // the whole tower on the monitor
        let target = vec2(target.x.clamp(inner.left() as f64, inner.right() as f64), target.y.clamp(inner.top() as f64, inner.bottom() as f64));
        self.towers.push(Rc::new(RefCell::new(Watchtower::new(start, target))));
    }

    // ---------- the field and the extra windows ----------

    /// The windows of the Pushers and Living Windows.
    fn lookouts(&self) -> Vec<ViewId> {
        self.enemies.iter().filter_map(|en| en.borrow().view).collect()
    }
    /// The open extra windows: the watchtowers', the radar's, the lookouts.
    fn extra_views(&self) -> Vec<ViewId> {
        let mut v: Vec<ViewId> = self.towers.iter().filter_map(|t| t.borrow().view).collect();
        if let Some(r) = self.radar.as_ref().and_then(|r| r.view) {
            v.push(r);
        }
        v.extend(self.lookouts());
        v
    }
    /// The field right now: the arena and the extra windows.
    pub fn field(&self, e: &Engine) -> Field {
        let lookouts = self.lookouts();
        let windows: Vec<Rect> = self.extra_views().into_iter().filter(|v| !lookouts.contains(v)).filter_map(|v| e.view_rect(v)).collect();
        let looks: Vec<Rect> = lookouts.into_iter().filter_map(|v| e.view_rect(v)).collect();
        Field::new(&self.arena, &windows, &looks)
    }

    // ---------- every tick ----------

    fn tick(&mut self, e: &mut Engine, dt: f64) {
        self.elapsed += dt;
        for n in &mut self.notices {
            n.2 -= dt;
        }
        self.notices.retain(|n| n.2 > 0.0);
        if !self.cleared && self.difficulty.goal.is_some_and(|g| self.elapsed >= g) {
            self.clear_level();
        }
        let phases: Vec<(usize, super::difficulty::Phase)> = self.spawner.active_phases().into_iter().map(|(i, p)| (i, p.clone())).collect();
        self.arena.changes = phases.iter().flat_map(|(_, p)| p.arena.iter().cloned()).collect();
        self.arena.breathe(phases);
        self.arena.update(dt);
        let moving = Ship::move_direction(e);
        self.ship.update(dt, e.mouse_position(), moving);
        self.arena.contain(&mut self.ship.rect);
        if self.arena.crushed(&self.ship.rect) {
            self.game_over(e);
            return;
        }
        if e.mouse_held()[FIRE_BUTTON] && !self.ship.stunned() {
            let shots = self.ship.fire();
            self.projectiles.extend(shots);
        }

        self.roster.borrow_mut().update(dt);
        let field = self.field(e);
        e.confine_mouse(Some(field.rects.clone())); // the mouse may go anywhere in the field
        let new = self.spawner.update(dt, self.arena.window_rect_f(), field.sight[1..].to_vec());
        self.enemies.extend(new);
        self.lure_enemies();
        self.slow_enemies(dt);
        for text in std::mem::take(&mut self.spawner.announcements) {
            self.notices.push(Notice(text, PHASE_COLOR, NOTICE_TIME));
        }
        let mut i = 0;
        while i < self.enemies.len() {
            // the list grows while going through it: spawned enemies are added
            let enemy = self.enemies[i].clone();
            let attacks = {
                let mut ctx = Ctx { ship_center: self.ship.center(), arena: Some(&mut self.arena), spawner: Some(&mut self.spawner) };
                enemy.borrow_mut().update(dt, &mut ctx)
            };
            for a in attacks {
                match a {
                    Attack::Strike(s) => self.strikes.push(s),
                    Attack::Beam(b) => self.beams.push(b),
                    Attack::Shot(p) => self.enemy_projectiles.push(p),
                }
            }
            let mut en = enemy.borrow_mut();
            if !en.seen && field.sees(&en.rect) {
                en.seen = true; // the player has had it in view
            }
            let spawned = std::mem::take(&mut en.spawned);
            drop(en);
            self.enemies.extend(spawned);
            i += 1;
        }
        for en in &self.enemies {
            let commander = en.borrow().info.commands;
            if commander {
                en.borrow().command(&self.enemies); // a Commander's aura
            }
        }
        self.separate_enemies();
        self.update_towers(e, dt);
        self.expose_enemies();
        self.update_radar(e, dt);
        self.update_points(e, dt);

        self.update_player_shots(e, dt);
        self.update_enemy_shots(dt);
        self.update_strikes(dt);
        self.update_beams(dt);
        self.update_burning(dt);
        for en in self.enemies.clone() {
            let (touch, damage) = {
                let en = en.borrow();
                (en.rect.colliderect(&self.ship.rect), en.stat("contact_damage"))
            };
            if touch && damage != 0.0 && self.hit_ship(damage) {
                if let Some((direction, distance, stun)) = en.borrow().on_hit_ship() {
                    self.ship.knock(direction, distance, stun); // the shield's dash knocks it back
                }
            }
        }

        for en in self.enemies.clone() {
            let en = en.borrow();
            if !en.alive() {
                self.kills += 1;
                self.explosions.push(Explosion::new(en.center(), en.size() * 1.6, en.info.color));
                if en.drops_loot {
                    self.drop_loot(en.center());
                }
                if let Some(v) = en.view {
                    e.close_view(v); // a Pusher's or Living Window's window
                }
            }
        }
        self.enemies.retain(|en| en.borrow().alive());
        self.update_lookouts(e);

        let (center, magnet) = (self.ship.center(), self.ship.magnet_radius);
        for p in &mut self.pickups {
            p.update(dt, center, magnet);
        }
        let ship_rect = self.ship.rect;
        let mut collected_core = false;
        let mut kept = vec![];
        for p in std::mem::take(&mut self.pickups) {
            if p.rect.colliderect(&ship_rect) {
                match p.currency {
                    Currency::Cores => {
                        self.cores += 1;
                        self.profile.cores += 1;
                        collected_core = true;
                    }
                    Currency::Stars => self.ship.stars += 1,
                }
            } else {
                kept.push(p);
            }
        }
        self.pickups = kept;
        if collected_core {
            self.profile.save();
        }
        for x in &mut self.explosions {
            x.update(dt);
        }
        self.explosions.retain(|x| !x.done());
        for x in &mut self.blasts {
            x.update(dt);
        }
        self.blasts.retain(|x| !x.done());
        for x in &mut self.sparks {
            x.update(dt);
        }
        self.sparks.retain(|x| !x.done());
        e.move_window(self.window_rect());
    }

    /// Watchtowers fly and land (their window opens, its rect is their area). Once their lifetime is over they
    /// vanish and their window closes.
    fn update_towers(&mut self, e: &mut Engine, dt: f64) {
        let vision = self.ship.watchtower.as_ref().map(|k| k.vision).unwrap_or(0.0);
        for t in &self.towers {
            let mut t = t.borrow_mut();
            if t.update(dt) {
                t.view = e.open_view();
            }
            if !t.landed() {
                continue;
            }
            let area = t.vision_rect(vision, &self.desktop.area_at(t.center()));
            t.area = Some(area);
            if let Some(v) = t.view {
                e.set_view_rect(v, area);
            }
        }
        for t in &self.towers {
            let t = t.borrow();
            if !t.alive() {
                self.explosions.push(Explosion::new(t.center(), t.rect.w as f64 * 2.5, buildings::COLOR));
                if let Some(v) = t.view {
                    e.close_view(v);
                }
            }
        }
        self.towers.retain(|t| t.borrow().alive());
    }

    /// Every Pusher and Living Window on a monitor gets a small window of its own that follows it.
    fn update_lookouts(&mut self, e: &mut Engine) {
        for en in &self.enemies {
            let mut en = en.borrow_mut();
            if en.info.key != "pusher" && en.info.key != "living_window" {
                continue;
            }
            let rect = en.body().inflate(2.0 * LOOKOUT_MARGIN, 2.0 * LOOKOUT_MARGIN).to_i();
            if en.view.is_none() && self.desktop.contains(rect.centerx() as f64, rect.centery() as f64) {
                en.view = e.open_view();
            }
            if let Some(v) = en.view {
                e.set_view_rect(v, rect.clamp(&self.desktop.area_at(rect.centerv())));
            }
        }
    }

    /// Enemies in a standing watchtower's area take its damage_bonus percent more damage this tick – the biggest
    /// bonus counts, towers do not stack.
    fn expose_enemies(&mut self) {
        let areas: Vec<(Rect, f64)> = self.towers.iter().filter_map(|t| t.borrow().area.map(|a| (a, t.borrow().value("damage_bonus")))).collect();
        for en in &self.enemies {
            let mut en = en.borrow_mut();
            let r = en.rect.to_i();
            let bonus = areas.iter().filter(|(a, _)| a.colliderect(&r)).map(|(_, b)| *b).fold(0.0, f64::max);
            en.damage_factor = 1.0 + bonus / 100.0;
        }
    }

    /// Melee enemies near a standing watchtower – within its lure_range and closer to it than to the ship – go for
    /// the tower instead of the ship.
    fn lure_enemies(&mut self) {
        let standing: Vec<TowerRef> = self.towers.iter().filter(|t| t.borrow().landed()).cloned().collect();
        let ship = self.ship.center();
        for en in &self.enemies {
            let mut en = en.borrow_mut();
            if !en.info.melee {
                continue;
            }
            en.lure = None;
            if standing.is_empty() {
                continue;
            }
            let c = en.center();
            let tower = standing.iter().min_by(|a, b| c.distance_squared_to(a.borrow().center()).total_cmp(&c.distance_squared_to(b.borrow().center()))).unwrap();
            let d = c.distance_to(tower.borrow().center());
            if d <= tower.borrow().value("lure_range") && d < c.distance_to(ship) {
                en.lure = Some(tower.clone());
            }
        }
    }

    /// A killed enemy's drop: a star if the shop is unlocked, and with core_chance a core next to it.
    fn drop_loot(&mut self, position: Vec2) {
        if self.shop_unlocked {
            self.pickups.push(Pickup::new(position, Currency::Stars));
        }
        if rng::random() < self.core_chance {
            let at = if self.shop_unlocked { position + vec2(14.0, 0.0) } else { position };
            self.pickups.push(Pickup::new(at, Currency::Cores));
        }
    }

    /// Enemies cannot overlap: every touching pair is pushed apart (ENEMY_GAP between them), the heavier one moves
    /// less. Standing towers are solid: enemies are pushed out, never them.
    fn separate_enemies(&mut self) {
        let solid: Vec<EnemyRef> = self.enemies.iter().filter(|en| en.borrow().info.solid).cloned().collect();
        if solid.is_empty() {
            return;
        }
        let cell = solid.iter().map(|en| en.borrow().size()).fold(0.0, f64::max) + ENEMY_GAP;
        let reach = ENEMY_GAP + SEPARATION_REACH;
        let centers: Vec<(f32, f32)> = solid.iter().map(|en| en.borrow().rect.center()).collect();
        // (a, b, mass a, mass b, gap) – a: an enemy index, or a tower (usize::MAX - tower index)
        let mut pairs: Vec<(usize, usize, f64, f64, f64)> = vec![];
        for (a, b) in nearby_pairs(&centers, cell) {
            let (ra, rb) = (solid[a].borrow().rect, solid[b].borrow().rect);
            if close(&ra, &rb, reach) {
                pairs.push((a, b, solid[a].borrow().mass(), solid[b].borrow().mass(), ENEMY_GAP));
            }
        }
        for (ti, t) in self.towers.iter().enumerate() {
            let t = t.borrow();
            if t.landed() {
                for (i, en) in solid.iter().enumerate() {
                    if close(&t.rect, &en.borrow().rect, reach) {
                        pairs.push((usize::MAX - ti, i, f64::INFINITY, en.borrow().mass(), 0.0));
                    }
                }
            }
        }
        let passes = SEPARATION_PASSES.1.min(SEPARATION_PASSES.0.max(SEPARATION_BUDGET / pairs.len().max(1)));
        // work on copies of the rects, written back at the end
        let mut rects: Vec<FRect> = solid.iter().map(|en| en.borrow().rect).collect();
        let mut tower_rects: Vec<FRect> = self.towers.iter().map(|t| t.borrow().rect).collect();
        for _ in 0..passes {
            for &(a, b, ma, mb, gap) in &pairs {
                if a > usize::MAX / 2 {
                    let ti = usize::MAX - a;
                    let (mut ta, mut rb) = (tower_rects[ti], rects[b]);
                    push_apart(&mut ta, &mut rb, ma, mb, gap);
                    tower_rects[ti] = ta;
                    rects[b] = rb;
                } else {
                    let (mut ra, mut rb) = (rects[a], rects[b]);
                    push_apart(&mut ra, &mut rb, ma, mb, gap);
                    rects[a] = ra;
                    rects[b] = rb;
                }
            }
        }
        for (en, r) in solid.iter().zip(rects) {
            en.borrow_mut().rect = r;
        }
    }

    /// The radar opens its window over enemies out of sight and closes it again after its duration.
    fn update_radar(&mut self, e: &mut Engine, dt: f64) {
        let field = self.field(e);
        let hidden: Vec<Vec2> = self.points.points.iter().filter(|p| !p.revealed).map(|p| p.position).collect();
        let centers: Vec<Vec2> = self.enemies.iter().map(|en| en.borrow().center()).collect();
        let Some(radar) = &mut self.radar else { return };
        match radar.update(dt, &centers, &field.sight, &self.desktop, &hidden) {
            Some(Scan::Open) => {
                radar.view = e.open_view();
                if let (Some(v), Some(r)) = (radar.view, radar.rect) {
                    e.set_view_rect(v, r);
                }
            }
            Some(Scan::Close) => {
                if let Some(v) = radar.view.take() {
                    e.close_view(v);
                }
            }
            None => {}
        }
    }

    fn update_points(&mut self, e: &mut Engine, dt: f64) {
        let field = self.field(e);
        let tower_areas = self.towers.iter().filter_map(|t| t.borrow().area).collect();
        let scanned = self.radar.as_ref().and_then(|r| r.rect);
        let mut ctx = PointsCtx {
            arena: &mut self.arena,
            spawner: &mut self.spawner,
            roster: &self.roster,
            field: &field,
            desktop: &self.desktop,
            scanned,
            tower_areas,
            ship_center: self.ship.rect.centerv(),
            stars: &mut self.ship.stars,
            pickups: &mut self.pickups,
            notes: vec![],
            explosions: vec![],
            new_enemies: vec![],
        };
        self.points.update(dt, &mut ctx);
        let (notes, explosions, new) = (ctx.notes, ctx.explosions, ctx.new_enemies);
        for n in notes {
            self.notify(n);
        }
        self.explosions.extend(explosions);
        self.enemies.extend(new);
    }

    /// Player shots end on an enemy (damage), on a shield (blocked: a spark), at the field's wall (push the arena
    /// edge) or at their target. Shots with a blast radius explode at their end instead. Piercing shots fly on
    /// through as many enemies as they may pierce; hits count along the whole way a shot flew this tick.
    fn update_player_shots(&mut self, e: &Engine, dt: f64) {
        let field = self.field(e);
        let alive: Vec<EnemyRef> = self.enemies.iter().filter(|en| en.borrow().alive()).cloned().collect();
        let shielded: Vec<EnemyRef> = alive.iter().filter(|en| en.borrow().info.shielded).cloned().collect();
        let rects: Vec<FRect> = alive.iter().map(|en| en.borrow().rect).collect();
        let mut kept = vec![];
        for mut p in std::mem::take(&mut self.projectiles) {
            p.update(dt);
            let shield = shielded.iter().find(|en| !p.pierced.contains(&en.borrow().id) && en.borrow().blocks(&p)).cloned();
            let mut enemy = shield.clone().or_else(|| first_hit(&p, &alive, &rects));
            while let Some(en) = enemy.clone() {
                if shield.is_some() || p.stats.blast_radius != 0.0 || !p.can_pierce() {
                    break;
                }
                en.borrow_mut().take_damage(p.stats.damage);
                p.pierced.push(en.borrow().id); // through it – flies on, maybe into the next one right away
                enemy = first_hit(&p, &alive, &rects);
            }
            let side = field.wall_hit(&mut p);
            if enemy.is_none() && side.is_none() && !p.arrived {
                kept.push(p);
                continue;
            }
            if p.stats.blast_radius != 0.0 {
                self.explode(&field, &p);
            } else if shield.is_some() {
                self.sparks.push(Spark::new(p.center(), -p.direction));
            } else if let Some(en) = enemy {
                en.borrow_mut().take_damage(p.stats.damage);
            } else if let Some(side) = side {
                self.arena.push(side, p.wall_push());
            }
        }
        self.projectiles = kept;
    }

    /// Enemy shots fly through the window's edges and through watchtowers; they end on the ship or when they leave the
    /// desktop.
    fn update_enemy_shots(&mut self, dt: f64) {
        let mut kept = vec![];
        for mut p in std::mem::take(&mut self.enemy_projectiles) {
            p.update(dt);
            if p.rect.colliderect(&self.ship.rect) {
                self.hit_ship(p.stats.damage);
            } else if self.desktop.bounds.collidev(p.center()) {
                kept.push(p);
            }
        }
        self.enemy_projectiles = kept;
    }

    /// Artillery shells land when their circle is full: they hurt the ship if it is in the circle. A splitter shell
    /// hurts nothing itself – it bursts into fragments, strikes of their own.
    fn update_strikes(&mut self, dt: f64) {
        let mut fragments = vec![];
        for i in 0..self.strikes.len() {
            if self.strikes[i].update(dt) {
                let s = self.strikes[i].clone();
                if s.damage != 0.0 {
                    if s.hits(&self.ship.rect) {
                        self.hit_ship(s.damage);
                    }
                    self.blasts.push(ArtilleryBlast::new(s.center, s.radius));
                } else {
                    self.explosions.push(Explosion::new(s.center, s.radius, s.color));
                }
                fragments.extend(s.fragments());
            }
        }
        self.strikes.retain(|s| !s.landed);
        self.strikes.extend(fragments);
    }

    /// Laser beams hurt the ship once per beam and go out after their time (or when their Laser dies).
    fn update_beams(&mut self, dt: f64) {
        for b in self.beams.clone() {
            let source = self.enemies.iter().find(|en| en.borrow().id == b.borrow().source).cloned();
            {
                let mut beam = b.borrow_mut();
                match &source {
                    Some(en) => {
                        let en = en.borrow();
                        beam.start = en.center();
                        beam.source_alive = en.alive();
                    }
                    None => beam.source_alive = false,
                }
                beam.update(dt);
            }
            let (done, hurt, hits, damage) = {
                let beam = b.borrow();
                (beam.done(), beam.hurt_ship, beam.hits(&self.ship.rect), beam.damage)
            };
            if done {
                continue;
            }
            if !hurt && hits && self.hit_ship(damage) {
                b.borrow_mut().hurt_ship = true;
            }
        }
        self.beams.retain(|b| !b.borrow().done());
    }

    /// The window is the ship's health: a hit squeezes the edges in by damage pixels (less the ship's armor), mostly
    /// the ones away from the ship – unless the ship is still invulnerable. True if the hit counted.
    fn hit_ship(&mut self, damage: f64) -> bool {
        if !self.ship.hit() {
            return false;
        }
        let d = self.ship.absorb(damage);
        self.arena.hit(d, self.ship.center());
        true
    }

    /// Blast at the end of the shot, inside the field: damage to every enemy within the radius, and the arena edges
    /// whose part of the field's wall it reaches are pushed out.
    fn explode(&mut self, field: &Field, p: &Projectile) {
        let position = field.clamp(p.center());
        let s = &p.stats;
        for en in &self.enemies {
            let mut en = en.borrow_mut();
            if en.distance_to(position) <= s.blast_radius + en.size() / 2.0 {
                en.take_damage(s.damage);
            }
        }
        for side in field.walls_within(position, s.blast_radius, p.beyond) {
            self.arena.push(side, s.wall_push);
        }
        self.explosions.push(Explosion::new(position, s.blast_radius, s.color));
        if s.burn_time != 0.0 {
            self.burning.push(BurningGround::new(position, s.burn_radius, s.burn_damage, s.burn_time));
        }
        if s.slow_time != 0.0 {
            self.slow_fields.push(SlowField::new(position, s.slow_radius, s.slow, s.slow_time));
        }
    }

    /// Slow fields slow every enemy in them for this tick (before the enemies move), then go out.
    fn slow_enemies(&mut self, dt: f64) {
        for f in &mut self.slow_fields {
            f.update(dt);
            for en in &self.enemies {
                let mut en = en.borrow_mut();
                if f.covers(en.center(), en.size()) {
                    en.slow(f.factor);
                }
            }
        }
        self.slow_fields.retain(|f| !f.done());
    }

    /// Burning ground hurts every enemy in it with each pulse, then goes out.
    fn update_burning(&mut self, dt: f64) {
        for g in &mut self.burning {
            g.update(dt);
            for _ in 0..g.pulses {
                for en in &self.enemies {
                    let mut en = en.borrow_mut();
                    if en.alive() && en.distance_to(g.position) <= g.radius + en.size() / 2.0 {
                        en.take_damage(g.damage);
                    }
                }
            }
        }
        self.burning.retain(|g| !g.done());
    }

    // ---------- drawing ----------

    /// Everything on top of the world in the main window: markers, keys, abilities, HUD, notices, hints.
    fn draw_ui(&mut self, e: &Engine, surface: &mut Surface, offset: Vec2) {
        if settings::OFFSCREEN_MARKERS {
            self.draw_offscreen_markers(e, surface, offset);
        }
        self.key_overlay.draw(e, surface);
        draw_ability_bar(surface, &self.ship);
        let towers = self.ship.watchtower.as_ref().map(|k| (self.towers.len(), k.max_count, k.price()));
        draw_hud(surface, self.elapsed, self.shop_unlocked.then_some(self.ship.stars), self.cores, towers, if self.cleared { None } else { self.difficulty.goal });
        let mut top = 14;
        for n in &self.notices {
            top = draw_text_a(surface, &n.0, 36, n.1, py_round(255.0 * n.2.min(1.0)), Anchor::MidTop, surface.width() as f64 / 2.0, top).bottom() + 4;
        }
        let hint = draw_text_a(surface, "ESC to open menu", 22, TEXT, settings::HINT_ALPHA, Anchor::TopLeft, 10, 8);
        draw_text_a(surface, "TAB to open inventory", 22, TEXT, settings::HINT_ALPHA, Anchor::TopLeft, 10, hint.bottom() + 2);
    }

    /// Everything in the world, as seen from a window whose top left corner is at `offset` (screen pixels).
    fn draw_world(&self, surface: &mut Surface, offset: Vec2) {
        let area = Rect::new(offset.x, offset.y, surface.width(), surface.height());
        for g in &self.burning {
            g.draw(surface, offset);
        }
        for f in &self.slow_fields {
            f.draw(surface, offset);
        }
        self.points.draw(surface, offset, area);
        for s in &self.strikes {
            s.draw(surface, offset);
        }
        for b in &self.beams {
            b.borrow().draw(surface, offset);
        }
        for x in &self.explosions {
            x.draw(surface, offset);
        }
        for x in &self.blasts {
            x.draw(surface, offset);
        }
        for x in &self.sparks {
            x.draw(surface, offset);
        }
        for p in &self.pickups {
            p.draw(surface, offset);
        }
        for t in &self.towers {
            let t = t.borrow();
            if let Some(a) = t.area {
                buildings::draw_area(surface, a.moved(-offset.x, -offset.y));
            }
            t.draw(surface, offset);
        }
        let area_f = area.to_f();
        for en in &self.enemies {
            let en = en.borrow();
            if en.rect.colliderect(&area_f) {
                en.draw(surface, offset);
            }
        }
        for p in self.enemy_projectiles.iter().chain(self.projectiles.iter()) {
            p.draw(surface, offset);
        }
        for s in &self.strikes {
            s.draw_air(surface, offset); // shells in the air, over everything on the ground
        }
        self.ship.draw(surface, offset);
    }

    /// The border around all windows together – the arena and every extra window – as far as it lies in this window:
    /// where windows overlap or touch, their edges merge into one line. Kept until a window moves.
    fn draw_outline(&self, e: &Engine, surface: &mut Surface, offset: Vec2) {
        let width = settings::BORDER_WIDTH;
        let size = (surface.width() + 2 * width, surface.height() + 2 * width);
        let origin = (offset.x as i32 - width, offset.y as i32 - width);
        let field = self.field(e);
        let mut covered_rects = vec![];
        for r in &field.sight[..field.sight.len() - field.lookouts.len()] {
            // a Pusher's window has its own frame
            let local = r.moved(-origin.0, -origin.1).clip(&Rect::new(0, 0, size.0, size.1));
            if local.w != 0 && local.h != 0 {
                covered_rects.push(local);
            }
        }
        let key = format!("outline {size:?} {covered_rects:?}");
        let picture = sprite_cache::get(&key, || Some(outline(size, &covered_rects, width)));
        if let Some(p) = picture {
            surface.blit(&p, -width, -width);
        }
    }

    /// At the window's edge, where it would come in: a dot for every enemy the player cannot see, in its colour – a
    /// swarm gets one swarm icon instead, the bigger the closer it is – and an arrow for every point of interest.
    fn draw_offscreen_markers(&self, e: &Engine, surface: &mut Surface, offset: Vec2) {
        let inside = surface.get_rect().inflate(-2 * MARKER_INSET, -2 * MARKER_INSET);
        let field = self.field(e);
        let mut swarms: Vec<(Rc<super::enemies::Swarm>, Vec<Vec2>)> = vec![];
        for en in &self.enemies {
            let en = en.borrow();
            if field.sees_point(en.center()) {
                continue;
            }
            if let Some(s) = &en.swarm {
                match swarms.iter_mut().find(|(x, _)| Rc::ptr_eq(x, s)) {
                    Some(slot) => slot.1.push(en.center()),
                    None => swarms.push((s.clone(), vec![en.center()])),
                }
                continue;
            }
            let p = en.center() - offset;
            let marker = vec2(p.x.clamp(inside.left() as f64, inside.right() as f64), p.y.clamp(inside.top() as f64, inside.bottom() as f64));
            surface.circle(en.info.color, marker, MARKER_RADIUS, 0);
        }
        let swarm_inside = surface.get_rect().inflate(-2 * SWARM_MARKER_INSET, -2 * SWARM_MARKER_INSET);
        for (s, members) in swarms {
            let middle = members.iter().fold(Vec2::ZERO, |a, p| a + *p) / members.len() as f64;
            let distance = distance_to_rect(middle, &self.arena.rect.to_i());
            let near = 1.0 - (distance / SWARM_MARKER_FAR).min(1.0); // 1 at the window, 0 far away
            let size = SWARM_MARKER_SIZES.0 + (SWARM_MARKER_SIZES.1 - SWARM_MARKER_SIZES.0) * near;
            let p = middle - offset;
            let marker = vec2(p.x.clamp(swarm_inside.left() as f64, swarm_inside.right() as f64), p.y.clamp(swarm_inside.top() as f64, swarm_inside.bottom() as f64));
            let color = type_of(&s.kind).map(|t| t.color).unwrap_or(rgb(230, 40, 40));
            draw_swarm_icon(surface, marker, color, py_round(size));
        }
        self.points.draw_markers(surface, offset, &field);
    }
}

/// The first enemy (alive, not pierced yet) the shot touched on this tick's way – or `None`.
fn first_hit(p: &Projectile, enemies: &[EnemyRef], rects: &[FRect]) -> Option<EnemyRef> {
    let sweep = p.sweep();
    let mut best: Option<(f64, EnemyRef)> = None;
    for (en, r) in enemies.iter().zip(rects) {
        if !sweep.colliderect(r) {
            continue;
        }
        let e = en.borrow();
        if !e.alive() || p.pierced.contains(&e.id) || !p.touches(&e.rect) {
            continue;
        }
        let d = e.center().distance_squared_to(p.previous);
        if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, en.clone()));
        }
    }
    best.map(|(_, e)| e)
}

/// The border picture: every covered pixel that has an uncovered one at most `width` away (outside counts as
/// covered, as pygame's mask did).
fn outline(size: (i32, i32), covered: &[Rect], width: i32) -> Surface {
    let (w, h) = (size.0.max(1) as usize, size.1.max(1) as usize);
    let mut mask = vec![false; w * h];
    for r in covered {
        for y in r.top().max(0)..r.bottom().min(h as i32) {
            for x in r.left().max(0)..r.right().min(w as i32) {
                mask[y as usize * w + x as usize] = true;
            }
        }
    }
    // distance to the nearest uncovered pixel along rows, then columns (a square neighbourhood)
    let wd = width as usize;
    let mut near_row = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let lo = x.saturating_sub(wd);
            let hi = (x + wd).min(w - 1);
            near_row[y * w + x] = (lo..=hi).any(|i| !mask[y * w + i]);
        }
    }
    let mut out = Surface::new_alpha(w as i32, h as i32);
    for y in 0..h {
        let lo = y.saturating_sub(wd);
        let hi = (y + wd).min(h - 1);
        for x in 0..w {
            if mask[y * w + x] && (lo..=hi).any(|j| near_row[j * w + x]) {
                out.set_at(x as i32, y as i32, settings::BORDER);
            }
        }
    }
    out
}

impl Arena {
    /// The arena as the spawner gets it: whole pixels, cut off (pygame's FRect → Rect).
    pub fn window_rect_f(&self) -> Rect {
        self.rect.to_i()
    }
}

impl Scene for PlayScene {
    fn enter(&mut self, e: &mut Engine) {
        self.set_state(e, State::Playing);
    }

    fn handle_event(&mut self, e: &mut Engine, event: &Event) {
        self.key_overlay.handle_event(event);
        let playing = self.state == State::Playing;
        match *event {
            Event::KeyDown(k) if k == K_ESCAPE => {
                let next = if playing { State::Paused } else { State::Playing };
                self.set_state(e, next);
            }
            Event::KeyDown(k) if k == K_TAB && (playing || self.state == State::Inventory) => {
                let next = if playing { State::Inventory } else { State::Playing };
                self.set_state(e, next);
            }
            Event::KeyDown(k) if k == K_T && (playing || self.state == State::Shop) && self.shop_unlocked => {
                let next = if playing { State::Shop } else { State::Playing };
                self.set_state(e, next);
            }
            Event::FocusLost { .. } if playing => self.set_state(e, State::Paused),
            Event::MouseWheel { .. } if playing => self.ship.switch_weapon(),
            Event::MouseButtonDown { button, .. } if playing && button == settings::WEAPON_SWITCH_BUTTON => self.ship.switch_weapon(), // also while stunned
            Event::KeyDown(_) if playing && self.ship.stunned() => {} // stunned: no abilities, no towers
            Event::KeyDown(k) if playing => {
                if settings::BUILD_KEYS.contains(&k) {
                    self.throw_watchtower(e);
                }
                for (slot, keys) in settings::ABILITY_KEYS.iter().enumerate() {
                    if keys.contains(&k) {
                        let moving = Ship::move_direction(e);
                        let shots = self.ship.use_ability(slot, e.mouse_position(), moving);
                        self.projectiles.extend(shots);
                    }
                }
            }
            _ if self.state == State::Paused => match self.pause_menu.handle_event(event) {
                Some(PauseAction::Resume) => self.set_state(e, State::Playing),
                Some(PauseAction::GameMenu) => self.to_game_menu(e),
                Some(PauseAction::Quit) => e.stop(),
                None => {}
            },
            _ if self.state == State::Shop => self.shop_panel.handle_event(event, &mut self.shop, &mut self.ship),
            _ => {}
        }
    }

    fn update(&mut self, e: &mut Engine, dt: f64) {
        if self.state == State::Playing && !self.over {
            self.tick(e, dt);
        }
    }

    fn draw(&mut self, e: &mut Engine, surface: &mut Surface) {
        self.ui_layer = None;
        match self.state {
            State::Paused => return self.pause_menu.draw(surface),
            State::Inventory => return draw_inventory(surface, &self.ship),
            State::Shop => return self.shop_panel.draw(surface, &self.shop, &self.ship),
            State::Playing => {}
        }
        let offset = e.window_rect().topleftv(); // world → window: subtract the window's position
        self.arena.draw(surface, offset);
        self.draw_world(surface, offset);
        self.draw_ui(e, surface, offset);
        let window = Rect::new(offset.x, offset.y, surface.width(), surface.height());
        if self.field(e).sight[1..].iter().any(|r| window.colliderect(r)) {
            // the HUD etc. once more on a see-through picture – the extra windows put their part of it on top
            let mut layer = Surface::new_alpha(surface.width(), surface.height());
            layer.fill(rgba(0, 0, 0, 0));
            self.draw_ui(e, &mut layer, offset);
            self.ui_layer = Some((window, layer));
        }
    }

    /// An extra window (a watchtower's, the radar's, a lookout): the world at its rect. Almost black (a hint of grey,
    /// blue for the radar) – except where it lies over the arena. The border is the outline of all windows together;
    /// where it covers the main window, the main window's HUD etc. is drawn on top.
    fn draw_view(&mut self, e: &mut Engine, surface: &mut Surface, view: ViewId, rect: Rect) {
        let offset = rect.topleftv();
        let radar = self.radar.as_ref().is_some_and(|r| r.view == Some(view));
        surface.fill(if radar { RADAR_BACKGROUND } else { TOWER_BACKGROUND });
        surface.fill_rect(self.arena.window_rect().moved(-offset.x, -offset.y), settings::BACKGROUND);
        self.draw_outline(e, surface, offset);
        self.draw_world(surface, offset);
        if self.lookouts().contains(&view) {
            let r = surface.get_rect();
            surface.rect(LOOKOUT_BORDER, r, settings::BORDER_WIDTH);
        }
        if let Some((window, layer)) = &self.ui_layer {
            surface.blit(layer, window.left() - offset.x as i32, window.top() - offset.y as i32);
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}
