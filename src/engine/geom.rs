//! Rectangles and vectors with exactly the rules of pygame's `Rect`, `FRect` and `Vector2`, so the game's numbers
//! behave as they did in the Python version:
//!
//! - `Rect` holds whole pixels (i32). Every value given to it is cut off towards zero (3.7 → 3, −3.7 → −3), and
//!   halves are whole-number divisions cut off towards zero, too (the centre of a rect 11 wide is 5 from its left).
//! - `FRect` holds `f32` – pygame's FRect uses C floats, not doubles, and the game relies on slow movement adding
//!   up even so.
//! - `Vec2` holds `f64`, like pygame's Vector2.

use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

// ---------- Vec2 ----------

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

pub const fn vec2(x: f64, y: f64) -> Vec2 {
    Vec2 { x, y }
}

impl Vec2 {
    pub const ZERO: Vec2 = vec2(0.0, 0.0);

    pub fn new(x: impl Into<f64>, y: impl Into<f64>) -> Self {
        Self { x: x.into(), y: y.into() }
    }
    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }
    pub fn length_squared(self) -> f64 {
        self.x * self.x + self.y * self.y
    }
    /// pygame raises for a zero vector; here it stays zero – callers check the length first, as the Python code did.
    pub fn normalize(self) -> Self {
        let l = self.length();
        if l == 0.0 { self } else { self / l }
    }
    pub fn distance_to(self, o: Vec2) -> f64 {
        (self - o).length()
    }
    pub fn distance_squared_to(self, o: Vec2) -> f64 {
        (self - o).length_squared()
    }
    pub fn dot(self, o: Vec2) -> f64 {
        self.x * o.x + self.y * o.y
    }
    pub fn cross(self, o: Vec2) -> f64 {
        self.x * o.y - self.y * o.x
    }
    /// Turned by `degrees` – in pygame's sense: positive turns from +x towards +y (clockwise on the screen).
    pub fn rotate(self, degrees: f64) -> Self {
        let (s, c) = degrees.to_radians().sin_cos();
        vec2(self.x * c - self.y * s, self.x * s + self.y * c)
    }
    pub fn rotate_rad(self, radians: f64) -> Self {
        let (s, c) = radians.sin_cos();
        vec2(self.x * c - self.y * s, self.x * s + self.y * c)
    }
    /// The angle from +x in degrees, −180 … 180 (pygame's `as_polar()[1]`).
    pub fn angle(self) -> f64 {
        self.y.atan2(self.x).to_degrees()
    }
    /// The angle from `self` to `o` in degrees (pygame's `angle_to`: `atan2(o) − atan2(self)`, not normalised).
    pub fn angle_to(self, o: Vec2) -> f64 {
        o.y.atan2(o.x).to_degrees() - self.y.atan2(self.x).to_degrees()
    }
    pub fn from_polar(length: f64, degrees: f64) -> Self {
        let (s, c) = degrees.to_radians().sin_cos();
        vec2(length * c, length * s)
    }
    /// Shortened to at most `max` (pygame's `clamp_magnitude(max)`).
    pub fn clamp_magnitude(self, max: f64) -> Self {
        let l = self.length();
        if l > max && l > 0.0 { self * (max / l) } else { self }
    }
    pub fn scale_to_length(self, length: f64) -> Self {
        let l = self.length();
        if l == 0.0 { self } else { self * (length / l) }
    }
    pub fn lerp(self, o: Vec2, t: f64) -> Self {
        self + (o - self) * t
    }
    /// Moves towards `target` by at most `step`.
    pub fn move_towards(self, target: Vec2, step: f64) -> Self {
        let d = target - self;
        let l = d.length();
        if l <= step || l == 0.0 { target } else { self + d * (step / l) }
    }
    pub fn round(self) -> (i32, i32) {
        (py_round(self.x) as i32, py_round(self.y) as i32)
    }
    /// Cut off towards zero – what pygame does when a Vector2 is given to a Rect or a drawing function.
    pub fn ints(self) -> (i32, i32) {
        (self.x as i32, self.y as i32)
    }
    pub fn tuple(self) -> (f64, f64) {
        (self.x, self.y)
    }
}

