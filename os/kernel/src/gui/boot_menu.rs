use alloc::format;
use alloc::string::String;
use x86_64::instructions::hlt;

use crate::drivers::framebuffer::FramebufferInfo;
use crate::drivers::keyboard::{self, KeyEvent};
use crate::drivers::mouse;
use crate::drivers::timer;
use crate::fs::snapshot::{self, BootDecision, GenHealth, Generation};
use crate::gui::assets;
use crate::gui::canvas::Canvas;
use crate::gui::font::{self, FontFace};
use crate::gui::theme::ThemePreset;
use crate::serial_println;

const AUTO_COUNTDOWN_MS: u64 = 3000;

pub fn run_if_needed(fb: FramebufferInfo, decision: BootDecision) {
    if !decision.show_boot_menu {
        if let Err(err) = snapshot::select_boot_generation(decision.recommended_gen) {
            serial_println!("[WARN] Snapshot mount fallback ({}): Gen #1", err);
            let _ = snapshot::select_boot_generation(1);
        }
        return;
    }

    serial_println!(
        "[BOOT] Atomic Rollback Manager active (recommended Gen #{}, crash={})",
        decision.recommended_gen,
        decision.crash_detected
    );

    let summary = snapshot::summary();
    let mut gens: alloc::vec::Vec<Generation> = summary.generations.into_iter().rev().collect();
    if gens.is_empty() {
        let _ = snapshot::select_boot_generation(1);
        return;
    }

    let mut selected_idx = gens
        .iter()
        .position(|g| g.id == decision.recommended_gen)
        .unwrap_or(0);
    let max_vis = gens.len().min(6);
    if selected_idx >= max_vis {
        gens.swap(0, selected_idx);
        selected_idx = 0;
    }

    let mut auto_paused = decision.crash_detected;
    let start_ms = timer::ticks_ms();
    let mut last_sec_drawn = u64::MAX;
    let mut err_msg = String::new();
    let (mut mx, mut my) = mouse::peek_pos();

    let w = fb.width as i32;
    let h = fb.height as i32;
    let mut canvas = Canvas::new(w, h, fb.red_shift, fb.green_shift, fb.blue_shift);
    let mut dirty = true;

    loop {
        while let Some(ev) = keyboard::pop_event() {
            auto_paused = true;
            err_msg.clear();
            match ev {
                KeyEvent::Up => {
                    selected_idx = selected_idx.saturating_sub(1);
                    dirty = true;
                }
                KeyEvent::Down => {
                    if selected_idx + 1 < max_vis {
                        selected_idx += 1;
                    }
                    dirty = true;
                }
                KeyEvent::Char('r') | KeyEvent::Char('R') => {
                    if snapshot::select_boot_generation(decision.recommended_gen).is_ok() {
                        return;
                    }
                }
                KeyEvent::Enter => {
                    let chosen = gens[selected_idx].id;
                    match snapshot::select_boot_generation(chosen) {
                        Ok(()) => return,
                        Err(e) => {
                            err_msg = String::from(e);
                            dirty = true;
                        }
                    }
                }
                _ => {
                    dirty = true;
                }
            }
        }

        mouse::poll();
        let ms = mouse::take_state();
        if ms.dirty {
            mx = ms.x;
            my = ms.y;
            dirty = true;
            if ms.left_pressed {
                auto_paused = true;
                err_msg.clear();

                let panel_w = 820.min(w - 80);
                let panel_h = 600.min(h - 60);
                let px = (w - panel_w) / 2;
                let py = (h - panel_h) / 2;
                let list_x = px + 24;
                let list_w = panel_w - 48;
                let list_y = py + if decision.crash_detected { 154 } else { 114 };

                for i in 0..max_vis {
                    let cy = list_y + (i as i32) * 54;
                    if mx >= list_x && mx <= list_x + list_w && my >= cy && my <= cy + 48 {
                        selected_idx = i;
                        dirty = true;
                    }
                }

                let btn_y = py + panel_h - 62;
                let bw = (panel_w - 64) / 2;
                if my >= btn_y && my <= btn_y + 38 {
                    if mx >= px + 24 && mx <= px + 24 + bw {
                        let chosen = gens[selected_idx].id;
                        match snapshot::select_boot_generation(chosen) {
                            Ok(()) => return,
                            Err(e) => {
                                err_msg = String::from(e);
                                dirty = true;
                            }
                        }
                    } else if mx >= px + 40 + bw && mx <= px + 40 + bw * 2 {
                        if snapshot::select_boot_generation(decision.recommended_gen).is_ok() {
                            return;
                        }
                    }
                }
            }
        }

        let now_ms = timer::ticks_ms();
        let elapsed = now_ms.saturating_sub(start_ms);
        if !auto_paused {
            if elapsed >= AUTO_COUNTDOWN_MS {
                let target = if gens[selected_idx].health != GenHealth::Corrupt {
                    gens[selected_idx].id
                } else {
                    decision.recommended_gen
                };
                if snapshot::select_boot_generation(target).is_ok() {
                    return;
                }
                auto_paused = true;
                dirty = true;
            } else {
                let sec_left = (AUTO_COUNTDOWN_MS - elapsed + 999) / 1000;
                if sec_left != last_sec_drawn {
                    last_sec_drawn = sec_left;
                    dirty = true;
                }
            }
        }

        if dirty {
            dirty = false;
            draw_boot_menu(
                &mut canvas,
                &gens[..max_vis],
                selected_idx,
                &decision,
                auto_paused,
                if auto_paused {
                    0
                } else {
                    (AUTO_COUNTDOWN_MS.saturating_sub(elapsed) + 999) / 1000
                },
                &err_msg,
                summary.disk_backed,
                summary.active_slot,
                summary.commit_seq,
                mx,
                my,
            );
            fb.blit_backbuffer(&canvas.buffer);
        }

        hlt();
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_boot_menu(
    canvas: &mut Canvas,
    gens: &[Generation],
    selected_idx: usize,
    decision: &BootDecision,
    auto_paused: bool,
    secs_left: u64,
    err_msg: &str,
    disk_backed: bool,
    active_slot: char,
    commit_seq: u64,
    mx: i32,
    my: i32,
) {
    let theme = ThemePreset::ObsidianEmber.colors();
    let w = canvas.width;
    let h = canvas.height;

    canvas.fill_gradient_v(0, 0, w, h, 0x090C12, 0x131722);

    let panel_w = 820.min(w - 80);
    let panel_h = 600.min(h - 60);
    let px = (w - panel_w) / 2;
    let py = (h - panel_h) / 2;

    canvas.draw_soft_shadow(px, py, panel_w, panel_h, 14);
    canvas.fill_rounded_rect(px, py, panel_w, panel_h, 14, theme.window_bg);
    canvas.draw_rounded_rect_outline(
        px,
        py,
        panel_w,
        panel_h,
        14,
        if decision.crash_detected { 0xEF4444 } else { theme.accent },
    );

    // Header
    canvas.draw_rgba_sprite(px + 24, py + 18, assets::logo_48(), 255);
    canvas.draw_text_font(
        px + 84,
        py + 22,
        "AmbaOS Atomic Boot & Rollback Manager",
        theme.text_primary,
        FontFace::UiBold,
    );
    let sub = format!(
        "Immutable Read-Only Core OS  |  Persistent Read-Write User Data ({} | Slot {} | seq {})",
        if disk_backed { "ATA Disk" } else { "RAM" },
        active_slot,
        commit_seq
    );
    canvas.draw_text_clipped_font(px + 84, py + 44, panel_w - 108, &sub, theme.accent_sec, FontFace::Ui);

    let list_x = px + 24;
    let list_w = panel_w - 48;
    let mut list_y = py + 84;

    if decision.crash_detected {
        canvas.fill_rounded_rect(list_x, list_y, list_w, 56, 8, 0x3B1016);
        canvas.draw_rounded_rect_outline(list_x, list_y, list_w, 56, 8, 0xEF4444);
        canvas.draw_text_clipped_font(
            list_x + 14,
            list_y + 9,
            list_w - 28,
            "CRASH / CORRUPT GENERATION DETECTED - Instant Atomic Rollback Ready",
            0xFCA5A5,
            FontFace::UiBold,
        );
        canvas.draw_text_clipped_font(
            list_x + 14,
            list_y + 30,
            list_w - 28,
            &format!(
                "{} Pre-selected safe Gen #{}.",
                decision.crash_reason, decision.recommended_gen
            ),
            0xFECACA,
            FontFace::Ui,
        );
        list_y += 70;
    } else {
        let info_txt = if auto_paused {
            String::from("Selection paused - Choose a system generation to boot or press Enter.")
        } else {
            format!(
                "Auto-booting Gen #{} in {}s - Press Up/Down or click any snapshot to roll back",
                gens.get(selected_idx).map(|g| g.id).unwrap_or(1),
                secs_left
            )
        };
        canvas.draw_text_clipped_font(list_x, list_y, list_w, &info_txt, theme.accent_soft, FontFace::UiBold);
        list_y += 30;
    }

    for (i, g) in gens.iter().enumerate() {
        let cy = list_y + (i as i32) * 54;
        if cy + 48 > py + panel_h - 94 {
            break;
        }
        let is_sel = i == selected_idx;
        let is_rec = g.id == decision.recommended_gen;
        canvas.fill_rounded_rect(
            list_x,
            cy,
            list_w,
            48,
            8,
            if is_sel { theme.titlebar_active } else { theme.window_surface },
        );
        let border = if is_sel {
            theme.accent
        } else if g.health == GenHealth::Corrupt {
            0x991B1B
        } else {
            theme.border_inactive
        };
        canvas.draw_rounded_rect_outline(list_x, cy, list_w, 48, 8, border);

        // Gen badge
        let gen_lbl = format!("Gen #{}", g.id);
        canvas.fill_rounded_rect(list_x + 10, cy + 12, 66, 24, 6, theme.term_bg);
        canvas.draw_text_font(list_x + 18, cy + 16, &gen_lbl, theme.accent_soft, FontFace::UiBold);

        // Health badge
        let (h_txt, h_bg, h_fg) = match g.health {
            GenHealth::Corrupt => ("CORRUPT", 0x7F1D1D, 0xFCA5A5),
            GenHealth::Verified if is_rec => ("SAFE / REC", 0x064E3B, 0x4ADE80),
            GenHealth::Verified => ("VERIFIED", 0x0E3A4C, 0x38BDF8),
            GenHealth::Unverified => ("UNVERIFIED", 0x451A03, 0xFBBF24),
        };
        canvas.fill_rounded_rect(list_x + 84, cy + 12, 96, 24, 6, h_bg);
        canvas.draw_text_font(list_x + 94, cy + 16, h_txt, h_fg, FontFace::UiBold);

        canvas.draw_text_clipped_font(
            list_x + 190,
            cy + 6,
            list_w - 290,
            &g.label,
            theme.text_primary,
            FontFace::UiBold,
        );
        let delta_bytes: usize = g.core_overrides.iter().map(|(_, d)| d.len()).sum();
        let meta = format!(
            "{}  |  sha:{:08x}  |  CoW: {} B",
            g.timestamp,
            (g.hash & 0xFFFF_FFFF) as u32,
            delta_bytes
        );
        canvas.draw_text_clipped_font(
            list_x + 190,
            cy + 25,
            list_w - 290,
            &meta,
            theme.text_muted,
            FontFace::Mono,
        );

        if is_sel {
            canvas.draw_text_font(list_x + list_w - 86, cy + 15, "[SELECTED]", theme.accent, FontFace::UiBold);
        }
    }

    if !err_msg.is_empty() {
        canvas.draw_text_clipped_font(
            px + 24,
            py + panel_h - 88,
            panel_w - 48,
            err_msg,
            0xF87171,
            FontFace::UiBold,
        );
    }

    // Action buttons
    let btn_y = py + panel_h - 62;
    let bw = (panel_w - 64) / 2;

    canvas.fill_rounded_rect(px + 24, btn_y, bw, 38, 8, theme.accent);
    let b1_lbl = "Boot Selected Snapshot (Enter)";
    let b1_w = font::measure_text(FontFace::UiBold, b1_lbl);
    canvas.draw_text_font(px + 24 + (bw - b1_w) / 2, btn_y + 10, b1_lbl, 0xFFFFFF, FontFace::UiBold);

    canvas.fill_rounded_rect(px + 40 + bw, btn_y, bw, 38, 8, 0x065F46);
    canvas.draw_rounded_rect_outline(px + 40 + bw, btn_y, bw, 38, 8, theme.term_green);
    let b2_lbl = format!("Instant Rollback to Safe Gen #{} (R)", decision.recommended_gen);
    let b2_w = font::measure_text(FontFace::UiBold, &b2_lbl);
    canvas.draw_text_clipped_font(
        px + 40 + bw + ((bw - b2_w) / 2).max(10),
        btn_y + 10,
        bw - 20,
        &b2_lbl,
        0xECFDF5,
        FontFace::UiBold,
    );

    canvas.draw_cursor(mx, my, theme.accent);
}
