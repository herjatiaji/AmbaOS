use alloc::format;

use crate::gui::assets;
use crate::gui::canvas::Canvas;
use crate::gui::font::FontFace;
use crate::gui::theme::ThemeColors;
use crate::mm::pmm;

pub struct AboutApp {
    fb_w: usize,
    fb_h: usize,
}

impl AboutApp {
    pub fn new(fb_w: usize, fb_h: usize) -> Self {
        Self { fb_w, fb_h }
    }

    pub fn render(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        canvas.fill_rect(cx, cy, cw, ch, theme.window_bg);

        let x = cx + 16;
        let w = cw - 32;
        let m = pmm::stats();

        // Top hero card with official 120x120 orange AmbaOS logo
        let card_h = 138;
        canvas.fill_rounded_rect(x, cy + 12, w, card_h, 10, theme.window_surface);
        canvas.draw_rounded_rect_outline(x, cy + 12, w, card_h, 10, theme.accent);

        let logo = assets::logo_120();
        canvas.draw_rgba_sprite(x + 12, cy + 21, logo, 255);

        let tx = x + 144;
        let tw = (w - 156).max(60);
        canvas.draw_text_clipped_font(tx, cy + 28, tw, "AmbaOS v0.7.1", theme.text_primary, FontFace::UiBold);
        canvas.draw_text_clipped_font(tx, cy + 50, tw, "Accessible Immutable + Async OS", theme.accent_soft, FontFace::UiBold);
        canvas.draw_text_clipped_font(tx, cy + 76, tw, "x86_64 Bare-Metal Rust Kernel", theme.text_secondary, FontFace::Ui);
        canvas.draw_text_clipped_font(tx, cy + 96, tw, "io_uring | ZC | SIP | Caps | vNPU", theme.text_muted, FontFace::Ui);
        canvas.draw_text_clipped_font(tx, cy + 116, tw, "15 Apps + A11y + Atomic Rollback", theme.accent_sec, FontFace::Ui);

        // Specs rows
        let mut y = cy + 164;
        let specs = [
            ("Architecture", "x86_64 (64-bit Long Mode)"),
            ("Memory", &format!("{} MiB Usable (16 GiB PMM)", m.total_bytes / (1024 * 1024))),
            ("Networking", "Intel E1000 NIC + smoltcp TCP/IP"),
            ("Graphics", &format!("{}x{}x32 ARGB + Ubuntu TTF", self.fb_w, self.fb_h)),
            ("Storage VFS", "USTAR Initramfs (In-Memory R/W)"),
        ];

        for (k, v) in specs {
            if y + 18 > cy + ch - 8 {
                break;
            }
            canvas.draw_text_clipped_font(x + 4, y, 110, k, theme.text_muted, FontFace::UiBold);
            canvas.draw_text_clipped_font(x + 118, y, w - 122, v, theme.text_primary, FontFace::Ui);
            y += 22;
        }
    }
}
