//! The tabs of the game menu: Levels (`level_panel.py`), Loadout (`loadout_panel.py`), Skill tree
//! (`skill_tree_panel.py`) and Cards (`card_shop.py`, `card_panel.py`). The game menu owns the profile; the panels
//! get it with every call.

use super::cards::{self, Part};
use super::difficulty::{self, Difficulty, Op, Phase, apply, evolution_chance};
use super::enemies::{draw_tier_frame, type_of};
use super::hud::clock as format_time;
use super::points::{FIND_COLOR, THREAT_COLOR, THREATS, name as point_name};
use super::profile::Profile;
use super::roster::evolution_of;
use super::settings::{BACKGROUND, HINT_ALPHA, TEXT, TEXT_MUTED};
use super::shapes::*;
use super::skill_tree::{self, BRANCHES, Node, branch, effect_text};
use super::weapons::{self, WEAPON_TYPES, unlocked};
use crate::engine::geom::py_round;
use crate::engine::gfx::Radii;
use crate::engine::ui::{draw_text, draw_text_a, text_size, wrap};
use crate::engine::window::Event;
use crate::engine::{Anchor, Color, Rect, Surface, Vec2, rgb, vec2};
use crate::pyfmt::{g, g_prec, plus_g};
use crate::rng;

const FILL: Color = rgb(24, 24, 30);
const HOVER_FILL: Color = rgb(34, 34, 42);
const LOCKED: Color = rgb(70, 70, 80);
const TOO_EXPENSIVE: Color = rgb(200, 70, 70);
const SELECTED_FILL: Color = rgb(255, 255, 255);
const LINE: Color = rgb(50, 50, 58);
pub const SUCCESS: Color = CORE_COLOR;

/// A message for the game menu's info line: text and colour.
pub type Message = Option<(String, Color)>;

fn percent(chance: f64) -> String {
    format!("{} %", g_prec(chance * 100.0, 3))
}

fn type_name(key: &str) -> &'static str {
    type_of(key).map(|t| t.name).unwrap_or("?")
}
fn type_color(key: &str) -> Color {
    type_of(key).map(|t| t.color).unwrap_or(rgb(230, 40, 40))
}

// ======================================================================================================
// Levels
// ======================================================================================================

/// Buttons in a row at most (C1 … C6 and Survival).
const COLUMNS: i32 = 7;
const BUTTON_HEIGHT: i32 = 74;
const GAP: i32 = 8;
const DETAILS_GAP: i32 = 18;
const ENEMY_ICON: f64 = 11.0;
const ENEMY_ICON_GAP: f64 = 3.0;
const EVENT_ICON: f64 = 13.0;
const EVENT_ICON_GAP: f64 = 7.0;
const DETAIL_ICON: f64 = 15.0;
const LINE_HEIGHT: i32 = 22;
const TIME_WIDTH: i32 = 104;
const LABEL_WIDTH: i32 = 70;
const HINT_WIDTH: i32 = 250;
const ENEMY_WIDTH: i32 = 330;
const COLUMN_GAP: i32 = 20;
/// Locked levels: their indicators this bright.
const DIM: f64 = 0.35;
const CLEARED: Color = rgb(110, 220, 110);
const DAMAGE_COLOR: Color = rgb(255, 90, 70);
const HP_COLOR: Color = rgb(110, 220, 110);
const WALL_COLOR: Color = rgb(175, 175, 190);
const NOTICE_COLOR: Color = rgb(255, 215, 60);
const DAMAGE_STATS: [&str; 3] = ["contact_damage", "projectile_damage", "blast_damage"];

/// Something that happens on a level besides single enemies coming.
#[derive(Debug, Clone, PartialEq)]
struct LevelEvent {
    time: f64,
    end: Option<f64>,
    /// swarm, wave, damage, hp, walls, notice, random, point
    kind: &'static str,
    text: String,
    color: Color,
    every: f64,
}

fn draw_event_icon(surface: &mut Surface, kind: &str, center: Vec2, color: Color, size: f64) {
    match kind {
        "swarm" => draw_swarm_icon(surface, center, color, size),
        "wave" => draw_wave_icon(surface, center, color, size),
        "damage" => draw_damage_icon(surface, center, color, size),
        "hp" => draw_hp_icon(surface, center, color, size),
        "walls" => draw_wall_icon(surface, center, color, size),
        "notice" => draw_notice_icon(surface, center, color, size),
        "random" => draw_random_icon(surface, center, color, size),
        _ => draw_pin_icon(surface, center, color, size),
    }
}

/// "Snipers", "Artilleries" – the type's name with an s (a y after a consonant becomes ies).
fn plural(key: &str) -> String {
    let name = type_name(key);
    let mut chars = name.chars().rev();
    if name.ends_with('y') && chars.nth(1).is_some_and(|c| !"aeiou".contains(c.to_ascii_lowercase())) {
        return format!("{}ies", &name[..name.len() - 1]);
    }
    format!("{name}s")
}

/// A phase's change as a short text: "×0.8", "+2 px/s", "= 3 s".
fn change_text(op: Op, number: f64, unit: &str) -> String {
    match op {
        Op::Multiply => format!("×{}", g(number)),
        Op::Add => format!("{}{unit}", plus_g(number)),
        Op::Set => format!("= {}{unit}", g(number)),
    }
}

/// The level's numbers at the start and where they go over time – arena size and cores with the player's skill tree.
fn numbers(level: &Difficulty, profile: &Profile) -> Vec<(String, String)> {
    let (arena, spawning) = (&level.arena, &level.spawning);
    let size = skill_tree::start_arena_size(profile, level);
    let mut walls = format!("{} px/s", g(arena.shrink_speed));
    if arena.shrink_growth != 0.0 && arena.max_shrink_speed > arena.shrink_speed {
        let reached = (arena.max_shrink_speed - arena.shrink_speed) / arena.shrink_growth * 60.0;
        walls = format!("{} to {} px/s by {}", g(arena.shrink_speed), g(arena.max_shrink_speed), format_time(reached));
    }
    let mut gap = format!("every {} s", g(spawning.start_interval));
    if spawning.interval_drop != 0.0 && spawning.min_interval < spawning.start_interval {
        let reached = (spawning.start_interval - spawning.min_interval) / spawning.interval_drop;
        gap = format!("{} to {} s by {}", g(spawning.start_interval), g(spawning.min_interval), format_time(reached));
    }
    vec![
        ("Arena".into(), format!("{} × {} px", g(size.0), g(size.1))),
        ("Walls".into(), walls),
        ("Spawns".into(), gap),
        ("First".into(), format!("after {} s", g(spawning.first_spawn))),
        ("Cores".into(), format!("{} per kill", percent(skill_tree::core_chance(profile, level)))),
    ]
}

/// (type key, first second it can come, note) for every type that can come on `level` – in order of appearance.
fn enemy_rows(level: &Difficulty) -> Vec<(String, f64, String)> {
    let mut first: Vec<(String, f64)> = vec![];
    let set_first = |first: &mut Vec<(String, f64)>, key: &str, after: f64| match first.iter_mut().find(|(k, _)| k == key) {
        Some(slot) => slot.1 = slot.1.min(after),
        None => first.push((key.to_string(), after)),
    };
    for e in &level.enemies {
        set_first(&mut first, &e.kind, e.after);
    }
    let mut notes: Vec<(String, String)> = vec![];
    for s in &level.swarms {
        set_first(&mut first, &s.kind, s.after);
        if let Some(l) = &s.leader {
            set_first(&mut first, l, s.after);
            let share = if s.leader_chance >= 1.0 { String::new() } else { format!("{} of ", percent(s.leader_chance)) };
            notes.retain(|(k, _)| k != l);
            notes.push((l.clone(), format!("leads {share}{} swarms", type_name(&s.kind))));
        }
    }
    let mut rows: Vec<(String, f64, String)> = first.iter().map(|(k, a)| (k.clone(), *a, notes.iter().find(|(n, _)| n == k).map(|(_, t)| t.clone()).unwrap_or_default())).collect();
    for e in &level.evolutions {
        let Some(tier1) = first.iter().find(|(k, _)| *k == e.kind).map(|(_, a)| *a) else { continue }; // its tier 1 type never comes
        let start = e.after.max(tier1);
        let low = evolution_chance(e, start);
        let high = if e.growth != 0.0 { e.max_chance } else { low };
        let chance = if high <= low { percent(low) } else { format!("{}–{}", g_prec(low * 100.0, 3), percent(high)) };
        rows.push((evolution_of(&e.kind).unwrap_or("fat").to_string(), start, format!("{chance} of {}", plural(&e.kind))));
    }
    rows.sort_by(|a, b| a.1.total_cmp(&b.1));
    rows
}