impl From<(f64, f64)> for Vec2 {
    fn from((x, y): (f64, f64)) -> Self {
        vec2(x, y)
    }
}
impl From<(i32, i32)> for Vec2 {
    fn from((x, y): (i32, i32)) -> Self {
        vec2(x as f64, y as f64)
    }
}
impl From<(f32, f32)> for Vec2 {
    fn from((x, y): (f32, f32)) -> Self {
        vec2(x as f64, y as f64)
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        vec2(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        vec2(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f64) -> Vec2 {
        vec2(self.x * k, self.y * k)
    }
}
impl Mul<Vec2> for f64 {
    type Output = Vec2;
    fn mul(self, v: Vec2) -> Vec2 {
        v * self
    }
}
impl Div<f64> for Vec2 {
    type Output = Vec2;
    fn div(self, k: f64) -> Vec2 {
        vec2(self.x / k, self.y / k)
    }
}
impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        vec2(-self.x, -self.y)
    }
}
impl AddAssign for Vec2 {
    fn add_assign(&mut self, o: Vec2) {
        *self = *self + o;
    }
}
impl SubAssign for Vec2 {
    fn sub_assign(&mut self, o: Vec2) {
        *self = *self - o;
    }
}
impl MulAssign<f64> for Vec2 {
    fn mul_assign(&mut self, k: f64) {
        *self = *self * k;
    }
}
impl DivAssign<f64> for Vec2 {
    fn div_assign(&mut self, k: f64) {
        *self = *self / k;
    }
}

/// Python's `round()`: halves go to the even neighbour (2.5 → 2, 3.5 → 4).
pub fn py_round(v: f64) -> f64 {
    let r = v.round();
    if (v - v.trunc()).abs() == 0.5 { 2.0 * (v / 2.0).round() } else { r }
}

/// Python's `%` for floats: the result has the sign of the divisor.
pub fn py_mod(a: f64, b: f64) -> f64 {
    let r = a % b;
    if r != 0.0 && (r < 0.0) != (b < 0.0) { r + b } else { r }
}

/// Python's `//` for whole numbers: rounds down, not towards zero.
pub fn py_div(a: i32, b: i32) -> i32 {
    let q = a / b;
    if (a % b != 0) && ((a < 0) != (b < 0)) { q - 1 } else { q }
}

// ---------- Rect and FRect ----------

