//! Pictures that are made once and then reused, instead of drawing the same thing again in every frame.
//!
//! `get(key, make)` – the picture for `key`; `make` draws it the first time. The key must hold everything that
//! changes how it looks (size, colour, white for a hit, …). `None` (e.g. no picture file) is kept, too.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::gfx::Surface;

/// How many pictures are kept – beyond that, the ones unused the longest are dropped.
const LIMIT: usize = 4000;

#[derive(Default)]
struct Cache {
    pictures: HashMap<String, (Option<Rc<Surface>>, u64)>,
    clock: u64,
}

thread_local! {
    static PICTURES: RefCell<Cache> = RefCell::new(Cache::default());
}

/// The picture for `key`, made by `make` the first time.
pub fn get(key: &str, make: impl FnOnce() -> Option<Surface>) -> Option<Rc<Surface>> {
    if let Some(found) = PICTURES.with(|c| {
        let mut c = c.borrow_mut();
        c.clock += 1;
        let clock = c.clock;
        c.pictures.get_mut(key).map(|e| {
            e.1 = clock;
            e.0.clone()
        })
    }) {
        return found;
    }
    let picture = make().map(Rc::new);
    PICTURES.with(|c| {
        let mut c = c.borrow_mut();
        let clock = c.clock;
        c.pictures.insert(key.to_string(), (picture.clone(), clock));
        if c.pictures.len() > LIMIT {
            // The oldest tenth at once, so this does not happen on every new picture.
            let mut ages: Vec<u64> = c.pictures.values().map(|e| e.1).collect();
            ages.sort_unstable();
            let cut = ages[LIMIT / 10];
            c.pictures.retain(|_, e| e.1 > cut);
        }
    });
    picture
}

thread_local! {
    static SCRATCH: RefCell<HashMap<(i32, i32), Surface>> = RefCell::new(HashMap::new());
}

/// A cleared see-through picture of `w` × `h` to draw something on that changes every frame – always the same one
/// for a size, handed to `f` and blitted by it.
pub fn scratch<R>(w: i32, h: i32, f: impl FnOnce(&mut Surface) -> R) -> R {
    let mut s = SCRATCH.with(|c| c.borrow_mut().remove(&(w, h))).unwrap_or_else(|| Surface::new_alpha(w, h));
    s.fill(super::gfx::rgba(0, 0, 0, 0));
    s.alpha = 255;
    let r = f(&mut s);
    SCRATCH.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 200 {
            c.clear();
        }
        c.insert((w, h), s);
    });
    r
}

/// A picture file (PNG) ready to draw, or `None` if it does not exist or cannot be read.
pub fn load_picture(path: &std::path::Path) -> Option<Surface> {
    Surface::load_png(path)
}