/// Everything that happens on `level` besides single enemies coming, in order of time.
fn events(level: &Difficulty) -> Vec<LevelEvent> {
    let mut found = vec![];
    let ev = |time, end, kind, text: String, color, every| LevelEvent { time, end, kind, text, color, every };
    if level.arena.armor != 0.0 {
        found.push(ev(0.0, None, "walls", format!("Armored walls ({} px)", g(level.arena.armor)), WALL_COLOR, 0.0));
    }
    let start_hp: f64 = level.enemy_stats.iter().filter(|(t, s, o, _)| t == "all" && s == "base_hp" && *o == Op::Add).map(|x| x.3).sum();
    if start_hp != 0.0 {
        found.push(ev(0.0, None, "hp", format!("All enemies {} HP", plus_g(start_hp)), HP_COLOR, 0.0));
    }
    for s in &level.swarms {
        found.push(ev(s.after, None, "swarm", format!("{} swarms", type_name(&s.kind)), type_color(&s.kind), 0.0));
    }
    for p in &level.phases {
        if !p.options.is_empty() {
            found.extend(option_events(p));
            continue;
        }
        let mut changes = phase_changes(p);
        if changes.is_empty() {
            if let Some(a) = &p.announce {
                changes = vec![("notice", a.clone(), NOTICE_COLOR)];
            }
        }
        for (i, (kind, text, color)) in changes.into_iter().enumerate() {
            // the phase's own announcement describes it best – for its first change
            let text = match (&p.announce, i) {
                (Some(a), 0) => a.clone(),
                _ => text,
            };
            found.push(ev(p.start, p.end, kind, text, color, p.every));
        }
    }
    found.sort_by(|a, b| a.time.total_cmp(&b.time));
    found
}

/// A phase with options: one event per option, with the die – what it brings and how likely it is.
fn option_events(p: &Phase) -> Vec<LevelEvent> {
    let total: f64 = p.options.iter().map(|(w, _)| *w).sum();
    p.options
        .iter()
        .map(|(w, o)| {
            let changes = phase_changes(o);
            let text = o.announce.clone().or_else(|| changes.first().map(|c| c.1.clone())).or_else(|| p.announce.clone()).unwrap_or_else(|| "nothing more".into());
            let color = changes.first().map(|c| c.2).unwrap_or(NOTICE_COLOR);
            LevelEvent { time: p.start, end: p.end, kind: "random", text: format!("{text} ({})", percent(w / total)), color, every: p.every }
        })
        .collect()
}

/// (kind, text, colour) – what a phase changes, one entry per kind.
fn phase_changes(p: &Phase) -> Vec<(&'static str, String, Color)> {
    let mut changes = vec![];
    let mut raised: Vec<String> = p.weights.iter().filter(|(_, o, n)| *o == Op::Set || (*o == Op::Multiply && *n > 1.0) || (*o == Op::Add && *n > 0.0)).map(|(k, _, _)| k.clone()).collect();
    for (k, _) in &p.spawn {
        if !raised.contains(k) {
            raised.push(k.clone());
        }
    }
    let faster = p.spawn_interval.is_some_and(|(o, n)| apply(1.0, o, n) < 1.0);
    let gap = p.spawn_interval.map(|(o, n)| change_text(o, n, " s")).unwrap_or_default();
    if !raised.is_empty() {
        let names: Vec<String> = raised.iter().map(|k| plural(k)).collect();
        let tail = if gap.is_empty() { String::new() } else { format!(", spawns {gap}") };
        changes.push(("wave", format!("More {}{tail}", names.join(" and ")), type_color(&raised[0])));
    } else if !gap.is_empty() {
        changes.push(("wave", format!("{} spawns ({gap})", if faster { "Faster" } else { "Slower" }), NOTICE_COLOR));
    }
    for (name, o, n) in &p.arena {
        let text = match name.as_str() {
            "armor" => format!("Walls harden ({})", change_text(*o, *n, " px")),
            "push_falloff" => format!("Walls get heavier ({})", change_text(*o, *n, "")),
            _ => format!("Walls close in {}", change_text(*o, *n, " px/s")),
        };
        changes.push(("walls", text, WALL_COLOR));
    }
    if p.breathe != 0.0 {
        changes.push(("walls", format!("Walls breathe in {} px, then spring back", g(p.breathe)), WALL_COLOR));
    }
    if !p.points.is_empty() {
        let names: Vec<String> = p.points.iter().map(|(k, c)| format!("{}{}", point_name(k), if *c > 1 { format!(" ×{c}") } else { String::new() })).collect();
        let threat = p.points.iter().any(|(k, _)| THREATS.contains(&k.as_str()));
        changes.push(("point", format!("{} out there", names.join(" and ")), if threat { THREAT_COLOR } else { FIND_COLOR }));
    }
    if p.enemy_stats.iter().any(|(_, s, _, _)| DAMAGE_STATS.contains(&s.as_str())) {
        changes.push(("damage", "Enemies hit harder".into(), DAMAGE_COLOR));
    }
    let hp: f64 = p.enemy_stats.iter().filter(|(_, s, o, _)| s == "base_hp" && *o == Op::Add).map(|x| x.3).sum();
    if hp != 0.0 {
        changes.push(("hp", format!("New enemies {} HP", plus_g(hp)), HP_COLOR));
    }
    changes
}

/// An enemy type as a small square in its colour – an evolution with its dark frame.
fn draw_enemy_icon(surface: &mut Surface, key: &str, center: Vec2, size: f64, factor: f64) {
    let mut r = Rect::new(0.0, 0.0, size, size);
    r.set_centerv(center);
    let color = shade(type_color(key), factor);
    surface.rect(color, r, 0);
    if type_of(key).is_some_and(|t| t.tier == 2) {
        draw_tier_frame(surface, r, color);
    }
}

pub struct LevelPanel {
    pub rect: Rect,
}

impl LevelPanel {
    pub fn new(rect: Rect) -> LevelPanel {
        LevelPanel { rect }
    }
    fn button_rects(&self) -> Vec<(String, Rect)> {
        let width = (self.rect.w - (COLUMNS - 1) * GAP) / COLUMNS;
        let (mut row, mut column, mut tens) = (0, 0, None);
        let mut rects = vec![];
        for level in difficulty::all() {
            let t = (level.order / 10.0).floor() as i64;
            if tens.is_some_and(|x| x != t) || (tens.is_some() && column == COLUMNS) {
                row += 1;
                column = 0;
            }
            tens = Some(t);
            rects.push((level.key.clone(), Rect::new(self.rect.left() + column * (width + GAP), self.rect.top() + row * (BUTTON_HEIGHT + GAP), width, BUTTON_HEIGHT)));
            column += 1;
        }
        rects
    }
    fn details_top(&self) -> i32 {
        self.button_rects().iter().map(|(_, r)| r.bottom()).max().unwrap_or(self.rect.top()) + DETAILS_GAP
    }
    /// The key of the level under the mouse – or `None`.
    pub fn thing_at(&self, x: i32, y: i32) -> Option<String> {
        self.button_rects().into_iter().find(|(_, r)| r.collidepoint(x, y)).map(|(k, _)| k)
    }
    /// `hovered`: the key of the level under the mouse (its details are shown) – or `None` (the chosen one's).
    pub fn draw(&self, surface: &mut Surface, profile: &Profile, hovered: Option<&str>) {
        for (key, r) in self.button_rects() {
            self.draw_button(surface, profile, &key, r, Some(key.as_str()) == hovered);
        }
        self.draw_details(surface, profile, hovered.unwrap_or(&profile.difficulty));
    }

