//! Pictures and drawing – what pygame's `Surface`, `pygame.draw` and `pygame.transform` did for the game, on
//! tiny-skia.
//!
//! The rules are pygame's, because the game's look rests on them:
//!
//! - **Drawing replaces pixels.** A shape drawn with a see-through colour on a see-through picture makes those
//!   pixels see-through; it does not mix with what was there. On an opaque picture the colour's alpha is ignored.
//!   Mixing happens only when a picture is **blitted** onto another one (with its own alpha and `set_alpha`).
//! - **No smoothing**: shapes have hard pixel edges, as `pygame.draw` drew them. Text is smoothed, as
//!   `font.render(…, True, …)` did.
//! - A point `(x, y)` of a line or polygon means the pixel at column x, row y – its centre is drawn through.
//!
//! **Channel order.** The window shows memory in Windows' order blue, green, red. To draw straight into it without
//! copying, every picture keeps that order: colours are swapped when they are handed to tiny-skia and when a PNG is
//! loaded. Nothing outside this file sees it.

use tiny_skia::{
    BlendMode, Color as SkColor, FillRule, FilterQuality, Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Shader,
    Stroke, Transform,
};

use super::geom::{Anchor, Rect, Vec2};

/// A colour as pygame takes it: red, green, blue, alpha (0 … 255).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b, a: 255 }
}
pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

impl Color {
    /// From computed numbers – cut off and kept within 0 … 255, as pygame does with a float colour.
    pub fn f(r: f64, g: f64, b: f64) -> Color {
        Color { r: chan(r), g: chan(g), b: chan(b), a: 255 }
    }
    pub fn fa(r: f64, g: f64, b: f64, a: f64) -> Color {
        Color { r: chan(r), g: chan(g), b: chan(b), a: chan(a) }
    }
    pub fn with_alpha(self, a: impl Into<f64>) -> Color {
        Color { a: chan(a.into()), ..self }
    }
    /// From a list in a data file: `[r, g, b]` or `[r, g, b, a]`.
    pub fn from_json(v: &serde_json::Value) -> Option<Color> {
        let list = v.as_array()?;
        if list.len() < 3 || list.len() > 4 {
            return None;
        }
        let n: Vec<f64> = list.iter().map(|x| x.as_f64()).collect::<Option<_>>()?;
        if n.iter().any(|x| !(0.0..=255.0).contains(x)) {
            return None;
        }
        Some(Color::fa(n[0], n[1], n[2], *n.get(3).unwrap_or(&255.0)))
    }
    pub fn tuple(self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }
    /// Mixed with `other`: 0 = this colour, 1 = the other one.
    pub fn lerp(self, other: Color, t: f64) -> Color {
        let t = t.clamp(0.0, 1.0);
        let m = |a: u8, b: u8| a as f64 + (b as f64 - a as f64) * t;
        Color::fa(m(self.r, other.r), m(self.g, other.g), m(self.b, other.b), m(self.a, other.a))
    }
    fn sk(self, opaque: bool) -> SkColor {
        // Swapped on purpose: the pictures hold blue, green, red (see the top of this file).
        SkColor::from_rgba8(self.b, self.g, self.r, if opaque { 255 } else { self.a })
    }
}

fn chan(v: f64) -> u8 {
    if v.is_nan() { 0 } else { v.clamp(0.0, 255.0) as u8 }
}

/// How a blit mixes the picture in (pygame's `special_flags`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    Normal,
    /// `BLEND_RGB_ADD`: the colours are added up.
    Add,
    /// `BLEND_RGBA_MAX`: the brighter one of each channel wins.
    Max,
}

enum Store {
    Owned(Pixmap),
    /// The window's bitmap: memory that Windows owns. `stride` pixels per row; only `w` × `h` of it are shown.
    Window { ptr: *mut u8, stride: u32, rows: u32 },
}

