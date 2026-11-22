//! Plays the game without a window: every level, with the whole skill tree bought and cards active, a crude bot at
//! the controls (it circles the mouse, fires, dashes, throws rockets and towers, switches weapons, opens the shop and
//! the inventory) – and every few frames the scene and its extra windows are drawn into memory. It does not judge the
//! play; it finds crashes (a panic, a double borrow) in parts a short test would not reach.

use crate::engine::desktop::set_test_areas;
use crate::engine::window::{Event, K_B, K_LSHIFT, K_SPACE, K_T, K_TAB};
use crate::engine::{Engine, Rect, Scene, Surface, vec2};
use crate::game::{cards, difficulty, play_scene::PlayScene, profile::Profile, skill_tree};
use crate::rng;

fn full_profile(level: &str) -> Profile {
    let mut p = Profile::new(None);
    for n in skill_tree::nodes() {
        p.levels.push((n.key.to_string(), n.max_level()));
    }
    p.cores = 10_000;
    p.loadout = [Some("laser".into()), Some("cannon".into())];
    for d in difficulty::all() {
        if let Some(goal) = d.goal {
            p.best_times.push((d.key.clone(), goal));
        }
    }
    let keys: Vec<String> = cards::all().into_iter().map(|c| c.key).take(3).collect();
    p.cards = keys.into_iter().map(|k| (k, true)).collect();
    p.difficulty = level.into();
    p
}

/// Plays `seconds` on `level`; returns how many runs it took (the ship gets crushed, a new run starts).
fn play(level: &str, seconds: f64, seed: u64) -> usize {
    rng::seed(seed);
    let profile = full_profile(level);
    let mut engine = Engine::headless(Rect::new(0, 0, 800, 560));
    let mut scene: Box<dyn Scene> = Box::new(PlayScene::new(profile.clone()));
    engine.start(&mut scene);
    let mut runs = 1;
    let ticks = (seconds * 60.0) as usize;
    for tick in 0..ticks {
        // a new run once the game over screen came
        if scene.as_any().downcast_mut::<PlayScene>().is_none() {
            scene = Box::new(PlayScene::new(profile.clone()));
            engine.start(&mut scene);
            runs += 1;
        }
        let window = engine.window_rect();
        let angle = tick as f64 * 0.05;
        engine.fake_mouse = window.centerv() + vec2(angle.cos(), angle.sin()) * (window.w.min(window.h) as f64 * 0.6);
        engine.fake_buttons = [true, false, false];
        engine.fake_keys.clear();
        engine.fake_keys.insert(if (tick / 90) % 2 == 0 { b'W' as u32 } else { b'S' as u32 });
        let mut events = vec![];
        match tick % 240 {
            30 => events.push(Event::KeyDown(K_LSHIFT)),
            60 => events.push(Event::KeyDown(K_SPACE)),
            90 => events.push(Event::KeyDown(K_B)),
            120 => events.push(Event::MouseWheel { y: 1 }),
            150 if tick % 1200 == 150 => events.push(Event::KeyDown(K_T)),
            152 if tick % 1200 == 152 => {
                let (x, y) = (300, 150);
                events.push(Event::MouseMotion { x, y });
                events.push(Event::MouseButtonUp { button: 1, x, y });
                events.push(Event::KeyDown(K_T));
            }
            200 if tick % 2400 == 200 => events.push(Event::KeyDown(K_TAB)),
            202 if tick % 2400 == 202 => events.push(Event::KeyDown(K_TAB)),
            _ => {}
        }
        engine.step(&mut scene, &events);
        if tick % 20 == 0 {
            let _ = engine.snapshot(&mut scene);
            // the extra windows, as the engine would draw them
            if let Some(ps) = scene.as_any().downcast_mut::<PlayScene>() {
                let _ = ps; // drawn below through the trait
            }
            let views: Vec<(crate::engine::ViewId, Rect)> = (1..400).filter_map(|i| engine.view_rect(view_id(i)).map(|r| (view_id(i), r))).collect();
            for (id, r) in views {
                let mut s = Surface::new(r.w.max(1), r.h.max(1));
                scene.draw_view(&mut engine, &mut s, id, r);
            }
        }
    }
    runs
}

fn view_id(i: u32) -> crate::engine::ViewId {
    crate::engine::view_id_for_tests(i)
}

#[test]
fn every_level_plays_without_a_crash() {
    set_test_areas(vec![Rect::new(0, 0, 1920, 1040), Rect::new(1920, 0, 1280, 1024)]);
    difficulty::reload();
    assert!(difficulty::warnings().is_empty());
    let levels: Vec<String> = difficulty::all().iter().map(|d| d.key.clone()).collect();
    for (i, level) in levels.iter().enumerate() {
        let runs = play(level, 150.0, i as u64 + 1);
        eprintln!("{level}: {runs} run(s)");
    }
}

#[test]
fn the_menus_draw() {
    set_test_areas(vec![Rect::new(0, 0, 1920, 1040)]);
    difficulty::reload();
    let mut engine = Engine::headless(Rect::new(0, 0, 1300, 900));
    let mut scene: Box<dyn Scene> = Box::new(crate::game::game_menu::GameMenuScene::new(Some(full_profile("a1"))));
    engine.start(&mut scene);
    for tab in 0..5 {
        for (x, y) in [(400, 300), (700, 400), (900, 500), (420, 240), (380, 260)] {
            engine.step(&mut scene, &[Event::MouseMotion { x, y }]);
            let _ = engine.snapshot(&mut scene);
        }
        let _ = tab;
        engine.step(&mut scene, &[Event::KeyDown(K_TAB)]);
    }
    let mut main: Box<dyn Scene> = Box::new(crate::game::scenes::MainMenuScene::new());
    engine.start(&mut main);
    let _ = engine.snapshot(&mut main);
}