    fn draw_button(&self, surface: &mut Surface, profile: &Profile, key: &str, r: Rect, hovered: bool) {
        let Some(level) = difficulty::get(key) else { return };
        let unlocked = profile.difficulty_unlocked(key);
        let selected = key == profile.difficulty;
        let text = if selected {
            surface.rect_round(SELECTED_FILL, r, 0, Radii::all(6));
            BACKGROUND
        } else {
            surface.rect_round(if hovered { HOVER_FILL } else { FILL }, r, 0, Radii::all(6));
            if unlocked { TEXT } else { LOCKED }
        };
        let border = if selected && hovered { CORE_COLOR } else if unlocked { TEXT_MUTED } else { LOCKED };
        if !selected || hovered {
            surface.rect_round(border, r, if selected { 3 } else { 2 }, Radii::all(6));
        }
        let size = if level.name.chars().count() <= 3 { 30 } else { 26 };
        let name = draw_text(surface, &level.name, size, text, Anchor::TopLeft, r.left() + 10, r.top() + 7);
        if profile.cleared(key) {
            draw_check(surface, vec2((r.right() - 16) as f64, name.centery() as f64), if selected { rgb(40, 150, 40) } else { CLEARED }, 14.0);
        } else if let Some(goal) = level.goal {
            draw_text_a(surface, &format_time(goal), 20, text, 170.0, Anchor::MidRight, r.right() - 10, name.centery());
        } else if !unlocked && name.right() + 8 + text_size("locked", 20).0 <= r.right() - 10 {
            draw_text(surface, "locked", 20, text, Anchor::MidRight, r.right() - 10, name.centery());
        }
        let factor = if unlocked { 1.0 } else { DIM };
        let mut x = r.left() as f64 + 10.0 + ENEMY_ICON / 2.0;
        for (enemy, _, _) in enemy_rows(&level) {
            if x > (r.right() - 10) as f64 {
                break;
            }
            draw_enemy_icon(surface, &enemy, vec2(x, (r.top() + 43) as f64), ENEMY_ICON, factor);
            x += ENEMY_ICON + ENEMY_ICON_GAP;
        }
        let mut x = r.left() as f64 + 10.0 + EVENT_ICON / 2.0;
        let mut shown: Vec<&str> = vec![];
        for ev in events(&level) {
            if shown.contains(&ev.kind) || x > (r.right() - 10) as f64 {
                continue;
            }
            shown.push(ev.kind);
            let color = if ev.kind == "swarm" && selected { WALL_COLOR } else { ev.color };
            draw_event_icon(surface, ev.kind, vec2(x, (r.top() + 60) as f64), shade(color, factor), EVENT_ICON);
            x += EVENT_ICON + EVENT_ICON_GAP;
        }
    }

    fn draw_details(&self, surface: &mut Surface, profile: &Profile, key: &str) {
        let Some(level) = difficulty::get(key) else { return };
        let (left, right, top) = (self.rect.left(), self.rect.right(), self.details_top());
        let ly = (top - DETAILS_GAP / 2) as f64;
        surface.line(TEXT_MUTED, vec2(left as f64, ly), vec2(right as f64, ly), 1);
        let name = draw_text(surface, &level.name, 34, TEXT, Anchor::TopLeft, left, top);
        draw_text(surface, &level.description, 22, TEXT_MUTED, Anchor::MidLeft, name.right() + 14, name.centery() + 2);
        draw_text(surface, &self.status(profile, key), 22, TEXT, Anchor::MidRight, right, name.centery() + 2);
        let top = name.bottom() + 14;
        let enemies_left = left + HINT_WIDTH + COLUMN_GAP;
        let events_left = enemies_left + ENEMY_WIDTH + COLUMN_GAP;
        let bottom = self.rect.bottom();
        let below = draw_hints(surface, &level, left, top, HINT_WIDTH, bottom);
        draw_numbers(surface, &level, profile, left, below + 12, bottom);
        draw_enemies(surface, &level, enemies_left, top, bottom);
        draw_events(surface, &level, events_left, top, right - events_left, bottom);
    }

    /// Goal, best time, cleared or what unlocks it – the line right of the level's name.
    fn status(&self, profile: &Profile, key: &str) -> String {
        let Some(level) = difficulty::get(key) else { return String::new() };
        let best = profile.best_time(key);
        let best_text = if best > 0.0 { format!("best {}", format_time(best)) } else { "not played yet".into() };
        if !profile.difficulty_unlocked(key) {
            let req = level.requires.as_deref().and_then(difficulty::get).map(|d| d.name.clone()).unwrap_or_default();
            return format!("locked – clear {req} first");
        }
        match level.goal {
            None => format!("endless  ·  {best_text}"),
            Some(_) if profile.cleared(key) => format!("cleared  ·  {best_text}"),
            Some(goal) => format!("goal: survive {}  ·  {best_text}", format_time(goal)),
        }
    }
}

fn heading(surface: &mut Surface, text: &str, left: i32, top: i32) -> i32 {
    draw_text(surface, text, 20, TEXT_MUTED, Anchor::TopLeft, left, top);
    top + 24
}

/// Returns where the next thing below can go.
fn draw_hints(surface: &mut Surface, level: &Difficulty, left: i32, top: i32, width: i32, bottom: i32) -> i32 {
    let mut y = heading(surface, "WHAT TO EXPECT", left, top);
    for hint in &level.hints {
        for (i, line) in wrap(hint, 21, width - 14).iter().enumerate() {
            if y + LINE_HEIGHT > bottom {
                return y;
            }
            if i == 0 {
                surface.circle(TEXT, vec2((left + 4) as f64, (y + 8) as f64), 3.0, 0);
            }
            draw_text(surface, line, 21, TEXT, Anchor::TopLeft, left + 14, y);
            y += LINE_HEIGHT - 2;
        }
        y += 4;
    }
    y
}

fn draw_numbers(surface: &mut Surface, level: &Difficulty, profile: &Profile, left: i32, top: i32, bottom: i32) {
    let mut y = heading(surface, "NUMBERS", left, top);
    for (label, value) in numbers(level, profile) {
        if y + LINE_HEIGHT > bottom {
            return;
        }
        draw_text(surface, &label, 19, TEXT_MUTED, Anchor::TopLeft, left, y + 1);
        draw_text(surface, &value, 20, TEXT, Anchor::TopLeft, left + LABEL_WIDTH, y);
        y += LINE_HEIGHT - 2;
    }
}

fn draw_enemies(surface: &mut Surface, level: &Difficulty, left: i32, top: i32, bottom: i32) {
    let mut y = heading(surface, "ENEMIES", left, top);
    let rows = enemy_rows(level);
    for (i, (key, after, note)) in rows.iter().enumerate() {
        if y + 2 * LINE_HEIGHT > bottom && i < rows.len() - 1 {
            draw_text(surface, &format!("+{} more", rows.len() - i), 20, TEXT_MUTED, Anchor::TopLeft, left, y);
            return;
        }
        draw_enemy_icon(surface, key, vec2(left as f64 + DETAIL_ICON / 2.0, (y + 8) as f64), DETAIL_ICON, 1.0);
        let name = draw_text(surface, type_name(key), 21, TEXT, Anchor::TopLeft, left + DETAIL_ICON as i32 + 10, y);
        let detail = format!("from {}{}", format_time(*after), if note.is_empty() { String::new() } else { format!("  ·  {note}") });
        draw_text(surface, &detail, 19, TEXT_MUTED, Anchor::MidLeft, name.right() + 10, name.centery() + 1);
        y += LINE_HEIGHT;
    }
}

