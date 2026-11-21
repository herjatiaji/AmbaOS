use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::drivers::{rtc, timer};
use crate::gui::canvas::Canvas;
use crate::gui::font::{self, FontFace};
use crate::gui::theme::ThemeColors;

// sin(2*pi*i/60) * 1000
const SIN60: [i32; 60] = [
    0, 105, 208, 309, 407, 500, 588, 669, 743, 809, 866, 914, 951, 978, 995,
    1000, 995, 978, 951, 914, 866, 809, 743, 669, 588, 500, 407, 309, 208, 105,
    0, -105, -208, -309, -407, -500, -588, -669, -743, -809, -866, -914, -951, -978, -995,
    -1000, -995, -978, -951, -914, -866, -809, -743, -669, -588, -500, -407, -309, -208, -105,
];

pub struct ClockApp {
    running: bool,
    start_ms: u64,
    accum_ms: u64,
    laps: Vec<String>,
}

impl ClockApp {
    pub fn new() -> Self {
        Self {
            running: false,
            start_ms: 0,
            accum_ms: 0,
            laps: Vec::new(),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    fn elapsed_ms(&self, now_ms: u64) -> u64 {
        if self.running {
            self.accum_ms + now_ms.saturating_sub(self.start_ms)
        } else {
            self.accum_ms
        }
    }

    pub fn handle_click(&mut self, lx: i32, ly: i32, cw: i32) -> bool {
        let right_x = cw / 2 + 12;
        if lx < right_x || !(108..=138).contains(&ly) {
            return false;
        }
        let now_ms = timer::ticks_ms();
        let bx = lx - right_x;
        if (0..=76).contains(&bx) {
            if self.running {
                self.accum_ms = self.elapsed_ms(now_ms);
                self.running = false;
            } else {
                self.start_ms = now_ms;
                self.running = true;
            }
            return true;
        }
        if (84..=142).contains(&bx) {
            let e = self.elapsed_ms(now_ms);
            let mins = (e / 60_000) % 100;
            let secs = (e / 1_000) % 60;
            let cs = (e / 10) % 100;
            self.laps.insert(0, format!("Lap {:02}: {:02}:{:02}.{:02}", self.laps.len() + 1, mins, secs, cs));
            if self.laps.len() > 5 {
                self.laps.pop();
            }
            return true;
        }
        if (150..=214).contains(&bx) {
            self.running = false;
            self.accum_ms = 0;
            self.laps.clear();
            return true;
        }
        false
    }

    pub fn render(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        canvas.fill_rect(cx, cy, cw, ch, theme.window_bg);

        let dt = rtc::read_rtc();
        let clock_cx = cx + cw / 4 + 4;
        let clock_cy = cy + 104;
        let r = 78;

        // Analog clock dial
        canvas.fill_circle(clock_cx, clock_cy, r + 4, theme.window_surface);
        canvas.fill_circle(clock_cx, clock_cy, r, theme.term_bg);

        for h in 0..12 {
            let idx = (h * 5) % 60;
            let sin_v = SIN60[idx];
            let cos_v = SIN60[(idx + 15) % 60];
            let x1 = clock_cx + (sin_v * (r - 10)) / 1000;
            let y1 = clock_cy - (cos_v * (r - 10)) / 1000;
            let x2 = clock_cx + (sin_v * (r - 2)) / 1000;
            let y2 = clock_cy - (cos_v * (r - 2)) / 1000;
            canvas.draw_line(x1, y1, x2, y2, theme.text_muted);
        }

        // Hour hand
        let h_idx = (((dt.hour % 12) as usize) * 5 + (dt.minute as usize / 12)) % 60;
        let hx = clock_cx + (SIN60[h_idx] * 42) / 1000;
        let hy = clock_cy - (SIN60[(h_idx + 15) % 60] * 42) / 1000;
        canvas.draw_thick_line(clock_cx, clock_cy, hx, hy, 2, theme.text_primary);

        // Minute hand
        let m_idx = (dt.minute as usize) % 60;
        let mx = clock_cx + (SIN60[m_idx] * 58) / 1000;
        let my = clock_cy - (SIN60[(m_idx + 15) % 60] * 58) / 1000;
        canvas.draw_thick_line(clock_cx, clock_cy, mx, my, 1, theme.accent_sec);

        // Second hand
        let s_idx = (dt.second as usize) % 60;
        let sx = clock_cx + (SIN60[s_idx] * 66) / 1000;
        let sy = clock_cy - (SIN60[(s_idx + 15) % 60] * 66) / 1000;
        canvas.draw_line(clock_cx, clock_cy, sx, sy, theme.accent);
        canvas.fill_circle(clock_cx, clock_cy, 4, theme.accent);

        let dig = format!("{:02}:{:02}:{:02}", dt.hour, dt.minute, dt.second);
        let dw = font::measure_text(FontFace::Mono, &dig);
        canvas.draw_text_font(clock_cx - dw / 2, clock_cy + r + 14, &dig, theme.text_primary, FontFace::Mono);
        let date_s = format!("{:04}-{:02}-{:02}", dt.year, dt.month, dt.day);
        let sw = font::measure_text(FontFace::Ui, &date_s);
        canvas.draw_text_font(clock_cx - sw / 2, clock_cy + r + 36, &date_s, theme.text_muted, FontFace::Ui);

        // Divider
        canvas.fill_rect(cx + cw / 2, cy + 16, 1, ch - 32, theme.border_inactive);

        // Right half: Stopwatch
        let rx = cx + cw / 2 + 12;
        let rw = cw / 2 - 24;
        canvas.draw_text_font(rx, cy + 18, "Stopwatch", theme.text_primary, FontFace::UiBold);

        let e = self.elapsed_ms(timer::ticks_ms());
        let sw_txt = format!("{:02}:{:02}.{:02}", (e / 60_000) % 100, (e / 1_000) % 60, (e / 10) % 100);
        canvas.fill_rounded_rect(rx, cy + 44, rw, 48, 8, theme.term_bg);
        canvas.draw_rounded_rect_outline(rx, cy + 44, rw, 48, 8, theme.border_inactive);
        let stw = font::measure_text(FontFace::Mono, &sw_txt);
        canvas.draw_text_font(rx + (rw - stw) / 2, cy + 58, &sw_txt, theme.accent_soft, FontFace::Mono);

        // Controls
        let by = cy + 108;
        canvas.fill_rounded_rect(rx, by, 76, 30, 6, theme.accent);
        canvas.draw_text_font(rx + 18, by + 6, if self.running { "Pause" } else { "Start" }, 0xFFFFFF, FontFace::UiBold);

        canvas.fill_rounded_rect(rx + 84, by, 58, 30, 6, theme.window_surface);
        canvas.draw_text_font(rx + 100, by + 6, "Lap", theme.text_primary, FontFace::UiBold);

        canvas.fill_rounded_rect(rx + 150, by, 64, 30, 6, theme.window_surface);
        canvas.draw_text_font(rx + 164, by + 6, "Reset", theme.text_primary, FontFace::UiBold);

        // Laps list
        let mut ly = cy + 152;
        for lap in &self.laps {
            if ly + 18 > cy + ch - 8 {
                break;
            }
            canvas.draw_text_clipped_font(rx, ly, rw, lap, theme.text_secondary, FontFace::Mono);
            ly += 20;
        }
    }
}