/// A picture to draw on – pygame's `Surface`.
pub struct Surface {
    store: Store,
    w: u32,
    h: u32,
    /// Has its own alpha per pixel (`pygame.SRCALPHA`). Without it, every pixel is opaque.
    pub srcalpha: bool,
    /// `set_alpha`: how visible the whole picture is when it is blitted (255 = as it is).
    pub alpha: u8,
}

// The window's memory is only touched by the thread that draws.
unsafe impl Send for Surface {}

impl Surface {
    /// An opaque picture, black.
    pub fn new(w: i32, h: i32) -> Surface {
        let mut s = Surface { store: Store::Owned(Pixmap::new(w.max(1) as u32, h.max(1) as u32).unwrap()), w: w.max(0) as u32, h: h.max(0) as u32, srcalpha: false, alpha: 255 };
        s.fill(rgb(0, 0, 0));
        s
    }
    /// A see-through picture (`pygame.SRCALPHA`), cleared.
    pub fn new_alpha(w: i32, h: i32) -> Surface {
        Surface { store: Store::Owned(Pixmap::new(w.max(1) as u32, h.max(1) as u32).unwrap()), w: w.max(0) as u32, h: h.max(0) as u32, srcalpha: true, alpha: 255 }
    }
    /// The window's bitmap as a picture. Safety: `ptr` points to `stride * rows * 4` bytes that stay valid and are
    /// used by nobody else while the surface lives; `w <= stride`, `h <= rows`.
    pub unsafe fn window(ptr: *mut u8, stride: u32, rows: u32, w: u32, h: u32) -> Surface {
        Surface { store: Store::Window { ptr, stride, rows }, w, h, srcalpha: false, alpha: 255 }
    }
    /// A loaded picture file (PNG). `None` if it cannot be read.
    pub fn load_png(path: &std::path::Path) -> Option<Surface> {
        let mut p = Pixmap::load_png(path).ok()?;
        swap_red_blue(p.data_mut());
        let (w, h) = (p.width(), p.height());
        Some(Surface { store: Store::Owned(p), w, h, srcalpha: true, alpha: 255 })
    }
    /// A see-through picture of one colour whose visibility per pixel is `coverage` (0 … 255, `w` × `h`) – text.
    pub fn from_coverage(w: i32, h: i32, coverage: &[u8], color: Color) -> Surface {
        let mut s = Surface::new_alpha(w, h);
        let (data, _) = s.pixels_mut();
        for (px, &v) in data.chunks_exact_mut(4).zip(coverage) {
            let a = v as u32 * color.a as u32 / 255;
            px[0] = (color.b as u32 * a / 255) as u8;
            px[1] = (color.g as u32 * a / 255) as u8;
            px[2] = (color.r as u32 * a / 255) as u8;
            px[3] = a as u8;
        }
        s
    }
    pub fn from_pixmap_rgba(mut p: Pixmap) -> Surface {
        swap_red_blue(p.data_mut());
        let (w, h) = (p.width(), p.height());
        Surface { store: Store::Owned(p), w, h, srcalpha: true, alpha: 255 }
    }

    pub fn width(&self) -> i32 {
        self.w as i32
    }
    pub fn height(&self) -> i32 {
        self.h as i32
    }
    pub fn size(&self) -> (i32, i32) {
        (self.w as i32, self.h as i32)
    }
    /// `get_rect()`: at 0, 0 with the picture's size.
    pub fn get_rect(&self) -> Rect {
        Rect::new(0, 0, self.w as i32, self.h as i32)
    }
    pub fn rect_at(&self, anchor: Anchor, x: impl Into<f64>, y: impl Into<f64>) -> Rect {
        self.get_rect().placed(anchor, x, y)
    }
    pub fn set_alpha(&mut self, a: impl Into<f64>) {
        self.alpha = chan(a.into());
    }

