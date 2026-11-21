#[derive(Clone, Copy)]
pub struct Sprite {
    pub width: i32,
    pub height: i32,
    pub rgba: &'static [u8],
}

static LOGO_120_BYTES: &[u8] = include_bytes!("assets/logo_120.rgba");
static LOGO_48_BYTES: &[u8] = include_bytes!("assets/logo_48.rgba");
static LOGO_22_BYTES: &[u8] = include_bytes!("assets/logo_22.rgba");

pub static FONT_UI_BYTES: &[u8] = include_bytes!("assets/font_ui.bin");
pub static FONT_BOLD_BYTES: &[u8] = include_bytes!("assets/font_ui_bold.bin");
pub static FONT_MONO_BYTES: &[u8] = include_bytes!("assets/font_mono.bin");

#[inline]
pub fn logo_120() -> Sprite {
    Sprite {
        width: 120,
        height: 120,
        rgba: LOGO_120_BYTES,
    }
}

#[inline]
pub fn logo_48() -> Sprite {
    Sprite {
        width: 48,
        height: 48,
        rgba: LOGO_48_BYTES,
    }
}

#[inline]
pub fn logo_22() -> Sprite {
    Sprite {
        width: 22,
        height: 22,
        rgba: LOGO_22_BYTES,
    }
}
