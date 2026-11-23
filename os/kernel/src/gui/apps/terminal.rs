use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::drivers::{keyboard::KeyEvent, power, rtc, timer};
use crate::fs::{snapshot, vfs};
use crate::gui::canvas::Canvas;
use crate::gui::font::{self, FontFace};
use crate::gui::theme::ThemeColors;
use crate::gui::window::AppKind;
use crate::mm::{heap, pmm, vmm};
use crate::net;
use crate::proc::sip::SipStatus;
use crate::proc::{caps, compute, loader, sip, task, uring, zerocopy};
use crate::serial_println;

#[derive(Clone, Copy)]
pub enum TermLineKind {
    Normal,
    Accent,
    Secondary,
    Green,
    Muted,
    Error,
}

#[derive(Clone)]
struct TermLine {
    text: String,
    kind: TermLineKind,
}

#[derive(Clone)]
pub enum TermAction {
    None,
    OpenWindow(AppKind),
    BrowseUrl(String),
    LaunchGame(String),
    RefreshTheme,
    ToggleA11y(&'static str),
}

pub struct TerminalApp {
    lines: Vec<TermLine>,
    input: String,
    scroll_offset: usize,
    fb_w: usize,
    fb_h: usize,
}

impl TerminalApp {
    pub fn new(fb_w: usize, fb_h: usize) -> Self {
        let mut app = Self {
            lines: Vec::new(),
            input: String::new(),
            scroll_offset: 0,
            fb_w,
            fb_h,
        };
        app.push("aji@AmbaOS:~$ ambaget install racer3d_setup.exe", TermLineKind::Green);
        app.push("[HTTP] Downloading racer3d_setup.exe (82%)", TermLineKind::Accent);
        app.push("[OK] Verified PE32+ & Saved to ATA Disk", TermLineKind::Normal);
        app.push("[Snapshot #14 Created]", TermLineKind::Green);
        app
    }

    fn push(&mut self, text: impl Into<String>, kind: TermLineKind) {
        self.lines.push(TermLine {
            text: text.into(),
            kind,
        });
        if self.lines.len() > 240 {
            let excess = self.lines.len() - 240;
            self.lines.drain(0..excess);
        }
        self.scroll_offset = 0;
    }

    pub fn handle_wheel(&mut self, dz: i32) -> bool {
        if dz == 0 || self.lines.is_empty() {
            return false;
        }
        if dz < 0 {
            self.scroll_offset = (self.scroll_offset + 3).min(self.lines.len().saturating_sub(1));
        } else {
            self.scroll_offset = self.scroll_offset.saturating_sub(3);
        }
        true
    }

    pub fn push_external_lines(&mut self, msgs: Vec<String>) -> bool {
        if msgs.is_empty() {
            return false;
        }
        for m in msgs {
            let kind = if m.contains("timeout") || m.contains("err") || m.contains("failed") {
                TermLineKind::Error
            } else if m.contains("[OK]") || m.contains("reply") || m.contains("done") {
                TermLineKind::Green
            } else {
                TermLineKind::Secondary
            };
            self.push(m, kind);
        }
        true
    }

    pub fn handle_key(&mut self, ev: KeyEvent) -> (bool, TermAction) {
        match ev {
            KeyEvent::Char(ch) => {
                if self.input.len() < 80 && ch.is_ascii() && !ch.is_ascii_control() {
                    self.input.push(ch);
                    return (true, TermAction::None);
                }
                (false, TermAction::None)
            }
            KeyEvent::Backspace => {
                if self.input.pop().is_some() {
                    return (true, TermAction::None);
                }
                (false, TermAction::None)
            }
            KeyEvent::Enter => {
                let cmd_line = core::mem::take(&mut self.input);
                self.push(format!("aji@AmbaOS:~$ {}", cmd_line), TermLineKind::Green);
                let action = self.exec_command(&cmd_line);
                (true, action)
            }
            KeyEvent::PageUp => {
                self.scroll_offset = (self.scroll_offset + 5).min(self.lines.len().saturating_sub(1));
                (true, TermAction::None)
            }
            KeyEvent::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_sub(5);
                (true, TermAction::None)
            }
            KeyEvent::End => {
                self.scroll_offset = 0;
                (true, TermAction::None)
            }
            _ => (false, TermAction::None),
        }
    }

    fn exec_command(&mut self, raw: &str) -> TermAction {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return TermAction::None;
        }
        serial_println!("[ash] exec: {}", trimmed);
        task::bump_pid(6);

