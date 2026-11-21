use alloc::format;
use alloc::string::String;

use crate::fs::snapshot::{self, GenHealth};
use crate::gui::canvas::Canvas;
use crate::gui::font::{self, FontFace};
use crate::gui::theme::ThemeColors;

#[derive(Clone, Copy)]
pub enum SnapAction {
    None,
    ConfigRestored(u8, bool),
    RequestReboot,
}

pub struct SnapshotsApp {
    selected_gen: u32,
    status_msg: String,
    status_err: bool,
}

impl SnapshotsApp {
    pub fn new() -> Self {
        let sum = snapshot::summary();
        Self {
            selected_gen: sum.active_gen,
            status_msg: String::from("Immutable Core OS active. All changes create atomic snapshots."),
            status_err: false,
        }
    }

    pub fn handle_click(
        &mut self,
        lx: i32,
        ly: i32,
        cw: i32,
        ch: i32,
        theme_idx: u8,
        show_watermark: bool,
    ) -> SnapAction {
        let btn_y = 98;
        let btn_h = 32;
        let avail = cw - 28 - 24;
        let bw = (avail / 4).max(80);

        if ly >= btn_y && ly <= btn_y + btn_h {
            if lx >= 14 && lx <= 14 + bw {
                let id = snapshot::create_snapshot("", theme_idx, show_watermark);
                self.selected_gen = id;
                self.status_msg = format!("Created CoW Snapshot Gen #{}", id);
                self.status_err = false;
                return SnapAction::None;
            }
            let x1 = 14 + bw + 8;
            if lx >= x1 && lx <= x1 + bw {
                let id = snapshot::apply_atomic_update(theme_idx, show_watermark);
                self.selected_gen = id;
                self.status_msg = format!("Atomic OS Update applied -> Gen #{} (0.5.{})", id, id);
                self.status_err = false;
                return SnapAction::None;
            }
            let x2 = 14 + (bw + 8) * 2;
            if lx >= x2 && lx <= x2 + bw {
                let id = snapshot::stage_corrupt_update(theme_idx, show_watermark);
                self.selected_gen = id;
                self.status_msg = format!("Staged CORRUPT Gen #{} for next boot. Reboot to test recovery!", id);
                self.status_err = true;
                return SnapAction::None;
            }
            let x3 = 14 + (bw + 8) * 3;
            if lx >= x3 && lx <= cw - 14 {
                return SnapAction::RequestReboot;
            }
        }

        let summary = snapshot::summary();
        let vis_count = summary.generations.len().min(5);
        for i in 0..vis_count {
            let ry = 166 + (i as i32) * 44;
            if ry + 38 > ch - 52 {
                break;
            }
            if lx >= 20 && lx <= cw - 20 && ly >= ry && ly <= ry + 38 {
                let idx = summary.generations.len() - 1 - i;
                self.selected_gen = summary.generations[idx].id;
                self.status_msg = format!("Selected Gen #{}", self.selected_gen);
                self.status_err = false;
                return SnapAction::None;
            }
        }

        let fy = ch - 42;
        if ly >= fy && ly <= fy + 30 {
            if lx >= 14 && lx <= 170 {
                match snapshot::rollback_live(self.selected_gen) {
                    Ok((t_idx, wm)) => {
                        self.status_msg = format!("Rolled back Core OS live to Gen #{}", self.selected_gen);
                        self.status_err = false;
                        return SnapAction::ConfigRestored(t_idx, wm);
                    }
                    Err(e) => {
                        self.status_msg = String::from(e);
                        self.status_err = true;
                    }
                }
            } else if lx >= 178 && lx <= 316 {
                match snapshot::delete_generation(self.selected_gen) {
                    Ok(()) => {
                        self.status_msg = format!("Deleted Gen #{}", self.selected_gen);
                        self.selected_gen = snapshot::summary().active_gen;
                        self.status_err = false;
                    }
                    Err(e) => {
                        self.status_msg = String::from(e);
                        self.status_err = true;
                    }
                }
            }
        }

        SnapAction::None
    }

