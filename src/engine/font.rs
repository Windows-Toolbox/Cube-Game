//! Text – what `pygame.font.Font(None, size)` did: pygame's built-in font (FreeSans Bold, `assets\freesansbold.ttf`,
//! compiled in), smoothed. pygame draws that font at 0.6875 of the asked size (`Font(None, 20)` is 13 pixels
//! high), so the sizes in the game's code stay the ones the Python version used.

use std::cell::RefCell;
use std::collections::HashMap;

use fontdue::{Font, FontSettings};

use super::geom::{Anchor, Rect};
use super::gfx::{Color, Surface};

const FONT: &[u8] = include_bytes!("../../assets/freesansbold.ttf");
/// pygame's factor for its built-in font.
const SCALE: f64 = 0.6875;

struct Glyph {
    /// Coverage 0 … 255 per pixel, `w` × `h`.
    coverage: Vec<u8>,
    w: usize,
    h: usize,
    /// Where the glyph's picture lies relative to the pen: left of it and above the baseline.
    left: i32,
    top: i32,
    advance: f32,
}

struct Fonts {
    font: Font,
    glyphs: HashMap<(char, u32), Glyph>,
}

thread_local! {
    static FONTS: RefCell<Fonts> = RefCell::new(Fonts {
        font: Font::from_bytes(FONT, FontSettings::default()).expect("the compiled-in font"),
        glyphs: HashMap::new(),
    });
}

/// The pixel size the font is drawn at for a size the game asks for.
fn px(size: i32) -> u32 {
    ((size as f64 * SCALE) as i32).max(1) as u32
}

impl Fonts {
    fn glyph(&mut self, c: char, px: u32) -> &Glyph {
        let font = &self.font;
        self.glyphs.entry((c, px)).or_insert_with(|| {
            let (m, coverage) = font.rasterize(c, px as f32);
            Glyph { coverage, w: m.width, h: m.height, left: m.xmin, top: m.ymin + m.height as i32, advance: m.advance_width }
        })
    }
    /// Ascent and descent in whole pixels, as SDL_ttf rounds them (both up: 10.4 → 11, −2.6 → −2).
    fn metrics(&self, px: u32) -> (i32, i32) {
        let lm = self.font.horizontal_line_metrics(px as f32).expect("a horizontal font");
        (lm.ascent.ceil() as i32, lm.descent.ceil() as i32)
    }
    /// The pen positions of the characters and the width of the whole line.
    fn layout(&mut self, text: &str, px: u32) -> (Vec<(char, i32)>, i32) {
        let mut pen = 0.0f32;
        let mut out = Vec::with_capacity(text.len());
        let mut prev: Option<char> = None;
        for c in text.chars() {
            if let Some(p) = prev {
                pen += self.font.horizontal_kern(p, c, px as f32).unwrap_or(0.0);
            }
            out.push((c, pen as i32));
            // SDL_ttf moves the pen by whole pixels (hinted advances).
            pen += self.glyph(c, px).advance.round();
            prev = Some(c);
        }
        (out, pen as i32)
    }
}

/// Width and height of `text` in the game's font size `size` – pygame's `font.size(text)`.
pub fn text_size(text: &str, size: i32) -> (i32, i32) {
    let px = px(size);
    FONTS.with(|f| {
        let mut f = f.borrow_mut();
        let (asc, desc) = f.metrics(px);
        let (_, w) = f.layout(text, px);
        (w, asc - desc)
    })
}

/// The height of a line – pygame's `font.get_height()`.
pub fn line_height(size: i32) -> i32 {
    text_size("", size).1
}

/// `font.render(text, True, color)`: the text on a see-through picture just big enough for it.
pub fn render(text: &str, size: i32, color: Color) -> Surface {
    let px = px(size);
    FONTS.with(|f| {
        let mut f = f.borrow_mut();
        let (asc, desc) = f.metrics(px);
        let (pens, w) = f.layout(text, px);
        let h = asc - desc;
        let mut cov = vec![0u8; (w.max(1) * h.max(1)) as usize];
        for (c, pen) in pens {
            let g = f.glyph(c, px);
            let gx = pen + g.left;
            let gy = asc - g.top;
            for row in 0..g.h {
                let y = gy + row as i32;
                if y < 0 || y >= h {
                    continue;
                }
                for col in 0..g.w {
                    let x = gx + col as i32;
                    if x < 0 || x >= w {
                        continue;
                    }
                    let v = g.coverage[row * g.w + col];
                    let i = (y * w + x) as usize;
                    cov[i] = cov[i].max(v);
                }
            }
        }
        Surface::from_coverage(w.max(1), h.max(1), &cov, color)
    })
}

thread_local! {
    /// Rendered texts: most of what the game writes is the same in every frame.
    static RENDERED: RefCell<HashMap<(String, i32, Color), Surface>> = RefCell::new(HashMap::new());
}

/// `ui.draw_text`: draws `text` with its `anchor` at `x, y` and returns where it landed. `alpha` < 255 fades it.
pub fn draw_text(surface: &mut Surface, text: &str, size: i32, color: Color, alpha: u8, anchor: Anchor, x: impl Into<f64>, y: impl Into<f64>) -> Rect {
    RENDERED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() > 3000 {
            cache.clear();
        }
        let key = (text.to_string(), size, color);
        let image = cache.entry(key).or_insert_with(|| render(text, size, color));
        image.set_alpha(alpha);
        let r = image.get_rect().placed(anchor, x, y);
        surface.blit(image, r.x, r.y);
        r
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_as_in_pygame() {
        // pygame-ce 2.5.8: Font(None, 20).size("Hello W") == (50, 13), Font(None, 26) → (64, 17).
        let (w20, h20) = text_size("Hello W", 20);
        let (w26, h26) = text_size("Hello W", 26);
        assert_eq!((h20, h26), (13, 17));
        assert!((w20 - 50).abs() <= 1 && (w26 - 64).abs() <= 1, "{w20} {w26}");
    }
}