macro_rules! rect_type {
    ($name:ident, $t:ty, $half:expr, $from:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq, Default)]
        pub struct $name {
            pub x: $t,
            pub y: $t,
            pub w: $t,
            pub h: $t,
        }

        impl $name {
            pub fn new(x: impl Into<f64>, y: impl Into<f64>, w: impl Into<f64>, h: impl Into<f64>) -> Self {
                let f = $from;
                Self { x: f(x.into()), y: f(y.into()), w: f(w.into()), h: f(h.into()) }
            }
            pub fn from_size(w: impl Into<f64>, h: impl Into<f64>) -> Self {
                Self::new(0.0, 0.0, w, h)
            }
            fn half(v: $t) -> $t {
                $half(v)
            }
            fn conv(v: f64) -> $t {
                $from(v)
            }

            pub fn left(&self) -> $t {
                self.x
            }
            pub fn top(&self) -> $t {
                self.y
            }
            pub fn right(&self) -> $t {
                self.x + self.w
            }
            pub fn bottom(&self) -> $t {
                self.y + self.h
            }
            pub fn centerx(&self) -> $t {
                self.x + Self::half(self.w)
            }
            pub fn centery(&self) -> $t {
                self.y + Self::half(self.h)
            }
            pub fn width(&self) -> $t {
                self.w
            }
            pub fn height(&self) -> $t {
                self.h
            }
            pub fn size(&self) -> ($t, $t) {
                (self.w, self.h)
            }
            pub fn center(&self) -> ($t, $t) {
                (self.centerx(), self.centery())
            }
            pub fn topleft(&self) -> ($t, $t) {
                (self.x, self.y)
            }
            pub fn topright(&self) -> ($t, $t) {
                (self.right(), self.y)
            }
            pub fn bottomleft(&self) -> ($t, $t) {
                (self.x, self.bottom())
            }
            pub fn bottomright(&self) -> ($t, $t) {
                (self.right(), self.bottom())
            }
            pub fn midtop(&self) -> ($t, $t) {
                (self.centerx(), self.y)
            }
            pub fn midbottom(&self) -> ($t, $t) {
                (self.centerx(), self.bottom())
            }
            pub fn midleft(&self) -> ($t, $t) {
                (self.x, self.centery())
            }
            pub fn midright(&self) -> ($t, $t) {
                (self.right(), self.centery())
            }
            pub fn centerv(&self) -> Vec2 {
                Vec2::new(self.centerx() as f64, self.centery() as f64)
            }
            pub fn topleftv(&self) -> Vec2 {
                Vec2::new(self.x as f64, self.y as f64)
            }

            pub fn set_left(&mut self, v: impl Into<f64>) {
                self.x = Self::conv(v.into());
            }
            pub fn set_top(&mut self, v: impl Into<f64>) {
                self.y = Self::conv(v.into());
            }
            pub fn set_right(&mut self, v: impl Into<f64>) {
                self.x = Self::conv(v.into()) - self.w;
            }
            pub fn set_bottom(&mut self, v: impl Into<f64>) {
                self.y = Self::conv(v.into()) - self.h;
            }
            pub fn set_centerx(&mut self, v: impl Into<f64>) {
                self.x = Self::conv(v.into()) - Self::half(self.w);
            }
            pub fn set_centery(&mut self, v: impl Into<f64>) {
                self.y = Self::conv(v.into()) - Self::half(self.h);
            }
            pub fn set_width(&mut self, v: impl Into<f64>) {
                self.w = Self::conv(v.into());
            }
            pub fn set_height(&mut self, v: impl Into<f64>) {
                self.h = Self::conv(v.into());
            }
            pub fn set_size(&mut self, w: impl Into<f64>, h: impl Into<f64>) {
                self.w = Self::conv(w.into());
                self.h = Self::conv(h.into());
            }
            pub fn set_center(&mut self, x: impl Into<f64>, y: impl Into<f64>) {
                self.set_centerx(x);
                self.set_centery(y);
            }
            pub fn set_centerv(&mut self, v: Vec2) {
                self.set_center(v.x, v.y);
            }
            pub fn set_topleft(&mut self, x: impl Into<f64>, y: impl Into<f64>) {
                self.set_left(x);
                self.set_top(y);
            }
            pub fn set_topright(&mut self, x: impl Into<f64>, y: impl Into<f64>) {
                self.set_right(x);
                self.set_top(y);
            }
            pub fn set_bottomleft(&mut self, x: impl Into<f64>, y: impl Into<f64>) {
                self.set_left(x);
                self.set_bottom(y);
            }
            pub fn set_bottomright(&mut self, x: impl Into<f64>, y: impl Into<f64>) {
                self.set_right(x);
                self.set_bottom(y);
            }
            pub fn set_midtop(&mut self, x: impl Into<f64>, y: impl Into<f64>) {
                self.set_centerx(x);
                self.set_top(y);
            }
            pub fn set_midbottom(&mut self, x: impl Into<f64>, y: impl Into<f64>) {
                self.set_centerx(x);
                self.set_bottom(y);
            }
            pub fn set_midleft(&mut self, x: impl Into<f64>, y: impl Into<f64>) {
                self.set_left(x);
                self.set_centery(y);
            }
            pub fn set_midright(&mut self, x: impl Into<f64>, y: impl Into<f64>) {
                self.set_right(x);
                self.set_centery(y);
            }

            /// A copy with its anchor at a point – pygame's `get_rect(center=…)` and friends.
            pub fn placed(mut self, anchor: Anchor, x: impl Into<f64>, y: impl Into<f64>) -> Self {
                let (x, y) = (x.into(), y.into());
                match anchor {
                    Anchor::TopLeft => self.set_topleft(x, y),
                    Anchor::TopRight => self.set_topright(x, y),
                    Anchor::BottomLeft => self.set_bottomleft(x, y),
                    Anchor::BottomRight => self.set_bottomright(x, y),
                    Anchor::Center => self.set_center(x, y),
                    Anchor::MidTop => self.set_midtop(x, y),
                    Anchor::MidBottom => self.set_midbottom(x, y),
                    Anchor::MidLeft => self.set_midleft(x, y),
                    Anchor::MidRight => self.set_midright(x, y),
                }
                self
            }

            pub fn moved(&self, dx: impl Into<f64>, dy: impl Into<f64>) -> Self {
                Self { x: self.x + Self::conv(dx.into()), y: self.y + Self::conv(dy.into()), ..*self }
            }
            pub fn move_ip(&mut self, dx: impl Into<f64>, dy: impl Into<f64>) {
                *self = self.moved(dx, dy);
            }
            pub fn inflate(&self, dx: impl Into<f64>, dy: impl Into<f64>) -> Self {
                let (dx, dy) = (Self::conv(dx.into()), Self::conv(dy.into()));
                Self { x: self.x - Self::half(dx), y: self.y - Self::half(dy), w: self.w + dx, h: self.h + dy }
            }
            pub fn inflate_ip(&mut self, dx: impl Into<f64>, dy: impl Into<f64>) {
                *self = self.inflate(dx, dy);
            }
            pub fn scale_by(&self, factor: f64) -> Self {
                let (w, h) = (self.w as f64 * factor, self.h as f64 * factor);
                let mut r = Self::new(0.0, 0.0, w, h);
                r.set_center(self.centerx() as f64, self.centery() as f64);
                r
            }

            /// Moved inside `other` (pygame's `clamp`); bigger than it: centred on it.
            pub fn clamp(&self, other: &Self) -> Self {
                let x = if self.w >= other.w {
                    other.x + Self::half(other.w) - Self::half(self.w)
                } else if self.x < other.x {
                    other.x
                } else if self.x + self.w > other.x + other.w {
                    other.x + other.w - self.w
                } else {
                    self.x
                };
                let y = if self.h >= other.h {
                    other.y + Self::half(other.h) - Self::half(self.h)
                } else if self.y < other.y {
                    other.y
                } else if self.y + self.h > other.y + other.h {
                    other.y + other.h - self.h
                } else {
                    self.y
                };
                Self { x, y, ..*self }
            }
            pub fn clamp_ip(&mut self, other: &Self) {
                *self = self.clamp(other);
            }
            /// The part inside `other`; nothing in common: size 0 at this rect's position.
            pub fn clip(&self, other: &Self) -> Self {
                let left = if self.x >= other.x && self.x < other.right() {
                    self.x
                } else if other.x >= self.x && other.x < self.right() {
                    other.x
                } else {
                    return Self { x: self.x, y: self.y, w: Self::conv(0.0), h: Self::conv(0.0) };
                };
                let right = if self.right() > other.x && self.right() <= other.right() {
                    self.right()
                } else if other.right() > self.x && other.right() <= self.right() {
                    other.right()
                } else {
                    return Self { x: self.x, y: self.y, w: Self::conv(0.0), h: Self::conv(0.0) };
                };
                let top = if self.y >= other.y && self.y < other.bottom() {
                    self.y
                } else if other.y >= self.y && other.y < self.bottom() {
                    other.y
                } else {
                    return Self { x: self.x, y: self.y, w: Self::conv(0.0), h: Self::conv(0.0) };
                };
                let bottom = if self.bottom() > other.y && self.bottom() <= other.bottom() {
                    self.bottom()
                } else if other.bottom() > self.y && other.bottom() <= self.bottom() {
                    other.bottom()
                } else {
                    return Self { x: self.x, y: self.y, w: Self::conv(0.0), h: Self::conv(0.0) };
                };
                Self { x: left, y: top, w: right - left, h: bottom - top }
            }
            pub fn union(&self, other: &Self) -> Self {
                let x = if self.x < other.x { self.x } else { other.x };
                let y = if self.y < other.y { self.y } else { other.y };
                let r = if self.right() > other.right() { self.right() } else { other.right() };
                let b = if self.bottom() > other.bottom() { self.bottom() } else { other.bottom() };
                Self { x, y, w: r - x, h: b - y }
            }
            pub fn union_all<'a>(&self, others: impl IntoIterator<Item = &'a Self>) -> Self {
                others.into_iter().fold(*self, |acc, r| acc.union(r))
            }
            pub fn colliderect(&self, o: &Self) -> bool {
                let zero = Self::conv(0.0);
                self.w != zero
                    && self.h != zero
                    && o.w != zero
                    && o.h != zero
                    && min2(self.x, self.x + self.w) < max2(o.x, o.x + o.w)
                    && min2(self.y, self.y + self.h) < max2(o.y, o.y + o.h)
                    && max2(self.x, self.x + self.w) > min2(o.x, o.x + o.w)
                    && max2(self.y, self.y + self.h) > min2(o.y, o.y + o.h)
            }
            pub fn collidepoint(&self, px: impl Into<f64>, py: impl Into<f64>) -> bool {
                let (px, py) = (px.into(), py.into());
                let (x, y, r, b) = (self.x as f64, self.y as f64, self.right() as f64, self.bottom() as f64);
                px >= x && px < r && py >= y && py < b
            }
            pub fn collidev(&self, p: Vec2) -> bool {
                self.collidepoint(p.x, p.y)
            }
            pub fn contains(&self, o: &Self) -> bool {
                self.x <= o.x
                    && self.y <= o.y
                    && self.x + self.w >= o.x + o.w
                    && self.y + self.h >= o.y + o.h
                    && self.x + self.w > o.x
                    && self.y + self.h > o.y
            }
            /// The first of `others` this one overlaps (its index).
            pub fn collidelist(&self, others: &[Self]) -> Option<usize> {
                others.iter().position(|o| self.colliderect(o))
            }
            /// Does the line from `a` to `b` touch the rectangle (pygame's `clipline`, as a yes/no)?
            pub fn clipline(&self, a: Vec2, b: Vec2) -> bool {
                if self.w as f64 <= 0.0 || self.h as f64 <= 0.0 {
                    return false;
                }
                // Liang–Barsky against [left, right) × [top, bottom), the last pixel included as pygame does
                let (x0, y0, x1, y1) = (self.x as f64, self.y as f64, (self.x + self.w) as f64, (self.y + self.h) as f64);
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let (mut t0, mut t1) = (0.0f64, 1.0f64);
                for (p, q) in [(-dx, a.x - x0), (dx, x1 - a.x), (-dy, a.y - y0), (dy, y1 - a.y)] {
                    if p == 0.0 {
                        if q < 0.0 {
                            return false;
                        }
                    } else {
                        let r = q / p;
                        if p < 0.0 {
                            if r > t1 {
                                return false;
                            }
                            t0 = t0.max(r);
                        } else {
                            if r < t0 {
                                return false;
                            }
                            t1 = t1.min(r);
                        }
                    }
                }
                t0 <= t1
            }
            pub fn is_empty(&self) -> bool {
                let zero = Self::conv(0.0);
                self.w <= zero || self.h <= zero
            }
        }
    };
}

