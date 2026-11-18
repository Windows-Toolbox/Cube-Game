//! The Windows desktop: the usable area (without the taskbar) of the main monitor, or of all monitors.
//!
//! With several monitors the game world can be all of them together. They do not have to form a rectangle – e.g.
//! two monitors of different height, or a taskbar on only one of them – so `covers` checks whether a rectangle lies
//! completely on monitors (nothing of it in the gaps, where it would be invisible).

use windows_sys::Win32::Foundation::{LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETWORKAREA, SystemParametersInfoW};

use super::geom::{Rect, Vec2};

/// Leftovers thinner than this (rounding) count as covered.
const EPSILON: f64 = 1e-6;

/// The usable area of the main monitor. A fixed 1920 × 1040 without Windows (tests).
pub fn work_area() -> Rect {
    if let Some(r) = TEST_AREAS.with(|t| t.borrow().as_ref().map(|a| a[0])) {
        return r;
    }
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    if unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut r as *mut RECT as *mut _, 0) } != 0 {
        return Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top);
    }
    Rect::new(0, 0, 1920, 1040)
}

/// The usable area of every monitor (main monitor first).
pub fn work_areas() -> Vec<Rect> {
    if let Some(a) = TEST_AREAS.with(|t| t.borrow().clone()) {
        return a;
    }
    unsafe extern "system" fn found(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> i32 {
        let areas = unsafe { &mut *(data as *mut Vec<Rect>) };
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info) } != 0 {
            let w = info.rcWork;
            areas.push(Rect::new(w.left, w.top, w.right - w.left, w.bottom - w.top));
        }
        1 // go on with the next monitor
    }
    let mut areas: Vec<Rect> = vec![];
    unsafe { EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(found), &mut areas as *mut Vec<Rect> as LPARAM) };
    let main = work_area();
    let mut out = vec![main];
    out.extend(areas.into_iter().filter(|a| *a != main));
    out
}

thread_local! {
    /// Tests give the desktop they want instead of the real monitors.
    static TEST_AREAS: std::cell::RefCell<Option<Vec<Rect>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
/// For tests: these areas instead of the real monitors (first = main monitor).
pub fn set_test_areas(areas: Vec<Rect>) {
    TEST_AREAS.with(|t| *t.borrow_mut() = Some(areas));
}

#[derive(Debug, Clone, PartialEq)]
pub struct Desktop {
    pub areas: Vec<Rect>,
    pub main: Rect,
    /// The smallest rectangle around all of them.
    pub bounds: Rect,
}

impl Desktop {
    pub fn new(areas: Vec<Rect>) -> Desktop {
        let main = areas[0];
        let bounds = main.union_all(&areas[1..]);
        Desktop { areas, main, bounds }
    }
    pub fn current(all_monitors: bool) -> Desktop {
        Desktop::new(if all_monitors { work_areas() } else { vec![work_area()] })
    }
    pub fn contains(&self, x: f64, y: f64) -> bool {
        self.areas.iter().any(|a| a.collidepoint(x, y))
    }
    /// The monitor area the point is on – or the nearest one.
    pub fn area_at(&self, p: Vec2) -> Rect {
        *self
            .areas
            .iter()
            .min_by(|a, b| clamp_point(p, a).distance_squared_to(p).total_cmp(&clamp_point(p, b).distance_squared_to(p)))
            .expect("at least one monitor")
    }
    /// The point itself if it is on a monitor, else the nearest point that is.
    pub fn clamp_point(&self, p: Vec2) -> Vec2 {
        clamp_point(p, &self.area_at(p))
    }
    /// True if the rectangle lies completely on monitors: every monitor is cut out of it, and whatever is left at
    /// the end is in a gap.
    pub fn covers(&self, left: f64, top: f64, right: f64, bottom: f64) -> bool {
        let mut pieces = vec![(left, top, right, bottom)];
        for a in &self.areas {
            let (al, at, ar, ab) = (a.left() as f64, a.top() as f64, a.right() as f64, a.bottom() as f64);
            let mut rest = vec![];
            for (l, t, r, b) in pieces {
                if r <= al || l >= ar || b <= at || t >= ab {
                    rest.push((l, t, r, b)); // does not touch this monitor
                    continue;
                }
                if l < al {
                    rest.push((l, t, al, b));
                }
                if r > ar {
                    rest.push((ar, t, r, b));
                }
                let (il, ir) = (l.max(al), r.min(ar));
                if t < at {
                    rest.push((il, t, ir, at));
                }
                if b > ab {
                    rest.push((il, ab, ir, b));
                }
            }
            pieces = rest.into_iter().filter(|p| p.2 - p.0 > EPSILON && p.3 - p.1 > EPSILON).collect();
            if pieces.is_empty() {
                return true;
            }
        }
        false
    }
    pub fn covers_rect(&self, r: &Rect) -> bool {
        self.covers(r.left() as f64, r.top() as f64, r.right() as f64, r.bottom() as f64)
    }
}

pub fn clamp_point(p: Vec2, a: &Rect) -> Vec2 {
    Vec2::new(p.x.clamp(a.left() as f64, (a.right() - 1) as f64), p.y.clamp(a.top() as f64, (a.bottom() - 1) as f64))
}
