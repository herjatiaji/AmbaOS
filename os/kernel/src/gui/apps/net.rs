use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::drivers::keyboard::KeyEvent;
use crate::gui::assets;
use crate::gui::canvas::Canvas;
use crate::gui::font::{self, FontFace};
use crate::gui::theme::ThemeColors;
use crate::net::{self, HttpViewState, PageBlock};
use crate::proc::caps::{self, AppId, CAP_NET_ACCESS};

#[derive(Clone, Copy, PartialEq, Eq)]
enum NetTab {
    Browser,
    Tools,
}

#[derive(Clone, Copy)]
enum RowStyle {
    H1,
    H2,
    Text,
    Bullet,
    Link,
    Divider,
    Blank,
}

struct RenderedRow {
    style: RowStyle,
    text: String,
    link_url: Option<String>,
}

const BOOKMARKS: [(&str, &str, i32, i32); 5] = [
    ("Google", "https://www.google.com/?hl=en", 12, 86),
    ("YouTube", "https://www.youtube.com/", 104, 92),
    ("Wikipedia", "https://en.wikipedia.org/wiki/Operating_system", 202, 108),
    ("Hacker News", "https://news.ycombinator.com/", 316, 114),
    ("Rust Lang", "https://www.rust-lang.org/", 436, 102),
];

const SPEED_DIAL: [(&str, &str, &str); 6] = [
    ("YouTube Video", "www.youtube.com", "https://www.youtube.com/"),
    ("Google Search", "www.google.com", "https://www.google.com/?hl=en"),
    ("Wikipedia", "en.wikipedia.org", "https://en.wikipedia.org/wiki/Operating_system"),
    ("Hacker News", "news.ycombinator.com", "https://news.ycombinator.com/"),
    ("Rust Language", "www.rust-lang.org", "https://www.rust-lang.org/"),
    ("Search: Operating System", "Live Web Search", "operating system"),
];

const ROW_H: i32 = 22;

pub struct NetApp {
    tab: NetTab,
    visual_mode: bool,
    live_stream: bool,
    last_stream_ms: u64,
    progressive_followups: u8,
    omnibox: String,
    url_focused: bool,
    last_synced_url: String,
    pending_text: String,
    pending_enter: bool,
    history: Vec<String>,
    hist_idx: usize,
    scroll_y: usize,
    last_vw: i32,
    last_vh: i32,
}

impl NetApp {
    pub fn new() -> Self {
        Self {
            tab: NetTab::Browser,
            visual_mode: true,
            live_stream: false,
            last_stream_ms: 0,
            progressive_followups: 0,
            omnibox: String::from("amba://home"),
            url_focused: true,
            last_synced_url: String::from("amba://home"),
            pending_text: String::new(),
            pending_enter: false,
            history: vec![String::from("amba://home")],
            hist_idx: 0,
            scroll_y: 0,
            last_vw: 818,
            last_vh: 448,
        }
    }

    pub fn tick(&mut self, now_ms: u64) -> bool {
        if self.tab != NetTab::Browser || !self.visual_mode {
            return false;
        }
        let state = net::http_state();
        if state.loading || state.url == "amba://home" {
            return false;
        }
        let (video_on, page_loading) = state
            .frame
            .as_ref()
            .map(|f| (f.video_active, f.page_loading))
            .unwrap_or((false, false));
        if self.live_stream || video_on {
            if now_ms.saturating_sub(self.last_stream_ms) >= 45 {
                self.last_stream_ms = now_ms;
                net::start_browser_frame(self.last_vw, self.last_vh);
                return true;
            }
        } else if page_loading && self.progressive_followups < 2 {
            if now_ms.saturating_sub(self.last_stream_ms) >= 600 {
                self.last_stream_ms = now_ms;
                self.progressive_followups += 1;
                net::start_browser_frame(self.last_vw, self.last_vh);
                return true;
            }
        }
        false
    }

    #[inline]
    fn viewport_rect(cw: i32, ch: i32) -> (i32, i32, i32, i32) {
        let vw = (cw - 42).clamp(320, 1200) & !1;
        let vh = (ch - 138).clamp(200, 680) & !1;
        (12, 112, vw, vh)
    }

    pub fn on_net_update(&mut self) {
        let state = net::http_state();
        if !state.loading && !state.url.is_empty() && state.url != self.last_synced_url {
            self.last_synced_url = state.url.clone();
            if !self.url_focused {
                self.omnibox = state.url.clone();
            }
            if state.url == "amba://home" {
                self.url_focused = true;
                self.omnibox = String::from("amba://home");
            }
        }
        if !state.loading && self.visual_mode && (!self.pending_text.is_empty() || self.pending_enter) {
            let batch = core::mem::take(&mut self.pending_text);
            let enter = core::mem::take(&mut self.pending_enter);
            net::start_browser_key(if enter { "Enter" } else { "" }, &batch, self.last_vw, self.last_vh);
        }
    }

