//! Cube Game – an arena shooter in which the game window itself is the arena, and it keeps shrinking. Shoot its edges
//! to push them back out.
//!
//! The files:
//!   main.rs       the connection to the Windows Toolbox: settings, the page, Play / Stop
//!   engine/       everything that is not specific to this game: windows, drawing, text, the game loop, menus
//!   game/         the game itself – scenes, ship, enemies, levels, skill tree, cards, …
//!   content.rs    the data files (`content\`): read from the repo on the PC it was built on, else compiled in
//!
//! A kit program without a background role (Toolbox\DESIGN.md 5): "Play" (or a double-click on the exe) starts the
//! worker, which opens the game; closing the game switches the worker off again, and with nobody looking at the
//! page the program ends. "Stop" in the Toolbox closes the game. The page's two settings are read when the game
//! starts (as in the Python version: they apply from the next start or run).
//!
//! The progress (cores, skill tree, cards, best times) is in `data\profile.json`.
#![windows_subsystem = "windows"]


mod content;
mod engine;
mod game;
mod pyfmt;
mod rng;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use kit::app::{self, Program, Runtime};
use kit::page::Builder;
use kit::proto::Status;
use kit::{process, t};
use serde::{Deserialize, Serialize};

use game::profile::{self, GameSettings};

/// The page's settings.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// The key display in the bottom right at the start of a run (F3 toggles it while playing).
    pub key_display: bool,
    /// The arena may spread over all monitors – only where there really is screen.
    pub all_monitors: bool,
}

impl Default for Settings {
    fn default() -> Self {
        // as in the Python version: the key display was on unless switched off
        Settings { key_display: true, all_monitors: false }
    }
}

struct CubeGame {
    settings: Settings,
    /// Set while the game window is open – the worker's "close it" (Stop) and "bring it to the front" (show).
    stop: Arc<AtomicBool>,
    show: Arc<AtomicBool>,
}

impl Program for CubeGame {
    type Settings = Settings;
    const META: kit::Meta = kit::meta!();

    fn page(&self, p: &mut Builder, _s: &Settings) {
        p.section(t!("page.play"), |p| {
            p.text(t!("page.play.hint"));
            p.text(t!("page.controls"));
        });
        p.section(t!("page.settings"), |p| {
            p.switch(t!("page.key_display"), "key_display");
            p.text(t!("page.key_display.hint"));
            p.switch(t!("page.all_monitors"), "all_monitors");
            p.text(t!("page.all_monitors.hint"));
        });
        p.section(t!("page.skins"), |p| {
            p.text(t!("page.skins.hint"));
            p.buttons(|b| {
                b.button(t!("page.skins.open"), "open_images");
            });
        });
    }

    fn status(&self, running: bool) -> Status {
        Status { running, text: if running { t!("status.playing") } else { String::new() }, ..Status::default() }
    }

    fn applied(&mut self, s: &Settings) {
        self.settings = s.clone();
    }

    fn action(&mut self, name: &str, _s: &mut Settings) -> Option<String> {
        if name == "open_images" {
            let dir = game::enemies::images_dir();
            let _ = std::fs::create_dir_all(&dir);
            process::open(&dir.display().to_string());
        }
        None
    }

    fn show(&mut self) {
        self.show.store(true, Ordering::SeqCst);
    }
}

/// The worker: whenever the workers are switched on, the game runs on this thread until its window closes.
fn work(rt: Runtime<CubeGame>) {
    loop {
        rt.wait_running();
        let (settings, stop, show) = rt.with(|g, _| (g.settings.clone(), g.stop.clone(), g.show.clone()));
        stop.store(false, Ordering::SeqCst);
        // Stop in the Toolbox switches the workers off: the game window closes.
        let watcher = {
            let rt = rt.clone();
            let stop = stop.clone();
            std::thread::spawn(move || {
                while rt.running() && !stop.load(Ordering::SeqCst) {
                    rt.sleep(Duration::from_millis(500));
                }
                stop.store(true, Ordering::SeqCst);
            })
        };
        play(&settings, stop.clone(), show);
        stop.store(true, Ordering::SeqCst);
        rt.stop();
        let _ = watcher.join();
    }
}

/// Runs the game until its window is closed (or `stop` is set).
fn play(settings: &Settings, stop: Arc<AtomicBool>, show: Arc<AtomicBool>) {
    game::difficulty::reload();
    profile::set_source(Some(app::data_dir().join("profile.json")), GameSettings { key_display: settings.key_display, all_monitors: settings.all_monitors });
    let scene = game::scenes::MainMenuScene::new();
    let mut engine = engine::Engine::new(game::settings::TITLE, scene.window_rect());
    engine.remote = engine::Remote { stop, show };
    engine.run(Box::new(scene));
}

fn main() {
    kit::texts!();
    app::init(&CubeGame::META);
    let game = CubeGame { settings: Settings::default(), stop: Arc::new(AtomicBool::new(false)), show: Arc::new(AtomicBool::new(false)) };
    app::run(game, work);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key the page names is a field of the settings (Toolbox\DESIGN.md 13).
    #[test]
    fn page_keys_are_settings() {
        kit::texts!();
        let game = CubeGame { settings: Settings::default(), stop: Default::default(), show: Default::default() };
        kit::page::check_keys::<Settings>(|p| game.page(p, &Settings::default())).unwrap();
    }

    /// Every text id used in the code exists, in every language.
    #[test]
    fn texts_are_complete() {
        kit::i18n::check_ids(env!("CARGO_MANIFEST_DIR")).unwrap();
    }
}