/// One line per event – the same thing happening again later (no end, same text) shares a line: "01:30, 03:00".
fn draw_events(surface: &mut Surface, level: &Difficulty, left: i32, top: i32, width: i32, bottom: i32) {
    let mut y = heading(surface, "EVENTS", left, top);
    let mut lines: Vec<(LevelEvent, Vec<f64>)> = vec![];
    for ev in events(level) {
        if let Some(same) = lines.iter_mut().find(|(l, _)| ev.end.is_none() && l.end.is_none() && (l.kind, &l.text, l.every) == (ev.kind, &ev.text, ev.every)) {
            same.1.push(ev.time);
        } else {
            let t = ev.time;
            lines.push((ev, vec![t]));
        }
    }
    if lines.is_empty() {
        draw_text(surface, "none – just enemies", 21, TEXT_MUTED, Anchor::TopLeft, left, y);
    }
    let n = lines.len();
    for (i, (ev, times)) in lines.into_iter().enumerate() {
        if y + 2 * LINE_HEIGHT > bottom && i < n - 1 {
            draw_text(surface, &format!("+{} more", n - i), 20, TEXT_MUTED, Anchor::TopLeft, left, y);
            return;
        }
        draw_event_icon(surface, ev.kind, vec2(left as f64 + DETAIL_ICON / 2.0, (y + 8) as f64), ev.color, DETAIL_ICON);
        let when = match ev.end {
            Some(end) => format!("{}–{}", format_time(ev.time), format_time(end)),
            None => times.iter().take(2).map(|t| format_time(*t)).collect::<Vec<_>>().join(", ") + if times.len() > 2 { " …" } else { "" },
        };
        draw_text(surface, &when, 19, TEXT_MUTED, Anchor::TopLeft, left + DETAIL_ICON as i32 + 10, y + 1);
        let text_left = left + DETAIL_ICON as i32 + 10 + TIME_WIDTH;
        let mut text = ev.text.clone() + &if ev.every != 0.0 { format!(" – every {}", format_time(ev.every)) } else { String::new() };
        while text.chars().count() > 4 && text_size(&text, 21).0 > left + width - text_left {
            let keep: String = text.chars().take(text.chars().count() - 2).collect();
            text = keep + "…";
        }
        draw_text(surface, &text, 21, TEXT, Anchor::TopLeft, text_left, y);
        y += LINE_HEIGHT;
    }
}

// ======================================================================================================
// Skill tree
// ======================================================================================================

/// Pixels per grid unit (x, y).
const UNIT: (f64, f64) = (70.0, 72.0);
const RADIUS: f64 = 18.0;
/// The first node of a branch (an unlock: a weapon, an ability, …).
const BIG_RADIUS: f64 = 24.0;
const HIT_REACH: f64 = 6.0;
const LABEL_SIZE: i32 = 18;
const TOOLTIP_WIDTH: i32 = 280;
const TOOLTIP_PADDING: i32 = 12;
const TOOLTIP_GAP: f64 = 14.0;
const NODE_HOVER_FILL: Color = rgb(40, 40, 50);
const TOOLTIP_FILL: Color = rgb(12, 12, 16);
const TREE_LINE: Color = rgb(45, 45, 52);

/// E.g. "Gun: Damage" – the name alone is the same in many branches.
fn full_name(node: &Node) -> String {
    if is_big(node) || node.branch == "ship" { node.name.to_string() } else { format!("{}: {}", branch(node.branch).name, node.name) }
}

/// Drawn bigger and named in its branch's colour: the middle (Shop) and the first node of a branch that has no label
/// of its own.
fn is_big(node: &Node) -> bool {
    if node.requires.is_empty() {
        return true;
    }
    branch(node.branch).label_at.is_none() && node.requires.iter().all(|k| skill_tree::node(k).is_some_and(|n| n.branch != node.branch))
}

pub struct SkillTreePanel {
    pub rect: Rect,
}

impl SkillTreePanel {
    pub fn new(rect: Rect) -> SkillTreePanel {
        SkillTreePanel { rect }
    }
    /// A grid position's middle in window pixels.
    fn center(&self, p: (f64, f64)) -> Vec2 {
        vec2(self.rect.centerx() as f64 + p.0 * UNIT.0, self.rect.centery() as f64 + p.1 * UNIT.1)
    }
    fn radius(node: &Node) -> f64 {
        if is_big(node) { BIG_RADIUS } else { RADIUS }
    }
    /// The key of the node under the mouse, or `None`.
    pub fn thing_at(&self, profile: &Profile, x: i32, y: i32) -> Option<&'static str> {
        let p = vec2(x as f64, y as f64);
        self.shown(profile).into_iter().find(|n| self.center(n.position).distance_to(p) <= SkillTreePanel::radius(n) + HIT_REACH).map(|n| n.key)
    }
    fn shown(&self, profile: &Profile) -> Vec<std::rc::Rc<Node>> {
        skill_tree::nodes().into_iter().filter(|n| profile.visible(n)).collect()
    }

    pub fn draw(&self, surface: &mut Surface, profile: &Profile, hovered: Option<&str>) {
        let shown = self.shown(profile);
        for node in &shown {
            // lines first, so the circles cover their ends
            let from = node.lines_from.clone().unwrap_or_else(|| node.requires.clone());
            for key in from {
                if let Some(parent) = skill_tree::node(key) {
                    self.draw_line(surface, profile, &parent, node);
                }
            }
        }
        let branches: Vec<&str> = shown.iter().filter(|n| !n.requires.is_empty()).map(|n| n.branch).collect();
        for b in BRANCHES {
            if let (Some(at), true) = (b.label_at, branches.contains(&b.key)) {
                let c = self.center(at);
                draw_text(surface, &b.name.to_uppercase(), 18, shade(b.color, 0.8), Anchor::Center, c.x, c.y);
            }
        }
        for node in &shown {
            self.draw_node(surface, profile, node, Some(node.key) == hovered);
        }
        if let Some(n) = hovered.and_then(skill_tree::node) {
            self.draw_tooltip(surface, profile, &n);
        }
    }

    fn draw_line(&self, surface: &mut Surface, profile: &Profile, parent: &Node, node: &Node) {
        let (a, b) = (self.center(parent.position), self.center(node.position));
        let done = if node.lines_from.is_some() {
            profile.unlocked(node) // it needs more than its lines show: lit once all is there
        } else if node.requires_max {
            profile.maxed(parent)
        } else {
            profile.owns(parent.key)
        };
        let color = if done { shade(branch(node.branch).color, if profile.owns(node.key) { 0.9 } else { 0.45 }) } else { TREE_LINE };
        surface.line(color, a, b, 2);
    }

    fn draw_node(&self, surface: &mut Surface, profile: &Profile, node: &Node, hovered: bool) {
        let color = branch(node.branch).color;
        let (center, radius) = (self.center(node.position), SkillTreePanel::radius(node));
        let level = profile.level(node.key);
        let (fill, ring, text) = if profile.maxed(node) {
            (shade(color, 0.55), color, TEXT)
        } else if !profile.unlocked(node) {
            (FILL, LOCKED, LOCKED)
        } else if level != 0 {
            (FILL, color, TEXT)
        } else {
            (FILL, if profile.can_buy(node) { TEXT } else { TEXT_MUTED }, TEXT)
        };
        surface.circle(if hovered && fill == FILL { NODE_HOVER_FILL } else { fill }, center, radius, 0);
        surface.circle(ring, center, radius, if hovered { 3 } else { 2 });
        if node.max_level() > 1 {
            draw_text(surface, &format!("{level}/{}", node.max_level()), 18, text, Anchor::Center, center.x, center.y);
        } else if level != 0 {
            surface.circle(TEXT, center, 4.0, 0); // bought: a dot in the middle
        }
        let big = is_big(node);
        let label_color = if big && profile.unlocked(node) { color } else if text == LOCKED { text } else { TEXT_MUTED };
        draw_text(surface, node.name, if big { 22 } else { LABEL_SIZE }, label_color, Anchor::MidTop, center.x, center.y + radius + 3.0);
    }

    /// Name, branch and level, the description and what one level adds, then what it costs (or needs).
    fn draw_tooltip(&self, surface: &mut Surface, profile: &Profile, node: &Node) {
        let b = branch(node.branch);
        let width = TOOLTIP_WIDTH - 2 * TOOLTIP_PADDING;
        let lines = wrap(&node.description, 20, width);
        let effect = wrap(&effect_text(node).unwrap_or_default(), 20, width);
        let level = profile.level(node.key);
        let cost = profile.next_cost(node);
        let missing: Vec<String> = profile.missing(node).iter().filter_map(|k| skill_tree::node(k)).map(|n| full_name(&n) + if node.requires_max { " (max)" } else { "" }).collect();
        let needs = if !missing.is_empty() && cost.is_some() { wrap(&format!("needs {}", missing.join(", ")), 20, width) } else { vec![] };
        let height = TOOLTIP_PADDING * 2 + 26 + 22 + (lines.len() + effect.len()) as i32 * 19 + 8 + 22 + (needs.len() as i32 - 1).max(0) * 19;
        let (center, radius) = (self.center(node.position), SkillTreePanel::radius(node));
        let mut bx = Rect::new(0, 0, TOOLTIP_WIDTH, height);
        bx.set_midleft(center.x + radius + TOOLTIP_GAP, center.y);
        if bx.right() > surface.width() - 8 {
            bx.set_midright(center.x - radius - TOOLTIP_GAP, center.y); // no room on the right: to the left
        }
        bx.clamp_ip(&surface.get_rect().inflate(-16, -16));
        surface.rect_round(TOOLTIP_FILL, bx, 0, Radii::all(8));
        surface.rect_round(b.color, bx, 2, Radii::all(8));
        let (left, mut y) = (bx.left() + TOOLTIP_PADDING, bx.top() + TOOLTIP_PADDING);
        draw_text(surface, node.name, 26, TEXT, Anchor::TopLeft, left, y);
        if node.max_level() > 1 {
            draw_text(surface, &format!("{level}/{}", node.max_level()), 22, TEXT, Anchor::TopRight, bx.right() - TOOLTIP_PADDING, y + 2);
        }
        y += 26;
        draw_text(surface, b.name, 18, b.color, Anchor::TopLeft, left, y);
        y += 22;
        for l in &lines {
            draw_text(surface, l, 20, TEXT_MUTED, Anchor::TopLeft, left, y);
            y += 19;
        }
        for l in &effect {
            draw_text(surface, l, 20, TEXT, Anchor::TopLeft, left, y);
            y += 19;
        }
        y += 8;
        match cost {
            None => {
                draw_text(surface, if node.max_level() == 1 { "OWNED" } else { "MAX" }, 20, CORE_COLOR, Anchor::TopLeft, left, y);
            }
            Some(_) if !needs.is_empty() => {
                for l in &needs {
                    draw_text(surface, l, 20, TOO_EXPENSIVE, Anchor::TopLeft, left, y);
                    y += 19;
                }
            }
            Some(cost) => {
                draw_core(surface, vec2((left + 7) as f64, (y + 8) as f64), 8.0, CORE_COLOR);
                let enough = profile.cores >= cost;
                let price = draw_text(surface, &cost.to_string(), 24, if enough { TEXT } else { TOO_EXPENSIVE }, Anchor::TopLeft, left + 20, y - 1);
                draw_text(surface, if enough { "click to buy" } else { "not enough cores" }, 18, TEXT_MUTED, Anchor::TopLeft, price.right() + 10, y + 2);
            }
        }
    }
}

