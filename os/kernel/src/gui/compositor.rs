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
use crate::gui::apps::files::FileViewerApp;
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

const DOCK_H: i32 = 54;
const DOCK_MARGIN_B: i32 = 10;
const DOCK_ICON_SIZE: i32 = 38;
const DOCK_GAP: i32 = 6;

struct DragState {
    kind: AppKind,
    offset_x: i32,
    offset_y: i32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DockItem {
    Launcher,
    App(AppKind),
}

impl DockItem {
    fn label(self) -> &'static str {
        match self {
            DockItem::Launcher => "Start Menu",
            DockItem::App(k) => k.label(),
        }
    }
}

const DOCK_ITEMS: [DockItem; 15] = [
    DockItem::Launcher,
    DockItem::App(AppKind::Terminal),
    DockItem::App(AppKind::Files),
    DockItem::App(AppKind::Notes),
    DockItem::App(AppKind::Media),
    DockItem::App(AppKind::Paint),
    DockItem::App(AppKind::Calculator),
    DockItem::App(AppKind::Clock),
    DockItem::App(AppKind::Snake),
    DockItem::App(AppKind::Doom),
    DockItem::App(AppKind::Net),
    DockItem::App(AppKind::Snapshots),
    DockItem::App(AppKind::SysMon),
    DockItem::App(AppKind::Settings),
    DockItem::App(AppKind::About),
];

pub struct DesktopCompositor {
    fb: FramebufferInfo,
    canvas: Canvas,
    wallpaper_cache: Vec<u32>,
    theme_preset: ThemePreset,
    show_watermark: bool,
    windows: Vec<Window>,
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
    hover_dock: Option<usize>,
    drag: Option<DragState>,
    paint_drawing: bool,
    prev_left: bool,
    last_mouse_x: i32,
    last_mouse_y: i32,
    last_sec: u64,
    last_clock_anim_ms: u64,
    dirty: bool,
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

        let right_w = 436.min((w / 2) - 60);
        let right_x = w - right_w - 28;
        let term_w = 516.min(w - right_w - 220);

        let windows = vec![
            Window::new(AppKind::Files, "File Manager (Core RO / User RW)", 160, 88, 660, 420, false),
            Window::new(AppKind::Notes, "AmbaNotes", 220, 120, 520, 380, false),
            Window::new(AppKind::Media, "AmbaMedia - Player & Gallery", 486, 56, 508, 422, false),
            Window::new(AppKind::Paint, "AmbaPaint - Studio", 260, 110, 520, 392, false),
            Window::new(AppKind::Calculator, "Calculator", 460, 120, 296, 400, false),
            Window::new(AppKind::Clock, "AmbaClock - RTC & Stopwatch", 748, 406, 490, 300, false),
            Window::new(AppKind::Snake, "AmbaSnake", 400, 140, 440, 360, false),
            Window::new(AppKind::Doom, "AmbaDoom - E1M1 Hangar", 310, 68, 580, 386, false),
            Window::new(AppKind::Net, "AmbaBrowser - Internet Web Browser", 210, 48, 860, 620, false),
            Window::new(AppKind::Snapshots, "AmbaSnap - Immutable OS & Atomic Rollback", 250, 74, 700, 480, false),
            Window::new(AppKind::Settings, "Settings & Appearance", 380, 140, 440, 376, false),
            Window::new(AppKind::SysMon, "System Monitor", right_x, 386, right_w, 330, true),
            Window::new(AppKind::About, "About AmbaOS", right_x, 48, right_w, 326, true),
            Window::new(AppKind::Terminal, "AmbaTerm - ash Shell", 28, 48, term_w, 438, true),
        ];

        let mut sysmon = SysMonApp::new(fb.width, fb.height);
        sysmon.sample_second();

