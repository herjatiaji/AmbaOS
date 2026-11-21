use alloc::format;
use alloc::vec::Vec;

use crate::drivers::{mouse, rtc, timer};
use crate::gui::canvas::Canvas;
use crate::gui::font::FontFace;
use crate::gui::theme::ThemeColors;
use crate::mm::{heap, pmm, vmm};
use crate::proc::task;

pub struct SysMonApp {
    fb_w: usize,
    fb_h: usize,
    history: Vec<u8>,
}

impl SysMonApp {
    pub fn new(fb_w: usize, fb_h: usize) -> Self {
        Self {
            fb_w,
            fb_h,
            history: Vec::with_capacity(48),
        }
    }

    pub fn sample_second(&mut self) {
        let (heap_used, heap_total) = heap::stats();
        let pct = ((heap_used * 100) / heap_total.max(1)).clamp(8, 100) as u8;
        if self.history.len() >= 48 {
            self.history.remove(0);
        }
        self.history.push(pct);
    }

    pub fn render(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        canvas.fill_rect(cx, cy, cw, ch, theme.window_bg);

        let m = pmm::stats();
        let (heap_used, heap_total) = heap::stats();
        let secs = timer::uptime_secs();
        let (mx, my) = mouse::peek_pos();
        let dt = rtc::read_rtc();

        let x = cx + 14;
        let w = cw - 28;
        let mut y = cy + 12;

        // Physical RAM bar
        canvas.draw_text_clipped_font(
            x,
            y,
            w,
            &format!("Physical RAM (PMM): {} / {} MiB", m.used_bytes / (1024 * 1024), m.total_bytes / (1024 * 1024)),
            theme.text_primary,
            FontFace::UiBold,
        );
        y += 20;
        canvas.draw_progress_bar(
            x,
            y,
            w,
            12,
            m.used_bytes as usize,
            (m.total_bytes as usize).max(1),
            theme.term_bg,
            theme.border_inactive,
            theme.accent,
        );
        y += 20;

        // Kernel Heap bar
        canvas.draw_text_clipped_font(
            x,
            y,
            w,
            &format!("Kernel Heap (256 MiB): {} / {} KiB", heap_used / 1024, heap_total / 1024),
            theme.text_primary,
            FontFace::UiBold,
        );
        y += 20;
        canvas.draw_progress_bar(
            x,
            y,
            w,
            12,
            heap_used,
            heap_total.max(1),
            theme.term_bg,
            theme.border_inactive,
            theme.accent_sec,
        );
        y += 22;

        // Activity sparkline bar graph
        let chart_h = 28;
        canvas.fill_rounded_rect(x, y, w, chart_h, 6, theme.window_surface);
        canvas.draw_rounded_rect_outline(x, y, w, chart_h, 6, theme.border_inactive);
        let bar_area_w = (w - 12).max(10);
        let step = (bar_area_w / 48).max(3);
        for (i, &val) in self.history.iter().enumerate() {
            let bx = x + 6 + (i as i32) * step;
            if bx + step > x + w - 6 {
                break;
            }
            let bh = ((val as i32) * (chart_h - 8)) / 100;
            let bh = bh.max(3);
            canvas.fill_rect(bx, y + chart_h - 4 - bh, (step - 2).max(2), bh, theme.accent_soft);
        }
        y += chart_h + 6;

        // Telemetry lines
        canvas.draw_text_clipped_font(
            x,
            y,
            w,
            &format!(
                "RTC: {:04}-{:02}-{:02} {:02}:{:02}:{:02}   UP: {:02}:{:02}:{:02}",
                dt.year,
                dt.month,
                dt.day,
                dt.hour,
                dt.minute,
                dt.second,
                secs / 3600,
                (secs % 3600) / 60,
                secs % 60
            ),
            theme.text_secondary,
            FontFace::Ui,
        );
        y += 16;
        canvas.draw_text_clipped_font(
            x,
            y,
            w,
            &format!("Display: {}x{}   Mouse: ({}, {})   PML4: {:#x}", self.fb_w, self.fb_h, mx, my, vmm::active_pml4_phys()),
            theme.text_muted,
            FontFace::Ui,
        );
        y += 17;

        // Active task table
        canvas.fill_rect(x, y, w, 1, theme.border_inactive);
        y += 4;
        canvas.draw_text_clipped_font(x, y, w, "PID   STATE       TICKS     TASK", theme.accent_sec, FontFace::Mono);
        y += 16;

        for t in task::snapshot() {
            if y + 15 > cy + ch - 2 {
                break;
            }
            let row = format!("{:<5} {:<11} {:<9} {}", t.pid, t.state, t.cpu_ticks, t.name);
            let col = if t.state == "RUNNING" { theme.term_green } else { theme.text_secondary };
            canvas.draw_text_clipped_font(x, y, w, &row, col, FontFace::Mono);
            y += 15;
        }
    }
}
