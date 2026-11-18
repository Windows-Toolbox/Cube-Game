//! All numbers and colours of the game in one place, so they can be tweaked quickly – except what depends on the
//! level (arena size and shrinking, cores, enemies): that is in `content\difficulties\*.json`.
//! Distances in screen pixels, speeds in pixels per second.

use crate::engine::window::{K_B, K_LSHIFT, K_RSHIFT, K_SPACE, Key};
use crate::engine::{Color, rgb};

pub const TITLE: &str = "Cube Game";
/// Window of the main menu.
pub const MENU_WINDOW_SIZE: (i32, i32) = (640, 460);
/// Window of the game menu (ship, level, skill tree) between runs.
pub const GAME_MENU_WINDOW_SIZE: (i32, i32) = (1300, 900);

/// The window is the arena: it keeps shrinking, shots against its edges push them back out. A hit pushes its edge
/// out smoothly at this speed, instead of jumping.
pub const PUSH_SPEED: f64 = 600.0;

/// Getting hit: the edges far from the ship move in more, the near ones less. 0 = every edge the same, 1 = with the
/// ship right at the left edge, the left edge does not move at all and the right one twice as far (the total stays).
pub const HIT_SIDE_BIAS: f64 = 1.0;
/// Armor is flat (`armor.rs`), but at least this share is always left: of a push against an armored wall …
pub const MIN_PUSH_SHARE: f64 = 0.1;
/// … of a hit on the armored ship …
pub const MIN_HIT_SHARE: f64 = 0.25;
/// … of a hit on an armored enemy.
pub const MIN_DAMAGE_SHARE: f64 = 0.15;
/// Recovery never makes the ship invulnerable longer than this after a hit (seconds).
pub const MAX_INVULNERABLE_TIME: f64 = 3.0;
/// Dots at the window's edge showing where enemies outside the window are.
pub const OFFSCREEN_MARKERS: bool = true;

/// Keys for the ability slots (slot 1: dash, slot 2: rocket) and how they are shown on screen.
pub const ABILITY_KEYS: [&[Key]; 2] = [&[K_LSHIFT, K_RSHIFT], &[K_SPACE]];
pub const ABILITY_KEY_NAMES: [&str; 2] = ["SHIFT", "SPACE"];
/// Throws a watchtower (once unlocked in the skill tree).
pub const BUILD_KEYS: &[Key] = &[K_B];
pub const BUILD_KEY_NAME: &str = "B";
/// Right mouse button: switches the weapon that fires (the mouse wheel does, too) – no cooldown.
pub const WEAPON_SWITCH_BUTTON: u8 = 3;
pub const WEAPON_SWITCH_KEY_NAME: &str = "RMB";

/// Gap between the key display and the window's corner.
pub const KEY_OVERLAY_INSET: i32 = 12;
/// How visible the hints top left ("ESC to open menu", …) are (0 = invisible, 255 = solid).
pub const HINT_ALPHA: f64 = 70.0;

/// Inside the arena.
pub const BACKGROUND: Color = rgb(0, 0, 0);
/// Outline of the arena, so you can see the walls.
pub const BORDER: Color = rgb(255, 255, 255);
pub const BORDER_WIDTH: i32 = 2;
pub const TEXT: Color = rgb(255, 255, 255);
pub const TEXT_MUTED: Color = rgb(150, 150, 160);

/// Armor is flat everywhere – the walls', the ship's and the enemies': every push or hit loses that many pixels or
/// points, but never all of it: at least `min_share` is left (`armor.py`).
pub fn reduce(amount: f64, armor: f64, min_share: f64) -> f64 {
    (amount - armor.max(0.0)).max(amount * min_share)
}