    pub fn navigate_to(&mut self, target: &str) {
        let trimmed = target.trim();
        if trimmed.is_empty() {
            return;
        }
        if trimmed != "amba://home" && caps::check(AppId::Browser, CAP_NET_ACCESS).is_err() {
            self.omnibox = String::from("CAPABILITY DENIED: NET_ACCESS revoked");
            return;
        }
        self.tab = NetTab::Browser;
        self.omnibox = String::from(trimmed);
        self.scroll_y = 0;
        self.progressive_followups = 0;
        self.pending_text.clear();
        self.pending_enter = false;
        self.url_focused = trimmed == "amba://home";

        if self.history.get(self.hist_idx).map(|s| s.as_str()) != Some(trimmed) {
            if self.hist_idx + 1 < self.history.len() {
                self.history.truncate(self.hist_idx + 1);
            }
            self.history.push(String::from(trimmed));
            self.hist_idx = self.history.len() - 1;
        }

        net::start_browser_navigate_mode(trimmed, self.visual_mode, self.last_vw, self.last_vh);
    }

    fn go_back(&mut self) -> bool {
        let state = net::http_state();
        if self.visual_mode {
            if state.url != "amba://home" {
                self.pending_text.clear();
                self.pending_enter = false;
                self.url_focused = false;
                net::start_browser_action("back", self.last_vw, self.last_vh);
                return true;
            }
            return false;
        }
        if self.hist_idx > 0 {
            self.hist_idx -= 1;
            let url = self.history[self.hist_idx].clone();
            self.omnibox = url.clone();
            self.scroll_y = 0;
            self.url_focused = url == "amba://home";
            net::start_browser_navigate_mode(&url, false, self.last_vw, self.last_vh);
            return true;
        }
        false
    }

    fn go_forward(&mut self) -> bool {
        if self.visual_mode {
            let state = net::http_state();
            if state.url != "amba://home" {
                self.pending_text.clear();
                self.pending_enter = false;
                net::start_browser_action("forward", self.last_vw, self.last_vh);
                return true;
            }
            return false;
        }
        if self.hist_idx + 1 < self.history.len() {
            self.hist_idx += 1;
            let url = self.history[self.hist_idx].clone();
            self.omnibox = url.clone();
            self.scroll_y = 0;
            self.url_focused = url == "amba://home";
            net::start_browser_navigate_mode(&url, false, self.last_vw, self.last_vh);
            return true;
        }
        false
    }

    fn reload(&mut self) -> bool {
        self.progressive_followups = 0;
        let state = net::http_state();
        if self.visual_mode && state.url != "amba://home" {
            self.pending_text.clear();
            self.pending_enter = false;
            net::start_browser_action("reload", self.last_vw, self.last_vh);
            return true;
        }
        let cur = self
            .history
            .get(self.hist_idx)
            .cloned()
            .unwrap_or_else(|| self.omnibox.clone());
        self.scroll_y = 0;
        net::start_browser_navigate_mode(&cur, self.visual_mode, self.last_vw, self.last_vh);
        true
    }

    pub fn sync_url_from_term(&mut self, target: &str) {
        self.navigate_to(target);
    }

    pub fn handle_wheel(&mut self, dz: i32) -> bool {
        if self.tab != NetTab::Browser || dz == 0 {
            return false;
        }
        let state = net::http_state();
        if state.url == "amba://home" {
            return false;
        }
        if self.visual_mode {
            if !state.loading {
                net::start_browser_scroll(dz * 120, None, self.last_vw, self.last_vh);
                return true;
            }
            return false;
        }
        if dz > 0 {
            self.scroll_y = self.scroll_y.saturating_add((dz as usize) * 3);
        } else {
            self.scroll_y = self.scroll_y.saturating_sub((-dz as usize) * 3);
        }
        true
    }