// ======================================================================================================
// Cards
// ======================================================================================================

/// The card shop in the game menu (`card_shop.py`): offers bought with cores, rerolls, the collection, which cards
/// are active.
pub struct CardShop {
    pub offers: Vec<Option<String>>,
}

impl CardShop {
    pub fn new(profile: &mut Profile) -> CardShop {
        let rules = cards::rules();
        let mut offers: Vec<Option<String>> = profile.card_offers.iter().take(rules.offers as usize).cloned().collect();
        while offers.len() < rules.offers as usize {
            offers.push(None);
        }
        let mut shop = CardShop { offers };
        if shop.offers.iter().all(|o| o.is_none()) && profile.card_offers.is_empty() {
            shop.roll(profile); // the very first visit: free offers
            shop.save(profile);
        }
        shop
    }
    /// Cores for the next card: more for every card already in the collection.
    pub fn price(&self, profile: &Profile) -> i64 {
        let r = cards::rules();
        r.first_price + r.price_step * profile.cards.len() as i64
    }
    pub fn full(&self, profile: &Profile) -> bool {
        profile.cards.len() as i64 >= cards::rules().max_owned
    }
    /// Card keys worth offering, without `exclude`: weapon cards for every unlocked weapon, the others if they would
    /// change something on the next run's ship.
    fn candidates(&self, profile: &Profile, exclude: &[String]) -> Vec<String> {
        let ship = skill_tree::start_ship(profile, Vec2::ZERO);
        cards::all()
            .into_iter()
            .filter(|c| !exclude.contains(&c.key))
            .filter(|c| if c.kind.part == Part::Weapon { unlocked(profile, c.weapon.unwrap_or("gun")) } else { c.available(&ship) })
            .map(|c| c.key)
            .collect()
    }
    /// New random offers, all different (free – for the first visit and by `reroll`).
    fn roll(&mut self, profile: &Profile) {
        let keys = self.candidates(profile, &[]);
        let n = cards::rules().offers as usize;
        let picked: Vec<Option<String>> = rng::sample(keys.len(), n.min(keys.len())).into_iter().map(|i| Some(keys[i].clone())).collect();
        self.offers = picked;
        while self.offers.len() < n {
            self.offers.push(None);
        }
    }
    /// Pays reroll_price for new offers. "" if it worked, else why not.
    pub fn reroll(&mut self, profile: &mut Profile) -> String {
        let price = cards::rules().reroll_price;
        if profile.cores < price {
            return "Not enough cores".into();
        }
        profile.cores -= price;
        self.roll(profile);
        self.save(profile);
        String::new()
    }
    /// Buys offer number `index` into the collection.
    pub fn buy(&mut self, profile: &mut Profile, index: usize) -> String {
        let Some(key) = self.offers[index].clone() else { return "Sold out – reroll for new cards".into() };
        if self.full(profile) {
            return "Collection full – right-click a card to sell it".into();
        }
        let price = self.price(profile);
        if profile.cores < price {
            return "Not enough cores".into();
        }
        profile.cores -= price;
        profile.cards.push((key, false));
        let others: Vec<String> = self.offers.iter().enumerate().filter(|(i, o)| o.is_some() && *i != index).map(|(_, o)| o.clone().unwrap()).collect();
        let keys = self.candidates(profile, &others);
        self.offers[index] = rng::index(keys.len()).map(|i| keys[i].clone());
        self.save(profile);
        String::new()
    }
    pub fn active_count(&self, profile: &Profile) -> i64 {
        profile.cards.iter().filter(|(_, a)| *a).count() as i64
    }
    /// Makes card number `index` active or inactive.
    pub fn toggle(&mut self, profile: &mut Profile, index: usize) -> String {
        let limit = profile.max_active_cards();
        if !profile.cards[index].1 && self.active_count(profile) >= limit {
            let slots = skill_tree::node(skill_tree::CARD_SLOTS);
            let more = if slots.is_some_and(|s| profile.maxed(&s)) { "" } else { " or buy Card slots in the skill tree" };
            return format!("Only {limit} {} can be active – deactivate one first{more}", if limit == 1 { "card" } else { "cards" });
        }
        profile.cards[index].1 = !profile.cards[index].1;
        self.save(profile);
        String::new()
    }
    /// Sells card number `index` for sell_refund cores.
    pub fn sell(&mut self, profile: &mut Profile, index: usize) {
        profile.cards.remove(index);
        profile.cores += cards::rules().sell_refund;
        self.save(profile);
    }
    fn save(&self, profile: &mut Profile) {
        profile.card_offers = self.offers.clone();
        profile.save();
    }
}

const OFFER_SIZE: (i32, i32) = (150, 118);
const OFFER_GAP: i32 = 16;
const GRID_COLUMNS: i32 = 6;
const SLOT_SIZE: (i32, i32) = (94, 56);
const SLOT_GAP: i32 = 10;
const COLLECTION_GAP: i32 = 28;
const EMPTY: Color = rgb(50, 50, 58);

/// What is under the mouse in the Cards tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardThing {
    Offer(usize),
    Reroll,
    Owned(usize),
}

