use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::drivers::framebuffer::FramebufferInfo;
use crate::drivers::keyboard::KeyEvent;
use crate::drivers::mouse::MouseState;
use crate::drivers::{audio, power, rtc, timer};
use crate::fs::snapshot;
use crate::gui::apps::about::AboutApp;
use crate::gui::apps::calculator::CalculatorApp;
use crate::gui::apps::clock::ClockApp;
use crate::gui::apps::doom::DoomApp;
use crate::gui::apps::files::{FileAction, FileViewerApp};
use crate::gui::apps::hub::{HubAction, HubApp};
use crate::gui::apps::media::MediaApp;
use crate::gui::apps::net::NetApp;
use crate::gui::apps::notes::NotesApp;
use crate::gui::apps::paint::PaintApp;
use crate::gui::apps::settings::{SettingsAction, SettingsApp};
use crate::gui::apps::snapshots::{SnapAction, SnapshotsApp};
use crate::gui::apps::snake::SnakeApp;
use crate::gui::apps::sysmon::SysMonApp;
use crate::gui::apps::terminal::{TermAction, TerminalApp};
use crate::gui::assets;
use crate::gui::canvas::Canvas;
use crate::gui::font::{self, FontFace};
use crate::gui::theme::{ThemeColors, ThemePreset};
use crate::gui::window::{AppKind, TOPBAR_H, TitleAction, Window};
use crate::mm::pmm;
use crate::net;
use crate::proc::compute;

const DOCK_H: i32 = 88;
const DOCK_MARGIN_B: i32 = 10;
const DOCK_ICON_SIZE: i32 = 54;
const DOCK_GAP: i32 = 24;

struct DragState {
    kind: AppKind,
    offset_x: i32,
    offset_y: i32,
}

const DOCK_APPS: [AppKind; 6] = [
    AppKind::Files,
    AppKind::Terminal,
    AppKind::Hub,
    AppKind::Media,
    AppKind::Net,
    AppKind::Settings,
];

pub struct DesktopCompositor {
    fb: FramebufferInfo,
    canvas: Canvas,
    wallpaper_cache: Vec<u32>,
    scene_cache: Vec<u32>,
    widget_logo_rgba: Vec<u8>,
    theme_preset: ThemePreset,
    show_watermark: bool,
    windows: Vec<Window>,
    hub: HubApp,
    terminal: TerminalApp,
    files: FileViewerApp,
    notes: NotesApp,
    media: MediaApp,
    paint: PaintApp,
    calculator: CalculatorApp,
    clock: ClockApp,
    snake: SnakeApp,
    doom: DoomApp,
    net_app: NetApp,
    snapshots: SnapshotsApp,
    sysmon: SysMonApp,
    settings: SettingsApp,
    about: AboutApp,
    start_menu_open: bool,
    search_focused: bool,
    search_input: String,
    boost_mode: bool,
    hover_dock: Option<usize>,
    drag: Option<DragState>,
    paint_drawing: bool,
    prev_left: bool,
    last_mouse_x: i32,
    last_mouse_y: i32,
    prev_cursor_x: i32,
    prev_cursor_y: i32,
    last_sec: u64,
    last_minute: u8,
    last_clock_anim_ms: u64,
    dirty: bool,
    focused_dirty: bool,
    cursor_dirty: bool,
}

fn prescale_sprite_rgba(src: assets::Sprite, dst_w: i32, dst_h: i32) -> Vec<u8> {
    let mut out = vec![0u8; (dst_w * dst_h * 4) as usize];
    for dy in 0..dst_h {
        let sy = (dy * src.height) / dst_h;
        for dx in 0..dst_w {
            let sx = (dx * src.width) / dst_w;
            let sp = ((sy * src.width + sx) as usize) * 4;
            let dp = ((dy * dst_w + dx) as usize) * 4;
            if sp + 3 < src.rgba.len() {
                out[dp..dp + 4].copy_from_slice(&src.rgba[sp..sp + 4]);
            }
        }
    }
    out
}

impl DesktopCompositor {
    pub fn new(fb: FramebufferInfo) -> Self {
        let w = fb.width as i32;
        let h = fb.height as i32;
        let (t_idx, show_watermark) = snapshot::active_config();
        let theme_preset = ThemePreset::from_index(t_idx);

        let mut canvas = Canvas::new(w, h, fb.red_shift, fb.green_shift, fb.blue_shift);
        canvas.render_wallpaper(theme_preset.colors(), show_watermark);
        let wallpaper_cache = canvas.buffer.clone();

        let right_w = 422;
        let right_x = (w - right_w - 26).max(520);

        let windows = vec![
            Window::new(AppKind::Files, "Files - VFS & PE32+ Inspector", 160, 102, 660, 440, false),
            Window::new(AppKind::Notes, "AmbaNotes", 220, 120, 520, 380, false),
            Window::new(AppKind::Media, "AmbaMedia - Video & Audio Player", 480, 102, 520, 446, false),
            Window::new(AppKind::Paint, "AmbaPaint - Studio", 260, 110, 520, 392, false),
            Window::new(AppKind::Calculator, "Calculator", 460, 120, 296, 400, false),
            Window::new(AppKind::Clock, "AmbaClock - RTC & Stopwatch", 680, 380, 490, 300, false),
            Window::new(AppKind::Snake, "AmbaSnake", 400, 140, 440, 360, false),
            Window::new(AppKind::Doom, "AmbaDoom - E1M1 Hangar", 310, 102, 580, 386, false),
            Window::new(AppKind::Net, "Browser - AmbaBrowser", 210, 88, 860, 580, false),
            Window::new(AppKind::Snapshots, "AmbaSnap - Rollback & Snapshots", 250, 102, 700, 480, false),
            Window::new(AppKind::Settings, "Settings & Appearance", 380, 140, 440, 376, false),
            Window::new(AppKind::SysMon, "System Monitor (io_uring / SIP / vNPU)", 300, 120, 560, 420, false),
            Window::new(AppKind::About, "About AmbaOS", 380, 140, 440, 340, false),
            Window::new(AppKind::Terminal, "AmbaTerm", right_x, 412, right_w, 214, true),
            Window::new(AppKind::Hub, "AmbaHub", 26, 102, 476, 546, true),
        ];

        let mut sysmon = SysMonApp::new(fb.width, fb.height);
        sysmon.sample_second();

        let widget_logo_rgba = prescale_sprite_rgba(assets::logo_120(), 76, 76);
        let scene_cache = wallpaper_cache.clone();

        Self {
            fb,
            canvas,
            wallpaper_cache,
            scene_cache,
            widget_logo_rgba,
            theme_preset,
            show_watermark,
            windows,
            hub: HubApp::new(),
            terminal: TerminalApp::new(fb.width, fb.height),
            files: FileViewerApp::new(),
            notes: NotesApp::new(),
            media: MediaApp::new(),
            paint: PaintApp::new(),
            calculator: CalculatorApp::new(),
            clock: ClockApp::new(),
            snake: SnakeApp::new(),
            doom: DoomApp::new(),
            net_app: NetApp::new(),
            snapshots: SnapshotsApp::new(),
            sysmon,
            settings: SettingsApp::new(),
            about: AboutApp::new(fb.width, fb.height),
            start_menu_open: false,
            search_focused: false,
            search_input: String::new(),
            boost_mode: false,
            hover_dock: None,
            drag: None,
            paint_drawing: false,
            prev_left: false,
            last_mouse_x: w / 2,
            last_mouse_y: h / 2,
            prev_cursor_x: w / 2,
            prev_cursor_y: h / 2,
            last_sec: u64::MAX,
            last_minute: rtc::read_rtc().minute,
            last_clock_anim_ms: 0,
            dirty: true,
            focused_dirty: false,
            cursor_dirty: false,
        }
    }

