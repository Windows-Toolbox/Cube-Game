//! The engine: everything that is not specific to the Cube Game. It opens the window and runs the game loop with a
//! fixed timestep.
//!
//! Game logic runs at exactly `TICK_RATE` ticks per second, no matter how fast the PC draws. Each frame, the time
//! that has passed is added to an accumulator; for every full tick in it, the scene gets one `update`. That way
//! movement is equally fast on every PC ("Fix Your Timestep").
//!
//! The window is borderless and can be moved and resized by the game (`move_window`). Positions in the game world
//! are screen pixels; the window is just the part of the screen you can see – like a camera.
//!
//! Moving without flicker: each frame, the scene first draws into a picture that already has the window's new size
//! – straight into the memory the window is shown from. Then position, size and picture go on the screen in one
//! step (`window.rs`). Before that, the engine waits for `DwmFlush`: Windows' compositor has just refreshed the
//! screen – that syncs the frames to the monitor.
//!
//! Extra windows (`open_view`): small passive windows next to the main one that show another part of the world –
//! e.g. a watchtower's view. They lie on top of the main window but never take the focus (`window.rs`); the scene
//! draws them in `draw_view` and can hide them for a while. They all close when the scene changes. Their events are
//! ignored – and if the main window loses the focus to one of them anyway, it takes the focus right back and the
//! scene never hears of it.
//!
//! **Without windows** (`Engine::headless`, for tests and the bot): the same scenes run, nothing is shown, views
//! exist only as rectangles, and the mouse is never touched.

pub mod collision;
pub mod desktop;
pub mod font;
pub mod geom;
pub mod gfx;
pub mod key_overlay;
pub mod menu;
pub mod sprite_cache;
pub mod ui;
pub mod window;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub use geom::{Anchor, FRect, Rect, Vec2, vec2};
pub use gfx::{Color, Surface, rgb, rgba};
pub use window::Event;

use window::{MainWindow, ViewWindow};

/// Logic updates per second.
pub const TICK_RATE: f64 = 60.0;
/// After a freeze, don't try to catch up endlessly (seconds).
const MAX_FRAME_TIME: f64 = 0.25;
/// With vsync: not more often than this, even on faster monitors.
const MAX_FPS: f64 = 144.0;

/// One extra window, as the scene sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ViewId(u32);

pub struct View {
    pub id: ViewId,
    /// Where it is on the screen = the part of the world it shows (set by the scene).
    pub rect: Option<Rect>,
    /// Not on the screen for now (e.g. while paused).
    pub hidden: bool,
    window: Option<ViewWindow>,
}

/// What the engine runs: one scene at a time (menu, game, game over, …).
pub trait Scene {
    /// Called when the scene becomes active – e.g. to move the window into place.
    fn enter(&mut self, _e: &mut Engine) {}
    /// One event (key pressed, mouse click, …). Closing the window is handled by the engine.
    fn handle_event(&mut self, _e: &mut Engine, _event: &Event) {}
    /// One logic tick. `dt` is always the same (1 / tick rate) in seconds.
    fn update(&mut self, _e: &mut Engine, _dt: f64) {}
    /// Draw the whole scene onto `surface` (the window's content).
    fn draw(&mut self, e: &mut Engine, surface: &mut Surface);
    /// Draw an extra window's content onto `surface` – the part of the world at the view's rect.
    fn draw_view(&mut self, _e: &mut Engine, _surface: &mut Surface, _view: ViewId, _rect: Rect) {}
    /// For tests and the bot: the scene as what it is.
    fn as_any(&mut self) -> &mut dyn std::any::Any;
}

/// Flags another thread may set: the Toolbox's Stop, a second start's "show".
#[derive(Default, Clone)]
pub struct Remote {
    pub stop: Arc<AtomicBool>,
    pub show: Arc<AtomicBool>,
}

pub struct Engine {
    window: Option<MainWindow>,
    headless: bool,
    target_rect: Rect,
    shown_topleft: (i32, i32),
    mouse_locked: bool,
    views: Vec<View>,
    next_view: u32,
    running: bool,
    next_scene: Option<Box<dyn Scene>>,
    pub remote: Remote,
    /// Without windows: where the mouse is (screen pixels) – tests and the bot move it.
    pub fake_mouse: Vec2,
    /// Without windows: the mouse buttons held (left, middle, right).
    pub fake_buttons: [bool; 3],
    /// Without windows: the keys held.
    pub fake_keys: std::collections::HashSet<window::Key>,
}

impl Engine {
    /// `rect`: position and size of the window on the screen. Without a window if Windows refuses one.
    pub fn new(title: &str, rect: Rect) -> Engine {
        window::dpi_aware();
        let window = MainWindow::new(title, rect);
        let headless = window.is_none();
        Engine::make(rect, window, headless)
    }
    /// Without any window (tests, the bot).
    pub fn headless(rect: Rect) -> Engine {
        Engine::make(rect, None, true)
    }
    fn make(rect: Rect, window: Option<MainWindow>, headless: bool) -> Engine {
        Engine {
            window,
            headless,
            target_rect: rect,
            shown_topleft: rect.topleft(),
            mouse_locked: false,
            views: vec![],
            next_view: 1,
            running: false,
            next_scene: None,
            remote: Remote::default(),
            fake_mouse: Vec2::ZERO,
            fake_buttons: [false; 3],
            fake_keys: Default::default(),
        }
    }
    pub fn is_headless(&self) -> bool {
        self.headless
    }