fn min2<T: PartialOrd>(a: T, b: T) -> T {
    if a < b { a } else { b }
}
fn max2<T: PartialOrd>(a: T, b: T) -> T {
    if a > b { a } else { b }
}

/// Where a rectangle is fixed to a point (pygame's keyword arguments `center=`, `topleft=` …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
    MidTop,
    MidBottom,
    MidLeft,
    MidRight,
}

rect_type!(Rect, i32, |v: i32| v / 2, |v: f64| v as i32);
rect_type!(FRect, f32, |v: f32| v / 2.0, |v: f64| v as f32);

impl Rect {
    pub fn to_f(&self) -> FRect {
        FRect { x: self.x as f32, y: self.y as f32, w: self.w as f32, h: self.h as f32 }
    }
}
impl FRect {
    /// What pygame does when an FRect is given where a Rect is needed: every value cut off towards zero.
    pub fn to_i(&self) -> Rect {
        Rect { x: self.x as i32, y: self.y as i32, w: self.w as i32, h: self.h as i32 }
    }
}
impl From<Rect> for FRect {
    fn from(r: Rect) -> Self {
        r.to_f()
    }
}
impl From<FRect> for Rect {
    fn from(r: FRect) -> Self {
        r.to_i()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_follows_pygame() {
        // Values from pygame-ce 2.5.8 (runtime\python.exe, 2026-11-18).
        let mut r = Rect::new(0, 0, 10, 10);
        r.set_left(3.7);
        assert_eq!(r.x, 3);
        r.set_left(-3.7);
        assert_eq!(r.x, -3);
        assert_eq!(Rect::new(0, 0, 11, 11).center(), (5, 5));
        assert_eq!(Rect::new(-5, 0, -3, 4).centerx(), -6);
        assert_eq!(Rect::new(0, 0, 10, 10).inflate(-3, -3), Rect::new(1, 1, 7, 7));
        assert_eq!(Rect::new(0, 0, 10, 10).inflate(3, 3), Rect::new(-1, -1, 13, 13));
        assert_eq!(Rect::new(0, 0, 10, 10).inflate(-2.5, 2.5), Rect::new(1, -1, 8, 12));
        assert_eq!(Rect::new(1.6, 2.5, 3.5, 4.5), Rect::new(1, 2, 3, 4));
        assert_eq!(FRect::new(0, 0, 10, 10).inflate(-3, -3), FRect::new(1.5, 1.5, 7, 7));
        assert_eq!(Rect::new(0, 0, 10, 10).moved(1.6, -1.6), Rect::new(1, -1, 10, 10));
        assert!(!Rect::new(0, 0, 0, 10).colliderect(&Rect::new(-5, 0, 10, 10)));
        assert!(Rect::new(0, 0, 10, 10).contains(&Rect::new(0, 0, 0, 0)));
        assert!(!Rect::new(0, 0, 10, 10).contains(&Rect::new(10, 10, 0, 0)));
        assert_eq!(Rect::new(0, 0, 30, 30).clamp(&Rect::new(0, 0, 11, 11)), Rect::new(-10, -10, 30, 30));
    }

    #[test]
    fn python_numbers() {
        assert_eq!(py_round(2.5), 2.0);
        assert_eq!(py_round(3.5), 4.0);
        assert_eq!(py_round(-2.5), -2.0);
        assert_eq!(py_mod(-1.0, 360.0), 359.0);
        assert_eq!(py_div(-7, 2), -4);
        assert_eq!(py_div(7, 2), 3);
    }
}
