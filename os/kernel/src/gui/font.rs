use crate::gui::assets;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FontFace {
    Ui,
    UiBold,
    Mono,
}

pub struct GlyphData<'a> {
    pub width: i32,
    pub height: i32,
    pub advance: i32,
    pub y_offset: i32,
    pub alpha: &'a [u8],
}

const META_START: usize = 4;
const GLYPH_COUNT: usize = 95;
const META_STRIDE: usize = 8;
const PIXELS_START: usize = META_START + GLYPH_COUNT * META_STRIDE;

#[inline]
fn font_bytes(face: FontFace) -> &'static [u8] {
    match face {
        FontFace::Ui => assets::FONT_UI_BYTES,
        FontFace::UiBold => assets::FONT_BOLD_BYTES,
        FontFace::Mono => assets::FONT_MONO_BYTES,
    }
}

#[inline]
pub fn line_height(face: FontFace) -> i32 {
    let data = font_bytes(face);
    if data.is_empty() {
        16
    } else {
        data[0] as i32
    }
}

pub fn glyph(face: FontFace, ch: char) -> Option<GlyphData<'static>> {
    let code = ch as u32;
    let idx = if (32..=126).contains(&code) {
        (code - 32) as usize
    } else {
        ('?' as usize) - 32
    };

    let data = font_bytes(face);
    let m = META_START + idx * META_STRIDE;
    if m + META_STRIDE > data.len() || PIXELS_START > data.len() {
        return None;
    }

    let offset = u32::from_le_bytes([data[m], data[m + 1], data[m + 2], data[m + 3]]) as usize;
    let width = data[m + 4] as usize;
    let height = data[m + 5] as usize;
    let advance = data[m + 6] as i32;
    let y_offset = (data[m + 7] as i8) as i32;

    let start = PIXELS_START + offset;
    let end = start + width * height;
    if end > data.len() {
        return None;
    }

    Some(GlyphData {
        width: width as i32,
        height: height as i32,
        advance,
        y_offset,
        alpha: &data[start..end],
    })
}

pub fn measure_text(face: FontFace, text: &str) -> i32 {
    measure_text_scaled(face, text, false)
}

pub fn measure_text_scaled(face: FontFace, text: &str, large: bool) -> i32 {
    let mut w = 0i32;
    for ch in text.chars() {
        if ch == '\n' {
            break;
        }
        if let Some(g) = glyph(face, ch) {
            w += if large { (g.advance * 3 + 1) / 2 } else { g.advance };
        }
    }
    w
}