    pub fn render(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        canvas.fill_rect(cx, cy, cw, ch, theme.window_bg);

        let sum = snapshot::summary();
        let active_hash = sum
            .generations
            .iter()
            .find(|g| g.id == sum.active_gen)
            .map(|g| (g.hash & 0xFFFF_FFFF) as u32)
            .unwrap_or(0);

        // Top partition status card
        canvas.fill_rounded_rect(cx + 14, cy + 12, cw - 28, 76, 8, theme.window_surface);
        canvas.draw_rounded_rect_outline(cx + 14, cy + 12, cw - 28, 76, 8, theme.border_inactive);

        canvas.draw_text_clipped_font(
            cx + 26,
            cy + 19,
            cw - 240,
            "CORE OS (/sys, /etc, /docs): IMMUTABLE READ-ONLY [LOCKED]",
            0xFBBF24,
            FontFace::UiBold,
        );
        let gen_badge = format!(
            "Active: Gen #{} (Slot {} | seq {})",
            sum.active_gen, sum.active_slot, sum.commit_seq
        );
        let gw = font::measure_text(FontFace::UiBold, &gen_badge);
        canvas.draw_text_font(cx + cw - gw - 24, cy + 19, &gen_badge, theme.accent_soft, FontFace::UiBold);

        let disk_info = if sum.disk_backed {
            format!("USER DATA (/home, /apps, /var): READ-WRITE PERSISTENT (ATA Disk {} MiB)", sum.disk_mib)
        } else {
            String::from("USER DATA (/home, /apps, /var): READ-WRITE (RAM CoW Store)")
        };
        canvas.draw_text_clipped_font(cx + 26, cy + 40, cw - 52, &disk_info, theme.term_green, FontFace::UiBold);

        let integ = format!(
            "Snapshot Integrity Hash: sha:{:08x}   |   Last Verified Safe: Gen #{}   |   Crash Recoveries: {}",
            active_hash, sum.last_good_gen, sum.crash_count
        );
        canvas.draw_text_clipped_font(cx + 26, cy + 61, cw - 52, &integ, theme.text_muted, FontFace::Mono);

        // Action buttons row
        let bw = ((cw - 28 - 24) / 4).max(80);
        let by = cy + 98;
        let actions = [
            (cx + 14, "+ Create Snapshot", theme.accent, 0xFFFFFF),
            (cx + 14 + bw + 8, "Atomic OS Update", 0x0284C7, 0xFFFFFF),
            (cx + 14 + (bw + 8) * 2, "Sim Corrupt Update", 0xB91C1C, 0xFFFFFF),
            (cx + 14 + (bw + 8) * 3, "Reboot to Rollback", theme.titlebar_active, theme.accent_soft),
        ];
        for (bx, lbl, bg, fg) in actions {
            let w_use = if bx == cx + 14 + (bw + 8) * 3 {
                (cx + cw - 14 - bx).max(bw)
            } else {
                bw
            };
            canvas.fill_rounded_rect(bx, by, w_use, 32, 7, bg);
            canvas.draw_rounded_rect_outline(bx, by, w_use, 32, 7, theme.border_inactive);
            let tw = font::measure_text(FontFace::UiBold, lbl);
            canvas.draw_text_clipped_font(bx + ((w_use - tw) / 2).max(6), by + 7, w_use - 12, lbl, fg, FontFace::UiBold);
        }

        // Section label
        canvas.draw_text_font(
            cx + 16,
            cy + 140,
            "Atomic CoW Snapshot Generations (Newest First - Click to select):",
            theme.text_primary,
            FontFace::UiBold,
        );

        // List container
        let list_h = (ch - 210).max(90);
        canvas.fill_rounded_rect(cx + 14, cy + 160, cw - 28, list_h, 8, theme.term_bg);
        canvas.draw_rounded_rect_outline(cx + 14, cy + 160, cw - 28, list_h, 8, theme.border_inactive);

        for (i, g) in sum.generations.iter().rev().enumerate() {
            let ry = cy + 166 + (i as i32) * 44;
            if ry + 38 > cy + 160 + list_h - 4 {
                break;
            }
            let is_sel = g.id == self.selected_gen;
            let is_act = g.id == sum.active_gen;
            canvas.fill_rounded_rect(
                cx + 20,
                ry,
                cw - 40,
                38,
                7,
                if is_sel { theme.titlebar_active } else { theme.window_surface },
            );
            canvas.draw_rounded_rect_outline(
                cx + 20,
                ry,
                cw - 40,
                38,
                7,
                if is_sel { theme.accent } else { theme.border_inactive },
            );

            canvas.draw_text_font(cx + 30, ry + 10, &format!("Gen #{}", g.id), theme.accent_soft, FontFace::UiBold);

            let (badge, bbg, bfg) = if g.health == GenHealth::Corrupt {
                ("CORRUPT", 0x7F1D1D, 0xFCA5A5)
            } else if is_act {
                ("ACTIVE", 0x065F46, 0x4ADE80)
            } else {
                ("VERIFIED", 0x0E3A4C, 0x38BDF8)
            };
            canvas.fill_rounded_rect(cx + 96, ry + 8, 76, 22, 5, bbg);
            canvas.draw_text_font(cx + 104, ry + 11, badge, bfg, FontFace::UiBold);

            canvas.draw_text_clipped_font(
                cx + 182,
                ry + 4,
                cw - 210,
                &g.label,
                theme.text_primary,
                FontFace::UiBold,
            );
            let cow_bytes: usize = g.core_overrides.iter().map(|(_, d)| d.len()).sum();
            canvas.draw_text_clipped_font(
                cx + 182,
                ry + 20,
                cw - 210,
                &format!("{}  |  sha:{:08x}  |  CoW: {} B", g.timestamp, (g.hash & 0xFFFF_FFFF) as u32, cow_bytes),
                theme.text_muted,
                FontFace::Mono,
            );
        }

        // Bottom controls
        let fy = cy + ch - 42;
        canvas.fill_rounded_rect(cx + 14, fy, 156, 30, 6, 0x065F46);
        canvas.draw_rounded_rect_outline(cx + 14, fy, 156, 30, 6, theme.term_green);
        canvas.draw_text_font(cx + 28, fy + 6, "Rollback Selected", 0xECFDF5, FontFace::UiBold);

        canvas.fill_rounded_rect(cx + 178, fy, 138, 30, 6, theme.window_surface);
        canvas.draw_rounded_rect_outline(cx + 178, fy, 138, 30, 6, theme.border_inactive);
        canvas.draw_text_font(cx + 194, fy + 6, "Delete Selected", theme.text_secondary, FontFace::UiBold);

        canvas.draw_text_clipped_font(
            cx + 328,
            fy + 6,
            cw - 342,
            &self.status_msg,
            if self.status_err { theme.err } else { theme.term_green },
            FontFace::Ui,
        );
    }
}
