use alloc::vec;
use alloc::vec::Vec;

use crate::gui::assets::Sprite;
use crate::gui::font::{self, FontFace};
use crate::gui::theme::ThemeColors;

pub struct Canvas {
    pub width: i32,
    pub height: i32,
    pub buffer: Vec<u32>,
    red_shift: u8,
    green_shift: u8,
    blue_shift: u8,
}

impl Canvas {
    pub fn new(width: i32, height: i32, red_shift: u8, green_shift: u8, blue_shift: u8) -> Self {
        let len = (width.max(1) as usize) * (height.max(1) as usize);
        Self {
            width,
            height,
            buffer: vec![0; len],
            red_shift,
            green_shift,
            blue_shift,
        }
    }

    #[inline]
    pub fn pack(&self, rgb: u32) -> u32 {
        let r = (rgb >> 16) & 0xFF;
        let g = (rgb >> 8) & 0xFF;
        let b = rgb & 0xFF;
        (r << self.red_shift) | (g << self.green_shift) | (b << self.blue_shift)
    }

    #[inline]
    pub fn unpack(&self, raw: u32) -> (u32, u32, u32) {
        let r = (raw >> self.red_shift) & 0xFF;
        let g = (raw >> self.green_shift) & 0xFF;
        let b = (raw >> self.blue_shift) & 0xFF;
        (r, g, b)
    }

    #[inline]
    pub fn blend_pixel(&mut self, x: i32, y: i32, sr: u32, sg: u32, sb: u32, alpha: u8) {
        if alpha == 0 || x < 0 || x >= self.width || y < 0 || y >= self.height {
            return;
        }
        let idx = (y as usize) * (self.width as usize) + (x as usize);
        if alpha == 255 {
            self.buffer[idx] = self.pack((sr << 16) | (sg << 8) | sb);
            return;
        }
        let a = (alpha as u32) + 1;
        let inv = 256 - a;
        let (dr, dg, db) = self.unpack(self.buffer[idx]);
        let nr = (sr * a + dr * inv) >> 8;
        let ng = (sg * a + dg * inv) >> 8;
        let nb = (sb * a + db * inv) >> 8;
        self.buffer[idx] = (nr << self.red_shift) | (ng << self.green_shift) | (nb << self.blue_shift);
    }

    #[inline]
    pub fn put_pixel_packed(&mut self, x: i32, y: i32, packed: u32) {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            let idx = (y as usize) * (self.width as usize) + (x as usize);
            self.buffer[idx] = packed;
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: u32) {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(self.width);
        let y1 = (y + h).min(self.height);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let packed = self.pack(rgb);
        let stride = self.width as usize;
        for row in y0..y1 {
            let start = (row as usize) * stride + (x0 as usize);
            let end = (row as usize) * stride + (x1 as usize);
            self.buffer[start..end].fill(packed);
        }
    }

    pub fn fill_rect_alpha(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: u32, alpha: u8) {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(self.width);
        let y1 = (y + h).min(self.height);
        if x0 >= x1 || y0 >= y1 || alpha == 0 {
            return;
        }
        let sr = (rgb >> 16) & 0xFF;
        let sg = (rgb >> 8) & 0xFF;
        let sb = rgb & 0xFF;
        let a = (alpha as u32) + 1;
        let inv = 256 - a;
        let stride = self.width as usize;
        for row in y0..y1 {
            let row_base = (row as usize) * stride;
            for col in x0..x1 {
                let idx = row_base + (col as usize);
                let (dr, dg, db) = self.unpack(self.buffer[idx]);
                let nr = (sr * a + dr * inv) >> 8;
                let ng = (sg * a + dg * inv) >> 8;
                let nb = (sb * a + db * inv) >> 8;
                self.buffer[idx] =
                    (nr << self.red_shift) | (ng << self.green_shift) | (nb << self.blue_shift);
            }
        }
    }