/// `text`, cut with "…" so it is at most `width` pixels wide.
fn fit(text: &str, size: i32, width: i32) -> String {
    if text_size(text, size).0 <= width {
        return text.to_string();
    }
    let mut t: String = text.to_string();
    while !t.is_empty() && text_size(&(t.clone() + "…"), size).0 > width {
        t.pop();
    }
    t + "…"
}

pub struct CardPanel {
    pub shop: CardShop,
    left: i32,
    top: i32,
    width: i32,
}

impl CardPanel {
    pub fn new(shop: CardShop, left: i32, top: i32, width: i32) -> CardPanel {
        CardPanel { shop, left, top, width }
    }
    fn offer_rects(&self) -> Vec<Rect> {
        (0..self.shop.offers.len() as i32).map(|i| Rect::new(self.left + i * (OFFER_SIZE.0 + OFFER_GAP), self.top, OFFER_SIZE.0, OFFER_SIZE.1)).collect()
    }
    fn reroll_rect(&self) -> Rect {
        let left = self.left + self.shop.offers.len() as i32 * (OFFER_SIZE.0 + OFFER_GAP);
        Rect::new(left, self.top, self.left + self.width - left, OFFER_SIZE.1)
    }
    fn collection_top(&self) -> i32 {
        self.top + OFFER_SIZE.1 + COLLECTION_GAP
    }
    fn slot_rects(&self) -> Vec<Rect> {
        let top = self.collection_top() + 24;
        (0..cards::rules().max_owned as i32).map(|i| Rect::new(self.left + (i % GRID_COLUMNS) * (SLOT_SIZE.0 + SLOT_GAP), top + (i / GRID_COLUMNS) * (SLOT_SIZE.1 + SLOT_GAP), SLOT_SIZE.0, SLOT_SIZE.1)).collect()
    }
    pub fn thing_at(&self, profile: &Profile, x: i32, y: i32) -> Option<CardThing> {
        if let Some(i) = self.offer_rects().iter().position(|r| r.collidepoint(x, y)) {
            return Some(CardThing::Offer(i));
        }
        if self.reroll_rect().collidepoint(x, y) {
            return Some(CardThing::Reroll);
        }
        self.slot_rects().iter().take(profile.cards.len()).position(|r| r.collidepoint(x, y)).map(CardThing::Owned)
    }

    /// A left click.
    pub fn click(&mut self, profile: &mut Profile, thing: &CardThing) -> Message {
        let report = |problem: String, success: String| if problem.is_empty() { Some((success, SUCCESS)) } else { Some((problem, TOO_EXPENSIVE)) };
        match *thing {
            CardThing::Reroll => {
                let p = self.shop.reroll(profile);
                report(p, "New offers".into())
            }
            CardThing::Offer(i) => {
                let name = self.shop.offers[i].as_deref().and_then(cards::get).map(|c| format!("{} {}", c.full_name(), c.effect())).unwrap_or_default();
                let p = self.shop.buy(profile, i);
                report(p, format!("{name} added to your cards"))
            }
            CardThing::Owned(i) => {
                let was = profile.cards[i].1;
                let p = self.shop.toggle(profile, i);
                report(p, if !was { "Active".into() } else { "Inactive".into() })
            }
        }
    }

    /// For a right click: (title, the "Sell" label) – `None` if it has no menu.
    pub fn menu_items(&self, profile: &Profile, thing: &CardThing) -> Option<(String, String)> {
        let CardThing::Owned(i) = *thing else { return None };
        let card = cards::get(&profile.cards[i].0)?;
        let refund = cards::rules().sell_refund;
        Some((format!("{} {}", card.full_name(), card.effect()), format!("Sell (+{refund} core{})", if refund == 1 { "" } else { "s" })))
    }
    pub fn sell(&mut self, profile: &mut Profile, index: usize) -> Message {
        let card = cards::get(&profile.cards[index].0)?;
        self.shop.sell(profile, index);
        Some((format!("Sold {} (+{} cores)", card.full_name(), cards::rules().sell_refund), SUCCESS))
    }

    /// The line under the cards for what the mouse is on.
    pub fn info(&self, profile: &Profile, thing: &CardThing) -> Option<String> {
        match *thing {
            CardThing::Reroll => Some(format!("New offers for {} cores", cards::rules().reroll_price)),
            CardThing::Offer(i) => self.shop.offers[i].as_deref().and_then(cards::get).map(|c| format!("{} {}  ·  active cards apply at the start of every run", c.full_name(), c.effect())),
            CardThing::Owned(i) => {
                let (key, active) = &profile.cards[i];
                let c = cards::get(key)?;
                let toggle = if *active { "deactivate" } else { "activate" };
                Some(format!("{} {}{}  ·  left click: {toggle}  ·  right click: more (sell)", c.full_name(), c.effect(), not_equipped(&c, profile)))
            }
        }
    }

    pub fn draw(&self, surface: &mut Surface, profile: &Profile, hovered: Option<&CardThing>) {
        for (i, r) in self.offer_rects().into_iter().enumerate() {
            self.draw_offer(surface, profile, r, self.shop.offers[i].clone(), hovered == Some(&CardThing::Offer(i)));
        }
        self.draw_reroll(surface, profile, self.reroll_rect(), hovered == Some(&CardThing::Reroll));
        let rules = cards::rules();
        let title = draw_text(surface, &format!("YOUR CARDS  {}/{}", profile.cards.len(), rules.max_owned), 22, TEXT_MUTED, Anchor::TopLeft, self.left, self.collection_top());
        draw_text(surface, &format!("active {}/{}", self.shop.active_count(profile), profile.max_active_cards()), 22, CORE_COLOR, Anchor::MidLeft, title.right() + 18, title.centery());
        for (i, r) in self.slot_rects().into_iter().enumerate() {
            if i < profile.cards.len() {
                self.draw_owned(surface, r, &profile.cards[i], hovered == Some(&CardThing::Owned(i)));
            } else {
                surface.rect_round(EMPTY, r, 1, Radii::all(6));
            }
        }
    }

    fn draw_offer(&self, surface: &mut Surface, profile: &Profile, r: Rect, key: Option<String>, hovered: bool) {
        surface.rect_round(if hovered && key.is_some() { HOVER_FILL } else { FILL }, r, 0, Radii::all(8));
        let Some(card) = key.as_deref().and_then(cards::get) else {
            surface.rect_round(EMPTY, r, 1, Radii::all(8));
            let c = r.center();
            draw_text_a(surface, "sold out", 22, TEXT, HINT_ALPHA, Anchor::Center, c.0, c.1);
            return;
        };
        let color = card.color();
        surface.rect_round(color, r, if hovered { 3 } else { 1 }, Radii::all(8));
        let band = Rect::new(r.left(), r.top(), r.w, 22);
        surface.rect_round(color, band, 0, Radii { tl: 8, tr: 8, ..Radii::default() });
        let bc = band.center();
        draw_text(surface, &card.label().to_uppercase(), 18, BACKGROUND, Anchor::Center, bc.0, bc.1);
        draw_text(surface, &fit(card.kind.name, 22, r.w - 12), 22, TEXT, Anchor::MidTop, r.centerx(), r.top() + 30);
        draw_text(surface, &card.effect(), 34, color, Anchor::MidTop, r.centerx(), r.top() + 52);
        let price = self.shop.price(profile);
        let affordable = profile.cores >= price && !self.shop.full(profile);
        draw_cores(surface, price, (r.centerx(), r.bottom() - 16), if affordable { TEXT } else { TOO_EXPENSIVE });
    }

    fn draw_reroll(&self, surface: &mut Surface, profile: &Profile, r: Rect, hovered: bool) {
        if hovered {
            surface.rect_round(HOVER_FILL, r, 0, Radii::all(8));
        }
        surface.rect_round(TEXT_MUTED, r, 2, Radii::all(8));
        draw_text(surface, "Reroll", 26, TEXT, Anchor::Center, r.centerx(), r.centery() - 14);
        let price = cards::rules().reroll_price;
        draw_cores(surface, price, (r.centerx(), r.centery() + 18), if profile.cores >= price { TEXT } else { TOO_EXPENSIVE });
    }