    /// Where the window is on the screen – or will be after this frame, if the scene has moved it.
    pub fn window_rect(&self) -> Rect {
        self.target_rect
    }
    /// Moves/resizes the window (screen pixels). Happens when the next picture is shown.
    pub fn move_window(&mut self, rect: Rect) {
        self.target_rect = rect;
    }
    /// True: keep the cursor inside the window, so a click cannot land on the desktop behind it.
    pub fn grab_mouse(&mut self, grab: bool) {
        self.mouse_locked = grab;
        if let Some(w) = &mut self.window {
            let focused = w.focused();
            w.lock_mouse(grab && focused);
        }
    }
    /// While the mouse is locked: keep it inside these screen rectangles instead of just the window – e.g. the arena
    /// and the extra windows connected to it. `None`: just the window.
    pub fn confine_mouse(&mut self, rects: Option<Vec<Rect>>) {
        if let Some(w) = &mut self.window {
            w.mouse_area = rects;
        }
    }
    /// The mouse in screen pixels (world coordinates), not relative to the window – also over an extra window.
    pub fn mouse_position(&self) -> Vec2 {
        if self.headless {
            return self.fake_mouse;
        }
        window::cursor_position().into()
    }
    /// The mouse relative to the main window (pygame's `mouse.get_pos()`).
    pub fn mouse_pos(&self) -> (i32, i32) {
        let p = self.mouse_position();
        (p.x as i32 - self.shown_topleft.0, p.y as i32 - self.shown_topleft.1)
    }
    /// Mouse buttons held: left, middle, right.
    pub fn mouse_held(&self) -> [bool; 3] {
        if self.headless { self.fake_buttons } else { window::mouse_held() }
    }
    pub fn key_held(&self, key: window::Key) -> bool {
        if self.headless { self.fake_keys.contains(&key) } else { window::key_held(key) }
    }
    pub fn any_key_held(&self, keys: &[window::Key]) -> bool {
        keys.iter().any(|k| self.key_held(*k))
    }

    // ---------- extra windows ----------

    /// A new extra window – `None` if there cannot be one. The scene sets its rect and draws it in `draw_view`.
    pub fn open_view(&mut self) -> Option<ViewId> {
        let window = if self.headless {
            None
        } else {
            let owner = self.window.as_ref()?.hwnd;
            Some(ViewWindow::new(owner)?)
        };
        let id = ViewId(self.next_view);
        self.next_view += 1;
        self.views.push(View { id, rect: None, hidden: false, window });
        Some(id)
    }
    pub fn view(&mut self, id: ViewId) -> Option<&mut View> {
        self.views.iter_mut().find(|v| v.id == id)
    }
    pub fn view_rect(&self, id: ViewId) -> Option<Rect> {
        self.views.iter().find(|v| v.id == id).and_then(|v| v.rect)
    }
    pub fn set_view_rect(&mut self, id: ViewId, rect: Rect) {
        if let Some(v) = self.view(id) {
            v.rect = Some(rect);
        }
    }
    pub fn set_view_hidden(&mut self, id: ViewId, hidden: bool) {
        if let Some(v) = self.view(id) {
            v.hidden = hidden;
        }
    }
    pub fn close_view(&mut self, id: ViewId) {
        self.views.retain(|v| v.id != id);
    }
    pub fn close_views(&mut self) {
        self.views.clear();
    }
    pub fn has_views(&self) -> bool {
        !self.views.is_empty()
    }

    // ---------- scenes ----------

    /// The next scene; it takes over after the current event, tick or frame.
    pub fn change_scene(&mut self, scene: Box<dyn Scene>) {
        self.next_scene = Some(scene);
    }
    pub fn stop(&mut self) {
        self.running = false;
    }
    pub fn running(&self) -> bool {
        self.running
    }

    fn switch(&mut self, current: &mut Box<dyn Scene>) -> bool {
        let mut switched = false;
        while let Some(mut next) = self.next_scene.take() {
            self.close_views();
            self.confine_mouse(None);
            next.enter(self);
            *current = next;
            switched = true;
        }
        switched
    }

    /// Draws the scene and shows it – moving/resizing the window at the same moment.
    pub fn present(&mut self, scene: &mut dyn Scene) {
        let rect = self.target_rect;
        if self.headless {
            self.shown_topleft = rect.topleft();
            return;
        }
        let Some(mut canvas) = self.window.as_mut().and_then(|w| w.canvas(rect.w, rect.h)) else { return };
        scene.draw(self, &mut canvas);
        drop(canvas);
        dwm_flush();
        if let Some(w) = &mut self.window {
            w.show(rect);
        }
        self.shown_topleft = rect.topleft();
        self.present_views(scene);
    }