    #[inline]
    fn theme(&self) -> ThemeColors {
        self.theme_preset.colors()
    }

    fn rebuild_wallpaper(&mut self) {
        self.canvas.render_wallpaper(self.theme(), self.show_watermark);
        self.wallpaper_cache.copy_from_slice(&self.canvas.buffer);
        self.dirty = true;
    }

    fn focused_window_kind(&self) -> Option<AppKind> {
        self.windows.iter().rev().find(|w| w.is_visible()).map(|w| w.kind)
    }

    fn search_bar_rect(&self) -> (i32, i32, i32, i32) {
        let sw = 516.min(self.canvas.width - 60);
        ((self.canvas.width - sw) / 2, 40, sw, 36)
    }

    fn control_widget_rect(&self) -> (i32, i32, i32, i32) {
        let rw = 422;
        let rx = (self.canvas.width - rw - 26).max(520);
        (rx, 102, rw, 294)
    }

    fn exec_spotlight_query(&mut self) {
        let q = self.search_input.trim().to_ascii_lowercase();
        self.search_input.clear();
        self.search_focused = false;
        if q.is_empty() {
            return;
        }
        if q.contains("racer") || q.contains("blocks") || q.contains("invader") || q.contains(".exe") || q.contains("hub") || q.contains("game") {
            self.open_and_focus(AppKind::Hub);
            if q.contains("racer") {
                let _ = self.hub.launch_by_id("racer3d");
            } else if q.contains("blocks") {
                let _ = self.hub.launch_by_id("blocks");
            } else if q.contains("invader") {
                self.hub.open_install_wizard(2);
            }
        } else if q.contains("snap") || q.contains("rollback") {
            self.open_and_focus(AppKind::Snapshots);
        } else if q.contains("media") || q.contains("video") || q.contains("ambv") {
            self.open_and_focus(AppKind::Media);
        } else if q.contains("browser") || q.contains("web") || q.contains("http") || q.contains("google") {
            self.open_and_focus(AppKind::Net);
        } else if q.contains("doom") {
            self.open_and_focus(AppKind::Doom);
        } else if q.contains("file") || q.contains(".elf") {
            self.open_and_focus(AppKind::Files);
        } else if q.contains("setting") || q.contains("theme") {
            self.open_and_focus(AppKind::Settings);
        } else if q.contains("sys") || q.contains("npu") {
            self.open_and_focus(AppKind::SysMon);
        } else if q.contains("term") {
            self.open_and_focus(AppKind::Terminal);
        } else {
            self.open_and_focus(AppKind::Net);
            self.net_app.sync_url_from_term(&q);
        }
        self.dirty = true;
    }