    fn pm(&mut self) -> Option<PixmapMut<'_>> {
        if self.w == 0 || self.h == 0 {
            return None;
        }
        match &mut self.store {
            Store::Owned(p) => Some(p.as_mut()),
            Store::Window { ptr, stride, rows } => {
                let data = unsafe { std::slice::from_raw_parts_mut(*ptr, (*stride * *rows * 4) as usize) };
                let len = (*stride * self.h * 4) as usize;
                PixmapMut::from_bytes(&mut data[..len], *stride, self.h)
            }
        }
    }
    /// The pixels as tiny-skia sees them (premultiplied, blue/green/red/alpha), with the row length in pixels.
    fn pixels(&self) -> (&[u8], u32) {
        match &self.store {
            Store::Owned(p) => (p.data(), p.width()),
            Store::Window { ptr, stride, .. } => (unsafe { std::slice::from_raw_parts(*ptr, (*stride * self.h * 4) as usize) }, *stride),
        }
    }
    fn pixels_mut(&mut self) -> (&mut [u8], u32) {
        let h = self.h;
        match &mut self.store {
            Store::Owned(p) => {
                let w = p.width();
                (p.data_mut(), w)
            }
            Store::Window { ptr, stride, .. } => (unsafe { std::slice::from_raw_parts_mut(*ptr, (*stride * h * 4) as usize) }, *stride),
        }
    }
    fn paint(&self, color: Color) -> Paint<'static> {
        let mut paint = Paint::default();
        paint.shader = Shader::SolidColor(color.sk(!self.srcalpha));
        paint.anti_alias = false;
        paint.blend_mode = BlendMode::Source;
        paint
    }
    /// The clip of the shown part (the window's bitmap may be wider than the window).
    fn clip_mask(&self) -> Option<tiny_skia::Mask> {
        None
    }

    // ---------- filling and pixels ----------

    pub fn fill(&mut self, color: Color) {
        let r = self.get_rect();
        self.fill_rect(r, color);
    }
    /// `fill(color, rect)` – also the fast path of a filled `draw.rect`.
    pub fn fill_rect(&mut self, r: Rect, color: Color) {
        let r = r.clip(&self.get_rect());
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        let c = color.sk(!self.srcalpha).premultiply().to_color_u8();
        let px = [c.red(), c.green(), c.blue(), c.alpha()];
        let (data, stride) = self.pixels_mut();
        for y in r.y..r.bottom() {
            let row = (y as u32 * stride) as usize * 4;
            for x in r.x..r.right() {
                let i = row + x as usize * 4;
                data[i..i + 4].copy_from_slice(&px);
            }
        }
    }
    pub fn get_at(&self, x: i32, y: i32) -> Color {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return Color::default();
        }
        let (data, stride) = self.pixels();
        let i = ((y as u32 * stride + x as u32) * 4) as usize;
        let c = tiny_skia::PremultipliedColorU8::from_rgba(data[i], data[i + 1], data[i + 2], data[i + 3]).map(|c| c.demultiply()).unwrap_or(tiny_skia::ColorU8::from_rgba(0, 0, 0, 0));
        rgba(c.blue(), c.green(), c.red(), c.alpha())
    }
    pub fn set_at(&mut self, x: i32, y: i32, color: Color) {
        self.fill_rect(Rect::new(x, y, 1, 1), color);
    }

    // ---------- pygame.draw ----------

    /// `draw.rect(surface, color, rect)` filled, or `width` pixels of outline inside the rect.
    pub fn rect(&mut self, color: Color, r: Rect, width: i32) {
        self.rect_round(color, r, width, Radii::all(0));
    }
    /// `draw.rect` with rounded corners (`border_radius`, or each corner on its own).
    pub fn rect_round(&mut self, color: Color, r: Rect, width: i32, radii: Radii) {
        if r.w <= 0 || r.h <= 0 || width < 0 {
            return;
        }
        let radii = radii.fitted(r.w, r.h);
        if radii.is_zero() {
            if width == 0 || width * 2 >= r.w || width * 2 >= r.h {
                self.fill_rect(r, color);
            } else {
                self.fill_rect(Rect::new(r.x, r.y, r.w, width), color);
                self.fill_rect(Rect::new(r.x, r.bottom() - width, r.w, width), color);
                self.fill_rect(Rect::new(r.x, r.y + width, width, r.h - 2 * width), color);
                self.fill_rect(Rect::new(r.right() - width, r.y + width, width, r.h - 2 * width), color);
            }
            return;
        }
        let mut pb = PathBuilder::new();
        rounded(&mut pb, r.x as f32, r.y as f32, r.w as f32, r.h as f32, radii);
        if width > 0 && width * 2 < r.w && width * 2 < r.h {
            let inner = Radii { tl: (radii.tl - width).max(0), tr: (radii.tr - width).max(0), bl: (radii.bl - width).max(0), br: (radii.br - width).max(0) };
            let wf = width as f32;
            rounded(&mut pb, r.x as f32 + wf, r.y as f32 + wf, r.w as f32 - 2.0 * wf, r.h as f32 - 2.0 * wf, inner);
        }
        let Some(path) = pb.finish() else { return };
        let paint = self.paint(color);
        let mask = self.clip_mask();
        if let Some(mut pm) = self.pm() {
            pm.fill_path(&path, &paint, FillRule::EvenOdd, Transform::identity(), mask.as_ref());
        }
    }
    /// `draw.circle`: filled, or a ring `width` pixels wide inside the radius.
    pub fn circle(&mut self, color: Color, center: impl Into<Vec2>, radius: f64, width: i32) {
        let c = center.into();
        if radius < 1.0 || width < 0 {
            return;
        }
        let (cx, cy) = (c.x as i32 as f32, c.y as i32 as f32);
        let r = radius as i32 as f32;
        let mut pb = PathBuilder::new();
        pb.push_circle(cx, cy, r);
        if width > 0 && (width as f32) < r {
            pb.push_circle(cx, cy, r - width as f32);
        }
        let Some(path) = pb.finish() else { return };
        let paint = self.paint(color);
        if let Some(mut pm) = self.pm() {
            pm.fill_path(&path, &paint, FillRule::EvenOdd, Transform::identity(), None);
        }
    }
    /// `draw.ellipse` in `r`: filled, or `width` pixels of outline.
    pub fn ellipse(&mut self, color: Color, r: Rect, width: i32) {
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        let mut pb = PathBuilder::new();
        if let Some(o) = tiny_skia::Rect::from_xywh(r.x as f32, r.y as f32, r.w as f32, r.h as f32) {
            pb.push_oval(o);
        }
        let wf = width as f32;
        if width > 0 && width * 2 < r.w && width * 2 < r.h {
            if let Some(i) = tiny_skia::Rect::from_xywh(r.x as f32 + wf, r.y as f32 + wf, r.w as f32 - 2.0 * wf, r.h as f32 - 2.0 * wf) {
                pb.push_oval(i);
            }
        }
        let Some(path) = pb.finish() else { return };
        let paint = self.paint(color);
        if let Some(mut pm) = self.pm() {
            pm.fill_path(&path, &paint, FillRule::EvenOdd, Transform::identity(), None);
        }
    }
    /// `draw.line` from pixel to pixel, `width` pixels thick.
    pub fn line(&mut self, color: Color, a: impl Into<Vec2>, b: impl Into<Vec2>, width: i32) {
        self.lines(color, false, &[a.into(), b.into()], width);
    }
    /// `draw.lines`: one line through all points, closed back to the first if `closed`.
    pub fn lines(&mut self, color: Color, closed: bool, points: &[Vec2], width: i32) {
        if points.len() < 2 || width < 1 {
            return;
        }
        let mut pb = PathBuilder::new();
        let p = |v: &Vec2| (v.x as i32 as f32 + 0.5, v.y as i32 as f32 + 0.5);
        let (x, y) = p(&points[0]);
        pb.move_to(x, y);
        for v in &points[1..] {
            let (x, y) = p(v);
            pb.line_to(x, y);
        }
        if closed {
            pb.close();
        }
        let single = points.len() == 2 && points[0].ints() == points[1].ints();
        let Some(path) = pb.finish() else {
            if single {
                let (x, y) = points[0].ints();
                self.fill_rect(Rect::new(x - (width - 1) / 2, y - (width - 1) / 2, width, width), color);
            }
            return;
        };
        let paint = self.paint(color);
        // Wide enough that a 1-pixel line hits every pixel centre it passes (pygame's lines have no gaps).
        let stroke = Stroke { width: if width == 1 { 1.0 } else { width as f32 }, line_cap: tiny_skia::LineCap::Square, ..Stroke::default() };
        let stroke = if width == 1 { Stroke { line_cap: tiny_skia::LineCap::Square, width: 1.0, ..stroke } } else { Stroke { line_cap: tiny_skia::LineCap::Butt, ..stroke } };
        if let Some(mut pm) = self.pm() {
            pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
    /// `draw.polygon`: filled, or its outline `width` pixels thick.
    pub fn polygon(&mut self, color: Color, points: &[Vec2], width: i32) {
        if points.len() < 3 {
            return;
        }
        if width > 0 {
            self.lines(color, true, points, width);
            return;
        }
        let mut pb = PathBuilder::new();
        // Through the pixel centres, grown by half a pixel – pygame fills the outline's pixels, too.
        let c = points.iter().fold(Vec2::ZERO, |a, p| a + *p) / points.len() as f64;
        let p = |v: &Vec2| {
            let (x, y) = (v.x as i32 as f64 + 0.5, v.y as i32 as f64 + 0.5);
            let d = Vec2::new(x - c.x - 0.5, y - c.y - 0.5);
            let grow = if d.length() > 0.0 { d.normalize() * 0.5 } else { Vec2::ZERO };
            ((x + grow.x) as f32, (y + grow.y) as f32)
        };
        let (x, y) = p(&points[0]);
        pb.move_to(x, y);
        for v in &points[1..] {
            let (x, y) = p(v);
            pb.line_to(x, y);
        }
        pb.close();
        let Some(path) = pb.finish() else { return };
        let paint = self.paint(color);
        if let Some(mut pm) = self.pm() {
            pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }
    /// `draw.arc` in the ellipse `r`, from `start` to `stop` (radians, counter-clockwise on the screen as in
    /// pygame: 0 = right, π/2 = up).
    pub fn arc(&mut self, color: Color, r: Rect, start: f64, stop: f64, width: i32) {
        if r.w <= 0 || r.h <= 0 || width < 1 {
            return;
        }
        let mut stop = stop;
        while stop < start {
            stop += std::f64::consts::TAU;
        }
        let (cx, cy) = (r.x as f64 + r.w as f64 / 2.0, r.y as f64 + r.h as f64 / 2.0);
        let w = width as f64;
        let (rx, ry) = (r.w as f64 / 2.0 - w / 2.0, r.h as f64 / 2.0 - w / 2.0);
        if rx <= 0.0 || ry <= 0.0 {
            return;
        }
        let steps = (((stop - start) * rx.max(ry)) / 2.0).ceil().clamp(2.0, 720.0) as usize;
        let mut pb = PathBuilder::new();
        for i in 0..=steps {
            let a = start + (stop - start) * i as f64 / steps as f64;
            let (x, y) = ((cx + rx * a.cos()) as f32, (cy - ry * a.sin()) as f32);
            if i == 0 { pb.move_to(x, y) } else { pb.line_to(x, y) }
        }
        let Some(path) = pb.finish() else { return };
        let paint = self.paint(color);
        let stroke = Stroke { width: w as f32, ..Stroke::default() };
        if let Some(mut pm) = self.pm() {
            pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }

    // ---------- blit and transform ----------

    /// `blit(picture, (x, y))`: the picture mixed in with its alpha and `set_alpha`.
    pub fn blit(&mut self, src: &Surface, x: i32, y: i32) {
        self.blit_mode(src, x, y, Blend::Normal);
    }
    pub fn blit_at(&mut self, src: &Surface, anchor: Anchor, x: impl Into<f64>, y: impl Into<f64>) -> Rect {
        let r = src.rect_at(anchor, x, y);
        self.blit(src, r.x, r.y);
        r
    }
    pub fn blit_mode(&mut self, src: &Surface, x: i32, y: i32, mode: Blend) {
        if src.w == 0 || src.h == 0 || src.alpha == 0 && mode == Blend::Normal {
            return;
        }
        let opacity = src.alpha as f32 / 255.0;
        let srcalpha = self.srcalpha;
        let blend = match mode {
            Blend::Normal if !src.srcalpha && src.alpha == 255 => BlendMode::Source,
            Blend::Normal => BlendMode::SourceOver,
            Blend::Add => BlendMode::Plus,
            Blend::Max => BlendMode::Lighten,
        };
        let paint = PixmapPaint { opacity: if mode == Blend::Normal { opacity } else { 1.0 }, blend_mode: blend, quality: FilterQuality::Nearest };
        let src_ref = src.as_ref_pixmap();
        let Some(mut pm) = self.pm() else { return };
        if let Some(src_ref) = src_ref {
            pm.draw_pixmap(x, y, src_ref.as_ref(), &paint, Transform::identity(), None);
        }
        if !srcalpha {
            // An opaque picture stays opaque, whatever was mixed in.
            drop(pm);
            self.make_opaque(Rect::new(x, y, src.w as i32, src.h as i32));
        }
    }
    fn make_opaque(&mut self, r: Rect) {
        let r = r.clip(&self.get_rect());
        let (data, stride) = self.pixels_mut();
        for y in r.y..r.bottom() {
            let row = (y as u32 * stride) as usize * 4;
            for x in r.x..r.right() {
                data[row + x as usize * 4 + 3] = 255;
            }
        }
    }
    /// A pixmap that shows this picture (shared for an owned one, copied out of the window's memory).
    fn as_ref_pixmap(&self) -> Option<PixRef<'_>> {
        match &self.store {
            Store::Owned(p) if p.width() == self.w && p.height() == self.h => Some(PixRef::Borrowed(p)),
            _ => {
                let mut p = Pixmap::new(self.w.max(1), self.h.max(1))?;
                let (data, stride) = self.pixels();
                for y in 0..self.h as usize {
                    let s = y * stride as usize * 4;
                    let d = y * self.w as usize * 4;
                    p.data_mut()[d..d + self.w as usize * 4].copy_from_slice(&data[s..s + self.w as usize * 4]);
                }
                Some(PixRef::Owned(p))
            }
        }
    }
    /// Saves the picture as a PNG file (tests: to look at what was drawn).
    pub fn save_png(&self, path: &std::path::Path) -> bool {
        let Some(mut p) = Pixmap::new(self.w.max(1), self.h.max(1)) else { return false };
        let (data, stride) = self.pixels();
        for y in 0..self.h as usize {
            let s = y * stride as usize * 4;
            let d = y * self.w as usize * 4;
            p.data_mut()[d..d + self.w as usize * 4].copy_from_slice(&data[s..s + self.w as usize * 4]);
        }
        swap_red_blue(p.data_mut());
        if !self.srcalpha {
            for px in p.data_mut().chunks_exact_mut(4) {
                px[3] = 255;
            }
        }
        p.save_png(path).is_ok()
    }
    /// A copy (`Surface.copy()`), as an owned picture.
    pub fn copy(&self) -> Surface {
        let p = match self.as_ref_pixmap() {
            Some(PixRef::Borrowed(p)) => p.clone(),
            Some(PixRef::Owned(p)) => p,
            None => Pixmap::new(1, 1).unwrap(),
        };
        Surface { store: Store::Owned(p), w: self.w, h: self.h, srcalpha: self.srcalpha, alpha: self.alpha }
    }
    /// `transform.rotate`: turned counter-clockwise by `degrees`, in a picture just big enough for it.
    pub fn rotated(&self, degrees: f64) -> Surface {
        let rad = degrees.to_radians();
        let (s, c) = (rad.sin().abs(), rad.cos().abs());
        let (w, h) = (self.w as f64, self.h as f64);
        let (nw, nh) = ((w * c + h * s).ceil() as i32, (w * s + h * c).ceil() as i32);
        let mut out = Surface::new_alpha(nw, nh);
        out.alpha = self.alpha;
        let t = Transform::from_translate(nw as f32 / 2.0, nh as f32 / 2.0).pre_rotate(-degrees as f32).pre_translate(-(w as f32) / 2.0, -(h as f32) / 2.0);
        if let (Some(src), Some(mut pm)) = (self.as_ref_pixmap(), out.pm()) {
            pm.draw_pixmap(0, 0, src.as_ref(), &PixmapPaint { quality: FilterQuality::Nearest, ..PixmapPaint::default() }, t, None);
        }
        out
    }
    /// `transform.smoothscale`: stretched to `w` × `h`, smoothly.
    pub fn smoothscaled(&self, w: i32, h: i32) -> Surface {
        let mut out = Surface::new_alpha(w.max(1), h.max(1));
        out.alpha = self.alpha;
        let t = Transform::from_scale(w as f32 / self.w.max(1) as f32, h as f32 / self.h.max(1) as f32);
        if let (Some(src), Some(mut pm)) = (self.as_ref_pixmap(), out.pm()) {
            pm.draw_pixmap(0, 0, src.as_ref(), &PixmapPaint { quality: FilterQuality::Bicubic, ..PixmapPaint::default() }, t, None);
        }
        out
    }
    /// Every visible pixel turned to `color` (keeping its alpha) – the white flash of a hit enemy.
    pub fn tinted(&self, color: Color) -> Surface {
        let mut out = self.copy();
        let (data, _) = out.pixels_mut();
        for px in data.chunks_exact_mut(4) {
            let a = px[3] as u32;
            px[0] = (color.b as u32 * a / 255) as u8;
            px[1] = (color.g as u32 * a / 255) as u8;
            px[2] = (color.r as u32 * a / 255) as u8;
        }
        out
    }
    /// The alpha of every pixel (0 = see-through) – what `pygame.mask.from_surface` reads.
    pub fn alpha_at(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return 0;
        }
        if !self.srcalpha {
            return 255;
        }
        let (data, stride) = self.pixels();
        data[((y as u32 * stride + x as u32) * 4 + 3) as usize]
    }
}

enum PixRef<'a> {
    Borrowed(&'a Pixmap),
    Owned(Pixmap),
}
impl PixRef<'_> {
    fn as_ref(&self) -> tiny_skia::PixmapRef<'_> {
        match self {
            PixRef::Borrowed(p) => p.as_ref(),
            PixRef::Owned(p) => p.as_ref(),
        }
    }
}

