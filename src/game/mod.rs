//! The Cube Game itself – everything that is specific to it (the engine is in `crate::engine`).

pub mod abilities;
pub mod arena;
pub mod buildings;
pub mod cards;
pub mod difficulty;
pub mod effects;
pub mod enemies;
pub mod field;
pub mod game_menu;
pub mod game_over;
pub mod hud;
pub mod panels;
pub mod pickups;
pub mod play_scene;
pub mod points;
pub mod profile;
pub mod projectile;
pub mod radar;
pub mod roster;
pub mod scenes;
pub mod settings;
pub mod shapes;
pub mod ship;
pub mod shop;
pub mod shop_data;
pub mod skill_tree;
pub mod spawner;
pub mod tables;
pub mod weapons;
#[cfg(test)]
mod smoke_test;