    pub fn handle_key(&mut self, ev: KeyEvent) -> bool {
        if self.tab != NetTab::Browser {
            return false;
        }
        let state = net::http_state();
        if ev == KeyEvent::CtrlChar('l') {
            self.url_focused = true;
            if state.url != "amba://home" && !state.url.is_empty() {
                self.omnibox = state.url;
            }
            return true;
        }
        if let KeyEvent::AltChar(c @ '1'..='6') = ev {
            if state.url == "amba://home" {
                let idx = (c as usize) - ('1' as usize);
                if let Some(&(_, _, url)) = SPEED_DIAL.get(idx) {
                    self.navigate_to(url);
                    return true;
                }
            }
        }
        if self.url_focused {
            return match ev {
                KeyEvent::Char(ch) if ch.is_ascii() && !ch.is_ascii_control() => {
                    if self.omnibox.len() < 180 {
                        if self.omnibox == "amba://home" {
                            self.omnibox.clear();
                        }
                        self.omnibox.push(ch);
                        true
                    } else {
                        false
                    }
                }
                KeyEvent::Backspace => self.omnibox.pop().is_some(),
                KeyEvent::Escape => {
                    self.omnibox.clear();
                    true
                }
                KeyEvent::Tab => {
                    self.url_focused = false;
                    true
                }
                KeyEvent::Enter => {
                    let target = self.omnibox.clone();
                    self.navigate_to(&target);
                    true
                }
                KeyEvent::Down | KeyEvent::PageDown => {
                    if state.url == "amba://home" {
                        self.url_focused = false;
                    } else {
                        self.scroll_y = self.scroll_y.saturating_add(6);
                    }
                    true
                }
                KeyEvent::Up | KeyEvent::PageUp => {
                    self.scroll_y = self.scroll_y.saturating_sub(6);
                    true
                }
                _ => false,
            };
        }

        // Speed Dial quick launch when url_focused == false at home
        if state.url == "amba://home" {
            if let KeyEvent::Char(c @ '1'..='6') = ev {
                let idx = (c as usize) - ('1' as usize);
                if let Some(&(_, _, url)) = SPEED_DIAL.get(idx) {
                    self.navigate_to(url);
                    return true;
                }
            }
        }

        // Viewport / HTML input focused in Visual Mode
        if self.visual_mode && state.url != "amba://home" {
            return match ev {
                KeyEvent::Escape => {
                    self.url_focused = true;
                    self.omnibox = state.url;
                    self.pending_text.clear();
                    self.pending_enter = false;
                    true
                }
                KeyEvent::Char(ch) if ch.is_ascii() && !ch.is_ascii_control() => {
                    if state.loading {
                        if self.pending_text.len() < 64 {
                            self.pending_text.push(ch);
                        }
                    } else {
                        let mut s = String::new();
                        s.push(ch);
                        net::start_browser_key("", &s, self.last_vw, self.last_vh);
                    }
                    true
                }
                KeyEvent::Backspace => {
                    if state.loading {
                        let _ = self.pending_text.pop();
                    } else {
                        net::start_browser_key("Backspace", "", self.last_vw, self.last_vh);
                    }
                    true
                }
                KeyEvent::Enter => {
                    if state.loading {
                        self.pending_enter = true;
                    } else {
                        let batch = core::mem::take(&mut self.pending_text);
                        net::start_browser_key("Enter", &batch, self.last_vw, self.last_vh);
                    }
                    true
                }
                KeyEvent::Tab => {
                    if !state.loading {
                        net::start_browser_key("Tab", "", self.last_vw, self.last_vh);
                    }
                    true
                }
                KeyEvent::Left => {
                    if !state.loading {
                        net::start_browser_key("ArrowLeft", "", self.last_vw, self.last_vh);
                    }
                    true
                }
                KeyEvent::Right => {
                    if !state.loading {
                        net::start_browser_key("ArrowRight", "", self.last_vw, self.last_vh);
                    }
                    true
                }
                KeyEvent::Down | KeyEvent::PageDown => {
                    if !state.loading {
                        let inp = state.frame.as_ref().map(|f| f.input_focused).unwrap_or(false);
                        let step = if matches!(ev, KeyEvent::PageDown) { 380 } else { 180 };
                        if inp && matches!(ev, KeyEvent::Down) {
                            net::start_browser_key("ArrowDown", "", self.last_vw, self.last_vh);
                        } else {
                            net::start_browser_scroll(step, None, self.last_vw, self.last_vh);
                        }
                    }
                    true
                }
                KeyEvent::Up | KeyEvent::PageUp => {
                    if !state.loading {
                        let inp = state.frame.as_ref().map(|f| f.input_focused).unwrap_or(false);
                        let step = if matches!(ev, KeyEvent::PageUp) { -380 } else { -180 };
                        if inp && matches!(ev, KeyEvent::Up) {
                            net::start_browser_key("ArrowUp", "", self.last_vw, self.last_vh);
                        } else {
                            net::start_browser_scroll(step, None, self.last_vw, self.last_vh);
                        }
                    }
                    true
                }
                KeyEvent::Home => {
                    if !state.loading {
                        net::start_browser_scroll(0, Some(0), self.last_vw, self.last_vh);
                    }
                    true
                }
                _ => false,
            };
        }

        match ev {
            KeyEvent::Escape => {
                self.url_focused = true;
                true
            }
            KeyEvent::Down => {
                self.scroll_y = self.scroll_y.saturating_add(3);
                true
            }
            KeyEvent::Up => {
                self.scroll_y = self.scroll_y.saturating_sub(3);
                true
            }
            KeyEvent::PageDown => {
                self.scroll_y = self.scroll_y.saturating_add(12);
                true
            }
            KeyEvent::PageUp => {
                self.scroll_y = self.scroll_y.saturating_sub(12);
                true
            }
            KeyEvent::Home => {
                self.scroll_y = 0;
                true
            }
            _ => false,
        }
    }

    fn speed_dial_rect(cw: i32, idx: usize) -> (i32, i32, i32, i32) {
        let col = (idx % 2) as i32;
        let row = (idx / 2) as i32;
        let card_w = 276;
        let card_h = 68;
        let gap = 16;
        let grid_w = card_w * 2 + gap;
        let start_x = (cw - 32 - grid_w) / 2 + 12;
        let start_y = 224;
        (
            start_x + col * (card_w + gap),
            start_y + row * (card_h + 12),
            card_w,
            card_h,
        )
    }