    fn draw_owned(&self, surface: &mut Surface, r: Rect, owned: &(String, bool), hovered: bool) {
        let Some(card) = cards::get(&owned.0) else { return };
        let active = owned.1;
        let color = card.color();
        let fill = if active { shade(color, 0.28) } else if hovered { HOVER_FILL } else { FILL };
        surface.rect_round(fill, r, 0, Radii::all(6));
        let border = if active || hovered { color } else { shade(color, 0.55) };
        surface.rect_round(border, r, if active { 3 } else { 1 }, Radii::all(6));
        // long names a bit smaller
        let mut size = 17;
        while size > 13 && text_size(card.kind.name, size).0 > r.w - 22 {
            size -= 1;
        }
        let name = fit(card.kind.name, size, r.w - 22);
        draw_text(surface, &name, size, TEXT, Anchor::MidLeft, r.left() + 6, r.top() + 11);
        draw_text(surface, &card.effect(), 24, if active { TEXT } else { color }, Anchor::MidBottom, r.centerx(), r.bottom() - 5);
        if active {
            surface.circle(CORE_COLOR, vec2((r.right() - 9) as f64, (r.top() + 10) as f64), 4.0, 0);
        }
    }
}

/// "  ·  no effect: … isn't in a slot" for a weapon card whose weapon is not in the loadout, else "".
fn not_equipped(card: &cards::Card, profile: &Profile) -> String {
    match card.weapon {
        Some(w) if !profile.loadout.iter().any(|s| s.as_deref() == Some(w)) => format!("  ·  no effect: the {} isn't in a slot", card.label()),
        _ => String::new(),
    }
}

/// A core icon and a number, centred on `center`.
fn draw_cores(surface: &mut Surface, amount: i64, center: (i32, i32), color: Color) {
    let text = amount.to_string();
    let width = 20 + text_size(&text, 26).0;
    let left = center.0 as f64 - width as f64 / 2.0;
    draw_core(surface, vec2(left + 7.0, (center.1 - 1) as f64), 8.0, CORE_COLOR);
    draw_text(surface, &text, 26, color, Anchor::MidLeft, left + 20.0, center.1);
}

// ======================================================================================================
// Loadout
// ======================================================================================================