        let mut parts = trimmed.splitn(2, ' ');
        let cmd = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("").trim();

        match cmd {
            "help" => {
                self.push("Available ash commands:", TermLineKind::Accent);
                self.push("  help               List available commands", TermLineKind::Normal);
                self.push("  ambafetch          System info & AmbaOS ASCII logo", TermLineKind::Normal);
                self.push("  ambaget <cmd>      Package manager (install <pkg.exe>|list|remove)", TermLineKind::Normal);
                self.push("  wine <file.exe>    Execute PE32+ (.exe) or ELF64 inside SIP Sandbox", TermLineKind::Normal);
                self.push("  hub / video        Open AmbaHub Game Launcher / AmbaMedia Video Player", TermLineKind::Normal);
                self.push("  browser            Open AmbaBrowser (HTML5 Web & Search)", TermLineKind::Normal);
                self.push("  browse <url/query> Open a URL or search the web", TermLineKind::Normal);
                self.push("  doom               Launch 3D FPS game AmbaDoom", TermLineKind::Normal);
                self.push("  snap [subcmd]      Immutable snapshots (list|create|update|rollback|corrupt|bootmenu)", TermLineKind::Normal);
                self.push("  uring [bench|zc]   Async io_uring benchmark & Zero-Copy DMA arena", TermLineKind::Normal);
                self.push("  sip [list|spawn|ipc|trap]  Single-Address-Space micro-unikernels", TermLineKind::Normal);
                self.push("  cap [list|toggle|attack]   Object-Capability Zero-Trust CSpace", TermLineKind::Normal);
                self.push("  compute [status|dag|zram]  Heterogeneous P/E/vNPU DAG Scheduler", TermLineKind::Normal);
                self.push("  ip / ping / dns    E1000 status, Ping, & DNS lookup", TermLineKind::Normal);
                self.push("  ls / cat / write   Initramfs VFS (Core RO / User RW)", TermLineKind::Normal);
                self.push("  date / mem / ps    RTC, RAM/Heap, & Kernel Tasks", TermLineKind::Normal);
                self.push("  a11y [feat]        Toggle contrast|cursor|zoom|text|mousekeys|sound", TermLineKind::Normal);
                self.push("  open <app>         doom|net|snap|media|paint|files|notes|calc|clock|snake|sysmon|settings|about", TermLineKind::Normal);
                self.push("  clear / reboot / shutdown", TermLineKind::Normal);
            }
            "snap" => {
                let mut sp = arg.splitn(2, ' ');
                let sub = sp.next().unwrap_or("");
                let rest = sp.next().unwrap_or("").trim();
                match sub {
                    "" | "list" => {
                        let sum = snapshot::summary();
                        self.push(
                            format!(
                                "Immutable Store: Active Gen #{} | Last Safe #{} | Slot {} seq {}",
                                sum.active_gen, sum.last_good_gen, sum.active_slot, sum.commit_seq
                            ),
                            TermLineKind::Accent,
                        );
                        for g in &sum.generations {
                            let tag = match g.health {
                                snapshot::GenHealth::Corrupt => "CORRUPT",
                                _ if g.id == sum.active_gen => "ACTIVE",
                                _ => "VERIFIED",
                            };
                            self.push(
                                format!("  Gen #{} [{}] sha:{:08x} - {}", g.id, tag, (g.hash & 0xFFFF_FFFF) as u32, g.label),
                                if g.health == snapshot::GenHealth::Corrupt { TermLineKind::Error } else { TermLineKind::Normal },
                            );
                        }
                    }
                    "create" => {
                        let (t_idx, wm) = snapshot::active_config();
                        let id = snapshot::create_snapshot(rest, t_idx, wm);
                        self.push(format!("Created CoW Snapshot Gen #{}", id), TermLineKind::Green);
                    }
                    "update" => {
                        let (t_idx, wm) = snapshot::active_config();
                        let id = snapshot::apply_atomic_update(t_idx, wm);
                        self.push(format!("Atomic OS Update applied -> Gen #{} (0.5.{})", id, id), TermLineKind::Green);
                    }
                    "corrupt" => {
                        let (t_idx, wm) = snapshot::active_config();
                        let id = snapshot::stage_corrupt_update(t_idx, wm);
                        self.push(format!("Staged CORRUPT Gen #{} for next boot. Run 'reboot' to test rollback.", id), TermLineKind::Error);
                    }
                    "rollback" => {
                        if let Ok(id) = rest.parse::<u32>() {
                            match snapshot::rollback_live(id) {
                                Ok(_) => {
                                    self.push(format!("Atomic live rollback to Gen #{} succeeded.", id), TermLineKind::Green);
                                    return TermAction::RefreshTheme;
                                }
                                Err(e) => self.push(format!("Rollback error: {}", e), TermLineKind::Error),
                            }
                        } else {
                            self.push("Usage: snap rollback <gen_id>", TermLineKind::Error);
                        }
                    }
                    "bootmenu" => snapshot::request_boot_menu_and_reboot(),
                    _ => self.push("Usage: snap [list|create <lbl>|update|rollback <id>|corrupt|bootmenu]", TermLineKind::Error),
                }
            }
            "uring" => match arg {
                "bench" => {
                    let (done, avg) = uring::run_burst_benchmark(1024);
                    let msg = format!("[uring] Burst {} async SQEs completed | 0 ctx-sw | ~{} cyc/op", done, avg);
                    serial_println!("{}", msg);
                    self.push(msg, TermLineKind::Green);
                }
                "zc" => {
                    let msg = zerocopy::run_vfs_to_net_zerocopy_demo();
                    serial_println!("[zc] {}", msg);
                    self.push(msg, TermLineKind::Green);
                }
                _ => {
                    let st = uring::stats();
                    let zc = zerocopy::stats();
                    self.push(
                        format!(
                            "io_uring: submitted={} completed={} ctx_switches={} avg={} cyc/op",
                            st.submitted, st.completed, st.ctx_switches, st.avg_cycles
                        ),
                        TermLineKind::Accent,
                    );
                    self.push(
                        format!(
                            "ZeroCopy: active={}/{} transfers={} saved={} B zram={} B",
                            zc.active_slots, zc.total_slots, zc.transfers, zc.bytes_saved, zc.zram_saved_bytes
                        ),
                        TermLineKind::Secondary,
                    );
                }
            },
            "sip" => match arg {
                "spawn" => {
                    let msg = sip::spawn_next_unikernel();
                    serial_println!("[sip] {}", msg);
                    self.push(msg, TermLineKind::Green);
                }
                "ipc" => {
                    let msg = sip::run_nanosecond_ipc_demo();
                    serial_println!("[sip] {}", msg);
                    self.push(msg, TermLineKind::Green);
                }
                "trap" => {
                    let msg = sip::run_rogue_sip_trap_demo();
                    serial_println!("[sip] {}", msg);
                    self.push(msg, TermLineKind::Error);
                }
                _ => {
                    let (isos, _, tlb) = sip::snapshot();
                    self.push(format!("SIP Micro-Unikernels (0 TLB flushes={}):", tlb), TermLineKind::Accent);
                    for iso in isos {
                        let st = if iso.status == SipStatus::TrappedFault { "TRAP" } else { "RUN" };
                        self.push(
                            format!(
                                "  #{} {} ({}) [{}] [{}] boot={}c ipc={}",
                                iso.id,
                                iso.name,
                                iso.profile,
                                st,
                                caps::format_caps_short(iso.caps),
                                iso.boot_cycles,
                                iso.ipc_count
                            ),
                            if iso.status == SipStatus::TrappedFault { TermLineKind::Error } else { TermLineKind::Normal },
                        );
                    }
                }
            },
            "cap" => {
                let mut sp = arg.splitn(3, ' ');
                let sub = sp.next().unwrap_or("");
                let a1 = sp.next().unwrap_or("");
                let a2 = sp.next().unwrap_or("");
                match sub {
                    "attack" => {
                        let msg = caps::simulate_unauthorized_attack();
                        serial_println!("[cap] {}", msg);
                        self.push(msg, TermLineKind::Error);
                    }
                    "toggle" | "grant" | "revoke" => {
                        if let (Some(app), Some(bit)) = (caps::parse_app(a1), caps::parse_cap(a2)) {
                            let active = match sub {
                                "grant" => {
                                    caps::grant(app, bit);
                                    true
                                }
                                "revoke" => {
                                    caps::revoke(app, bit);
                                    false
                                }
                                _ => caps::toggle(app, bit),
                            };
                            let msg = format!(
                                "[cap] {} {} -> {}",
                                app.name(),
                                caps::cap_name(bit),
                                if active { "GRANTED" } else { "REVOKED" }
                            );
                            serial_println!("{}", msg);
                            self.push(msg, TermLineKind::Accent);
                        } else {
                            self.push("Usage: cap <toggle|grant|revoke> <notes|browser|doom|snap> <write|net|admin>", TermLineKind::Error);
                        }
                    }
                    _ => {
                        let sn = caps::snapshot();
                        self.push(
                            format!("Capability CSpace (OK={} DENIED={}): {}", sn.granted_checks, sn.denied_checks, sn.last_audit),
                            TermLineKind::Accent,
                        );
                        for e in sn.entries {
                            self.push(
                                format!("  {:<12} [{}]", e.app.name(), caps::format_caps_short(e.caps)),
                                TermLineKind::Normal,
                            );
                        }
                    }
                }
            }
            "compute" => match arg {
                "dag" => {
                    let msg = compute::schedule_npu_dag();
                    serial_println!("[compute] {}", msg);
                    self.push(msg, TermLineKind::Green);
                }
                "zram" => {
                    let msg = compute::run_ecore_zram_pass();
                    serial_println!("[compute] {}", msg);
                    self.push(msg, TermLineKind::Green);
                }
                _ => {
                    let st = compute::stats();
                    self.push(
                        format!(
                            "Compute Graph: P-Core={} ({}c) | E-Core={} ({}c) | vNPU={} ({}c) | Zram={} B",
                            st.p_core_ops, st.p_core_cyc, st.e_core_ops, st.e_core_cyc, st.npu_ops, st.npu_cyc, st.zram_saved_bytes
                        ),
                        TermLineKind::Accent,
                    );
                    for n in st.nodes {
                        self.push(
                            format!("  Node #{} [{}] Stage {}: {} ({}c)", n.id, n.unit.label(), n.stage, n.desc, n.cycles),
                            TermLineKind::Normal,
                        );
                    }
                }
            },
            "hub" | "ambahub" | "store" => return TermAction::OpenWindow(AppKind::Hub),
            "video" => return TermAction::OpenWindow(AppKind::Media),
            "doom" => return TermAction::OpenWindow(AppKind::Doom),
            "ambaget" => {
                let mut sp = arg.splitn(2, ' ');
                let sub = sp.next().unwrap_or("");
                let rest = sp.next().unwrap_or("").trim();
                match sub {
                    "install" => {
                        let pkg = if rest.is_empty() { "invaders_setup.exe" } else { rest };
                        let id = pkg.split('_').next().unwrap_or(pkg).split('.').next().unwrap_or("app");
                        let is_elf = pkg.ends_with(".elf");
                        self.push(format!("[HTTP] Downloading {} (100%)", pkg), TermLineKind::Accent);
                        match loader::install_package(id, pkg, is_elf) {
                            Ok((path, snap)) => {
                                self.push(format!("[OK] Verified {} & Saved to {}", if is_elf { "ELF64" } else { "PE32+" }, path), TermLineKind::Green);
                                self.push(format!("[Snapshot #{} Created]", snap), TermLineKind::Green);
                            }
                            Err(e) => self.push(format!("Install error: {}", e), TermLineKind::Error),
                        }
                    }
                    "remove" => {
                        let id = rest.split('.').next().unwrap_or(rest);
                        match loader::uninstall_package(id) {
                            Ok(()) => self.push(format!("Removed package '{}'", id), TermLineKind::Accent),
                            Err(e) => self.push(format!("Error: {}", e), TermLineKind::Error),
                        }
                    }
                    _ => {
                        self.push("AmbaGet Packages:", TermLineKind::Accent);
                        for id in ["racer3d", "blocks", "invaders"] {
                            let st = loader::installed_binary_path(id).unwrap_or_else(|| String::from("not installed"));
                            self.push(format!("  {:<10} -> {}", id, st), TermLineKind::Normal);
                        }
                    }
                }
            }
            "wine" | "exec" => {
                let target = if arg.is_empty() { "/apps/racer3d/racer3d.exe" } else { arg };
                let path = if target.starts_with('/') {
                    String::from(target)
                } else {
                    format!("/apps/{}/{}", target.split('.').next().unwrap_or(target), target)
                };
                if let Some(bytes) = vfs::read_file(&path) {
                    if let Some(info) = loader::inspect_binary(&bytes) {
                        self.push(format!("[PE/ELF] {} ({}) Entry={:#x}", path, info.magic_str, info.entry_rva), TermLineKind::Green);
                        return TermAction::LaunchGame(info.app_id);
                    }
                    self.push("Invalid binary header.", TermLineKind::Error);
                } else {
                    let id = target.split('_').next().unwrap_or(target).split('.').next().unwrap_or(target);
                    return TermAction::LaunchGame(String::from(id));
                }
            }
            "browser" => return TermAction::OpenWindow(AppKind::Net),
            "browse" => {
                let target = if arg.is_empty() { "amba://home" } else { arg };
                return TermAction::BrowseUrl(String::from(target));
            }
            "clear" => self.lines.clear(),
            "echo" => self.push(arg, TermLineKind::Normal),
            "ip" | "ifconfig" => {
                let s = net::snapshot();
                if !s.present {
                    self.push("No E1000 NIC found.", TermLineKind::Error);
                } else {
                    let ip_s = s
                        .ip
                        .map(|a| format!("{}.{}.{}.{}/{}", a[0], a[1], a[2], a[3], s.prefix_len))
                        .unwrap_or_else(|| String::from("0.0.0.0 (negotiating)"));
                    let gw_s = s
                        .gateway
                        .map(|a| format!("{}.{}.{}.{}", a[0], a[1], a[2], a[3]))
                        .unwrap_or_else(|| String::from("-"));
                    let dns_s = s
                        .dns
                        .map(|a| format!("{}.{}.{}.{}", a[0], a[1], a[2], a[3]))
                        .unwrap_or_else(|| String::from("-"));
                    self.push(
                        format!(
                            "eth0 (Intel E1000) Link={}  MAC={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                            if s.link_up { "UP" } else { "DOWN" },
                            s.mac[0], s.mac[1], s.mac[2], s.mac[3], s.mac[4], s.mac[5]
                        ),
                        TermLineKind::Secondary,
                    );
                    self.push(format!("  IPv4    : {}", ip_s), TermLineKind::Normal);
                    self.push(format!("  Gateway : {}   DNS: {}", gw_s, dns_s), TermLineKind::Normal);
                    self.push(format!("  Packets : RX={}  TX={}", s.rx_packets, s.tx_packets), TermLineKind::Muted);
                }
            }
            "ping" => {
                let target = if arg.is_empty() { "10.0.2.2" } else { arg };
                if let Some(ip) = net::parse_ipv4_bytes(target) {
                    net::start_ping(ip);
                } else {
                    self.push("Usage: ping <a.b.c.d> (mis: ping 10.0.2.2)", TermLineKind::Error);
                }
            }
            "dns" => {
                let host = if arg.is_empty() { "example.com" } else { arg };
                net::start_dns(host);
            }
            "httpget" | "curl" => {
                let url = if arg.is_empty() { "http://10.0.2.2:18080/" } else { arg };
                net::start_http_get(url);
            }
            "date" => {
                let dt = rtc::read_rtc();
                self.push(
                    format!(
                        "Hardware RTC: {:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
                        dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second
                    ),
                    TermLineKind::Secondary,
                );
            }
            "ls" => {
                let files = vfs::list_paths();
                self.push(format!("VFS initramfs ({} files):", files.len()), TermLineKind::Accent);
                for (path, size) in files {
                    self.push(format!("  {:<28} {:>6} B", path, size), TermLineKind::Normal);
                }
            }
            "cat" => {
                if arg.is_empty() {
                    self.push("Usage: cat <path>", TermLineKind::Error);
                } else if let Some(bytes) = vfs::read_file(arg) {
                    let text = core::str::from_utf8(&bytes).unwrap_or("[binary]");
                    for line in text.lines() {
                        self.push(line, TermLineKind::Normal);
                    }
                } else {
                    self.push(format!("File not found: {}", arg), TermLineKind::Error);
                }
            }
            "write" => {
                let mut wparts = arg.splitn(2, ' ');
                let path = wparts.next().unwrap_or("").trim();
                let content = wparts.next().unwrap_or("");
                if path.is_empty() {
                    self.push("Usage: write <path> <text>", TermLineKind::Error);
                } else {
                    match vfs::write_file(path, content.as_bytes().to_vec()) {
                        Ok(()) => self.push(
                            format!("Saved {} bytes persistently to {}", content.len(), path),
                            TermLineKind::Green,
                        ),
                        Err(e) => self.push(e, TermLineKind::Error),
                    }
                }
            }
            "mem" => {
                let m = pmm::stats();
                let (heap_used, heap_total) = heap::stats();
                self.push(
                    format!(
                        "Physical RAM : {} MiB usable ({} MiB used / {} MiB free)",
                        m.total_bytes / (1024 * 1024),
                        m.used_bytes / (1024 * 1024),
                        m.free_bytes / (1024 * 1024)
                    ),
                    TermLineKind::Secondary,
                );
                self.push(
                    format!(
                        "Kernel Heap  : {} KiB / {} KiB ({} MiB)",
                        heap_used / 1024,
                        heap_total / 1024,
                        heap_total / (1024 * 1024)
                    ),
                    TermLineKind::Normal,
                );
                self.push(
                    format!("CR3 PML4     : {:#018x}", vmm::active_pml4_phys()),
                    TermLineKind::Muted,
                );
            }
            "ps" => {
                self.push("PID  PRIV   STATE     TICKS    NAME", TermLineKind::Accent);
                for line in task::format_summary() {
                    self.push(line, TermLineKind::Normal);
                }
            }
            "uptime" => {
                let secs = timer::uptime_secs();
                self.push(
                    format!(
                        "Uptime: {:02}:{:02}:{:02} ({} ticks @ {} Hz)",
                        secs / 3600,
                        (secs % 3600) / 60,
                        secs % 60,
                        timer::ticks_ms(),
                        timer::TARGET_HZ
                    ),
                    TermLineKind::Secondary,
                );
            }
            "ambafetch" | "neofetch" => {
                let m = pmm::stats();
                let (heap_used, heap_total) = heap::stats();
                let s = net::snapshot();
                let ip_s = s
                    .ip
                    .map(|a| format!("{}.{}.{}.{}", a[0], a[1], a[2], a[3]))
                    .unwrap_or_else(|| String::from("offline"));
                let secs = timer::uptime_secs();
                self.push("     _              _           ___  ____  ", TermLineKind::Accent);
                self.push("    / \\   _ __ ___ | |__   __ _/ _ \\/ ___| ", TermLineKind::Accent);
                self.push("   / _ \\ | '_ ` _ \\| '_ \\ / _` | | | \\___ \\ ", TermLineKind::Accent);
                self.push("  / ___ \\| | | | | | |_) | (_| | |_| |___) |", TermLineKind::Secondary);
                self.push(" /_/   \\_\\_| |_| |_|_.__/ \\__,_|\\___/|____/ ", TermLineKind::Secondary);
                self.push(" -------------------------------------------", TermLineKind::Muted);
                self.push(" OS          : AmbaOS v0.7.1 (x86_64 Rust #![no_std])", TermLineKind::Normal);
                self.push(" Desktop     : AmbaDE Clean Compositor (15 Apps + A11y Suite)", TermLineKind::Normal);
                self.push(format!(" Display     : {}x{} 32-bpp ARGB", self.fb_w, self.fb_h), TermLineKind::Normal);
                self.push(
                    format!(
                        " Memory      : {} MiB RAM | Heap {}/{} MiB",
                        m.total_bytes / (1024 * 1024),
                        heap_used / (1024 * 1024),
                        heap_total / (1024 * 1024)
                    ),
                    TermLineKind::Normal,
                );
                self.push(format!(" Network     : Intel E1000 ({})", ip_s), TermLineKind::Normal);
                self.push(format!(" VFS / Up    : {} files | {}s", vfs::file_count(), secs), TermLineKind::Normal);
            }
            "open" => match arg {
                "hub" | "store" => return TermAction::OpenWindow(AppKind::Hub),
                "files" => return TermAction::OpenWindow(AppKind::Files),
                "notes" => return TermAction::OpenWindow(AppKind::Notes),
                "media" => return TermAction::OpenWindow(AppKind::Media),
                "paint" => return TermAction::OpenWindow(AppKind::Paint),
                "calc" | "calculator" => return TermAction::OpenWindow(AppKind::Calculator),
                "clock" => return TermAction::OpenWindow(AppKind::Clock),
                "snake" => return TermAction::OpenWindow(AppKind::Snake),
                "doom" => return TermAction::OpenWindow(AppKind::Doom),
                "net" | "browser" => return TermAction::OpenWindow(AppKind::Net),
                "snap" | "snapshots" => return TermAction::OpenWindow(AppKind::Snapshots),
                "sysmon" => return TermAction::OpenWindow(AppKind::SysMon),
                "settings" => return TermAction::OpenWindow(AppKind::Settings),
                "about" => return TermAction::OpenWindow(AppKind::About),
                _ => self.push(
                    "Usage: open <hub|files|notes|media|paint|calc|clock|snake|doom|net|snap|sysmon|settings|about>",
                    TermLineKind::Error,
                ),
            },
            "a11y" => match arg {
                "contrast" | "hc" => {
                    self.push("[A11y] Toggled High Contrast theme", TermLineKind::Accent);
                    return TermAction::ToggleA11y("contrast");
                }
                "cursor" => {
                    self.push("[A11y] Toggled Large High-Vis Cursor (2x)", TermLineKind::Accent);
                    return TermAction::ToggleA11y("cursor");
                }
                "zoom" | "magnifier" => {
                    self.push("[A11y] Toggled Screen Magnifier Lens (2x)", TermLineKind::Accent);
                    return TermAction::ToggleA11y("zoom");
                }
                "text" | "large" => {
                    self.push("[A11y] Toggled Readable Large Text (1.5x)", TermLineKind::Accent);
                    return TermAction::ToggleA11y("text");
                }
                "mousekeys" | "mouse" => {
                    self.push("[A11y] Toggled Keyboard Mouse Keys (Arrows + Enter)", TermLineKind::Accent);
                    return TermAction::ToggleA11y("mousekeys");
                }
                "sound" | "audio" => {
                    self.push("[A11y] Toggled Audio Feedback Chimes", TermLineKind::Accent);
                    return TermAction::ToggleA11y("sound");
                }
                _ => {
                    self.push("Accessibility Suite Shortcuts:", TermLineKind::Accent);
                    self.push("  Alt+C : High Contrast | Alt+Z : Zoom 2x | Alt+L : Large Text", TermLineKind::Normal);
                    self.push("  Alt+K : Mouse Keys    | Alt+Tab: Next Win| F1 : A11y Panel", TermLineKind::Normal);
                    return TermAction::ToggleA11y("panel");
                }
            },
            "reboot" => power::reboot(),
            "shutdown" => power::shutdown(),
            _ => {
                self.push(
                    format!("Unknown command: '{}'. Type 'help'.", cmd),
                    TermLineKind::Error,
                );
            }
        }
        TermAction::None
    }

    pub fn render(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32, focused: bool, large_text: bool) {
        canvas.fill_rect(cx, cy, cw, ch, theme.term_bg);

        let pad_x = cx + 12;
        let pad_y = cy + 8;
        let line_h = if large_text { 26 } else { font::line_height(FontFace::Mono).max(18) };
        let max_rows = ((ch - 14) / line_h).max(1) as usize;
        let history_rows = max_rows.saturating_sub(1);
        let end_idx = self.lines.len().saturating_sub(self.scroll_offset);
        let start_idx = end_idx.saturating_sub(history_rows);

        let mut y = pad_y;
        for line in &self.lines[start_idx..end_idx] {
            let color = match line.kind {
                TermLineKind::Normal => theme.text_secondary,
                TermLineKind::Accent => theme.accent,
                TermLineKind::Secondary => theme.accent_sec,
                TermLineKind::Green => theme.term_green,
                TermLineKind::Muted => theme.text_muted,
                TermLineKind::Error => theme.err,
            };
            canvas.draw_text_scaled_clipped_font(pad_x, y, cw - 24, &line.text, color, FontFace::Mono, large_text);
            y += line_h;
        }

        let prompt = "aji@AmbaOS:~$ ";
        canvas.draw_text_scaled_clipped_font(pad_x, y, cw - 24, prompt, theme.term_prompt, FontFace::Mono, large_text);
        let prompt_w = font::measure_text_scaled(FontFace::Mono, prompt, large_text);
        let input_x = pad_x + prompt_w;
        canvas.draw_text_scaled_clipped_font(input_x, y, cw - 32 - prompt_w, &self.input, theme.text_primary, FontFace::Mono, large_text);

        let cursor_x = input_x + font::measure_text_scaled(FontFace::Mono, &self.input, large_text);
        if cursor_x + 8 < cx + cw - 8 {
            let ccol = if focused { theme.accent } else { theme.text_muted };
            canvas.fill_rect(cursor_x + 1, y + 2, if large_text { 10 } else { 8 }, if large_text { 21 } else { 15 }, ccol);
        }
    }
}