    pub fn handle_click(&mut self, lx: i32, ly: i32, cw: i32, ch: i32) -> bool {
        let (vx, vy, vw, vh) = Self::viewport_rect(cw, ch);
        self.last_vw = vw;
        self.last_vh = vh;

        // 1. Top tabs (y: 8..34)
        if (8..=34).contains(&ly) {
            if (12..=196).contains(&lx) {
                self.tab = NetTab::Browser;
                return true;
            }
            if (204..=352).contains(&lx) {
                self.tab = NetTab::Tools;
                return true;
            }
            if (360..=472).contains(&lx) {
                self.visual_mode = !self.visual_mode;
                self.pending_text.clear();
                self.pending_enter = false;
                let state = net::http_state();
                if self.tab == NetTab::Browser && state.url != "amba://home" {
                    let cur = if state.url.starts_with("search://") {
                        state.url.trim_start_matches("search://")
                    } else {
                        state.url.as_str()
                    };
                    net::start_browser_navigate_mode(cur, self.visual_mode, vw, vh);
                }
                return true;
            }
            if (480..=574).contains(&lx) {
                self.live_stream = !self.live_stream;
                return true;
            }
        }

        match self.tab {
            NetTab::Browser => {
                // 2. Nav & Omnibox bar (y: 42..72)
                if (42..=72).contains(&ly) {
                    if (12..=42).contains(&lx) {
                        return self.go_back();
                    }
                    if (46..=76).contains(&lx) {
                        return self.go_forward();
                    }
                    if (80..=110).contains(&lx) {
                        return self.reload();
                    }
                    if (114..=144).contains(&lx) {
                        self.navigate_to("amba://home");
                        return true;
                    }
                    if lx >= 152 && lx < cw - 80 {
                        self.url_focused = true;
                        return true;
                    }
                    if lx >= cw - 74 && lx <= cw - 12 {
                        let target = self.omnibox.clone();
                        self.navigate_to(&target);
                        return true;
                    }
                }

                // 3. Bookmarks bar (y: 78..104)
                if (78..=104).contains(&ly) {
                    for &(_, url, bx, bw) in &BOOKMARKS {
                        if lx >= bx && lx <= bx + bw {
                            self.navigate_to(url);
                            return true;
                        }
                    }
                }

                let state = net::http_state();

                // 4. Right scrollbar clicks
                if lx >= cw - 26 && lx <= cw - 10 && ly >= vy && ly <= vy + vh {
                    if self.visual_mode {
                        if let Some(ref frame) = state.frame {
                            if ly <= vy + 24 {
                                net::start_browser_scroll(-180, None, vw, vh);
                            } else if ly >= vy + vh - 24 {
                                net::start_browser_scroll(180, None, vw, vh);
                            } else {
                                let track_h = (vh - 48).max(1) as u64;
                                let rel = (ly - (vy + 24)).clamp(0, vh - 48) as u64;
                                let max_s = frame.scroll_h.saturating_sub(frame.height as u32) as u64;
                                let abs_y = (rel * max_s / track_h) as u32;
                                net::start_browser_scroll(0, Some(abs_y), vw, vh);
                            }
                            return true;
                        }
                    }
                    if ly <= vy + 24 {
                        self.scroll_y = self.scroll_y.saturating_sub(4);
                    } else if ly >= vy + vh - 24 {
                        self.scroll_y = self.scroll_y.saturating_add(4);
                    } else if ly < vy + vh / 2 {
                        self.scroll_y = self.scroll_y.saturating_sub(10);
                    } else {
                        self.scroll_y = self.scroll_y.saturating_add(10);
                    }
                    return true;
                }

                // 5. Viewport clicks (Speed Dial, Visual HTML5 Click, or Reader Hyperlinks)
                if lx >= vx && lx < vx + vw && ly >= vy && ly < vy + vh {
                    self.url_focused = false;
                    if state.url == "amba://home" && !state.loading {
                        for (idx, &(_, _, url)) in SPEED_DIAL.iter().enumerate() {
                            let (sx, sy, sw, sh) = Self::speed_dial_rect(cw, idx);
                            if lx >= sx && lx <= sx + sw && ly >= sy && ly <= sy + sh {
                                self.navigate_to(url);
                                return true;
                            }
                        }
                    } else if self.visual_mode && state.frame.is_some() {
                        net::start_browser_click(lx - vx, ly - vy, vw, vh);
                        return true;
                    } else if !state.loading && ly >= vy + 10 {
                        let rows = build_rendered_rows(&state, vw - 28);
                        let row_idx = self.scroll_y + ((ly - (vy + 10)) / ROW_H) as usize;
                        if let Some(row) = rows.get(row_idx) {
                            if let Some(ref target_url) = row.link_url {
                                let cloned = target_url.clone();
                                self.navigate_to(&cloned);
                                return true;
                            }
                        }
                    }
                }
            }
            NetTab::Tools => {
                if (178..=208).contains(&ly) {
                    if caps::check(AppId::Browser, CAP_NET_ACCESS).is_err() {
                        self.omnibox = String::from("CAPABILITY DENIED: NET_ACCESS revoked");
                        return true;
                    }
                    if (14..=180).contains(&lx) {
                        net::start_ping([10, 0, 2, 2]);
                        return true;
                    }
                    if (188..=312).contains(&lx) {
                        net::start_ping([1, 1, 1, 1]);
                        return true;
                    }
                    if (320..=478).contains(&lx) {
                        net::start_dns("example.com");
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn render(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32, large_text: bool) {
        canvas.fill_rect(cx, cy, cw, ch, theme.window_bg);

        let snap = net::snapshot();
        let state = net::http_state();

        // Top tabs
        let b_act = self.tab == NetTab::Browser;
        canvas.fill_rounded_rect(
            cx + 12,
            cy + 8,
            184,
            26,
            6,
            if b_act { theme.accent } else { theme.window_surface },
        );
        let tab_title = if state.loading {
            "Loading..."
        } else if state.title.is_empty() {
            "AmbaBrowser"
        } else {
            &state.title
        };
        canvas.draw_text_clipped_font(
            cx + 22,
            cy + 12,
            164,
            tab_title,
            if b_act { 0xFFFFFF } else { theme.text_secondary },
            FontFace::UiBold,
        );

        let t_act = self.tab == NetTab::Tools;
        canvas.fill_rounded_rect(
            cx + 204,
            cy + 8,
            148,
            26,
            6,
            if t_act { theme.accent } else { theme.window_surface },
        );
        canvas.draw_text_font(
            cx + 224,
            cy + 12,
            "Network Tools",
            if t_act { 0xFFFFFF } else { theme.text_secondary },
            FontFace::UiBold,
        );

        // Mode toggle pill: HTML Visual vs Text Reader
        canvas.fill_rounded_rect(cx + 360, cy + 8, 112, 26, 6, theme.window_surface);
        canvas.draw_rounded_rect_outline(
            cx + 360,
            cy + 8,
            112,
            26,
            6,
            if self.visual_mode { theme.accent_sec } else { theme.border_inactive },
        );
        canvas.draw_text_font(
            cx + 372,
            cy + 12,
            if self.visual_mode { "HTML Visual" } else { "Text Reader" },
            if self.visual_mode { theme.accent_sec } else { theme.text_secondary },
            FontFace::UiBold,
        );

        let auto_vid = self.live_stream || state.frame.as_ref().map(|f| f.video_active).unwrap_or(false);
        canvas.fill_rounded_rect(cx + 480, cy + 8, 94, 26, 6, theme.window_surface);
        canvas.draw_rounded_rect_outline(
            cx + 480,
            cy + 8,
            94,
            26,
            6,
            if auto_vid { theme.term_green } else { theme.border_inactive },
        );
        canvas.draw_text_font(
            cx + 492,
            cy + 12,
            if auto_vid { "Live: ON" } else { "Live: OFF" },
            if auto_vid { theme.term_green } else { theme.text_secondary },
            FontFace::UiBold,
        );

        let (badge_txt, badge_col) = if state.loading {
            ("LOADING...", theme.accent_soft)
        } else if snap.dhcp_ready {
            ("HTTPS READY", theme.term_green)
        } else if snap.present {
            ("DHCP...", theme.accent_soft)
        } else {
            ("OFFLINE", theme.err)
        };
        canvas.fill_rounded_rect(cx + cw - 118, cy + 8, 106, 26, 6, theme.window_surface);
        canvas.draw_text_font(cx + cw - 106, cy + 12, badge_txt, badge_col, FontFace::UiBold);

        match self.tab {
            NetTab::Browser => self.render_browser(canvas, theme, &state, cx, cy, cw, ch, large_text),
            NetTab::Tools => self.render_tools(canvas, theme, snap, cx, cy, cw, ch),
        }
    }

    fn render_browser(
        &self,
        canvas: &mut Canvas,
        theme: ThemeColors,
        state: &HttpViewState,
        cx: i32,
        cy: i32,
        cw: i32,
        ch: i32,
        large_text: bool,
    ) {
        // Navigation buttons (< > R H)
        let in_visual_page = self.visual_mode && state.url != "amba://home";
        let can_back = in_visual_page || self.hist_idx > 0;
        let can_fwd = in_visual_page || self.hist_idx + 1 < self.history.len();
        let nav_btns = [
            (12, "<", can_back),
            (46, ">", can_fwd),
            (80, "R", true),
            (114, "H", true),
        ];
        for (bx, lbl, enabled) in nav_btns {
            canvas.fill_rounded_rect(cx + bx, cy + 42, 30, 28, 6, theme.window_surface);
            canvas.draw_rounded_rect_outline(cx + bx, cy + 42, 30, 28, 6, theme.border_inactive);
            canvas.draw_text_font(
                cx + bx + 10,
                cy + 47,
                lbl,
                if enabled { theme.text_primary } else { theme.text_muted },
                FontFace::UiBold,
            );
        }

        // Omnibox URL / Search input
        let omni_x = cx + 152;
        let omni_w = (cw - 234).max(160);
        canvas.fill_rounded_rect(omni_x, cy + 42, omni_w, 28, 6, theme.term_bg);
        canvas.draw_rounded_rect_outline(
            omni_x,
            cy + 42,
            omni_w,
            28,
            6,
            if self.url_focused { theme.accent } else { theme.border_inactive },
        );
        canvas.draw_text_clipped_font(
            omni_x + 10,
            cy + 47,
            omni_w - 22,
            &self.omnibox,
            theme.text_primary,
            FontFace::Mono,
        );
        if self.url_focused {
            let cur_x = omni_x + 10 + font::measure_text(FontFace::Mono, &self.omnibox);
            if cur_x < omni_x + omni_w - 8 {
                canvas.fill_rect(cur_x + 1, cy + 48, 2, 16, theme.accent);
            }
        }

        // Go button
        canvas.fill_rounded_rect(cx + cw - 74, cy + 42, 62, 28, 6, theme.accent);
        canvas.draw_text_font(cx + cw - 54, cy + 47, "Go", 0xFFFFFF, FontFace::UiBold);

        // Bookmarks bar
        for &(lbl, url, bx, bw) in &BOOKMARKS {
            let active = state.url == url;
            canvas.fill_rounded_rect(
                cx + bx,
                cy + 78,
                bw,
                24,
                5,
                if active { theme.titlebar_active } else { theme.window_surface },
            );
            canvas.draw_rounded_rect_outline(
                cx + bx,
                cy + 78,
                bw,
                24,
                5,
                if active { theme.accent } else { theme.border_inactive },
            );
            canvas.draw_text_clipped_font(cx + bx + 10, cy + 81, bw - 16, lbl, theme.accent_sec, FontFace::UiBold);
        }

        // Main viewport
        let (lvx, lvy, vw, vh) = Self::viewport_rect(cw, ch);
        let vx = cx + lvx;
        let vy = cy + lvy;
        canvas.fill_rounded_rect(vx, vy, vw, vh, 8, theme.term_bg);
        canvas.draw_rounded_rect_outline(vx, vy, vw, vh, 8, theme.border_inactive);

        let mut total_rows = 1usize;
        let mut link_count = 0usize;
        let mut is_visual_frame = false;

        if state.url == "amba://home" && !state.loading {
            self.render_speed_dial(canvas, theme, vx, vy, vw, cx, cy, cw);
        } else if let Some(ref frame) = state.frame {
            is_visual_frame = true;
            canvas.blit_rgb_u32(
                vx,
                vy,
                vw,
                vh,
                frame.width as i32,
                frame.height as i32,
                &frame.pixels,
            );
            canvas.draw_rounded_rect_outline(vx, vy, vw, vh, 0, theme.border_inactive);
            if state.loading {
                canvas.fill_rect(vx + 1, vy + 1, (vw - 2) / 2, 3, theme.accent);
            }
        } else {
            let wrap_w = if large_text { ((vw - 28) * 2) / 3 } else { vw - 28 };
            let rows = build_rendered_rows(state, wrap_w);
            total_rows = rows.len().max(1);
            link_count = rows.iter().filter(|r| r.link_url.is_some()).count();

            let row_h = if large_text { 29 } else { ROW_H };
            let max_vis = ((vh - 18) / row_h).max(1) as usize;
            let start = self.scroll_y.min(total_rows.saturating_sub(1));
            let mut ry = vy + 10;

            for row in rows.iter().skip(start).take(max_vis) {
                match row.style {
                    RowStyle::H1 => {
                        canvas.draw_text_scaled_clipped_font(vx + 14, ry, vw - 28, &row.text, theme.accent_soft, FontFace::UiBold, large_text);
                        canvas.fill_rect(vx + 14, ry + row_h - 4, (vw - 28).min(360), 1, theme.accent);
                    }
                    RowStyle::H2 => {
                        canvas.draw_text_scaled_clipped_font(vx + 14, ry, vw - 28, &row.text, theme.accent_sec, FontFace::UiBold, large_text);
                    }
                    RowStyle::Text => {
                        canvas.draw_text_scaled_clipped_font(vx + 14, ry, vw - 28, &row.text, theme.text_secondary, FontFace::Ui, large_text);
                    }
                    RowStyle::Bullet => {
                        canvas.fill_circle(vx + 20, ry + row_h / 2, 2, theme.accent);
                        canvas.draw_text_scaled_clipped_font(vx + 28, ry, vw - 42, &row.text, theme.text_secondary, FontFace::Ui, large_text);
                    }
                    RowStyle::Link => {
                        canvas.draw_text_scaled_clipped_font(vx + 16, ry, vw - 32, &row.text, 0x38BDF8, FontFace::UiBold, large_text);
                        let lw = font::measure_text_scaled(FontFace::UiBold, &row.text, large_text).min(vw - 32);
                        canvas.fill_rect(vx + 16, ry + row_h - 4, lw, 1, 0x38BDF8);
                    }
                    RowStyle::Divider => {
                        canvas.fill_rect(vx + 14, ry + row_h / 2, vw - 28, 1, theme.border_inactive);
                    }
                    RowStyle::Blank => {}
                }
                ry += row_h;
            }
        }

        // Scrollbar on right edge
        let sx = cx + cw - 25;
        canvas.fill_rounded_rect(sx, vy, 15, vh, 6, theme.window_surface);
        canvas.draw_text_font(sx + 4, vy + 2, "^", theme.text_primary, FontFace::UiBold);
        canvas.draw_text_font(sx + 4, vy + vh - 20, "v", theme.text_primary, FontFace::UiBold);

        let track_h = (vh - 48).max(20);
        let (thumb_y, thumb_h) = if let (true, Some(frame)) = (is_visual_frame, &state.frame) {
            let max_s = frame.scroll_h.saturating_sub(frame.height as u32).max(1);
            let th = ((track_h as u32 * (frame.height as u32)) / frame.scroll_h.max(frame.height as u32))
                .clamp(18, track_h as u32) as i32;
            let cl = frame.scroll_y.min(max_s);
            let ty = vy + 24 + (((cl as u64 * (track_h - th) as u64) / (max_s as u64)) as i32);
            (ty, th)
        } else {
            let max_vis = ((vh - 18) / ROW_H).max(1);
            let th = ((track_h * max_vis) / (total_rows as i32).max(max_vis)).clamp(18, track_h);
            let max_scroll = total_rows.saturating_sub(max_vis as usize).max(1);
            let clamped_scroll = self.scroll_y.min(max_scroll);
            let ty = vy + 24 + ((clamped_scroll as i32 * (track_h - th)) / (max_scroll as i32));
            (ty, th)
        };
        canvas.fill_rounded_rect(sx + 3, thumb_y, 9, thumb_h, 4, theme.accent);

        // Bottom Status Bar
        let status_y = cy + ch - 21;
        canvas.draw_text_clipped_font(
            cx + 14,
            status_y,
            cw - 260,
            &format!("{}  |  {}", state.status, state.url),
            theme.text_muted,
            FontFace::Ui,
        );
        let meta = if state.url == "amba://home" {
            String::from("Speed Dial Ready")
        } else if let Some(ref frame) = state.frame {
            if frame.input_focused {
                String::from("Input Active | Type directly")
            } else {
                format!("HTML5 Visual  |  {}x{}", frame.width, frame.height)
            }
        } else {
            format!("{} links  |  Line {}", link_count, self.scroll_y + 1)
        };
        let mw = font::measure_text(FontFace::Ui, &meta);
        canvas.draw_text_font(cx + cw - mw - 14, status_y, &meta, theme.accent_sec, FontFace::Ui);
    }

    fn render_speed_dial(
        &self,
        canvas: &mut Canvas,
        theme: ThemeColors,
        vx: i32,
        vy: i32,
        vw: i32,
        cx: i32,
        cy: i32,
        cw: i32,
    ) {
        let logo = assets::logo_48();
        canvas.draw_rgba_sprite(vx + (vw - logo.width) / 2, vy + 18, logo, 255);

        let title = "AmbaBrowser";
        let tw = font::measure_text(FontFace::UiBold, title);
        canvas.draw_text_font(vx + (vw - tw) / 2, vy + 72, title, theme.text_primary, FontFace::UiBold);

        let sub = "Enter a website URL (https://...) or search query above";
        let sw = font::measure_text(FontFace::Ui, sub);
        canvas.draw_text_font(vx + (vw - sw) / 2, vy + 92, sub, theme.text_muted, FontFace::Ui);

        for (idx, &(name, host, _)) in SPEED_DIAL.iter().enumerate() {
            let (rx, ry_local, rw, rh) = Self::speed_dial_rect(cw, idx);
            let bx = cx + rx;
            let by = cy + ry_local;
            canvas.fill_rounded_rect(bx, by, rw, rh, 8, theme.window_surface);
            canvas.draw_rounded_rect_outline(bx, by, rw, rh, 8, theme.border_inactive);
            canvas.fill_circle(bx + 22, by + rh / 2, 10, theme.accent);
            canvas.draw_text_font(bx + 18, by + rh / 2 - 9, ">", 0xFFFFFF, FontFace::UiBold);
            canvas.draw_text_clipped_font(bx + 42, by + 13, rw - 52, name, theme.text_primary, FontFace::UiBold);
            canvas.draw_text_clipped_font(bx + 42, by + 35, rw - 52, host, theme.accent_sec, FontFace::Ui);
        }
    }

    fn render_tools(
        &self,
        canvas: &mut Canvas,
        theme: ThemeColors,
        snap: net::NetSnapshot,
        cx: i32,
        cy: i32,
        cw: i32,
        ch: i32,
    ) {
        let x = cx + 14;
        let w = cw - 28;

        canvas.fill_rounded_rect(x, cy + 44, w, 124, 8, theme.window_surface);
        canvas.draw_rounded_rect_outline(x, cy + 44, w, 124, 8, theme.border_inactive);

        let mac = format!(
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            snap.mac[0], snap.mac[1], snap.mac[2], snap.mac[3], snap.mac[4], snap.mac[5]
        );
        let ip = snap
            .ip
            .map(|a| format!("{}.{}.{}.{}/{}", a[0], a[1], a[2], a[3], snap.prefix_len))
            .unwrap_or_else(|| String::from("0.0.0.0 (negotiating...)"));
        let gw = snap
            .gateway
            .map(|a| format!("{}.{}.{}.{}", a[0], a[1], a[2], a[3]))
            .unwrap_or_else(|| String::from("-"));
        let dns = snap
            .dns
            .map(|a| format!("{}.{}.{}.{}", a[0], a[1], a[2], a[3]))
            .unwrap_or_else(|| String::from("-"));

        let rows = [
            format!("NIC     : Intel 82540EM E1000  (Link: {})", if snap.link_up { "UP" } else { "DOWN" }),
            format!("MAC     : {}   RX: {}   TX: {}", mac, snap.rx_packets, snap.tx_packets),
            format!("IPv4    : {}", ip),
            format!("Gateway : {}   DNS: {}", gw, dns),
        ];
        for (i, line) in rows.iter().enumerate() {
            canvas.draw_text_clipped_font(x + 12, cy + 54 + (i as i32) * 24, w - 24, line, theme.text_primary, FontFace::Mono);
        }

        let by = cy + 178;
        canvas.fill_rounded_rect(cx + 14, by, 166, 30, 6, theme.accent);
        canvas.draw_text_font(cx + 28, by + 6, "Ping Gateway", 0xFFFFFF, FontFace::UiBold);

        canvas.fill_rounded_rect(cx + 188, by, 124, 30, 6, theme.titlebar_active);
        canvas.draw_text_font(cx + 208, by + 6, "Ping 1.1.1.1", theme.text_primary, FontFace::UiBold);

        canvas.fill_rounded_rect(cx + 320, by, 158, 30, 6, theme.titlebar_active);
        canvas.draw_text_font(cx + 334, by + 6, "DNS example.com", theme.text_primary, FontFace::UiBold);

        let ly = cy + 218;
        let lh = (ch - 230).max(80);
        canvas.fill_rounded_rect(x, ly, w, lh, 8, theme.term_bg);
        canvas.draw_rounded_rect_outline(x, ly, w, lh, 8, theme.border_inactive);

        let logs = net::tool_logs();
        let max_rows = ((lh - 16) / 19).max(1) as usize;
        let start = logs.len().saturating_sub(max_rows);
        let mut ry = ly + 10;
        for msg in &logs[start..] {
            canvas.draw_text_clipped_font(x + 10, ry, w - 20, msg, theme.term_green, FontFace::Mono);
            ry += 19;
        }
    }
}

fn build_rendered_rows(state: &HttpViewState, max_w: i32) -> Vec<RenderedRow> {
    let mut rows = Vec::new();
    for blk in &state.blocks {
        match blk {
            PageBlock::Heading1(t) => {
                for line in wrap_pixel(t, FontFace::UiBold, max_w) {
                    rows.push(RenderedRow {
                        style: RowStyle::H1,
                        text: line,
                        link_url: None,
                    });
                }
                rows.push(RenderedRow {
                    style: RowStyle::Blank,
                    text: String::new(),
                    link_url: None,
                });
            }
            PageBlock::Heading2(t) => {
                rows.push(RenderedRow {
                    style: RowStyle::Blank,
                    text: String::new(),
                    link_url: None,
                });
                for line in wrap_pixel(t, FontFace::UiBold, max_w) {
                    rows.push(RenderedRow {
                        style: RowStyle::H2,
                        text: line,
                        link_url: None,
                    });
                }
            }
            PageBlock::Paragraph(t) => {
                for line in wrap_pixel(t, FontFace::Ui, max_w) {
                    rows.push(RenderedRow {
                        style: RowStyle::Text,
                        text: line,
                        link_url: None,
                    });
                }
            }
            PageBlock::ListItem(t) => {
                let mut first = true;
                for line in wrap_pixel(t, FontFace::Ui, max_w - 16) {
                    rows.push(RenderedRow {
                        style: if first { RowStyle::Bullet } else { RowStyle::Text },
                        text: line,
                        link_url: None,
                    });
                    first = false;
                }
            }
            PageBlock::Link { label, url } => {
                for line in wrap_pixel(label, FontFace::UiBold, max_w - 8) {
                    rows.push(RenderedRow {
                        style: RowStyle::Link,
                        text: line,
                        link_url: Some(url.clone()),
                    });
                }
            }
            PageBlock::Divider => {
                rows.push(RenderedRow {
                    style: RowStyle::Divider,
                    text: String::new(),
                    link_url: None,
                });
            }
        }
    }
    if rows.is_empty() && !state.body.is_empty() {
        for line in wrap_pixel(&state.body, FontFace::Ui, max_w) {
            rows.push(RenderedRow {
                style: RowStyle::Text,
                text: line,
                link_url: None,
            });
        }
    }
    rows
}

fn wrap_pixel(text: &str, face: FontFace, max_w: i32) -> Vec<String> {
    let mut out = Vec::new();
    let max_w = max_w.max(60);

    for raw_line in text.lines() {
        if raw_line.trim().is_empty() {
            continue;
        }
        let mut cur = String::new();
        for word in raw_line.split_whitespace() {
            if cur.is_empty() {
                if font::measure_text(face, word) <= max_w {
                    cur.push_str(word);
                } else {
                    for ch in word.chars() {
                        cur.push(ch);
                        if font::measure_text(face, &cur) > max_w {
                            cur.pop();
                            if !cur.is_empty() {
                                out.push(cur);
                            }
                            cur = String::new();
                            cur.push(ch);
                        }
                    }
                }
            } else {
                let candidate = format!("{} {}", cur, word);
                if font::measure_text(face, &candidate) <= max_w {
                    cur = candidate;
                } else {
                    out.push(cur);
                    cur = String::from(word);
                }
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
    }
    out
}