const CARD_SIZE: (i32, i32) = (210, 112);
const CARD_GAP: i32 = 16;
const CARD_COLUMNS: i32 = 2;
/// The slots start this far right of the tab's left edge.
const SLOTS_LEFT: i32 = 480;
const LSLOT_SIZE: (i32, i32) = (228, 230);
const LSLOT_GAP: i32 = 16;
const SLOT_KEYS: [&str; 2] = ["Slot 1", "Slot 2"];
/// Pixels the mouse has to move before a press becomes a drag.
const DRAG_START: f64 = 5.0;
const DROP_HIGHLIGHT: Color = rgb(90, 225, 255);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadoutThing {
    Weapon(&'static str),
    Slot(usize),
}

struct Drag {
    key: String,
    source: Option<usize>,
    start: (i32, i32),
}

pub struct LoadoutPanel {
    rect: Rect,
    drag: Option<Drag>,
    /// The mouse has moved far enough: it is a real drag (the card follows the mouse).
    dragging: bool,
    mouse: (i32, i32),
}

impl LoadoutPanel {
    pub fn new(rect: Rect) -> LoadoutPanel {
        LoadoutPanel { rect, drag: None, dragging: false, mouse: (0, 0) }
    }
    fn card_rects(&self) -> Vec<(&'static str, Rect)> {
        let top = self.rect.top() + 28;
        WEAPON_TYPES.iter().enumerate().map(|(i, w)| (w.0, Rect::new(self.rect.left() + (i as i32 % CARD_COLUMNS) * (CARD_SIZE.0 + CARD_GAP), top + (i as i32 / CARD_COLUMNS) * (CARD_SIZE.1 + CARD_GAP), CARD_SIZE.0, CARD_SIZE.1))).collect()
    }
    fn slot_rects(&self) -> Vec<Rect> {
        (0..2).map(|i| Rect::new(self.rect.left() + SLOTS_LEFT + i * (LSLOT_SIZE.0 + LSLOT_GAP), self.rect.top() + 28, LSLOT_SIZE.0, LSLOT_SIZE.1)).collect()
    }
    pub fn thing_at(&self, x: i32, y: i32) -> Option<LoadoutThing> {
        if let Some((k, _)) = self.card_rects().into_iter().find(|(_, r)| r.collidepoint(x, y)) {
            return Some(LoadoutThing::Weapon(k));
        }
        self.slot_rects().iter().position(|r| r.collidepoint(x, y)).map(LoadoutThing::Slot)
    }

    /// Mouse input for the loadout: (used, message).
    pub fn handle_event(&mut self, profile: &mut Profile, event: &Event) -> (bool, Message) {
        match *event {
            Event::MouseMotion { x, y } => {
                self.mouse = (x, y);
                if let (Some(d), false) = (&self.drag, self.dragging) {
                    self.dragging = vec2(d.start.0 as f64, d.start.1 as f64).distance_to(vec2(x as f64, y as f64)) >= DRAG_START;
                }
                (false, None)
            }
            Event::MouseButtonDown { button: 1, x, y } => {
                match self.thing_at(x, y) {
                    Some(LoadoutThing::Weapon(k)) => {
                        if !unlocked(profile, k) {
                            return (true, Some((format!("Unlock the {} in the skill tree first", weapons::name_of(k)), TOO_EXPENSIVE)));
                        }
                        self.drag = Some(Drag { key: k.into(), source: None, start: (x, y) });
                    }
                    Some(LoadoutThing::Slot(i)) if profile.loadout[i].is_some() => {
                        self.drag = Some(Drag { key: profile.loadout[i].clone().unwrap(), source: Some(i), start: (x, y) });
                    }
                    _ => return (false, None),
                }
                self.dragging = false;
                (true, None)
            }
            Event::MouseButtonUp { button: 1, x, y } if self.drag.is_some() => {
                let drag = self.drag.take().unwrap();
                let dragging = std::mem::replace(&mut self.dragging, false);
                let msg = if dragging {
                    self.drop(profile, &drag, self.thing_at(x, y))
                } else if drag.source.is_none() {
                    self.put_in_free_slot(profile, &drag.key)
                } else {
                    None
                };
                (true, msg)
            }
            _ => (false, None),
        }
    }

    /// A dragged weapon was let go over `target`.
    fn drop(&self, profile: &mut Profile, drag: &Drag, target: Option<LoadoutThing>) -> Message {
        let mut loadout = profile.loadout.clone();
        let mut source = drag.source;
        match target {
            Some(LoadoutThing::Slot(slot)) => {
                if source.is_none() {
                    source = loadout.iter().position(|s| s.as_deref() == Some(drag.key.as_str())); // already in a slot: swap
                }
                match source {
                    Some(src) => loadout.swap(src, slot),
                    None => loadout[slot] = Some(drag.key.clone()),
                }
            }
            _ => match source {
                Some(src) => loadout[src] = None, // dragged off its slot: empty it
                None => return None,
            },
        }
        set_loadout(profile, loadout)
    }

    fn put_in_free_slot(&self, profile: &mut Profile, key: &str) -> Message {
        let mut loadout = profile.loadout.clone();
        if loadout.iter().any(|s| s.as_deref() == Some(key)) {
            return Some(("Drag it onto the other slot to swap".into(), TEXT_MUTED));
        }
        let Some(free) = loadout.iter().position(|s| s.is_none()) else {
            return Some(("Both slots are full – drag it onto one to replace it".into(), TEXT_MUTED));
        };
        loadout[free] = Some(key.into());
        set_loadout(profile, loadout)
    }

    pub fn draw(&self, surface: &mut Surface, profile: &Profile, hovered: Option<&LoadoutThing>) {
        let (left, top) = self.rect.topleft();
        draw_text(surface, "WEAPONS", 22, TEXT_MUTED, Anchor::TopLeft, left, top);
        for (k, r) in self.card_rects() {
            self.draw_card(surface, profile, k, r, hovered == Some(&LoadoutThing::Weapon(k)), false);
        }
        let ship = skill_tree::start_ship(profile, Vec2::ZERO); // the next run's energy (cards included)
        draw_text(surface, "SLOTS", 22, TEXT_MUTED, Anchor::TopLeft, left + SLOTS_LEFT, top);
        let target = if self.dragging { self.thing_at(self.mouse.0, self.mouse.1) } else { None };
        for (i, r) in self.slot_rects().into_iter().enumerate() {
            self.draw_slot(surface, profile, i, r, target == Some(LoadoutThing::Slot(i)), hovered == Some(&LoadoutThing::Slot(i)));
        }
        self.draw_energy(surface, &ship, self.slot_rects()[0].bottom() + 18);
        let details: Option<String> = match (&self.drag, hovered) {
            (Some(d), _) => Some(d.key.clone()),
            (None, Some(LoadoutThing::Weapon(k))) => Some(k.to_string()),
            _ => None,
        };
        if let Some(k) = details {
            self.draw_details(surface, profile, &k);
        }
        if let (true, Some(d)) = (self.dragging, &self.drag) {
            let mut r = Rect::new(0, 0, CARD_SIZE.0, CARD_SIZE.1);
            r.set_center(self.mouse.0, self.mouse.1);
            let key = WEAPON_TYPES.iter().find(|w| w.0 == d.key).map(|w| w.0).unwrap_or("gun");
            self.draw_card(surface, profile, key, r, true, true);
        }
    }

    fn draw_card(&self, surface: &mut Surface, profile: &Profile, key: &'static str, r: Rect, hovered: bool, floating: bool) {
        let available = unlocked(profile, key);
        let color = if available { weapons::color_of(key) } else { LOCKED };
        surface.rect_round(if hovered { HOVER_FILL } else { FILL }, r, 0, Radii::all(8));
        surface.rect_round(color, r, if hovered { 3 } else { 1 }, Radii::all(8));
        surface.rect_round(color, Rect::new(r.left(), r.top(), 5, r.h), 0, Radii { tl: 8, bl: 8, ..Radii::default() });
        draw_text(surface, weapons::name_of(key), 26, if available { TEXT } else { LOCKED }, Anchor::TopLeft, r.left() + 16, r.top() + 12);
        if !available {
            draw_text(surface, "locked – skill tree", 20, LOCKED, Anchor::TopLeft, r.left() + 16, r.top() + 42);
            return;
        }
        if let (Some(slot), false) = (profile.loadout.iter().position(|s| s.as_deref() == Some(key)), floating) {
            draw_text(surface, SLOT_KEYS[slot], 18, TEXT, Anchor::TopRight, r.right() - 12, r.top() + 14);
        }
        let words: Vec<&str> = weapons::description_of(key).split_whitespace().collect();
        let half = words.len().div_ceil(2);
        for (line, text) in [words[..half].join(" "), words[half..].join(" ")].iter().enumerate() {
            draw_text(surface, text, 19, TEXT_MUTED, Anchor::TopLeft, r.left() + 16, r.top() + 42 + line as i32 * 19);
        }
        let w = skill_tree::build_weapon(profile, key);
        draw_text(surface, &format!("{} energy/s while firing", g_prec(w.energy_cost * w.fire_rate, 3)), 18, color, Anchor::BottomLeft, r.left() + 16, r.bottom() - 10);
    }

    fn draw_slot(&self, surface: &mut Surface, profile: &Profile, index: usize, r: Rect, drop_target: bool, hovered: bool) {
        let key = profile.loadout[index].clone();
        surface.rect_round(if hovered || drop_target { HOVER_FILL } else { FILL }, r, 0, Radii::all(8));
        let border = if drop_target { DROP_HIGHLIGHT } else { key.as_deref().map(weapons::color_of).unwrap_or(LOCKED) };
        surface.rect_round(border, r, if drop_target { 3 } else { 2 }, Radii::all(8));
        draw_text_a(surface, SLOT_KEYS[index], 20, TEXT, HINT_ALPHA * 2.0, Anchor::TopRight, r.right() - 12, r.top() + 12);
        let Some(key) = key else {
            let c = r.center();
            draw_text_a(surface, "empty", 26, TEXT, HINT_ALPHA, Anchor::Center, c.0, c.1);
            return;
        };
        let w = skill_tree::build_weapon(profile, &key);
        draw_text(surface, w.name(), 28, w.color(), Anchor::TopLeft, r.left() + 14, r.top() + 12);
        if !unlocked(profile, &key) {
            draw_text(surface, "locked – not used", 20, TOO_EXPENSIVE, Anchor::TopLeft, r.left() + 14, r.top() + 44);
        }
        draw_rows(surface, &w.stat_lines(), r.left() + 14, r.right() - 14, r.top() + 50, 20, 24);
    }

    /// The ship's energy, what firing each weapon in the slots costs per second and how long the hungriest one can
    /// fire non-stop.
    fn draw_energy(&self, surface: &mut Surface, ship: &super::ship::Ship, top: i32) {
        let left = self.rect.left() + SLOTS_LEFT;
        let right = self.slot_rects()[1].right();
        let ws: Vec<&weapons::Weapon> = ship.weapons.iter().flatten().collect();
        let mut rows = vec![("Energy".to_string(), g(ship.max_energy)), ("Energy regen".to_string(), format!("{}/s", g(ship.energy_regen)))];
        for w in &ws {
            rows.push((format!("Firing the {}", w.name()), format!("{} energy/s", g_prec(w.energy_cost * w.fire_rate, 3))));
        }
        draw_rows(surface, &rows, left, right, top, 22, 27);
        let Some(hungriest) = ws.iter().max_by(|a, b| (a.energy_cost * a.fire_rate).total_cmp(&(b.energy_cost * b.fire_rate))) else { return };
        let used = hungriest.energy_cost * hungriest.fire_rate;
        let text = if used > ship.energy_regen {
            let seconds = ship.max_energy / (used - ship.energy_regen);
            let with = if ws.len() > 1 { format!(" with the {}", hungriest.name()) } else { String::new() };
            format!("full energy lasts {} s of non-stop fire{with}, then it fires slower", py_round(seconds))
        } else {
            "the energy keeps up with non-stop fire".into()
        };
        draw_text(surface, &text, 19, TEXT_MUTED, Anchor::TopLeft, left, top + rows.len() as i32 * 27 + 4);
    }

    /// The values of the weapon under the mouse (or being dragged), below the cards.
    fn draw_details(&self, surface: &mut Surface, profile: &Profile, key: &str) {
        let top = self.card_rects().iter().map(|(_, r)| r.bottom()).max().unwrap_or(0) + 24;
        let w = skill_tree::build_weapon(profile, key);
        draw_text(surface, &w.name().to_uppercase(), 22, if unlocked(profile, key) { w.color() } else { LOCKED }, Anchor::TopLeft, self.rect.left(), top);
        let width = CARD_COLUMNS * CARD_SIZE.0 + (CARD_COLUMNS - 1) * CARD_GAP;
        draw_rows(surface, &w.stat_lines(), self.rect.left(), self.rect.left() + width, top + 28, 22, 27);
    }
}

fn set_loadout(profile: &mut Profile, loadout: [Option<String>; 2]) -> Message {
    if loadout.iter().all(|s| s.is_none()) {
        return Some(("Keep at least one weapon".into(), TOO_EXPENSIVE));
    }
    if loadout != profile.loadout {
        profile.loadout = loadout;
        profile.save();
    }
    None
}

/// (name, value) rows: name faded on the left, value on the right, a thin line below each.
fn draw_rows(surface: &mut Surface, rows: &[(String, String)], left: i32, right: i32, mut top: i32, size: i32, step: i32) {
    for (name, value) in rows {
        draw_text(surface, name, size, TEXT_MUTED, Anchor::TopLeft, left, top);
        draw_text(surface, value, size, TEXT, Anchor::TopRight, right, top);
        surface.line(LINE, vec2(left as f64, (top + size - 2) as f64), vec2(right as f64, (top + size - 2) as f64), 1);
        top += step;
    }
}