/// Pictures to look at (not a check): `cargo test pictures -- --ignored`, they land in the temp folder.
#[test]
#[ignore]
fn pictures() {
    set_test_areas(vec![Rect::new(0, 0, 1920, 1040)]);
    difficulty::reload();
    let dir = std::env::temp_dir().join("cube-game-pictures");
    let _ = std::fs::create_dir_all(&dir);
    let mut engine = Engine::headless(Rect::new(0, 0, 640, 460));
    let mut main: Box<dyn Scene> = Box::new(crate::game::scenes::MainMenuScene::new());
    engine.start(&mut main);
    engine.fake_mouse = vec2(1100.0, 400.0);
    engine.snapshot(&mut main).save_png(&dir.join("main_menu.png"));
    let mut menu: Box<dyn Scene> = Box::new(crate::game::game_menu::GameMenuScene::new(Some(full_profile("b2"))));
    engine.start(&mut menu);
    for (i, name) in ["levels", "loadout", "tree", "cards"].iter().enumerate() {
        let r = engine.window_rect();
        engine.step(&mut menu, &[Event::MouseMotion { x: 700 + (i as i32) * 40, y: 330 }]);
        engine.fake_mouse = vec2(r.x as f64 + 700.0, r.y as f64 + 330.0);
        engine.snapshot(&mut menu).save_png(&dir.join(format!("menu_{name}.png")));
        engine.step(&mut menu, &[Event::KeyDown(K_TAB)]);
    }
    rng::seed(7);
    let mut play: Box<dyn Scene> = Box::new(PlayScene::new(full_profile("b4")));
    engine.start(&mut play);
    for tick in 0..60 * 40 {
        let w = engine.window_rect();
        let a = tick as f64 * 0.03;
        engine.fake_mouse = w.centerv() + vec2(a.cos(), a.sin()) * 200.0;
        engine.fake_buttons = [true, false, false];
        let ev = if tick == 600 { vec![Event::KeyDown(K_B)] } else { vec![] };
        engine.step(&mut play, &ev);
    }
    engine.snapshot(&mut play).save_png(&dir.join("play.png"));
    eprintln!("pictures in {}", dir.display());
}

#[test]
#[ignore]
fn walls_get_pushed() {
    set_test_areas(vec![Rect::new(0, 0, 1920, 1040)]);
    difficulty::reload();
    rng::seed(3);
    let mut engine = Engine::headless(Rect::new(0, 0, 800, 560));
    let mut play: Box<dyn Scene> = Box::new(PlayScene::new(full_profile("a1")));
    engine.start(&mut play);
    for tick in 0..60 * 6 {
        let w = engine.window_rect();
        engine.fake_mouse = vec2(w.right() as f64 + 50.0, w.centery() as f64);
        engine.fake_buttons = [true, false, false];
        engine.step(&mut play, &[]);
        if tick % 30 == 0 {
            if let Some(ps) = play.as_any().downcast_mut::<PlayScene>() {
                eprintln!("t={:.1} arena={:?} shots={} pending={:?} energy={:.0} weapon={:?}", tick as f64 / 60.0, ps.arena.rect, ps.projectiles.len(), ps.arena.pending, ps.ship.energy, ps.ship.weapon().map(|w| w.key));
            }
        }
    }
}

/// The ship dies with a swarm out of sight, and every frame is drawn on the way: the bot plays until a swarm is
/// there, then the ship is left standing until it is crushed – the window shrinks to the ship's size (30 px), smaller
/// than the margins of the swarm markers. That crashed the game on every death (2026-11-22).
#[test]
fn dying_with_a_swarm_around_draws() {
    set_test_areas(vec![Rect::new(0, 0, 1920, 1040)]);
    difficulty::reload();
    let mut checked = vec![];
    for (i, level) in difficulty::all().iter().map(|d| d.key.clone()).enumerate() {
        rng::seed(i as u64 + 1);
        let mut engine = Engine::headless(Rect::new(0, 0, 800, 560));
        let mut scene: Box<dyn Scene> = Box::new(PlayScene::new(full_profile(&level)));
        engine.start(&mut scene);
        let mut swarm = false;
        for tick in 0..(200 * 60) {
            let window = engine.window_rect();
            if swarm {
                engine.fake_mouse = window.centerv();
                engine.fake_buttons = [false, false, false];
                engine.fake_keys.clear();
            } else {
                let angle = tick as f64 * 0.05;
                engine.fake_mouse = window.centerv() + vec2(angle.cos(), angle.sin()) * (window.w.min(window.h) as f64 * 0.6);
                engine.fake_buttons = [true, false, false];
            }
            engine.step(&mut scene, &[]);
            let Some(ps) = scene.as_any().downcast_mut::<PlayScene>() else {
                if swarm {
                    checked.push(level.clone());
                }
                break;
            };
            swarm = swarm || ps.enemies.iter().any(|e| e.borrow().swarm.is_some());
            if swarm {
                let _ = engine.snapshot(&mut scene);
            }
        }
    }
    eprintln!("died with a swarm around: {checked:?}");
    assert!(checked.len() >= 3, "only {} level(s) got there", checked.len());
}
