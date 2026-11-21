use crate::gui::canvas::Canvas;
use crate::gui::font::{self, FontFace};
use crate::gui::theme::{self, ThemeColors};

pub const TOPBAR_H: i32 = 28;
pub const DOCK_RESERVE_H: i32 = 104;
pub const TITLEBAR_H: i32 = 32;
pub const WIN_RADIUS: i32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKind {
    Hub,
    Terminal,
    Files,
    Notes,
    Media,
    Paint,
    Calculator,
    Clock,
    Snake,
    Doom,
    Net,
    Snapshots,
    SysMon,
    Settings,
    About,
}

impl AppKind {
    pub const ALL: [AppKind; 15] = [
        AppKind::Hub,
        AppKind::Terminal,
        AppKind::Files,
        AppKind::Media,
        AppKind::Net,
        AppKind::Snapshots,
        AppKind::Doom,
        AppKind::Snake,
        AppKind::Notes,
        AppKind::Paint,
        AppKind::Calculator,
        AppKind::Clock,
        AppKind::SysMon,
        AppKind::Settings,
        AppKind::About,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AppKind::Hub => "AmbaHub",
            AppKind::Terminal => "AmbaTerm",
            AppKind::Files => "Files",
            AppKind::Notes => "AmbaNotes",
            AppKind::Media => "AmbaMedia",
            AppKind::Paint => "AmbaPaint",
            AppKind::Calculator => "Calculator",
            AppKind::Clock => "AmbaClock",
            AppKind::Snake => "AmbaSnake",
            AppKind::Doom => "AmbaDoom",
            AppKind::Net => "Browser",
            AppKind::Snapshots => "AmbaSnap",
            AppKind::SysMon => "SysMon",
            AppKind::Settings => "Settings",
            AppKind::About => "About",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            AppKind::Hub => "Games & .exe Store",
            AppKind::Terminal => "ash Command Shell",
            AppKind::Files => "VFS & PE32+ Inspector",
            AppKind::Notes => "Persistent Editor",
            AppKind::Media => "Video, Music & Gallery",
            AppKind::Paint => "Drawing Studio",
            AppKind::Calculator => "Integer Math",
            AppKind::Clock => "Clock & Stopwatch",
            AppKind::Snake => "Arcade Mini-Game",
            AppKind::Doom => "3D Raycaster FPS",
            AppKind::Net => "Internet Web Browser",
            AppKind::Snapshots => "Immutable & Rollback",
            AppKind::SysMon => "io_uring, SIP & NPU",
            AppKind::Settings => "Themes & Desktop",
            AppKind::About => "Logo & System Info",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleAction {
    None,
    Close,
    Minimize,
    MaximizeToggle,
    DragStart,
}

pub struct Window {
    pub kind: AppKind,
    pub title: &'static str,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub open: bool,
    pub minimized: bool,
    pub maximized: bool,
    pub saved_rect: (i32, i32, i32, i32),
}

impl Window {
    pub fn new(kind: AppKind, title: &'static str, x: i32, y: i32, w: i32, h: i32, open: bool) -> Self {
        Self {
            kind,
            title,
            x,
            y,
            w,
            h,
            open,
            minimized: false,
            maximized: false,
            saved_rect: (x, y, w, h),
        }
    }

    #[inline]
    pub fn is_visible(&self) -> bool {
        self.open && !self.minimized
    }

    #[inline]
    pub fn contains(&self, px: i32, py: i32) -> bool {
        self.is_visible() && px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    pub fn toggle_maximize(&mut self, screen_w: i32, screen_h: i32) {
        if self.maximized {
            let (sx, sy, sw, sh) = self.saved_rect;
            self.x = sx;
            self.y = sy;
            self.w = sw;
            self.h = sh;
            self.maximized = false;
        } else {
            self.saved_rect = (self.x, self.y, self.w, self.h);
            self.x = 14;
            self.y = TOPBAR_H + 8;
            self.w = (screen_w - 28).max(320);
            self.h = (screen_h - TOPBAR_H - DOCK_RESERVE_H - 12).max(240);
            self.maximized = true;
        }
    }

    pub fn hit_titlebar_action(&self, px: i32, py: i32) -> TitleAction {
        if py < self.y || py >= self.y + TITLEBAR_H {
            return TitleAction::None;
        }
        let btn_y_top = self.y + (TITLEBAR_H - 18) / 2;
        let btn_y_bot = btn_y_top + 18;
        if py >= btn_y_top && py <= btn_y_bot {
            // Left-aligned macOS-style traffic light controls
            let close_cx = self.x + 18;
            let min_cx = self.x + 38;
            let max_cx = self.x + 58;
            if (px - close_cx).abs() <= 9 {
                return TitleAction::Close;
            }
            if (px - min_cx).abs() <= 9 {
                return TitleAction::Minimize;
            }
            if (px - max_cx).abs() <= 9 {
                return TitleAction::MaximizeToggle;
            }
        }
        if px >= self.x + 68 && px < self.x + self.w {
            return TitleAction::DragStart;
        }
        TitleAction::None
    }

    #[inline]
    pub fn client_rect(&self) -> (i32, i32, i32, i32) {
        (self.x + 1, self.y + TITLEBAR_H, self.w - 2, self.h - TITLEBAR_H - 1)
    }

    pub fn draw_chrome(&self, canvas: &mut Canvas, theme: ThemeColors, focused: bool) {
        let r = if self.maximized { 6 } else { WIN_RADIUS };
        canvas.draw_soft_shadow(self.x, self.y, self.w, self.h, r);
        canvas.fill_rounded_rect(self.x, self.y, self.w, self.h, r, theme.window_bg);

        let title_bg = if focused {
            theme.titlebar_active
        } else {
            theme.titlebar_inactive
        };
        let border = if focused {
            theme.border_active
        } else {
            theme.border_inactive
        };

        canvas.fill_rounded_rect(self.x, self.y, self.w, TITLEBAR_H, r, title_bg);
        canvas.fill_rect(self.x, self.y + TITLEBAR_H - r, self.w, r, title_bg);
        canvas.fill_rect(self.x + 1, self.y + TITLEBAR_H - 1, self.w - 2, 1, theme.border_inactive);
        canvas.draw_rounded_rect_outline(self.x, self.y, self.w, self.h, r, border);

        // macOS Traffic Light Buttons on the Left
        let cy = self.y + TITLEBAR_H / 2;
        canvas.fill_circle(self.x + 18, cy, 6, theme::BTN_CLOSE);
        canvas.fill_circle(self.x + 38, cy, 6, theme::BTN_MIN);
        canvas.fill_circle(self.x + 58, cy, 6, theme::BTN_MAX);

        let title_w = font::measure_text(FontFace::UiBold, self.title);
        let max_title_w = (self.w - 148).max(40);
        let title_x = if title_w < max_title_w {
            self.x + (self.w - title_w) / 2
        } else {
            self.x + 76
        };
        canvas.draw_text_clipped_font(
            title_x,
            self.y + 7,
            max_title_w,
            self.title,
            if focused { theme.text_primary } else { theme.text_secondary },
            FontFace::UiBold,
        );
    }
}