    fn present_views(&mut self, scene: &mut dyn Scene) {
        let ids: Vec<ViewId> = self.views.iter().map(|v| v.id).collect();
        for id in ids {
            let Some(view) = self.views.iter_mut().find(|v| v.id == id) else { continue };
            let Some(w) = view.window.as_mut() else { continue };
            if view.hidden {
                w.hide();
                continue;
            }
            let Some(rect) = view.rect.filter(|r| r.w > 0 && r.h > 0) else { continue };
            let Some(mut canvas) = w.canvas(rect.w, rect.h) else { continue };
            scene.draw_view(self, &mut canvas, id, rect);
            drop(canvas);
            if let Some(w) = self.views.iter_mut().find(|v| v.id == id).and_then(|v| v.window.as_mut()) {
                w.show(rect);
            }
        }
    }

    /// Runs `scene` and the ones it leads to until the game ends.
    pub fn run(&mut self, scene: Box<dyn Scene>) {
        let mut scene = scene;
        self.start(&mut scene);
        let tick = 1.0 / TICK_RATE;
        let mut accumulator = 0.0;
        let mut last = Instant::now();
        while self.running {
            let frame_time = limit_frame(&mut last, if self.window.is_some() { MAX_FPS } else { TICK_RATE });
            accumulator += frame_time.min(MAX_FRAME_TIME);
            window::pump();
            if self.remote.stop.swap(false, Ordering::SeqCst) {
                self.stop();
            }
            if self.remote.show.swap(false, Ordering::SeqCst) {
                if let Some(w) = &self.window {
                    w.focus();
                }
            }
            for event in window::take_events() {
                if let Event::FocusLost { to } = event {
                    if self.has_views() && window::is_own(to) {
                        // An extra window got clicked: back to the main window, the game goes on.
                        if let Some(w) = &self.window {
                            w.focus();
                        }
                        continue;
                    }
                }
                match event {
                    Event::Quit => self.stop(),
                    _ => {
                        self.handle_focus(&event);
                        scene.handle_event(self, &event);
                    }
                }
                self.switch(&mut scene);
            }
            while accumulator >= tick {
                scene.update(self, tick);
                accumulator -= tick;
                self.switch(&mut scene);
            }
            self.present(scene.as_mut());
            self.switch(&mut scene);
        }
        self.close_views();
        self.window = None; // lets the mouse go and closes the window
    }

    /// Lets the mouse go while another window is active (Alt+Tab) – otherwise it would stay locked in.
    fn handle_focus(&mut self, event: &Event) {
        let locked = self.mouse_locked;
        if let Some(w) = &mut self.window {
            match event {
                Event::FocusLost { .. } => w.lock_mouse(false),
                Event::FocusGained => w.lock_mouse(locked),
                _ => {}
            }
        }
    }

    /// Starts `scene` (and whatever it switches to right away).
    pub fn start(&mut self, scene: &mut Box<dyn Scene>) {
        self.close_views();
        scene.enter(self);
        self.switch(scene);
        self.running = true;
    }
    /// For tests and the bot: one tick of `scene` without windows, after `events`, with the scene switches it asks
    /// for.
    pub fn step(&mut self, scene: &mut Box<dyn Scene>, events: &[Event]) {
        for e in events {
            scene.handle_event(self, e);
            self.switch(scene);
        }
        scene.update(self, 1.0 / TICK_RATE);
        self.switch(scene);
        self.shown_topleft = self.target_rect.topleft();
    }
    /// For tests: draws `scene` into a picture of the window's size, as the window would show it.
    pub fn snapshot(&mut self, scene: &mut Box<dyn Scene>) -> Surface {
        let r = self.target_rect;
        let mut s = Surface::new(r.w, r.h);
        scene.draw(self, &mut s);
        s
    }
}

/// Waits so that frames come at most `fps` times a second; returns the seconds since the last frame.
fn limit_frame(last: &mut Instant, fps: f64) -> f64 {
    let min = Duration::from_secs_f64(1.0 / fps);
    let elapsed = last.elapsed();
    if elapsed < min {
        std::thread::sleep(min - elapsed);
    }
    let now = Instant::now();
    let dt = now.duration_since(*last).as_secs_f64();
    *last = now;
    dt
}

/// Waits until Windows' compositor has just refreshed the screen – that syncs the frames to the monitor.
fn dwm_flush() {
    unsafe { windows_sys::Win32::Graphics::Dwm::DwmFlush() };
}

/// Milliseconds since the game started (pygame's `time.get_ticks()`).
pub fn ticks() -> u64 {
    use std::sync::OnceLock;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// For tests: the view with this number (views are numbered from 1 in the order they were opened).
#[cfg(test)]
pub fn view_id_for_tests(i: u32) -> ViewId {
    ViewId(i)
}
