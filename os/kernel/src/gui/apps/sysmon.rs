use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::drivers::{mouse, rtc, timer};
use crate::gui::canvas::Canvas;
use crate::gui::font::{self, FontFace};
use crate::gui::theme::ThemeColors;
use crate::mm::{heap, pmm, vmm};
use crate::proc::caps::{self, AppId, CAP_CORE_ADMIN, CAP_FS_WRITE, CAP_NET_ACCESS};
use crate::proc::sip::SipStatus;
use crate::proc::{compute, sip, task, uring, zerocopy};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SysTab {
    Tasks,
    RingZc,
    SipVm,
    CapNpu,
}

pub struct SysMonApp {
    fb_w: usize,
    fb_h: usize,
    tab: SysTab,
    history: Vec<u8>,
    ring_feedback: String,
    sip_feedback: String,
    cap_feedback: String,
}

impl SysMonApp {
    pub fn new(fb_w: usize, fb_h: usize) -> Self {
        Self {
            fb_w,
            fb_h,
            tab: SysTab::Tasks,
            history: Vec::with_capacity(48),
            ring_feedback: String::from("Ready: click Burst or Zero-Copy to test"),
            sip_feedback: String::from("Ready: Single-Address-Space Micro-Unikernels online"),
            cap_feedback: String::from("Zero-Trust CSpace: click pills to grant/revoke caps"),
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

    pub fn handle_click(&mut self, lx: i32, ly: i32, cw: i32, _ch: i32) -> bool {
        let avail = cw - 24 - 18;
        let tbw = (avail / 4).max(70);

        if (6..=28).contains(&ly) {
            for (idx, tab) in [SysTab::Tasks, SysTab::RingZc, SysTab::SipVm, SysTab::CapNpu]
                .iter()
                .enumerate()
            {
                let bx = 12 + (idx as i32) * (tbw + 6);
                if lx >= bx && lx <= bx + tbw {
                    self.tab = *tab;
                    return true;
                }
            }
        }

        match self.tab {
            SysTab::Tasks => false,
            SysTab::RingZc => {
                if (212..=236).contains(&ly) {
                    let bw = (cw - 36) / 2;
                    if lx >= 14 && lx <= 14 + bw {
                        let (done, avg) = uring::run_burst_benchmark(1024);
                        self.ring_feedback = format!("Burst {} async SQEs | 0 ctx-sw | {} cyc/op", done, avg);
                        return true;
                    }
                    if lx >= 22 + bw && lx <= cw - 14 {
                        self.ring_feedback = zerocopy::run_vfs_to_net_zerocopy_demo();
                        return true;
                    }
                }
                false
            }
            SysTab::SipVm => {
                if (206..=230).contains(&ly) {
                    let bw = (cw - 44) / 3;
                    if lx >= 14 && lx <= 14 + bw {
                        self.sip_feedback = sip::spawn_next_unikernel();
                        return true;
                    }
                    if lx >= 22 + bw && lx <= 22 + bw * 2 {
                        self.sip_feedback = sip::run_nanosecond_ipc_demo();
                        return true;
                    }
                    if lx >= 30 + bw * 2 && lx <= cw - 14 {
                        self.sip_feedback = sip::run_rogue_sip_trap_demo();
                        return true;
                    }
                }
                false
            }
            SysTab::CapNpu => {
                let bw = (cw - 36) / 2;
                if (54..=76).contains(&ly) {
                    if lx >= 14 && lx <= 14 + bw {
                        let on = caps::toggle(AppId::Notes, CAP_FS_WRITE);
                        self.cap_feedback = format!("Notes FS_WRITE -> {}", if on { "ON" } else { "OFF" });
                        return true;
                    }
                    if lx >= 22 + bw && lx <= cw - 14 {
                        let on = caps::toggle(AppId::Browser, CAP_NET_ACCESS);
                        self.cap_feedback = format!("Browser NET -> {}", if on { "ON" } else { "OFF" });
                        return true;
                    }
                }
                if (82..=104).contains(&ly) {
                    if lx >= 14 && lx <= 14 + bw {
                        let on = caps::toggle(AppId::Doom, CAP_FS_WRITE);
                        self.cap_feedback = format!("Doom FS_WRITE -> {}", if on { "ON" } else { "OFF" });
                        return true;
                    }
                    if lx >= 22 + bw && lx <= cw - 14 {
                        let on = caps::toggle(AppId::Snapshots, CAP_CORE_ADMIN);
                        self.cap_feedback = format!("Snap CORE_ADMIN -> {}", if on { "ON" } else { "OFF" });
                        return true;
                    }
                }
                if (206..=230).contains(&ly) {
                    let bw3 = (cw - 44) / 3;
                    if lx >= 14 && lx <= 14 + bw3 {
                        self.cap_feedback = caps::simulate_unauthorized_attack();
                        return true;
                    }
                    if lx >= 22 + bw3 && lx <= 22 + bw3 * 2 {
                        self.cap_feedback = compute::schedule_npu_dag();
                        return true;
                    }
                    if lx >= 30 + bw3 * 2 && lx <= cw - 14 {
                        self.cap_feedback = compute::run_ecore_zram_pass();
                        return true;
                    }
                }
                false
            }
        }
    }

    pub fn render(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        canvas.fill_rect(cx, cy, cw, ch, theme.window_bg);

        // Top 4-tab bar
        let avail = cw - 24 - 18;
        let tbw = (avail / 4).max(70);
        let tabs = [
            (SysTab::Tasks, "1:Tasks"),
            (SysTab::RingZc, "2:Ring/ZC"),
            (SysTab::SipVm, "3:SIP/VM"),
            (SysTab::CapNpu, "4:Cap/NPU"),
        ];
        for (i, &(tab, lbl)) in tabs.iter().enumerate() {
            let bx = cx + 12 + (i as i32) * (tbw + 6);
            let act = self.tab == tab;
            canvas.fill_rounded_rect(
                bx,
                cy + 6,
                tbw,
                22,
                5,
                if act { theme.accent } else { theme.window_surface },
            );
            let lw = font::measure_text(FontFace::UiBold, lbl);
            canvas.draw_text_clipped_font(
                bx + ((tbw - lw) / 2).max(4),
                cy + 8,
                tbw - 8,
                lbl,
                if act { 0xFFFFFF } else { theme.text_secondary },
                FontFace::UiBold,
            );
        }

        match self.tab {
            SysTab::Tasks => self.render_tasks(canvas, theme, cx, cy + 32, cw, ch - 32),
            SysTab::RingZc => self.render_ring_zc(canvas, theme, cx, cy + 32, cw, ch - 32),
            SysTab::SipVm => self.render_sip_vm(canvas, theme, cx, cy + 32, cw, ch - 32),
            SysTab::CapNpu => self.render_cap_npu(canvas, theme, cx, cy + 32, cw, ch - 32),
        }
    }

    fn render_tasks(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        let m = pmm::stats();
        let (heap_used, heap_total) = heap::stats();
        let secs = timer::uptime_secs();
        let (mx, my) = mouse::peek_pos();
        let dt = rtc::read_rtc();

        let x = cx + 14;
        let w = cw - 28;
        let mut y = cy + 4;

        canvas.draw_text_clipped_font(
            x,
            y,
            w,
            &format!("Physical RAM (PMM): {} / {} MiB", m.used_bytes / (1024 * 1024), m.total_bytes / (1024 * 1024)),
            theme.text_primary,
            FontFace::UiBold,
        );
        y += 18;
        canvas.draw_progress_bar(
            x,
            y,
            w,
            10,
            m.used_bytes as usize,
            (m.total_bytes as usize).max(1),
            theme.term_bg,
            theme.border_inactive,
            theme.accent,
        );
        y += 15;

        canvas.draw_text_clipped_font(
            x,
            y,
            w,
            &format!("Kernel Heap (256 MiB): {} / {} KiB", heap_used / 1024, heap_total / 1024),
            theme.text_primary,
            FontFace::UiBold,
        );
        y += 18;
        canvas.draw_progress_bar(
            x,
            y,
            w,
            10,
            heap_used,
            heap_total.max(1),
            theme.term_bg,
            theme.border_inactive,
            theme.accent_sec,
        );
        y += 16;

        let chart_h = 22;
        canvas.fill_rounded_rect(x, y, w, chart_h, 5, theme.window_surface);
        canvas.draw_rounded_rect_outline(x, y, w, chart_h, 5, theme.border_inactive);
        let bar_area_w = (w - 12).max(10);
        let step = (bar_area_w / 48).max(3);
        for (i, &val) in self.history.iter().enumerate() {
            let bx = x + 6 + (i as i32) * step;
            if bx + step > x + w - 6 {
                break;
            }
            let bh = (((val as i32) * (chart_h - 6)) / 100).max(2);
            canvas.fill_rect(bx, y + chart_h - 3 - bh, (step - 2).max(2), bh, theme.accent_soft);
        }
        y += chart_h + 5;

        canvas.draw_text_clipped_font(
            x,
            y,
            w,
            &format!(
                "RTC {:02}:{:02}:{:02} | UP {:02}:{:02}:{:02} | {}x{} ({},{}) PML4:{:#x}",
                dt.hour, dt.minute, dt.second, secs / 3600, (secs % 3600) / 60, secs % 60, self.fb_w, self.fb_h, mx, my, vmm::active_pml4_phys()
            ),
            theme.text_muted,
            FontFace::Ui,
        );
        y += 16;

        canvas.fill_rect(x, y, w, 1, theme.border_inactive);
        y += 4;
        canvas.draw_text_clipped_font(x, y, w, "PID  UNIT    STATE   TICKS   TASK", theme.accent_sec, FontFace::Mono);
        y += 16;

        for t in task::snapshot() {
            if y + 14 > cy + ch - 2 {
                break;
            }
            let row = format!("{:<4} {:<7} {:<7} {:<7} {}", t.pid, t.unit, t.state, t.cpu_ticks, t.name);
            let col = if t.state == "RUNNING" { theme.term_green } else { theme.text_secondary };
            canvas.draw_text_clipped_font(x, y, w, &row, col, FontFace::Mono);
            y += 15;
        }
    }

    fn render_ring_zc(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, _ch: i32) {
        let ur = uring::stats();
        let zc = zerocopy::stats();
        let x = cx + 14;
        let w = cw - 28;

        canvas.draw_text_font(x, cy + 4, "#1 Async Lock-Free io_uring Syscall Ring", theme.accent_soft, FontFace::UiBold);
        canvas.fill_rounded_rect(x, cy + 22, w, 62, 7, theme.window_surface);
        canvas.draw_rounded_rect_outline(x, cy + 22, w, 62, 7, theme.border_inactive);
        canvas.draw_text_clipped_font(
            x + 10,
            cy + 27,
            w - 20,
            &format!("SQ/CQ Depth: {}/{} (cap 64) | Ctx Switches: {}", ur.sq_depth, ur.cq_depth, ur.ctx_switches),
            theme.term_green,
            FontFace::UiBold,
        );
        canvas.draw_text_clipped_font(
            x + 10,
            cy + 45,
            w - 20,
            &format!("Submitted: {}  |  Completed: {}  |  Avg: {} cyc/op", ur.submitted, ur.completed, ur.avg_cycles),
            theme.text_primary,
            FontFace::Ui,
        );
        canvas.draw_text_clipped_font(x + 10, cy + 63, w - 20, &ur.last_summary, theme.text_muted, FontFace::Ui);

        canvas.draw_text_font(x, cy + 92, "#2 Kernel Bypass & Zero-Copy DMA Arena", theme.accent_sec, FontFace::UiBold);
        canvas.fill_rounded_rect(x, cy + 110, w, 62, 7, theme.window_surface);
        canvas.draw_rounded_rect_outline(x, cy + 110, w, 62, 7, theme.border_inactive);
        canvas.draw_text_clipped_font(
            x + 10,
            cy + 115,
            w - 20,
            &format!("DMA Slots: {}/{} (4 KiB ea) | Handoffs: {}", zc.active_slots, zc.total_slots, zc.transfers),
            theme.text_primary,
            FontFace::UiBold,
        );
        canvas.draw_text_clipped_font(
            x + 10,
            cy + 133,
            w - 20,
            &format!("Zero-Copy Bytes Saved: {} B | E-Core Zram Saved: {} B", zc.bytes_saved, zc.zram_saved_bytes),
            theme.term_green,
            FontFace::Ui,
        );
        canvas.draw_text_clipped_font(x + 10, cy + 151, w - 20, &zc.last_op, theme.text_muted, FontFace::Ui);

        let bw = (cw - 36) / 2;
        canvas.fill_rounded_rect(x, cy + 180, bw, 24, 6, theme.accent);
        canvas.draw_text_clipped_font(x + 12, cy + 183, bw - 20, "Burst 1024 SQEs", 0xFFFFFF, FontFace::UiBold);

        canvas.fill_rounded_rect(x + bw + 8, cy + 180, bw, 24, 6, 0x0284C7);
        canvas.draw_text_clipped_font(x + bw + 18, cy + 183, bw - 20, "Zero-Copy VFS->Net", 0xFFFFFF, FontFace::UiBold);

        canvas.fill_rounded_rect(x, cy + 212, w, 42, 6, theme.term_bg);
        canvas.draw_rounded_rect_outline(x, cy + 212, w, 42, 6, theme.border_inactive);
        canvas.draw_text_clipped_font(x + 10, cy + 224, w - 20, &self.ring_feedback, theme.accent_soft, FontFace::Mono);
    }

    fn render_sip_vm(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, _ch: i32) {
        let (isolates, _, tlb_flushes) = sip::snapshot();
        let x = cx + 14;
        let w = cw - 28;

        canvas.draw_text_clipped_font(
            x,
            cy + 4,
            w,
            &format!("#3/#4 SIP Micro-Unikernels (1-CR3 SAS | TLB Flushes: {})", tlb_flushes),
            theme.accent_soft,
            FontFace::UiBold,
        );

        for (i, iso) in isolates.iter().take(5).enumerate() {
            let ry = cy + 24 + (i as i32) * 28;
            canvas.fill_rounded_rect(x, ry, w, 24, 5, theme.window_surface);
            let is_trap = iso.status == SipStatus::TrappedFault;
            canvas.draw_rounded_rect_outline(x, ry, w, 24, 5, if is_trap { theme.err } else { theme.border_inactive });
            let tag = if is_trap { "TRAP" } else { "RUN" };
            let tag_col = if is_trap { theme.err } else { theme.term_green };
            canvas.draw_text_font(x + 8, ry + 4, tag, tag_col, FontFace::Mono);
            let line = format!(
                "#{} {:<16} 16K  boot:{}c  ipc:{}",
                iso.id, iso.name, iso.boot_cycles, iso.ipc_count
            );
            canvas.draw_text_clipped_font(x + 54, ry + 4, w - 62, &line, theme.text_primary, FontFace::Mono);
        }

        let bw = (cw - 44) / 3;
        let by = cy + 174;
        canvas.fill_rounded_rect(x, by, bw, 24, 6, theme.accent);
        canvas.draw_text_clipped_font(x + 8, by + 3, bw - 12, "+Spawn Unikernel", 0xFFFFFF, FontFace::UiBold);

        canvas.fill_rounded_rect(x + bw + 8, by, bw, 24, 6, 0x065F46);
        canvas.draw_text_clipped_font(x + bw + 18, by + 3, bw - 12, "0-Copy IPC", 0xECFDF5, FontFace::UiBold);

        canvas.fill_rounded_rect(x + (bw + 8) * 2, by, bw, 24, 6, 0x991B1B);
        canvas.draw_text_clipped_font(x + (bw + 8) * 2 + 12, by + 3, bw - 12, "Trap Rogue", 0xFEE2E2, FontFace::UiBold);

        canvas.fill_rounded_rect(x, cy + 206, w, 46, 6, theme.term_bg);
        canvas.draw_rounded_rect_outline(x, cy + 206, w, 46, 6, theme.border_inactive);
        canvas.draw_text_clipped_font(x + 10, cy + 219, w - 20, &self.sip_feedback, theme.accent_sec, FontFace::Mono);
    }

    fn render_cap_npu(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, _ch: i32) {
        let snap = caps::snapshot();
        let comp = compute::stats();
        let x = cx + 14;
        let w = cw - 28;
        let bw = (cw - 36) / 2;

        canvas.draw_text_clipped_font(
            x,
            cy + 2,
            w,
            &format!("#5 Capabilities (OK:{} DENIED:{}) - Click to toggle:", snap.granted_checks, snap.denied_checks),
            theme.accent_soft,
            FontFace::UiBold,
        );

        let has_cap = |app: AppId, bit: u32| -> bool {
            snap.entries
                .iter()
                .find(|e| e.app == app)
                .map(|e| (e.caps & bit) != 0)
                .unwrap_or(false)
        };

        let pills = [
            (x, cy + 22, "Notes: FS_WRITE", has_cap(AppId::Notes, CAP_FS_WRITE)),
            (x + bw + 8, cy + 22, "Browser: NET", has_cap(AppId::Browser, CAP_NET_ACCESS)),
            (x, cy + 50, "Doom: FS_WRITE", has_cap(AppId::Doom, CAP_FS_WRITE)),
            (x + bw + 8, cy + 50, "Snap: CORE_ADMIN", has_cap(AppId::Snapshots, CAP_CORE_ADMIN)),
        ];
        for (px, py, lbl, on) in pills {
            canvas.fill_rounded_rect(px, py, bw, 22, 5, if on { 0x064E3B } else { 0x3F1D2B });
            canvas.draw_rounded_rect_outline(px, py, bw, 22, 5, if on { theme.term_green } else { theme.err });
            canvas.draw_text_clipped_font(
                px + 8,
                py + 3,
                bw - 16,
                &format!("{} [{}]", lbl, if on { "ON" } else { "OFF" }),
                if on { 0xECFDF5 } else { 0xFCA5A5 },
                FontFace::UiBold,
            );
        }

        canvas.draw_text_font(x, cy + 80, "#6 Heterogeneous Compute Graph (P/E/vNPU)", theme.accent_sec, FontFace::UiBold);
        canvas.fill_rounded_rect(x, cy + 98, w, 66, 7, theme.window_surface);
        canvas.draw_rounded_rect_outline(x, cy + 98, w, 66, 7, theme.border_inactive);
        canvas.draw_text_clipped_font(
            x + 10,
            cy + 104,
            w - 20,
            &format!("P-Core (UI): {} ops ({} cyc) | vNPU (INT8): {} ops ({} cyc)", comp.p_core_ops, comp.p_core_cyc, comp.npu_ops, comp.npu_cyc),
            theme.text_primary,
            FontFace::Ui,
        );
        canvas.draw_text_clipped_font(
            x + 10,
            cy + 123,
            w - 20,
            &format!("E-Core (BG/Zram): {} ops ({} cyc) | Zram Saved: {} B", comp.e_core_ops, comp.e_core_cyc, comp.zram_saved_bytes),
            theme.term_green,
            FontFace::Ui,
        );
        canvas.draw_text_clipped_font(x + 10, cy + 142, w - 20, &comp.last_status, theme.text_muted, FontFace::Ui);

        let bw3 = (cw - 44) / 3;
        let by = cy + 174;
        canvas.fill_rounded_rect(x, by, bw3, 24, 6, 0x991B1B);
        canvas.draw_text_clipped_font(x + 12, by + 3, bw3 - 12, "Test Attack", 0xFEE2E2, FontFace::UiBold);

        canvas.fill_rounded_rect(x + bw3 + 8, by, bw3, 24, 6, theme.accent);
        canvas.draw_text_clipped_font(x + bw3 + 18, by + 3, bw3 - 12, "Run vNPU DAG", 0xFFFFFF, FontFace::UiBold);

        canvas.fill_rounded_rect(x + (bw3 + 8) * 2, by, bw3, 24, 6, 0x0284C7);
        canvas.draw_text_clipped_font(x + (bw3 + 8) * 2 + 12, by + 3, bw3 - 12, "E-Core Zram", 0xFFFFFF, FontFace::UiBold);

        canvas.fill_rounded_rect(x, cy + 206, w, 46, 6, theme.term_bg);
        canvas.draw_rounded_rect_outline(x, cy + 206, w, 46, 6, theme.border_inactive);
        canvas.draw_text_clipped_font(x + 10, cy + 219, w - 20, &self.cap_feedback, theme.accent_soft, FontFace::Mono);
    }
}
