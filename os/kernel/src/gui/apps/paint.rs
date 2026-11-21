use alloc::vec;
use alloc::vec::Vec;

use crate::gui::canvas::Canvas;
use crate::gui::font::FontFace;
use crate::gui::theme::ThemeColors;

const PAINT_W: usize = 492;
const PAINT_H: usize = 296;
const CANVAS_BG: u32 = 0x0D111A;

const PALETTE: [u32; 8] = [
    0xFF6A00, // Amba Orange
    0x22D3EE, // Cyan
    0xF8FAFC, // White
    0xFF5F56, // Red
    0x4ADE80, // Green
    0xFACC15, // Yellow
    0xA78BFA, // Violet
    CANVAS_BG, // Eraser
];

pub struct PaintApp {
    pixels: Vec<u32>,
    color_idx: usize,
    brush_size: i32,
    last_pos: Option<(i32, i32)>,
}

impl PaintApp {
    pub fn new() -> Self {
        let mut pixels = vec![CANVAS_BG; PAINT_W * PAINT_H];
        // Clean starter border mark inside corner
        for y in 0..PAINT_H {
            pixels[y * PAINT_W] = 0x1E293B;
            pixels[y * PAINT_W + PAINT_W - 1] = 0x1E293B;
        }
        Self {
            pixels,
            color_idx: 0,
            brush_size: 4,
            last_pos: None,
        }
    }

    fn plot_circle(&mut self, cx: i32, cy: i32, radius: i32, color: u32) {
        let r2 = radius * radius;
        for dy in -radius..=radius {
            let py = cy + dy;
            if py < 0 || py >= PAINT_H as i32 {
                continue;
            }
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= r2 {
                    let px = cx + dx;
                    if px >= 0 && px < PAINT_W as i32 {
                        self.pixels[(py as usize) * PAINT_W + (px as usize)] = color;
                    }
                }
            }
        }
    }

    fn stroke_to(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32) {
        let col = PALETTE[self.color_idx];
        let r = self.brush_size;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            self.plot_circle(x0, y0, r, col);
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

    pub fn handle_press(&mut self, lx: i32, ly: i32, cw: i32, ch: i32) -> bool {
        // Toolbar row (y: 6..36)
        if (6..=36).contains(&ly) {
            for i in 0..PALETTE.len() {
                let sx = 12 + (i as i32) * 30;
                if lx >= sx && lx <= sx + 24 {
                    self.color_idx = i;
                    return true;
                }
            }
            // Brush sizes S / M / L
            for (i, &sz) in [2i32, 4, 7].iter().enumerate() {
                let bx = 264 + (i as i32) * 30;
                if lx >= bx && lx <= bx + 26 {
                    self.brush_size = sz;
                    return true;
                }
            }
            // Clear button
            if lx >= cw - 78 && lx <= cw - 12 {
                self.pixels.fill(CANVAS_BG);
                return true;
            }
        }

        if let Some((px, py)) = self.map_to_canvas(lx, ly, cw, ch) {
            self.plot_circle(px, py, self.brush_size, PALETTE[self.color_idx]);
            self.last_pos = Some((px, py));
            return true;
        }
        false
    }

    pub fn handle_drag(&mut self, lx: i32, ly: i32, cw: i32, ch: i32) -> bool {
        if let Some((px, py)) = self.map_to_canvas(lx, ly, cw, ch) {
            if let Some((ox, oy)) = self.last_pos {
                self.stroke_to(ox, oy, px, py);
            } else {
                self.plot_circle(px, py, self.brush_size, PALETTE[self.color_idx]);
            }
            self.last_pos = Some((px, py));
            return true;
        }
        false
    }

    pub fn handle_release(&mut self) {
        self.last_pos = None;
    }

    fn map_to_canvas(&self, lx: i32, ly: i32, cw: i32, ch: i32) -> Option<(i32, i32)> {
        let bx = 14;
        let by = 46;
        let bw = (cw - 28).max(1);
        let bh = (ch - 58).max(1);
        if lx < bx || lx >= bx + bw || ly < by || ly >= by + bh {
            return None;
        }
        let px = ((lx - bx) * (PAINT_W as i32)) / bw;
        let py = ((ly - by) * (PAINT_H as i32)) / bh;
        Some((px.clamp(0, PAINT_W as i32 - 1), py.clamp(0, PAINT_H as i32 - 1)))
    }

    pub fn render(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        canvas.fill_rect(cx, cy, cw, ch, theme.window_bg);

        // Toolbar
        canvas.fill_rect(cx, cy, cw, 40, theme.window_surface);
        canvas.fill_rect(cx, cy + 40, cw, 1, theme.border_inactive);

        for (i, &col) in PALETTE.iter().enumerate() {
            let sx = cx + 12 + (i as i32) * 30;
            canvas.fill_rounded_rect(sx, cy + 8, 24, 24, 5, col);
            let border = if i == self.color_idx { 0xFFFFFF } else { theme.border_inactive };
            canvas.draw_rounded_rect_outline(sx, cy + 8, 24, 24, 5, border);
            if i == PALETTE.len() - 1 {
                canvas.draw_text_font(sx + 7, cy + 11, "E", theme.text_secondary, FontFace::UiBold);
            }
        }

        for (i, (&sz, lbl)) in [2i32, 4, 7].iter().zip(["S", "M", "L"]).enumerate() {
            let bx = cx + 264 + (i as i32) * 30;
            let act = self.brush_size == sz;
            canvas.fill_rounded_rect(
                bx,
                cy + 8,
                26,
                24,
                5,
                if act { theme.accent } else { theme.titlebar_active },
            );
            canvas.draw_text_font(
                bx + 8,
                cy + 11,
                lbl,
                if act { 0xFFFFFF } else { theme.text_secondary },
                FontFace::UiBold,
            );
        }

        canvas.fill_rounded_rect(cx + cw - 78, cy + 8, 66, 24, 6, theme.titlebar_active);
        canvas.draw_rounded_rect_outline(cx + cw - 78, cy + 8, 66, 24, 6, theme.border_inactive);
        canvas.draw_text_font(cx + cw - 62, cy + 11, "Clear", theme.text_primary, FontFace::UiBold);

        // Canvas surface blit
        let bx = cx + 14;
        let by = cy + 46;
        let bw = (cw - 28).max(1);
        let bh = (ch - 58).max(1);

        canvas.draw_rounded_rect_outline(bx - 1, by - 1, bw + 2, bh + 2, 4, theme.border_inactive);
        for dy in 0..bh {
            let sy = ((dy * PAINT_H as i32) / bh) as usize;
            let row_off = sy * PAINT_W;
            for dx in 0..bw {
                let sx = ((dx * PAINT_W as i32) / bw) as usize;
                let rgb = self.pixels[row_off + sx];
                let packed = canvas.pack(rgb);
                canvas.put_pixel_packed(bx + dx, by + dy, packed);
            }
        }
    }
}