        Self {
            fb,
            canvas,
            wallpaper_cache,
            theme_preset,
            show_watermark,
            windows,
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
            hover_dock: None,
            drag: None,
            paint_drawing: false,
            prev_left: false,
            last_mouse_x: w / 2,
            last_mouse_y: h / 2,
            last_sec: u64::MAX,
            last_clock_anim_ms: 0,
            dirty: true,
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

    pub fn handle_key(&mut self, ev: KeyEvent) {
        let target = match self.focused_window_kind() {
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
                    self.dirty = true;
                }
            }
            AppKind::Calculator => {
                if self.calculator.handle_key(ev) {
                    self.dirty = true;
                }
            }
            AppKind::Snake => {
                if self.snake.handle_key(ev) {
                    self.dirty = true;
                }
            }
            AppKind::Doom => {
                if self.doom.handle_key(ev) {
                    self.dirty = true;
                }
            }
            AppKind::Net => {
                if self.net_app.handle_key(ev) {
                    self.dirty = true;
                }
            }
            AppKind::Terminal => {
                let (changed, action) = self.terminal.handle_key(ev);
                if changed {
                    self.dirty = true;
                }
                if let TermAction::OpenWindow(kind) = action {
                    self.open_and_focus(kind);
                    self.dirty = true;
                } else if let TermAction::BrowseUrl(target) = action {
                    self.open_and_focus(AppKind::Net);
                    self.net_app.sync_url_from_term(&target);
                    self.dirty = true;
                } else if let TermAction::RefreshTheme = action {
                    let (t_idx, wm) = snapshot::active_config();
                    self.theme_preset = ThemePreset::from_index(t_idx);
                    self.show_watermark = wm;
                    self.rebuild_wallpaper();
                }
            }
            _ => {}
        }
    }

    fn dock_rect(&self) -> (i32, i32, i32, i32) {
        let count = DOCK_ITEMS.len() as i32;
        let dw = count * DOCK_ICON_SIZE + (count - 1) * DOCK_GAP + 24;
        let dx = (self.canvas.width - dw) / 2;
        let dy = self.canvas.height - DOCK_H - DOCK_MARGIN_B;
        (dx, dy, dw, DOCK_H)
    }

    fn dock_hit_index(&self, mx: i32, my: i32) -> Option<usize> {
        let (dx, dy, dw, dh) = self.dock_rect();
        if mx < dx || mx >= dx + dw || my < dy || my >= dy + dh {
            return None;
        }
        let iy = dy + (DOCK_H - DOCK_ICON_SIZE) / 2 - 2;
        if my < iy || my >= iy + DOCK_ICON_SIZE {
            return None;
        }
        for i in 0..DOCK_ITEMS.len() {
            let ix = dx + 12 + (i as i32) * (DOCK_ICON_SIZE + DOCK_GAP);
            if mx >= ix && mx < ix + DOCK_ICON_SIZE {
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
            }
            self.dirty = true;
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

        if let Some(idx) = self.dock_hit_index(mx, my) {
            match DOCK_ITEMS[idx] {
                DockItem::Launcher => {
                    self.start_menu_open = !self.start_menu_open;
                }
                DockItem::App(kind) => {
                    if let Some(pos) = self.windows.iter().position(|w| w.kind == kind) {
                        let is_top_visible = self.focused_window_kind() == Some(kind);
                        if self.windows[pos].open && !self.windows[pos].minimized && is_top_visible {
                            self.windows[pos].minimized = true;
                        } else {
                            self.open_and_focus(kind);
                        }
                    }
                    self.start_menu_open = false;
                }
            }
            self.dirty = true;
            return;
        }

        let hit_idx = self.windows.iter().rposition(|w| w.contains(mx, my));
        if let Some(idx) = hit_idx {
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
                AppKind::Files => {
                    if self.files.handle_click(lx, ly, cw) {
                        self.dirty = true;
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

    pub fn tick_and_render_if_needed(&mut self) {
        let now_ms = timer::ticks_ms();

        if net::poll(now_ms) {
            self.net_app.on_net_update();
            self.dirty = true;
        }
        let msgs = net::take_cli_messages();
        if self.terminal.push_external_lines(msgs) {
            self.dirty = true;
        }

        let sec = now_ms / (timer::TARGET_HZ as u64);
        if sec != self.last_sec {
            self.last_sec = sec;
            self.sysmon.sample_second();
            self.dirty = true;
        }
        if self.media.tick(now_ms) {
            self.dirty = true;
        }
        if self.clock.is_running() && now_ms.saturating_sub(self.last_clock_anim_ms) >= 50 {
            self.last_clock_anim_ms = now_ms;
            self.dirty = true;
        }
        let snake_vis = self.windows.iter().any(|w| w.kind == AppKind::Snake && w.is_visible());
        if snake_vis && self.snake.tick(now_ms) {
            self.dirty = true;
        }
        let doom_vis = self.windows.iter().any(|w| w.kind == AppKind::Doom && w.is_visible());
        let doom_focused = self.focused_window_kind() == Some(AppKind::Doom);
        if doom_vis && self.doom.tick(now_ms, doom_focused) {
            self.dirty = true;
        }

        if !self.dirty {
            return;
        }
        self.dirty = false;
        self.render_frame();
        self.fb.blit_backbuffer(&self.canvas.buffer);
    }

    fn render_frame(&mut self) {
        let theme = self.theme();
        self.canvas.buffer.copy_from_slice(&self.wallpaper_cache);

        let focused_kind = self.focused_window_kind();
        for win in &self.windows {
            if !win.is_visible() {
                continue;
            }
            let focused = Some(win.kind) == focused_kind;
            win.draw_chrome(&mut self.canvas, theme, focused);
            let (cx, cy, cw, ch) = win.client_rect();
            match win.kind {
                AppKind::Terminal => self.terminal.render(&mut self.canvas, theme, cx, cy, cw, ch, focused),
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

        self.draw_topbar(theme, focused_kind);
        self.draw_dock(theme, focused_kind);

        if self.start_menu_open {
            self.draw_start_menu(theme);
        }

        self.canvas
            .draw_cursor(self.last_mouse_x, self.last_mouse_y, theme.accent);
    }

    fn draw_topbar(&mut self, theme: ThemeColors, focused_kind: Option<AppKind>) {
        let w = self.canvas.width;
        self.canvas.fill_rect_alpha(0, 0, w, TOPBAR_H, theme.topbar_bg, 232);
        self.canvas.fill_rect(0, TOPBAR_H - 1, w, 1, theme.border_inactive);

        let pill_bg = if self.start_menu_open {
            theme.accent
        } else {
            theme.window_surface
        };
        self.canvas.fill_rounded_rect(6, 3, 112, 26, 7, pill_bg);
        self.canvas.draw_rgba_sprite(10, 5, assets::logo_22(), 255);
        self.canvas.draw_text_font(36, 7, "AmbaOS", 0xFFFFFF, FontFace::UiBold);

        if let Some(kind) = focused_kind {
            self.canvas.draw_text_font(
                130,
                7,
                kind.label(),
                theme.text_secondary,
                FontFace::UiBold,
            );
        }

        let dt = rtc::read_rtc();
        let clock = format!(
            "{:04}-{:02}-{:02}  {:02}:{:02}:{:02}",
            dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second
        );
        let cw = font::measure_text(FontFace::UiBold, &clock);
        self.canvas.draw_text_font((w - cw) / 2, 7, &clock, theme.text_primary, FontFace::UiBold);

        let snap = net::snapshot();
        let m = pmm::stats();
        let ram_txt = format!("{} MiB", m.total_bytes / (1024 * 1024));
        let rw = font::measure_text(FontFace::Ui, &ram_txt) + 18;
        let rx = w - rw - 66;
        self.canvas.fill_rounded_rect(rx, 4, rw, 24, 6, theme.window_surface);
        self.canvas.draw_text_font(rx + 9, 7, &ram_txt, theme.text_secondary, FontFace::Ui);

        // Live DHCP IP / status pill next to RAM pill
        let (net_txt, net_col) = if let Some(ip) = snap.ip {
            (format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]), theme.term_green)
        } else if snap.present {
            (String::from("DHCP..."), theme.accent_soft)
        } else {
            (String::from("OFFLINE"), theme.err)
        };
        let nw = font::measure_text(FontFace::UiBold, &net_txt) + 18;
        let nx = rx - nw - 8;
        self.canvas.fill_rounded_rect(nx, 4, nw, 24, 6, theme.window_surface);
        self.canvas.draw_text_font(nx + 9, 7, &net_txt, net_col, FontFace::UiBold);

        let bar_col = if snap.dhcp_ready { theme.term_green } else if snap.present { theme.accent_soft } else { theme.text_muted };
        let sx = w - 56;
        self.canvas.fill_rect(sx, 18, 3, 4, bar_col);
        self.canvas.fill_rect(sx + 5, 14, 3, 8, bar_col);
        self.canvas.fill_rect(sx + 10, 10, 3, 12, bar_col);

        self.canvas.fill_circle(w - 22, 16, 9, theme.window_surface);
        self.canvas.draw_text_font(w - 26, 7, "O", theme.accent_soft, FontFace::UiBold);
    }

    fn draw_dock(&mut self, theme: ThemeColors, focused_kind: Option<AppKind>) {
        let (dx, dy, dw, dh) = self.dock_rect();
        self.canvas.draw_soft_shadow(dx, dy, dw, dh, 16);
        self.canvas.fill_rounded_rect_alpha(dx, dy, dw, dh, 16, theme.dock_bg, 224);
        self.canvas.draw_rounded_rect_outline(dx, dy, dw, dh, 16, theme.border_inactive);

        let iy = dy + (DOCK_H - DOCK_ICON_SIZE) / 2 - 2;

        for (i, item) in DOCK_ITEMS.iter().enumerate() {
            let ix = dx + 12 + (i as i32) * (DOCK_ICON_SIZE + DOCK_GAP);
            let hovered = self.hover_dock == Some(i);

            let (is_open, is_focused) = match item {
                DockItem::Launcher => (self.start_menu_open, self.start_menu_open),
                DockItem::App(kind) => {
                    let open = self.windows.iter().any(|w| w.kind == *kind && w.open);
                    let foc = Some(*kind) == focused_kind;
                    (open, foc)
                }
            };

            let tile_bg = if hovered || is_focused {
                theme.titlebar_active
            } else {
                theme.window_surface
            };
            self.canvas.fill_rounded_rect(ix, iy, DOCK_ICON_SIZE, DOCK_ICON_SIZE, 10, tile_bg);
            self.canvas.draw_rounded_rect_outline(
                ix,
                iy,
                DOCK_ICON_SIZE,
                DOCK_ICON_SIZE,
                10,
                if hovered || is_focused { theme.accent } else { theme.border_inactive },
            );

            match item {
                DockItem::Launcher => {
                    self.canvas.draw_rgba_sprite(ix + 8, iy + 8, assets::logo_22(), 255);
                }
                DockItem::App(AppKind::Terminal) => {
                    self.canvas.fill_rounded_rect(ix + 5, iy + 7, 28, 24, 4, theme.term_bg);
                    self.canvas.draw_text_font(ix + 9, iy + 9, ">_", theme.accent, FontFace::Mono);
                }
                DockItem::App(AppKind::Files) => {
                    self.canvas.fill_rounded_rect(ix + 5, iy + 9, 14, 7, 2, 0xD97706);
                    self.canvas.fill_rounded_rect(ix + 5, iy + 12, 28, 17, 4, 0xF59E0B);
                }
                DockItem::App(AppKind::Notes) => {
                    self.canvas.fill_rounded_rect(ix + 7, iy + 6, 24, 26, 4, 0xE2E8F0);
                    self.canvas.fill_rect(ix + 7, iy + 6, 24, 6, theme.accent);
                    self.canvas.fill_rect(ix + 11, iy + 15, 16, 2, 0x475569);
                    self.canvas.fill_rect(ix + 11, iy + 20, 16, 2, 0x475569);
                    self.canvas.fill_rect(ix + 11, iy + 25, 11, 2, 0x475569);
                }
                DockItem::App(AppKind::Media) => {
                    self.canvas.fill_circle(ix + 19, iy + 19, 11, theme.accent);
                    self.canvas.draw_text_font(ix + 15, iy + 10, "|>", 0xFFFFFF, FontFace::UiBold);
                }
                DockItem::App(AppKind::Paint) => {
                    self.canvas.fill_circle(ix + 19, iy + 19, 11, 0x1E293B);
                    self.canvas.fill_circle(ix + 14, iy + 15, 3, theme.accent);
                    self.canvas.fill_circle(ix + 24, iy + 15, 3, theme.accent_sec);
                    self.canvas.fill_circle(ix + 15, iy + 24, 3, theme.term_green);
                    self.canvas.fill_circle(ix + 24, iy + 24, 3, 0xFACC15);
                }
                DockItem::App(AppKind::Calculator) => {
                    self.canvas.fill_rounded_rect(ix + 7, iy + 6, 24, 26, 4, theme.term_bg);
                    self.canvas.fill_rect(ix + 10, iy + 9, 18, 5, theme.accent_sec);
                    for r in 0..2 {
                        for c in 0..3 {
                            let col = if r == 1 && c == 2 { theme.accent } else { 0x64748B };
                            self.canvas.fill_rect(ix + 10 + c * 7, iy + 17 + r * 6, 5, 4, col);
                        }
                    }
                }
                DockItem::App(AppKind::Clock) => {
                    self.canvas.fill_circle(ix + 19, iy + 19, 11, theme.term_bg);
                    self.canvas.draw_line(ix + 19, iy + 19, ix + 19, iy + 11, theme.accent);
                    self.canvas.draw_line(ix + 19, iy + 19, ix + 25, iy + 22, theme.accent_sec);
                }
                DockItem::App(AppKind::Snake) => {
                    self.canvas.fill_rounded_rect(ix + 7, iy + 9, 24, 20, 5, theme.term_bg);
                    self.canvas.fill_rect(ix + 10, iy + 20, 12, 4, theme.accent_soft);
                    self.canvas.fill_rect(ix + 18, iy + 13, 4, 11, theme.accent);
                    self.canvas.fill_circle(ix + 26, iy + 14, 2, theme.accent_sec);
                }
                DockItem::App(AppKind::Doom) => {
                    // DOOM crimson/orange helmet visor icon
                    self.canvas.fill_rounded_rect(ix + 7, iy + 7, 24, 24, 6, 0x991B1B);
                    self.canvas.fill_rect(ix + 10, iy + 13, 18, 6, 0xFF6A00);
                    self.canvas.fill_rect(ix + 12, iy + 14, 5, 3, 0xFACC15);
                    self.canvas.fill_rect(ix + 21, iy + 14, 5, 3, 0xFACC15);
                    self.canvas.fill_rect(ix + 14, iy + 23, 10, 4, 0x1E293B);
                }
                DockItem::App(AppKind::Net) => {
                    self.canvas.fill_circle(ix + 19, iy + 19, 11, theme.accent_sec);
                    self.canvas.fill_circle(ix + 19, iy + 19, 8, theme.term_bg);
                    self.canvas.draw_line(ix + 9, iy + 19, ix + 29, iy + 19, theme.accent_sec);
                    self.canvas.draw_line(ix + 19, iy + 9, ix + 19, iy + 29, theme.accent_sec);
                    self.canvas.fill_circle(ix + 25, iy + 13, 3, theme.accent);
                }
                DockItem::App(AppKind::Snapshots) => {
                    self.canvas.fill_rounded_rect(ix + 8, iy + 7, 22, 24, 6, 0x065F46);
                    self.canvas.draw_rounded_rect_outline(ix + 8, iy + 7, 22, 24, 6, theme.term_green);
                    self.canvas.fill_circle(ix + 19, iy + 19, 6, theme.accent);
                    self.canvas.fill_circle(ix + 19, iy + 19, 3, 0xECFDF5);
                }
                DockItem::App(AppKind::SysMon) => {
                    self.canvas.fill_rect(ix + 7, iy + 22, 4, 10, theme.accent_sec);
                    self.canvas.fill_rect(ix + 14, iy + 13, 4, 19, theme.accent);
                    self.canvas.fill_rect(ix + 21, iy + 17, 4, 15, theme.term_green);
                    self.canvas.fill_rect(ix + 28, iy + 9, 4, 23, theme.accent_soft);
                }
                DockItem::App(AppKind::Settings) => {
                    self.canvas.fill_circle(ix + 19, iy + 19, 10, 0x64748B);
                    self.canvas.fill_circle(ix + 19, iy + 19, 5, tile_bg);
                    self.canvas.fill_circle(ix + 19, iy + 19, 2, theme.accent);
                }
                DockItem::App(AppKind::About) => {
                    self.canvas.fill_circle(ix + 19, iy + 19, 11, theme.accent);
                    self.canvas.draw_text_font(ix + 17, iy + 10, "i", 0xFFFFFF, FontFace::UiBold);
                }
            }

            if is_open {
                let dot_col = if is_focused { theme.accent } else { theme.text_secondary };
                self.canvas.fill_rect(ix + 14, dy + DOCK_H - 6, 12, 3, dot_col);
            }

            if hovered {
                let lbl = item.label();
                let tw = font::measure_text(FontFace::UiBold, lbl) + 16;
                let tx = (ix + DOCK_ICON_SIZE / 2 - tw / 2).clamp(8, self.canvas.width - tw - 8);
                let ty = dy - 30;
                self.canvas.fill_rounded_rect(tx, ty, tw, 24, 6, theme.topbar_bg);
                self.canvas.draw_rounded_rect_outline(tx, ty, tw, 24, 6, theme.accent);
                self.canvas.draw_text_font(tx + 8, ty + 4, lbl, theme.text_primary, FontFace::UiBold);
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
        self.canvas.draw_text_font(sx + 70, sy + 16, "AmbaOS v0.5.0", theme.text_primary, FontFace::UiBold);
        self.canvas.draw_text_font(sx + 70, sy + 36, "14 Apps + Immutable Atomic Rollback", theme.accent_soft, FontFace::Ui);

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