fn swap_red_blue(data: &mut [u8]) {
    for px in data.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
}

/// The corner radii of a rounded rect (`border_radius`, `border_top_left_radius` …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Radii {
    pub tl: i32,
    pub tr: i32,
    pub bl: i32,
    pub br: i32,
}
impl Radii {
    pub const fn all(r: i32) -> Radii {
        Radii { tl: r, tr: r, bl: r, br: r }
    }
    fn is_zero(&self) -> bool {
        self.tl <= 0 && self.tr <= 0 && self.bl <= 0 && self.br <= 0
    }
    /// As pygame does: no corner bigger than half the shorter side.
    fn fitted(self, w: i32, h: i32) -> Radii {
        let m = w.min(h) / 2;
        Radii { tl: self.tl.clamp(0, m), tr: self.tr.clamp(0, m), bl: self.bl.clamp(0, m), br: self.br.clamp(0, m) }
    }
}

fn rounded(pb: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32, r: Radii) {
    // A quarter circle as a cubic curve: the control points lie this far along the tangents.
    const K: f32 = 0.552_284_8;
    let (tl, tr, bl, br) = (r.tl as f32, r.tr as f32, r.bl as f32, r.br as f32);
    pb.move_to(x + tl, y);
    pb.line_to(x + w - tr, y);
    if tr > 0.0 {
        pb.cubic_to(x + w - tr + tr * K, y, x + w, y + tr - tr * K, x + w, y + tr);
    }
    pb.line_to(x + w, y + h - br);
    if br > 0.0 {
        pb.cubic_to(x + w, y + h - br + br * K, x + w - br + br * K, y + h, x + w - br, y + h);
    }
    pb.line_to(x + bl, y + h);
    if bl > 0.0 {
        pb.cubic_to(x + bl - bl * K, y + h, x, y + h - bl + bl * K, x, y + h - bl);
    }
    pb.line_to(x, y + tl);
    if tl > 0.0 {
        pb.cubic_to(x, y + tl - tl * K, x + tl - tl * K, y, x + tl, y);
    }
    pb.close();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_replaces_and_blitting_mixes() {
        let mut s = Surface::new(10, 10);
        s.fill(rgb(10, 20, 30));
        assert_eq!(s.get_at(3, 3), rgb(10, 20, 30));
        // On an opaque picture the alpha of the colour does not count.
        s.rect(rgba(200, 100, 50, 10), Rect::new(0, 0, 5, 5), 0);
        assert_eq!(s.get_at(1, 1), rgb(200, 100, 50));
        // A see-through picture blitted at half alpha mixes.
        let mut t = Surface::new_alpha(2, 2);
        t.fill(rgba(255, 255, 255, 128));
        s.blit(&t, 8, 8);
        let c = s.get_at(8, 8);
        assert!((c.r as i32 - 132).abs() <= 2 && c.a == 255, "{c:?}");
        assert_eq!(s.get_at(7, 7), rgb(10, 20, 30));
    }

    #[test]
    fn outlines_lie_inside() {
        let mut s = Surface::new(20, 20);
        s.rect(rgb(255, 255, 255), Rect::new(2, 2, 10, 10), 2);
        assert_eq!(s.get_at(2, 2), rgb(255, 255, 255));
        assert_eq!(s.get_at(3, 3), rgb(255, 255, 255));
        assert_eq!(s.get_at(4, 4), rgb(0, 0, 0));
        assert_eq!(s.get_at(11, 11), rgb(255, 255, 255));
        assert_eq!(s.get_at(12, 12), rgb(0, 0, 0));
    }

    #[test]
    fn a_circle_is_twice_its_radius_wide() {
        let mut s = Surface::new(30, 30);
        s.circle(rgb(255, 0, 0), (15, 15), 5.0, 0);
        let row: Vec<i32> = (0..30).filter(|&x| s.get_at(x, 15).r == 255).collect();
        assert_eq!((row[0], *row.last().unwrap()), (10, 19));
    }

    #[test]
    fn lines_touch_both_ends() {
        let mut s = Surface::new(20, 20);
        s.line(rgb(0, 255, 0), (2, 3), (12, 3), 1);
        assert_eq!(s.get_at(2, 3).g, 255);
        assert_eq!(s.get_at(12, 3).g, 255);
        assert_eq!(s.get_at(13, 3).g, 0);
        assert_eq!(s.get_at(5, 4).g, 0);
    }
}