    pub fn handle_key(&mut self, ev: KeyEvent) {
        if ev == KeyEvent::CtrlSpace {
            self.search_focused = !self.search_focused;
            self.dirty = true;
            return;
        }

        if self.search_focused {
            match ev {
                KeyEvent::Escape => {
                    self.search_focused = false;
                }
                KeyEvent::Backspace => {
                    self.search_input.pop();
                }
                KeyEvent::Enter => {
                    self.exec_spotlight_query();
                }
                KeyEvent::Char(ch) if ch.is_ascii() && !ch.is_ascii_control() => {
                    if self.search_input.len() < 48 {
                        self.search_input.push(ch);
                    }
                }
                _ => {}
            }
            self.dirty = true;
            return;
        }

        let focused = self.focused_window_kind();
        if focused == Some(AppKind::Hub) && self.hub.is_playing_game() {
            if self.hub.handle_key(ev) {
                self.mark_app_dirty(AppKind::Hub);
            }
            return;
        }

        let target = match focused {
            Some(AppKind::Notes) => AppKind::Notes,
            Some(AppKind::Calculator) => AppKind::Calculator,
            Some(AppKind::Snake) => AppKind::Snake,
            Some(AppKind::Doom) => AppKind::Doom,
            Some(AppKind::Net) => AppKind::Net,
            _ => {
                if self.windows.iter().any(|w| w.kind == AppKind::Terminal && w.is_visible()) {
                    AppKind::Terminal
                } else {
                    return;
                }
            }
        };

        match target {
            AppKind::Notes => {
                if self.notes.handle_key(ev) {
                    self.mark_app_dirty(AppKind::Notes);
                }
            }
            AppKind::Calculator => {
                if self.calculator.handle_key(ev) {
                    self.mark_app_dirty(AppKind::Calculator);
                }
            }
            AppKind::Snake => {
                if self.snake.handle_key(ev) {
                    self.mark_app_dirty(AppKind::Snake);
                }
            }
            AppKind::Doom => {
                if self.doom.handle_key(ev) {
                    self.mark_app_dirty(AppKind::Doom);
                }
            }
            AppKind::Net => {
                if self.net_app.handle_key(ev) {
                    self.mark_app_dirty(AppKind::Net);
                }
            }
            AppKind::Terminal => {
                let (changed, action) = self.terminal.handle_key(ev);
                match action {
                    TermAction::OpenWindow(kind) => {
                        self.open_and_focus(kind);
                        self.dirty = true;
                    }
                    TermAction::BrowseUrl(url) => {
                        self.open_and_focus(AppKind::Net);
                        self.net_app.sync_url_from_term(&url);
                        self.dirty = true;
                    }
                    TermAction::LaunchGame(id) => {
                        self.open_and_focus(AppKind::Hub);
                        let act = self.hub.launch_by_id(&id);
                        if act == HubAction::OpenDoom {
                            self.open_and_focus(AppKind::Doom);
                        }
                        self.dirty = true;
                    }
                    TermAction::RefreshTheme => {
                        let (t_idx, wm) = snapshot::active_config();
                        self.theme_preset = ThemePreset::from_index(t_idx);
                        self.show_watermark = wm;
                        self.rebuild_wallpaper();
                    }
                    TermAction::None => {
                        if changed {
                            self.mark_app_dirty(AppKind::Terminal);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn dock_rect(&self) -> (i32, i32, i32, i32) {
        let count = DOCK_APPS.len() as i32;
        let dw = count * DOCK_ICON_SIZE + (count - 1) * DOCK_GAP + 40;
        let dx = (self.canvas.width - dw) / 2;
        let dy = self.canvas.height - DOCK_H - DOCK_MARGIN_B;
        (dx, dy, dw, DOCK_H)
    }

    fn dock_hit_index(&self, mx: i32, my: i32) -> Option<usize> {
        let (dx, dy, dw, dh) = self.dock_rect();
        if mx < dx || mx >= dx + dw || my < dy || my >= dy + dh {
            return None;
        }
        for i in 0..DOCK_APPS.len() {
            let ix = dx + 20 + (i as i32) * (DOCK_ICON_SIZE + DOCK_GAP);
            if mx >= ix - 8 && mx < ix + DOCK_ICON_SIZE + 8 {
                return Some(i);
            }
        }
        None
    }

    pub fn handle_mouse(&mut self, ms: MouseState) {
        if ms.x != self.last_mouse_x || ms.y != self.last_mouse_y {
            self.last_mouse_x = ms.x;
            self.last_mouse_y = ms.y;
            let new_hover = self.dock_hit_index(ms.x, ms.y);
            if new_hover != self.hover_dock {
                self.hover_dock = new_hover;
                self.dirty = true;
            } else {
                self.cursor_dirty = true;
            }
        }

        if ms.wheel != 0 && self.focused_window_kind() == Some(AppKind::Net) {
            if self.net_app.handle_wheel(ms.wheel) {
                self.dirty = true;
            }
        }

        let just_pressed = ms.left_pressed || (ms.left && !self.prev_left);
        let just_released = ms.left_released || (!ms.left && self.prev_left);
        self.prev_left = ms.left;

        if just_released {
            if self.drag.is_some() {
                self.drag = None;
                self.dirty = true;
            }
            if self.paint_drawing {
                self.paint.handle_release();
                self.paint_drawing = false;
            }
        }

        if ms.left {
            if let Some(drag) = &self.drag {
                if let Some(win) = self.windows.iter_mut().find(|w| w.kind == drag.kind) {
                    if !win.maximized {
                        let nx = (ms.x - drag.offset_x).clamp(0, (self.canvas.width - 80).max(0));
                        let ny = (ms.y - drag.offset_y).clamp(TOPBAR_H, (self.canvas.height - 60).max(TOPBAR_H));
                        if win.x != nx || win.y != ny {
                            win.x = nx;
                            win.y = ny;
                            self.dirty = true;
                        }
                    }
                }
                return;
            }
            if self.paint_drawing {
                if let Some(pw) = self.windows.iter().find(|w| w.kind == AppKind::Paint && w.is_visible()) {
                    let (cx, cy, cw, ch) = pw.client_rect();
                    if self.paint.handle_drag(ms.x - cx, ms.y - cy, cw, ch) {
                        self.dirty = true;
                    }
                }
                return;
            }
        }

        if just_pressed {
            self.on_left_click(ms.x, ms.y);
        }
    }

    fn start_menu_rect(&self) -> (i32, i32, i32, i32) {
        (10, TOPBAR_H + 8, 464, 348)
    }

    fn on_left_click(&mut self, mx: i32, my: i32) {
        if my < TOPBAR_H {
            if (6..=118).contains(&mx) || mx >= self.canvas.width - 36 {
                self.start_menu_open = !self.start_menu_open;
                self.search_focused = false;
                self.dirty = true;
                return;
            }
            self.start_menu_open = false;
            self.dirty = true;
            return;
        }

        if self.start_menu_open {
            let (sx, sy, sw, sh) = self.start_menu_rect();
            if mx >= sx && mx < sx + sw && my >= sy && my < sy + sh {
                let lx = mx - sx;
                let ly = my - sy;

                if ly >= 72 && ly < 292 && lx >= 10 && lx < sw - 10 {
                    let col = ((lx - 10) / 150).clamp(0, 2);
                    let row = (ly - 72) / 44;
                    let idx = (row * 3 + col) as usize;
                    if let Some(&kind) = AppKind::ALL.get(idx) {
                        self.open_and_focus(kind);
                        self.start_menu_open = false;
                        self.dirty = true;
                        return;
                    }
                }

                if ly >= 302 && ly <= 336 {
                    if lx < sw / 2 - 4 {
                        power::reboot();
                    } else if lx > sw / 2 + 4 {
                        power::shutdown();
                    }
                }
                return;
            }
            self.start_menu_open = false;
            self.dirty = true;
        }

        // Bottom Dock hit check
        if let Some(idx) = self.dock_hit_index(mx, my) {
            let kind = DOCK_APPS[idx];
            if let Some(pos) = self.windows.iter().position(|w| w.kind == kind) {
                let is_top_visible = self.focused_window_kind() == Some(kind);
                if self.windows[pos].open && !self.windows[pos].minimized && is_top_visible {
                    self.windows[pos].minimized = true;
                } else {
                    self.open_and_focus(kind);
                }
            }
            self.start_menu_open = false;
            self.search_focused = false;
            self.dirty = true;
            return;
        }

        // Open windows hit check (top to bottom)
        let hit_idx = self.windows.iter().rposition(|w| w.contains(mx, my));
        if let Some(idx) = hit_idx {
            self.search_focused = false;
            let kind = self.windows[idx].kind;
            self.focus_index(idx);

            let sw = self.canvas.width;
            let sh = self.canvas.height;
            let action = self.windows.last().unwrap().hit_titlebar_action(mx, my);

            match action {
                TitleAction::Close => {
                    if let Some(top) = self.windows.last_mut() {
                        top.open = false;
                        top.minimized = false;
                    }
                    if kind == AppKind::Media {
                        self.media.stop_playback();
                    } else if kind == AppKind::Doom {
                        audio::stop();
                    }
                    self.dirty = true;
                    return;
                }
                TitleAction::Minimize => {
                    if let Some(top) = self.windows.last_mut() {
                        top.minimized = true;
                    }
                    self.dirty = true;
                    return;
                }
                TitleAction::MaximizeToggle => {
                    if let Some(top) = self.windows.last_mut() {
                        top.toggle_maximize(sw, sh);
                    }
                    self.dirty = true;
                    return;
                }
                TitleAction::DragStart => {
                    if let Some(top) = self.windows.last() {
                        if !top.maximized {
                            self.drag = Some(DragState {
                                kind: top.kind,
                                offset_x: mx - top.x,
                                offset_y: my - top.y,
                            });
                        }
                    }
                    self.dirty = true;
                    return;
                }
                TitleAction::None => {}
            }

            let (cx, cy, cw, ch) = self.windows.last().unwrap().client_rect();
            let lx = mx - cx;
            let ly = my - cy;
            let now_ms = timer::ticks_ms();
            match kind {
                AppKind::Hub => {
                    let (changed, hact) = self.hub.handle_click(lx, ly, cw, ch);
                    if changed {
                        self.dirty = true;
                    }
                    match hact {
                        HubAction::OpenDoom => self.open_and_focus(AppKind::Doom),
                        HubAction::OpenMedia => self.open_and_focus(AppKind::Media),
                        HubAction::None => {}
                    }
                }
                AppKind::Files => {
                    let (changed, fact) = self.files.handle_click(lx, ly, cw, ch);
                    if changed {
                        self.dirty = true;
                    }
                    if let FileAction::LaunchBinary(path) = fact {
                        if path.ends_with(".ambv") {
                            self.media.open_video_clip(0);
                            self.open_and_focus(AppKind::Media);
                        } else if path.contains("doom") {
                            self.open_and_focus(AppKind::Doom);
                        } else if path.contains("blocks") {
                            self.open_and_focus(AppKind::Hub);
                            let _ = self.hub.launch_by_id("blocks");
                        } else if path.contains("invaders") {
                            self.open_and_focus(AppKind::Hub);
                            let _ = self.hub.launch_by_id("invaders");
                        } else {
                            self.open_and_focus(AppKind::Hub);
                            let _ = self.hub.launch_by_id("racer3d");
                        }
                    }
                }
                AppKind::Notes => {
                    if self.notes.handle_click(lx, ly) {
                        self.dirty = true;
                    }
                }
                AppKind::Media => {
                    if self.media.handle_click(lx, ly, cw, now_ms) {
                        self.dirty = true;
                    }
                }
                AppKind::Paint => {
                    if self.paint.handle_press(lx, ly, cw, ch) {
                        self.paint_drawing = true;
                        self.dirty = true;
                    }
                }
                AppKind::Calculator => {
                    if self.calculator.handle_click(lx, ly, cw, ch) {
                        self.dirty = true;
                    }
                }
                AppKind::Clock => {
                    if self.clock.handle_click(lx, ly, cw) {
                        self.dirty = true;
                    }
                }
                AppKind::Snake => {
                    if self.snake.handle_click(lx, ly, cw) {
                        self.dirty = true;
                    }
                }
                AppKind::Doom => {
                    if self.doom.handle_click(lx, ly) {
                        self.dirty = true;
                    }
                }
                AppKind::Net => {
                    if self.net_app.handle_click(lx, ly, cw, ch) {
                        self.dirty = true;
                    }
                }
                AppKind::Snapshots => match self.snapshots.handle_click(
                    lx,
                    ly,
                    cw,
                    ch,
                    self.theme_preset.to_index(),
                    self.show_watermark,
                ) {
                    SnapAction::None => {}
                    SnapAction::ConfigRestored(t_idx, wm) => {
                        self.theme_preset = ThemePreset::from_index(t_idx);
                        self.show_watermark = wm;
                        self.rebuild_wallpaper();
                    }
                    SnapAction::RequestReboot => snapshot::request_boot_menu_and_reboot(),
                },
                AppKind::SysMon => {
                    if self.sysmon.handle_click(lx, ly, cw, ch) {
                        self.dirty = true;
                    }
                }
                AppKind::Settings => match self.settings.handle_click(lx, ly, cw) {
                    SettingsAction::SelectTheme(preset) => {
                        self.theme_preset = preset;
                        let _ = snapshot::create_snapshot(
                            &format!("Theme: {}", preset.name()),
                            preset.to_index(),
                            self.show_watermark,
                        );
                        self.rebuild_wallpaper();
                    }
                    SettingsAction::ToggleWatermark => {
                        self.show_watermark = !self.show_watermark;
                        let _ = snapshot::create_snapshot(
                            "Toggle Watermark",
                            self.theme_preset.to_index(),
                            self.show_watermark,
                        );
                        self.rebuild_wallpaper();
                    }
                    SettingsAction::None => {}
                },
                _ => {}
            }
            self.dirty = true;
            return;
        }

        // Spotlight Search Bar hit check
        let (sbx, sby, sbw, sbh) = self.search_bar_rect();
        if mx >= sbx && mx < sbx + sbw && my >= sby && my < sby + sbh {
            self.search_focused = true;
            self.dirty = true;
            return;
        }
        self.search_focused = false;

        // Desktop Top-Right Control Center Widget hit check
        let (wx, wy, ww, wh) = self.control_widget_rect();
        if mx >= wx && mx < wx + ww && my >= wy && my < wy + wh {
            let lx = mx - wx;
            let ly = my - wy;
            // Quick Action Tiles row (ly: 112..188)
            if (112..=188).contains(&ly) {
                if (16..=142).contains(&lx) {
                    self.hub.open_install_wizard(2);
                    self.open_and_focus(AppKind::Hub);
                    self.dirty = true;
                    return;
                }
                if (150..=272).contains(&lx) {
                    self.open_and_focus(AppKind::Snapshots);
                    self.dirty = true;
                    return;
                }
                if (280..=406).contains(&lx) {
                    self.open_and_focus(AppKind::Media);
                    self.dirty = true;
                    return;
                }
            }
            // Boost Mode toggle switch (lx >= 330, ly >= 230)
            if lx >= 320 && ly >= 220 {
                self.boost_mode = !self.boost_mode;
                if self.boost_mode {
                    let _ = compute::schedule_npu_dag();
                }
                self.dirty = true;
                return;
            }
        }
    }

    fn focus_index(&mut self, idx: usize) {
        if idx + 1 < self.windows.len() {
            let win = self.windows.remove(idx);
            self.windows.push(win);
        }
    }

    fn open_and_focus(&mut self, kind: AppKind) {
        if let Some(idx) = self.windows.iter().position(|w| w.kind == kind) {
            self.windows[idx].open = true;
            self.windows[idx].minimized = false;
            self.focus_index(idx);
        }
    }

    #[inline]
    fn mark_app_dirty(&mut self, kind: AppKind) {
        if !self.start_menu_open && self.focused_window_kind() == Some(kind) {
            self.focused_dirty = true;
        } else {
            self.dirty = true;
        }
    }

    pub fn tick_and_render_if_needed(&mut self) {
        let now_ms = timer::ticks_ms();

        if net::poll(now_ms) {
            self.net_app.on_net_update();
            let net_vis = self.windows.iter().any(|w| w.kind == AppKind::Net && w.is_visible());
            if net_vis {
                self.mark_app_dirty(AppKind::Net);
            } else {
                self.dirty = true;
            }
        }
        let msgs = net::take_cli_messages();
        if self.terminal.push_external_lines(msgs) {
            self.mark_app_dirty(AppKind::Terminal);
        }

        let sec = now_ms / (timer::TARGET_HZ as u64);
        if sec != self.last_sec {
            self.last_sec = sec;
            self.sysmon.sample_second();
            let cur_min = rtc::read_rtc().minute;
            if cur_min != self.last_minute {
                self.last_minute = cur_min;
                self.dirty = true;
            } else if self.windows.iter().any(|w| w.kind == AppKind::SysMon && w.is_visible()) {
                self.mark_app_dirty(AppKind::SysMon);
            } else if self.windows.iter().any(|w| w.kind == AppKind::Clock && w.is_visible()) {
                self.mark_app_dirty(AppKind::Clock);
            }
        }
        let hub_vis = self.windows.iter().any(|w| w.kind == AppKind::Hub && w.is_visible());
        if hub_vis && self.hub.tick(now_ms) {
            self.mark_app_dirty(AppKind::Hub);
        }
        let media_vis = self.windows.iter().any(|w| w.kind == AppKind::Media && w.is_visible());
        if (media_vis || self.media_is_playing_audio()) && self.media.tick(now_ms) {
            self.mark_app_dirty(AppKind::Media);
        }
        if self.clock.is_running() && now_ms.saturating_sub(self.last_clock_anim_ms) >= 50 {
            self.last_clock_anim_ms = now_ms;
            self.mark_app_dirty(AppKind::Clock);
        }
        let snake_vis = self.windows.iter().any(|w| w.kind == AppKind::Snake && w.is_visible());
        if snake_vis && self.snake.tick(now_ms) {
            self.mark_app_dirty(AppKind::Snake);
        }
        let doom_vis = self.windows.iter().any(|w| w.kind == AppKind::Doom && w.is_visible());
        let doom_focused = self.focused_window_kind() == Some(AppKind::Doom);
        if doom_vis && self.doom.tick(now_ms, doom_focused) {
            self.mark_app_dirty(AppKind::Doom);
        }
        let net_vis = self.windows.iter().any(|w| w.kind == AppKind::Net && w.is_visible());
        if net_vis && self.net_app.tick(now_ms) {
            self.mark_app_dirty(AppKind::Net);
        }

        if self.dirty {
            self.dirty = false;
            self.focused_dirty = false;
            self.cursor_dirty = false;
            self.render_frame();
            self.fb.blit_backbuffer(&self.canvas.buffer);
        } else if self.focused_dirty {
            self.focused_dirty = false;
            self.render_focused_window_only();
        } else if self.cursor_dirty {
            self.cursor_dirty = false;
            let ox = self.prev_cursor_x;
            let oy = self.prev_cursor_y;
            self.restore_cursor_bg(ox, oy, 16, 22);
            let nx = self.last_mouse_x;
            let ny = self.last_mouse_y;
            self.canvas.draw_cursor(nx, ny, 0xF8FAFC);
            self.fb.blit_rect(&self.canvas.buffer, ox, oy, 16, 22);
            self.fb.blit_rect(&self.canvas.buffer, nx, ny, 16, 22);
            self.prev_cursor_x = nx;
            self.prev_cursor_y = ny;
        }
    }

    fn copy_rect_to_scene_cache(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let cw = self.canvas.width;
        let ch = self.canvas.height;
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(cw);
        let y1 = (y + h).min(ch);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let stride = cw as usize;
        for py in y0..y1 {
            let start = (py as usize) * stride + (x0 as usize);
            let end = (py as usize) * stride + (x1 as usize);
            self.scene_cache[start..end].copy_from_slice(&self.canvas.buffer[start..end]);
        }
    }

    fn render_window_client(
        &mut self,
        kind: AppKind,
        theme: ThemeColors,
        cx: i32,
        cy: i32,
        cw: i32,
        ch: i32,
        focused: bool,
    ) {
        match kind {
            AppKind::Hub => self.hub.render(&mut self.canvas, theme, cx, cy, cw, ch),
            AppKind::Terminal => self.terminal.render(&mut self.canvas, theme, cx, cy, cw, ch, focused && !self.search_focused),
            AppKind::Files => self.files.render(&mut self.canvas, theme, cx, cy, cw, ch),
            AppKind::Notes => self.notes.render(&mut self.canvas, theme, cx, cy, cw, ch, focused),
            AppKind::Media => self.media.render(&mut self.canvas, theme, cx, cy, cw, ch),
            AppKind::Paint => self.paint.render(&mut self.canvas, theme, cx, cy, cw, ch),
            AppKind::Calculator => self.calculator.render(&mut self.canvas, theme, cx, cy, cw, ch),
            AppKind::Clock => self.clock.render(&mut self.canvas, theme, cx, cy, cw, ch),
            AppKind::Snake => self.snake.render(&mut self.canvas, theme, cx, cy, cw, ch),
            AppKind::Doom => self.doom.render(&mut self.canvas, theme, cx, cy, cw, ch),
            AppKind::Net => self.net_app.render(&mut self.canvas, theme, cx, cy, cw, ch),
            AppKind::Snapshots => self.snapshots.render(&mut self.canvas, theme, cx, cy, cw, ch),
            AppKind::SysMon => self.sysmon.render(&mut self.canvas, theme, cx, cy, cw, ch),
            AppKind::Settings => self.settings.render(
                &mut self.canvas,
                theme,
                self.theme_preset,
                self.show_watermark,
                cx,
                cy,
                cw,
                ch,
            ),
            AppKind::About => self.about.render(&mut self.canvas, theme, cx, cy, cw, ch),
        }
    }

    fn render_focused_window_only(&mut self) {
        let Some((kind, cx, cy, cw, ch)) = self
            .windows
            .iter()
            .rev()
            .find(|w| w.is_visible())
            .map(|w| {
                let (rx, ry, rw, rh) = w.client_rect();
                (w.kind, rx, ry, rw, rh)
            })
        else {
            return;
        };
        let theme = self.theme();
        let ox = self.prev_cursor_x;
        let oy = self.prev_cursor_y;
        self.restore_cursor_bg(ox, oy, 16, 22);

        self.render_window_client(kind, theme, cx, cy, cw, ch, true);
        self.copy_rect_to_scene_cache(cx, cy, cw, ch);

        let nx = self.last_mouse_x;
        let ny = self.last_mouse_y;
        self.canvas.draw_cursor(nx, ny, 0xF8FAFC);

        if ox != nx || oy != ny {
            self.fb.blit_rect(&self.canvas.buffer, ox, oy, 16, 22);
        }
        self.fb.blit_rect(&self.canvas.buffer, cx, cy, cw, ch);
        self.fb.blit_rect(&self.canvas.buffer, nx, ny, 16, 22);
        self.prev_cursor_x = nx;
        self.prev_cursor_y = ny;
        self.cursor_dirty = false;
    }

    fn restore_cursor_bg(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let cw = self.canvas.width;
        let ch = self.canvas.height;
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(cw);
        let y1 = (y + h).min(ch);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let stride = cw as usize;
        for py in y0..y1 {
            let start = (py as usize) * stride + (x0 as usize);
            let end = (py as usize) * stride + (x1 as usize);
            self.canvas.buffer[start..end].copy_from_slice(&self.scene_cache[start..end]);
        }
    }

    fn media_is_playing_audio(&self) -> bool {
        false
    }

    fn render_frame(&mut self) {
        let theme = self.theme();
        self.canvas.buffer.copy_from_slice(&self.wallpaper_cache);

        self.draw_spotlight_search(theme);
        self.draw_control_widget(theme);

        let focused_kind = self.focused_window_kind();
        let vis_windows: Vec<(AppKind, i32, i32, i32, i32, bool)> = self
            .windows
            .iter()
            .filter(|w| w.is_visible())
            .map(|w| {
                let focused = Some(w.kind) == focused_kind;
                w.draw_chrome(&mut self.canvas, theme, focused);
                let (cx, cy, cw, ch) = w.client_rect();
                (w.kind, cx, cy, cw, ch, focused)
            })
            .collect();

        for (kind, cx, cy, cw, ch, focused) in vis_windows {
            self.render_window_client(kind, theme, cx, cy, cw, ch, focused);
        }

        self.draw_topbar(theme);
        self.draw_dock(theme, focused_kind);

        if self.start_menu_open {
            self.draw_start_menu(theme);
        }

        self.scene_cache.copy_from_slice(&self.canvas.buffer);
        self.prev_cursor_x = self.last_mouse_x;
        self.prev_cursor_y = self.last_mouse_y;
        self.canvas
            .draw_cursor(self.last_mouse_x, self.last_mouse_y, 0xF8FAFC);
    }

    fn draw_spotlight_search(&mut self, theme: ThemeColors) {
        let (x, y, w, h) = self.search_bar_rect();
        self.canvas.draw_soft_shadow(x, y, w, h, 18);
        self.canvas.fill_rounded_rect_alpha(x, y, w, h, 18, 0x1D222B, 238);
        self.canvas.draw_rounded_rect_outline(
            x,
            y,
            w,
            h,
            18,
            if self.search_focused { theme.accent } else { 0x9E5B30 },
        );

        // Magnifying glass icon
        let mx = x + 22;
        let my = y + 16;
        self.canvas.draw_rounded_rect_outline(mx - 5, my - 5, 11, 11, 5, theme.text_muted);
        self.canvas.draw_line(mx + 4, my + 4, mx + 8, my + 8, theme.text_muted);

        if self.search_input.is_empty() && !self.search_focused {
            self.canvas.draw_text_clipped_font(
                x + 38,
                y + 9,
                w - 52,
                "Search apps, run .exe / .elf, or restore snapshot... (Ctrl+Space)",
                0x8A94A6,
                FontFace::Ui,
            );
        } else {
            self.canvas.draw_text_clipped_font(
                x + 38,
                y + 9,
                w - 56,
                &self.search_input,
                theme.text_primary,
                FontFace::Ui,
            );
            if self.search_focused {
                let cw = font::measure_text(FontFace::Ui, &self.search_input);
                self.canvas.fill_rect(x + 40 + cw, y + 10, 2, 16, theme.accent);
            }
        }
    }

    fn draw_control_widget(&mut self, theme: ThemeColors) {
        let (wx, wy, ww, wh) = self.control_widget_rect();
        self.canvas.draw_soft_shadow(wx, wy, ww, wh, 12);
        self.canvas.fill_rounded_rect_alpha(wx, wy, ww, wh, 12, 0x1B2029, 238);
        self.canvas.draw_rounded_rect_outline(wx, wy, ww, wh, 12, theme.border_active);

        // Top Emblem (pre-scaled once) + Version + SIP Sandbox Badge
        self.canvas
            .draw_rgba_slice(wx + 24, wy + 18, 76, 76, &self.widget_logo_rgba, 255);
        self.canvas.draw_text_font(wx + 124, wy + 26, "AmbaOS v0.7.0", theme.text_primary, FontFace::UiBold);
        self.canvas.fill_rounded_rect(wx + 124, wy + 54, 152, 26, 13, 0x1E4D36);
        self.canvas.draw_rounded_rect_outline(wx + 124, wy + 54, 152, 26, 13, 0x2E7D54);
        self.canvas.draw_text_font(wx + 136, wy + 59, "SIP Sandbox Active", 0xA7F3D0, FontFace::UiBold);

        // 3 Quick-Action Tiles
        let tiles = [
            ("Install", ".exe / .elf", 0u8),
            ("AmbaSnap", "Rollback", 1u8),
            ("Media", "Player", 2u8),
        ];
        for (i, (l1, l2, glyph)) in tiles.iter().enumerate() {
            let tx = wx + 18 + (i as i32) * 131;
            let ty = wy + 112;
            let tw = 122;
            let th = 76;
            self.canvas.fill_rounded_rect(tx, ty, tw, th, 9, theme.window_surface);
            self.canvas.draw_rounded_rect_outline(tx, ty, tw, th, 9, theme.border_inactive);

            let gx = tx + tw / 2;
            let gy = ty + 18;
            match glyph {
                0 => {
                    // Gamepad glyph
                    self.canvas.fill_rounded_rect(gx - 12, gy - 6, 24, 14, 5, theme.accent);
                    self.canvas.fill_rect(gx - 8, gy - 1, 5, 2, 0x1B2029);
                    self.canvas.fill_circle(gx + 6, gy - 1, 2, 0x1B2029);
                }
                1 => {
                    // Snapshot disk glyph
                    self.canvas.fill_rounded_rect(gx - 9, gy - 8, 18, 18, 3, theme.accent);
                    self.canvas.fill_rect(gx - 5, gy - 7, 10, 6, 0xFDE68A);
                    self.canvas.fill_circle(gx, gy + 4, 3, 0x1B2029);
                }
                _ => {
                    // Media Play circle glyph
                    self.canvas.fill_circle(gx, gy, 10, theme.accent);
                    self.canvas.draw_text_font(gx - 4, gy - 8, ">", 0x1B2029, FontFace::UiBold);
                }
            }

            let w1 = font::measure_text(FontFace::UiBold, l1);
            let w2 = font::measure_text(FontFace::Ui, l2);
            self.canvas.draw_text_font(tx + (tw - w1) / 2, ty + 34, l1, theme.text_primary, FontFace::UiBold);
            self.canvas.draw_text_font(tx + (tw - w2) / 2, ty + 52, l2, theme.text_secondary, FontFace::Ui);
        }

        // Bottom CPU & Memory Telemetry + Boost Mode Toggle
        let cpu_used = if self.boost_mode { 42 } else { 18 };
        self.canvas.draw_text_font(wx + 20, wy + 206, "CPU", theme.text_secondary, FontFace::Ui);
        self.canvas.draw_progress_bar(wx + 94, wy + 211, 200, 8, cpu_used, 100, 0x12161E, 0x2A3242, theme.accent);
        self.canvas.draw_text_font(wx + 318, wy + 206, "Boost Mode", theme.text_primary, FontFace::Ui);

        let m = pmm::stats();
        let gb10 = (m.total_bytes * 10 + 512 * 1024 * 1024) / (1024 * 1024 * 1024);
        self.canvas.draw_text_font(
            wx + 94,
            wy + 230,
            &format!("{}.{} GB of 16 GB", gb10 / 10, gb10 % 10),
            theme.text_primary,
            FontFace::Ui,
        );
        self.canvas.draw_text_font(wx + 20, wy + 248, "Memory", theme.text_secondary, FontFace::Ui);
        self.canvas.draw_progress_bar(wx + 94, wy + 253, 200, 8, 20, 100, 0x12161E, 0x2A3242, theme.accent);

        // Toggle switch pill on bottom right
        let sx = wx + 338;
        let sy = wy + 240;
        self.canvas.fill_rounded_rect(sx, sy, 44, 24, 12, if self.boost_mode { theme.accent } else { 0x4B5563 });
        self.canvas.draw_rounded_rect_outline(sx, sy, 44, 24, 12, theme.border_inactive);
        let kx = if self.boost_mode { sx + 32 } else { sx + 12 };
        self.canvas.fill_circle(kx, sy + 12, 8, 0xE2E8F0);
    }

    fn draw_topbar(&mut self, theme: ThemeColors) {
        let w = self.canvas.width;
        self.canvas.fill_rect_alpha(0, 0, w, TOPBAR_H, theme.topbar_bg, 236);
        self.canvas.fill_rect(0, TOPBAR_H - 1, w, 1, theme.border_inactive);

        self.canvas.draw_rgba_sprite(12, 3, assets::logo_22(), 255);
        self.canvas.draw_text_font(40, 5, "AmbaOS", 0xFFFFFF, FontFace::UiBold);

        let dt = rtc::read_rtc();
        let clock = format!(
            "{:04}-{:02}-{:02}  {:02}:{:02}",
            dt.year, dt.month, dt.day, dt.hour, dt.minute
        );
        let cw = font::measure_text(FontFace::UiBold, &clock);
        self.canvas.draw_text_font((w - cw) / 2, 5, &clock, theme.text_primary, FontFace::UiBold);

        let snap = net::snapshot();
        let m = pmm::stats();
        let gb10 = (m.total_bytes * 10 + 512 * 1024 * 1024) / (1024 * 1024 * 1024);
        let ram_txt = format!("{}.{} GB RAM", gb10 / 10, gb10 % 10);
        let rw = font::measure_text(FontFace::Ui, &ram_txt) + 18;
        let rx = w - rw - 92;
        self.canvas.fill_rounded_rect(rx, 3, rw, 22, 5, 0x1F2530);
        self.canvas.draw_rounded_rect_outline(rx, 3, rw, 22, 5, theme.border_inactive);
        self.canvas.draw_text_font(rx + 9, 5, &ram_txt, theme.text_secondary, FontFace::Ui);

        let (net_txt, net_col) = if let Some(ip) = snap.ip {
            (format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]), theme.text_primary)
        } else if snap.present {
            (String::from("10.0.2.15"), theme.accent_soft)
        } else {
            (String::from("OFFLINE"), theme.err)
        };
        let nw = font::measure_text(FontFace::UiBold, &net_txt) + 18;
        let nx = rx - nw - 6;
        self.canvas.fill_rounded_rect(nx, 3, nw, 22, 5, 0x1F2530);
        self.canvas.draw_rounded_rect_outline(nx, 3, nw, 22, 5, theme.border_inactive);
        self.canvas.draw_text_font(nx + 9, 5, &net_txt, net_col, FontFace::UiBold);

        // Right-side tray icons: Signal, Volume, Power
        let bar_col = if snap.dhcp_ready { theme.text_primary } else { theme.text_muted };
        let sx = w - 80;
        self.canvas.fill_rect(sx, 16, 3, 4, bar_col);
        self.canvas.fill_rect(sx + 5, 12, 3, 8, bar_col);
        self.canvas.fill_rect(sx + 10, 8, 3, 12, bar_col);

        let vx = w - 54;
        self.canvas.fill_rect(vx, 11, 5, 7, theme.text_secondary);
        self.canvas.fill_rect(vx + 5, 8, 4, 13, theme.text_secondary);

        self.canvas.fill_circle(w - 22, 14, 9, 0x1F2530);
        self.canvas.draw_rounded_rect_outline(w - 31, 5, 18, 18, 9, theme.border_inactive);
        self.canvas.fill_rect(w - 23, 8, 2, 7, theme.accent_soft);
    }

    fn draw_dock(&mut self, theme: ThemeColors, focused_kind: Option<AppKind>) {
        let (dx, dy, dw, dh) = self.dock_rect();
        self.canvas.draw_soft_shadow(dx, dy, dw, dh, 22);
        self.canvas.fill_rounded_rect_alpha(dx, dy, dw, dh, 22, theme.dock_bg, 224);
        self.canvas.draw_rounded_rect_outline(dx, dy, dw, dh, 22, 0x454F60);

        let iy = dy + 10;

        for (i, &kind) in DOCK_APPS.iter().enumerate() {
            let ix = dx + 20 + (i as i32) * (DOCK_ICON_SIZE + DOCK_GAP);
            let hovered = self.hover_dock == Some(i);
            let is_focused = Some(kind) == focused_kind;
            let is_open = self.windows.iter().any(|w| w.kind == kind && w.open);

            if is_focused || hovered {
                self.canvas.fill_rounded_rect_alpha(ix - 5, iy - 4, DOCK_ICON_SIZE + 10, DOCK_ICON_SIZE + 20, 14, theme.accent, 28);
                self.canvas.draw_rounded_rect_outline(ix - 5, iy - 4, DOCK_ICON_SIZE + 10, DOCK_ICON_SIZE + 20, 14, 0xB8632E);
            }

            match kind {
                AppKind::Files => {
                    self.canvas.fill_rounded_rect(ix + 3, iy + 8, 24, 12, 4, 0xD97706);
                    self.canvas.fill_rounded_rect(ix + 3, iy + 14, 48, 34, 7, 0xF59E0B);
                    self.canvas.fill_rect(ix + 5, iy + 18, 44, 3, 0xFBBF24);
                }
                AppKind::Terminal => {
                    self.canvas.fill_rounded_rect(ix, iy, DOCK_ICON_SIZE, DOCK_ICON_SIZE, 13, 0x11161F);
                    self.canvas.draw_rounded_rect_outline(ix, iy, DOCK_ICON_SIZE, DOCK_ICON_SIZE, 13, 0x475569);
                    self.canvas.draw_text_font(ix + 13, iy + 17, ">_", 0x4ADE80, FontFace::UiBold);
                }
                AppKind::Hub => {
                    self.canvas.fill_rounded_rect(ix, iy, DOCK_ICON_SIZE, DOCK_ICON_SIZE, 13, 0xF97316);
                    self.canvas.draw_rounded_rect_outline(ix, iy, DOCK_ICON_SIZE, DOCK_ICON_SIZE, 13, 0xFDBA74);
                    // Stylized 'A' Store glyph
                    self.canvas.draw_thick_line(ix + 27, iy + 13, ix + 15, iy + 40, 2, 0xFFFFFF);
                    self.canvas.draw_thick_line(ix + 27, iy + 13, ix + 39, iy + 40, 2, 0xFFFFFF);
                    self.canvas.draw_thick_line(ix + 14, iy + 33, ix + 40, iy + 33, 2, 0xFFFFFF);
                }
                AppKind::Media => {
                    self.canvas.fill_rounded_rect(ix, iy, DOCK_ICON_SIZE, DOCK_ICON_SIZE, 13, 0xEA580C);
                    self.canvas.fill_rounded_rect(ix + 11, iy + 13, 32, 26, 5, 0xFFFFFF);
                    self.canvas.fill_circle(ix + 27, iy + 26, 8, 0xEA580C);
                    self.canvas.draw_text_font(ix + 24, iy + 18, ">", 0xFFFFFF, FontFace::UiBold);
                }
                AppKind::Net => {
                    self.canvas.fill_rounded_rect(ix, iy, DOCK_ICON_SIZE, DOCK_ICON_SIZE, 13, 0xE2E8F0);
                    self.canvas.fill_circle(ix + 27, iy + 27, 17, 0x3B82F6);
                    self.canvas.draw_rounded_rect_outline(ix + 19, iy + 10, 16, 34, 8, 0x93C5FD);
                    self.canvas.draw_line(ix + 11, iy + 27, ix + 43, iy + 27, 0xBFDBFE);
                    self.canvas.draw_line(ix + 27, iy + 10, ix + 27, iy + 44, 0xBFDBFE);
                }
                AppKind::Settings => {
                    self.canvas.fill_rounded_rect(ix, iy, DOCK_ICON_SIZE, DOCK_ICON_SIZE, 13, 0xE2E8F0);
                    self.canvas.fill_circle(ix + 27, iy + 27, 15, 0x475569);
                    self.canvas.fill_circle(ix + 27, iy + 27, 8, 0xCBD5E1);
                    self.canvas.fill_circle(ix + 27, iy + 27, 4, 0x1E293B);
                }
                _ => {}
            }

            let lbl = kind.label();
            let lw = font::measure_text(FontFace::Ui, lbl);
            self.canvas.draw_text_font(
                ix + (DOCK_ICON_SIZE - lw) / 2,
                dy + 64,
                lbl,
                if is_focused { 0xFFFFFF } else { theme.text_secondary },
                FontFace::Ui,
            );

            if is_open {
                self.canvas.fill_circle(ix + DOCK_ICON_SIZE / 2, dy + 83, 2, if is_focused { theme.accent } else { theme.text_muted });
            }
        }
    }

    fn draw_start_menu(&mut self, theme: ThemeColors) {
        let (sx, sy, sw, sh) = self.start_menu_rect();
        self.canvas.draw_soft_shadow(sx, sy, sw, sh, 12);
        self.canvas.fill_rounded_rect(sx, sy, sw, sh, 12, theme.window_bg);
        self.canvas.draw_rounded_rect_outline(sx, sy, sw, sh, 12, theme.accent);

        self.canvas.fill_rounded_rect(sx + 8, sy + 8, sw - 16, 56, 8, theme.window_surface);
        self.canvas.draw_rgba_sprite(sx + 14, sy + 12, assets::logo_48(), 255);
        self.canvas.draw_text_font(sx + 70, sy + 16, "AmbaOS v0.7.0", theme.text_primary, FontFace::UiBold);
        self.canvas.draw_text_font(sx + 70, sy + 36, "15 Apps | PE32+/ELF64 | SIP + AmbaSnap", theme.accent_soft, FontFace::Ui);

        for (i, &kind) in AppKind::ALL.iter().enumerate() {
            let col = (i % 3) as i32;
            let row = (i / 3) as i32;
            let bx = sx + 10 + col * 150;
            let by = sy + 72 + row * 44;
            self.canvas.fill_rounded_rect(bx, by, 144, 38, 8, theme.window_surface);
            self.canvas.draw_rounded_rect_outline(bx, by, 144, 38, 8, theme.border_inactive);
            self.canvas.draw_text_clipped_font(bx + 10, by + 3, 128, kind.label(), theme.text_primary, FontFace::UiBold);
            self.canvas.draw_text_clipped_font(bx + 10, by + 19, 128, kind.subtitle(), theme.text_muted, FontFace::Ui);
        }

        self.canvas.fill_rect(sx + 10, sy + 294, sw - 20, 1, theme.border_inactive);
        self.canvas.fill_rounded_rect(sx + 10, sy + 302, 217, 34, 8, theme.titlebar_active);
        self.canvas.draw_text_font(sx + 92, sy + 310, "Reboot", theme.text_primary, FontFace::UiBold);

        self.canvas.fill_rounded_rect(sx + 237, sy + 302, 217, 34, 8, theme.accent);
        self.canvas.draw_text_font(sx + 308, sy + 310, "Shut Down", 0xFFFFFF, FontFace::UiBold);
    }
}