    #[inline]
    fn inside_rounded(px: i32, py: i32, w: i32, h: i32, r: i32) -> bool {
        if r <= 0 {
            return true;
        }
        let r = r.min(w / 2).min(h / 2);
        let dx = if px < r {
            r - 1 - px
        } else if px >= w - r {
            px - (w - r)
        } else {
            return true;
        };
        let dy = if py < r {
            r - 1 - py
        } else if py >= h - r {
            py - (h - r)
        } else {
            return true;
        };
        dx * dx + dy * dy <= r * r
    }

    pub fn fill_rounded_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, rgb: u32) {
        if w <= 0 || h <= 0 {
            return;
        }
        if r <= 0 {
            self.fill_rect(x, y, w, h, rgb);
            return;
        }
        let r = r.min(w / 2).min(h / 2);
        let packed = self.pack(rgb);

        // Middle solid band
        if h > 2 * r {
            self.fill_rect(x, y + r, w, h - 2 * r, rgb);
        }

        // Top and bottom rounded bands
        for py in 0..r {
            let dy = r - 1 - py;
            let mut inset = r;
            for dx in 0..r {
                if dx * dx + dy * dy <= r * r {
                    inset = r - 1 - dx;
                }
            }
            let span_x = x + inset;
            let span_w = w - 2 * inset;
            if span_w > 0 {
                let top_y = y + py;
                let bot_y = y + h - 1 - py;
                for gx in span_x.max(0)..(span_x + span_w).min(self.width) {
                    if top_y >= 0 && top_y < self.height {
                        self.buffer[(top_y as usize) * (self.width as usize) + (gx as usize)] = packed;
                    }
                    if bot_y >= 0 && bot_y < self.height {
                        self.buffer[(bot_y as usize) * (self.width as usize) + (gx as usize)] = packed;
                    }
                }
            }
        }
    }

    pub fn fill_rounded_rect_alpha(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        r: i32,
        rgb: u32,
        alpha: u8,
    ) {
        if w <= 0 || h <= 0 || alpha == 0 {
            return;
        }
        if r <= 0 {
            self.fill_rect_alpha(x, y, w, h, rgb, alpha);
            return;
        }
        let r = r.min(w / 2).min(h / 2);

        if h > 2 * r {
            self.fill_rect_alpha(x, y + r, w, h - 2 * r, rgb, alpha);
        }

        for py in 0..r {
            let dy = r - 1 - py;
            let mut inset = r;
            for dx in 0..r {
                if dx * dx + dy * dy <= r * r {
                    inset = r - 1 - dx;
                }
            }
            let span_w = w - 2 * inset;
            if span_w > 0 {
                self.fill_rect_alpha(x + inset, y + py, span_w, 1, rgb, alpha);
                self.fill_rect_alpha(x + inset, y + h - 1 - py, span_w, 1, rgb, alpha);
            }
        }
    }

    pub fn draw_rounded_rect_outline(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, rgb: u32) {
        if w <= 1 || h <= 1 {
            return;
        }
        let r = r.min(w / 2).min(h / 2);
        let packed = self.pack(rgb);

        for py in 0..h {
            let gy = y + py;
            if gy < 0 || gy >= self.height {
                continue;
            }
            if py == 0 || py == h - 1 {
                for px in r..(w - r) {
                    self.put_pixel_packed(x + px, gy, packed);
                }
            }
            if py >= r && py < h - r {
                self.put_pixel_packed(x, gy, packed);
                self.put_pixel_packed(x + w - 1, gy, packed);
            } else {
                let left_end = r.min(w);
                let right_start = (w - r).max(left_end);
                for px in (0..left_end).chain(right_start..w) {
                    if Self::inside_rounded(px, py, w, h, r) {
                        let on_edge = px == 0
                            || px == w - 1
                            || py == 0
                            || py == h - 1
                            || !Self::inside_rounded(px - 1, py, w, h, r)
                            || !Self::inside_rounded(px + 1, py, w, h, r)
                            || !Self::inside_rounded(px, py - 1, w, h, r)
                            || !Self::inside_rounded(px, py + 1, w, h, r);
                        if on_edge {
                            self.put_pixel_packed(x + px, gy, packed);
                        }
                    }
                }
            }
        }
    }

    pub fn draw_soft_shadow(&mut self, x: i32, y: i32, w: i32, h: i32, _r: i32) {
        self.fill_rect_alpha(x + 4, y + h, (w - 2).max(0), 4, 0x000000, 70);
        self.fill_rect_alpha(x + w, y + 4, 4, (h - 2).max(0), 0x000000, 70);
    }

    pub fn fill_circle(&mut self, cx: i32, cy: i32, r: i32, rgb: u32) {
        let packed = self.pack(rgb);
        let r2 = r * r;
        for dy in -r..=r {
            let py = cy + dy;
            if py < 0 || py >= self.height {
                continue;
            }
            for dx in -r..=r {
                if dx * dx + dy * dy <= r2 {
                    self.put_pixel_packed(cx + dx, py, packed);
                }
            }
        }
    }

    pub fn draw_line(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, rgb: u32) {
        let packed = self.pack(rgb);
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            self.put_pixel_packed(x0, y0, packed);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    pub fn draw_thick_line(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, radius: i32, rgb: u32) {
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            self.fill_circle(x0, y0, radius, rgb);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    pub fn fill_gradient_v(&mut self, x: i32, y: i32, w: i32, h: i32, top_rgb: u32, bot_rgb: u32) {
        if h <= 0 || w <= 0 {
            return;
        }
        let tr = ((top_rgb >> 16) & 0xFF) as i32;
        let tg = ((top_rgb >> 8) & 0xFF) as i32;
        let tb = (top_rgb & 0xFF) as i32;
        let br = ((bot_rgb >> 16) & 0xFF) as i32;
        let bg = ((bot_rgb >> 8) & 0xFF) as i32;
        let bb = (bot_rgb & 0xFF) as i32;

        let denom = (h - 1).max(1);
        for row in 0..h {
            let r = (tr + (br - tr) * row / denom) as u32;
            let g = (tg + (bg - tg) * row / denom) as u32;
            let b = (tb + (bb - tb) * row / denom) as u32;
            self.fill_rect(x, y + row, w, 1, (r << 16) | (g << 8) | b);
        }
    }

    pub fn draw_rgba_sprite(&mut self, x: i32, y: i32, sprite: Sprite, opacity_scale: u8) {
        self.draw_rgba_slice(x, y, sprite.width, sprite.height, sprite.rgba, opacity_scale);
    }

    pub fn draw_rgba_slice(
        &mut self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        rgba: &[u8],
        opacity_scale: u8,
    ) {
        if opacity_scale == 0 {
            return;
        }
        let scale = opacity_scale as u32;
        for sy in 0..height {
            let dy = y + sy;
            if dy < 0 || dy >= self.height {
                continue;
            }
            for sx in 0..width {
                let dx = x + sx;
                if dx < 0 || dx >= self.width {
                    continue;
                }
                let p = ((sy * width + sx) as usize) * 4;
                if p + 3 >= rgba.len() {
                    continue;
                }
                let a = ((rgba[p + 3] as u32) * scale) / 255;
                if a == 0 {
                    continue;
                }
                self.blend_pixel(
                    dx,
                    dy,
                    rgba[p] as u32,
                    rgba[p + 1] as u32,
                    rgba[p + 2] as u32,
                    a as u8,
                );
            }
        }
    }

    pub fn draw_text_font(&mut self, x: i32, y: i32, text: &str, rgb: u32, face: FontFace) {
        let sr = (rgb >> 16) & 0xFF;
        let sg = (rgb >> 8) & 0xFF;
        let sb = rgb & 0xFF;
        let mut cx = x;
        for ch in text.chars() {
            if ch == '\n' {
                break;
            }
            if let Some(g) = font::glyph(face, ch) {
                if cx + g.width > 0 && cx < self.width {
                    for gy in 0..g.height {
                        let dy = y + g.y_offset + gy;
                        if dy < 0 || dy >= self.height {
                            continue;
                        }
                        let row_off = (gy as usize) * (g.width as usize);
                        for gx in 0..g.width {
                            let alpha = g.alpha[row_off + (gx as usize)];
                            if alpha > 0 {
                                self.blend_pixel(cx + gx, dy, sr, sg, sb, alpha);
                            }
                        }
                    }
                }
                cx += g.advance;
            }
        }
    }

    pub fn draw_text_clipped_font(
        &mut self,
        x: i32,
        y: i32,
        max_w: i32,
        text: &str,
        rgb: u32,
        face: FontFace,
    ) {
        if max_w <= 0 {
            return;
        }
        let sr = (rgb >> 16) & 0xFF;
        let sg = (rgb >> 8) & 0xFF;
        let sb = rgb & 0xFF;
        let limit_x = (x + max_w).min(self.width);
        let mut cx = x;
        for ch in text.chars() {
            if ch == '\n' {
                break;
            }
            if let Some(g) = font::glyph(face, ch) {
                if cx + g.advance > limit_x {
                    break;
                }
                for gy in 0..g.height {
                    let dy = y + g.y_offset + gy;
                    if dy < 0 || dy >= self.height {
                        continue;
                    }
                    let row_off = (gy as usize) * (g.width as usize);
                    for gx in 0..g.width {
                        let px = cx + gx;
                        if px >= limit_x {
                            break;
                        }
                        let alpha = g.alpha[row_off + (gx as usize)];
                        if alpha > 0 {
                            self.blend_pixel(px, dy, sr, sg, sb, alpha);
                        }
                    }
                }
                cx += g.advance;
            }
        }
    }

    pub fn draw_text_scaled_clipped_font(
        &mut self,
        x: i32,
        y: i32,
        max_w: i32,
        text: &str,
        rgb: u32,
        face: FontFace,
        large: bool,
    ) {
        if !large {
            self.draw_text_clipped_font(x, y, max_w, text, rgb, face);
            return;
        }
        if max_w <= 0 {
            return;
        }
        let sr = (rgb >> 16) & 0xFF;
        let sg = (rgb >> 8) & 0xFF;
        let sb = rgb & 0xFF;
        let limit_x = (x + max_w).min(self.width);
        let mut cx = x;
        for ch in text.chars() {
            if ch == '\n' {
                break;
            }
            if let Some(g) = font::glyph(face, ch) {
                let adv = (g.advance * 3 + 1) / 2;
                if cx + adv > limit_x {
                    break;
                }
                let sw = (g.width * 3 + 1) / 2;
                let sh = (g.height * 3 + 1) / 2;
                let y_off = (g.y_offset * 3) / 2;
                for dy in 0..sh {
                    let py = y + y_off + dy;
                    if py < 0 || py >= self.height {
                        continue;
                    }
                    let sy = ((dy * 2) / 3).min(g.height - 1);
                    let row_off = (sy as usize) * (g.width as usize);
                    for dx in 0..sw {
                        let px = cx + dx;
                        if px >= limit_x {
                            break;
                        }
                        let sx = ((dx * 2) / 3).min(g.width - 1);
                        let alpha = g.alpha[row_off + (sx as usize)];
                        if alpha > 0 {
                            self.blend_pixel(px, py, sr, sg, sb, alpha);
                        }
                    }
                }
                cx += adv;
            }
        }
    }

    pub fn draw_progress_bar(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        used: usize,
        total: usize,
        track_rgb: u32,
        border_rgb: u32,
        fill_rgb: u32,
    ) {
        let r = (h / 2).min(6);
        self.fill_rounded_rect(x, y, w, h, r, track_rgb);
        self.draw_rounded_rect_outline(x, y, w, h, r, border_rgb);
        if total > 0 && w > 4 && h > 4 {
            let inner_w = w - 4;
            let bar_w = (((used as u128) * (inner_w as u128)) / (total as u128)) as i32;
            let bar_w = bar_w.clamp(0, inner_w);
            if bar_w > 0 {
                self.fill_rounded_rect(x + 2, y + 2, bar_w.max(4).min(inner_w), h - 4, (r - 1).max(1), fill_rgb);
            }
        }
    }

    pub fn blit_rgb_u32(
        &mut self,
        dst_x: i32,
        dst_y: i32,
        max_w: i32,
        max_h: i32,
        src_w: i32,
        src_h: i32,
        pixels: &[u32],
    ) {
        if max_w <= 0 || max_h <= 0 || src_w <= 0 || src_h <= 0 {
            return;
        }
        let scaled = src_w < max_w || src_h < max_h;
        let copy_w = if scaled { max_w } else { max_w.min(src_w) };
        let copy_h = if scaled { max_h } else { max_h.min(src_h) };
        let x0 = dst_x.max(0);
        let y0 = dst_y.max(0);
        let x1 = (dst_x + copy_w).min(self.width);
        let y1 = (dst_y + copy_h).min(self.height);
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let dst_stride = self.width as usize;
        let src_stride = src_w as usize;
        let fast_xrgb = self.red_shift == 16 && self.green_shift == 8 && self.blue_shift == 0;

        if !scaled {
            let dx_off = (x0 - dst_x) as usize;
            let count = (x1 - x0) as usize;

            for py in y0..y1 {
                let sy = (py - dst_y) as usize;
                let src_start = sy * src_stride + dx_off;
                if src_start + count > pixels.len() {
                    break;
                }
                let dst_start = (py as usize) * dst_stride + (x0 as usize);
                if fast_xrgb {
                    self.buffer[dst_start..dst_start + count]
                        .copy_from_slice(&pixels[src_start..src_start + count]);
                } else {
                    for i in 0..count {
                        self.buffer[dst_start + i] = self.pack(pixels[src_start + i]);
                    }
                }
            }
        } else {
            for py in y0..y1 {
                let sy = (((py - dst_y) * src_h) / max_h).clamp(0, src_h - 1) as usize;
                let src_row = sy * src_stride;
                if src_row + src_stride > pixels.len() {
                    break;
                }
                let dst_row = (py as usize) * dst_stride;
                for px in x0..x1 {
                    let sx = (((px - dst_x) * src_w) / max_w).clamp(0, src_w - 1) as usize;
                    let col = pixels[src_row + sx];
                    self.buffer[dst_row + (px as usize)] = if fast_xrgb { col } else { self.pack(col) };
                }
            }
        }
    }

    pub fn render_wallpaper(&mut self, theme: ThemeColors, show_logo: bool) {
        self.fill_gradient_v(0, 0, self.width, self.height, theme.bg_top, theme.bg_bottom);

        // Soft centered ambient radial glow behind logo
        let cx = self.width / 2;
        let cy = self.height / 2 - 22;
        let glow_r = 250i32;
        let gr = (theme.bg_glow >> 16) & 0xFF;
        let gg = (theme.bg_glow >> 8) & 0xFF;
        let gb = theme.bg_glow & 0xFF;
        let max_d2 = glow_r * glow_r;

        let y0 = (cy - glow_r).max(0);
        let y1 = (cy + glow_r).min(self.height);
        let x0 = (cx - glow_r).max(0);
        let x1 = (cx + glow_r).min(self.width);

        for py in y0..y1 {
            let dy = py - cy;
            let dy2 = dy * dy;
            for px in x0..x1 {
                let dx = px - cx;
                let d2 = dx * dx + dy2;
                if d2 < max_d2 {
                    let strength = 255 - (d2 * 255) / max_d2;
                    let alpha = ((strength * strength) / (255 * 255)) * 22;
                    if alpha > 0 {
                        self.blend_pixel(px, py, gr, gg, gb, alpha as u8);
                    }
                }
            }
        }

        if show_logo {
            let logo = crate::gui::assets::logo_120();
            self.draw_rgba_sprite(cx - logo.width / 2, cy - logo.height / 2 - 10, logo, 195);
            let title = "AmbaOS";
            let tw = font::measure_text(FontFace::UiBold, title);
            self.draw_text_font(cx - tw / 2, cy + 62, title, 0xD0D7DE, FontFace::UiBold);
        }
    }

    pub fn draw_cursor(&mut self, x: i32, y: i32, accent_rgb: u32) {
        self.draw_cursor_ex(x, y, accent_rgb, false);
    }

    pub fn draw_cursor_ex(&mut self, x: i32, y: i32, accent_rgb: u32, large: bool) {
        const CURSOR: [&str; 18] = [
            "W...........",
            "WW..........",
            "WBW.........",
            "WBBW........",
            "WBBBW.......",
            "WBBBBW......",
            "WBBBBBW.....",
            "WBBBBBBW....",
            "WBBBBBBBW...",
            "WBBBBBBBBW..",
            "WBBBBBBWWWW.",
            "WBBWBBW.....",
            "WBW.WBBW....",
            "WW..WBBW....",
            "W....WBBW...",
            ".....WBBW...",
            ".....WBBW...",
            "......WW....",
        ];

        let scale = if large { 2 } else { 1 };
        let border = self.pack(0xFFFFFF);
        let fill = self.pack(accent_rgb);
        for (dy, row) in CURSOR.iter().enumerate() {
            for (dx, ch) in row.bytes().enumerate() {
                for sy in 0..scale {
                    for sx in 0..scale {
                        let gx = x + (dx as i32) * scale + sx;
                        let gy = y + (dy as i32) * scale + sy;
                        match ch {
                            b'W' => self.put_pixel_packed(gx, gy, border),
                            b'B' => self.put_pixel_packed(gx, gy, fill),
                            b'.' if dx > 0 && row.as_bytes().get(dx - 1) == Some(&b'W') => {
                                self.blend_pixel(gx, gy, 0, 0, 0, 95);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    pub fn draw_magnifier_lens(
        &mut self,
        src_buf: &[u32],
        focus_x: i32,
        focus_y: i32,
        border_rgb: u32,
    ) -> (i32, i32, i32, i32) {
        let lens_w = 240i32;
        let lens_h = 150i32;
        let footer_h = 22i32;
        let total_h = lens_h + footer_h;

        // Place lens on opposite side of cursor so it never covers the inspected spot
        let lx = if focus_x < self.width / 2 {
            (focus_x + 36).min(self.width - lens_w - 14)
        } else {
            (focus_x - lens_w - 36).max(14)
        };
        let ly = if focus_y > self.height / 2 {
            (focus_y - total_h - 24).max(36)
        } else {
            (focus_y + 28).min(self.height - total_h - 16)
        };

        self.draw_soft_shadow(lx, ly, lens_w, total_h, 8);

        let src_w = lens_w / 2;
        let src_h = lens_h / 2;
        let sx0 = (focus_x - src_w / 2).clamp(0, (self.width - src_w).max(0));
        let sy0 = (focus_y - src_h / 2).clamp(0, (self.height - src_h).max(0));
        let stride = self.width as usize;

        for dy in 0..lens_h {
            let py = ly + dy;
            if py < 0 || py >= self.height {
                continue;
            }
            let sy = (sy0 + dy / 2).clamp(0, self.height - 1) as usize;
            let dst_row = (py as usize) * stride;
            let src_row = sy * stride;
            for dx in 0..lens_w {
                let px = lx + dx;
                if px < 0 || px >= self.width {
                    continue;
                }
                let sx = (sx0 + dx / 2).clamp(0, self.width - 1) as usize;
                self.buffer[dst_row + (px as usize)] = src_buf[src_row + sx];
            }
        }

        // Crosshair at zoomed focus spot
        let cx = lx + (focus_x - sx0) * 2;
        let cy = ly + (focus_y - sy0) * 2;
        self.fill_rect(cx - 8, cy, 17, 1, border_rgb);
        self.fill_rect(cx, cy - 8, 1, 17, border_rgb);

        self.fill_rect(lx, ly + lens_h, lens_w, footer_h, 0x090C12);
        self.draw_text_font(lx + 10, ly + lens_h + 3, "ZOOM 2x (Alt+Z to close)", 0xFACC15, FontFace::UiBold);
        self.draw_rounded_rect_outline(lx - 1, ly - 1, lens_w + 2, total_h + 2, 4, border_rgb);

        (lx - 2, ly - 2, lens_w + 8, total_h + 8)
    }
}
