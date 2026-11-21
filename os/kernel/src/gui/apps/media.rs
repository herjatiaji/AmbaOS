use alloc::format;

use crate::drivers::audio;
use crate::gui::assets;
use crate::gui::canvas::Canvas;
use crate::gui::font::{self, FontFace};
use crate::gui::theme::ThemeColors;

#[derive(Clone, Copy)]
struct Note {
    freq: u32,
    ms: u64,
}

#[derive(Clone, Copy)]
struct Track {
    title: &'static str,
    artist: &'static str,
    bpm: u32,
    notes: &'static [Note],
}

const TRACK_BOOT: &[Note] = &[
    Note { freq: 523, ms: 160 },
    Note { freq: 659, ms: 160 },
    Note { freq: 784, ms: 160 },
    Note { freq: 1046, ms: 320 },
    Note { freq: 0, ms: 80 },
    Note { freq: 784, ms: 140 },
    Note { freq: 1046, ms: 420 },
    Note { freq: 880, ms: 160 },
    Note { freq: 988, ms: 160 },
    Note { freq: 1046, ms: 360 },
    Note { freq: 0, ms: 160 },
];

const TRACK_SYNTH: &[Note] = &[
    Note { freq: 220, ms: 180 },
    Note { freq: 262, ms: 180 },
    Note { freq: 330, ms: 180 },
    Note { freq: 440, ms: 180 },
    Note { freq: 392, ms: 180 },
    Note { freq: 330, ms: 180 },
    Note { freq: 294, ms: 220 },
    Note { freq: 330, ms: 220 },
    Note { freq: 220, ms: 180 },
    Note { freq: 262, ms: 180 },
    Note { freq: 349, ms: 180 },
    Note { freq: 440, ms: 240 },
    Note { freq: 392, ms: 300 },
    Note { freq: 0, ms: 100 },
];

const TRACK_CYBER: &[Note] = &[
    Note { freq: 330, ms: 140 },
    Note { freq: 330, ms: 140 },
    Note { freq: 0, ms: 70 },
    Note { freq: 330, ms: 140 },
    Note { freq: 0, ms: 70 },
    Note { freq: 262, ms: 140 },
    Note { freq: 330, ms: 180 },
    Note { freq: 392, ms: 280 },
    Note { freq: 0, ms: 120 },
    Note { freq: 196, ms: 280 },
    Note { freq: 262, ms: 180 },
    Note { freq: 196, ms: 180 },
    Note { freq: 165, ms: 240 },
];

const TRACK_LOFI: &[Note] = &[
    Note { freq: 262, ms: 300 },
    Note { freq: 330, ms: 300 },
    Note { freq: 392, ms: 300 },
    Note { freq: 494, ms: 420 },
    Note { freq: 440, ms: 280 },
    Note { freq: 392, ms: 300 },
    Note { freq: 349, ms: 300 },
    Note { freq: 330, ms: 420 },
    Note { freq: 294, ms: 320 },
    Note { freq: 392, ms: 420 },
];

const TRACKS: [Track; 4] = [
    Track {
        title: "AmbaOS Startup Chime",
        artist: "AmbaSynth Core",
        bpm: 132,
        notes: TRACK_BOOT,
    },
    Track {
        title: "Midnight Synthwave",
        artist: "Obsidian FM",
        bpm: 118,
        notes: TRACK_SYNTH,
    },
    Track {
        title: "Cyber Groove Loop",
        artist: "Ring-0 Chiptune",
        bpm: 140,
        notes: TRACK_CYBER,
    },
    Track {
        title: "Lo-Fi Kernel Chill",
        artist: "AmbaDE Beats",
        bpm: 88,
        notes: TRACK_LOFI,
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum MediaTab {
    Player,
    Gallery,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GalleryBackdrop {
    DarkGlass,
    WarmEmber,
    LightCard,
}

pub struct MediaApp {
    tab: MediaTab,
    backdrop: GalleryBackdrop,
    track_idx: usize,
    note_idx: usize,
    playing: bool,
    last_note_ms: u64,
    last_anim_ms: u64,
    bars: [u8; 16],
}

impl MediaApp {
    pub fn new() -> Self {
        Self {
            tab: MediaTab::Player,
            backdrop: GalleryBackdrop::DarkGlass,
            track_idx: 0,
            note_idx: 0,
            playing: false,
            last_note_ms: 0,
            last_anim_ms: 0,
            bars: [22, 34, 48, 30, 56, 70, 44, 64, 52, 38, 60, 46, 32, 42, 28, 18],
        }
    }

    pub fn start_playing(&mut self, now_ms: u64) {
        self.playing = true;
        self.last_note_ms = now_ms;
        self.last_anim_ms = now_ms;
        let note = TRACKS[self.track_idx].notes[self.note_idx];
        audio::play_freq(note.freq);
    }

    pub fn stop_playback(&mut self) {
        self.playing = false;
        self.note_idx = 0;
        audio::stop();
    }

    pub fn tick(&mut self, now_ms: u64) -> bool {
        if !self.playing {
            return false;
        }
        let mut redraw = false;
        let notes = TRACKS[self.track_idx].notes;
        let cur_dur = notes[self.note_idx].ms;

        if now_ms.saturating_sub(self.last_note_ms) >= cur_dur {
            self.last_note_ms = now_ms;
            self.note_idx = (self.note_idx + 1) % notes.len();
            let freq = notes[self.note_idx].freq;
            audio::play_freq(freq);
            redraw = true;
        }

        if now_ms.saturating_sub(self.last_anim_ms) >= 65 {
            self.last_anim_ms = now_ms;
            let cur_freq = notes[self.note_idx].freq;
            for (i, b) in self.bars.iter_mut().enumerate() {
                let target = if cur_freq == 0 {
                    12u8
                } else {
                    let seed = ((cur_freq as u64) * 31 + (i as u64) * 19 + now_ms / 65) % 85;
                    (15 + seed) as u8
                };
                *b = ((*b as u16 * 2 + target as u16) / 3) as u8;
            }
            redraw = true;
        }

        redraw
    }

    pub fn handle_click(&mut self, lx: i32, ly: i32, cw: i32, now_ms: u64) -> bool {
        // Tabs header (y: 8..34)
        if (8..=34).contains(&ly) {
            if (16..=152).contains(&lx) {
                self.tab = MediaTab::Player;
                return true;
            }
            if (160..=270).contains(&lx) {
                self.tab = MediaTab::Gallery;
                return true;
            }
        }

        match self.tab {
            MediaTab::Player => {
                // Transport buttons (y: 164..194)
                if (164..=194).contains(&ly) {
                    if (26..=72).contains(&lx) {
                        self.track_idx = (self.track_idx + TRACKS.len() - 1) % TRACKS.len();
                        self.note_idx = 0;
                        if self.playing {
                            self.start_playing(now_ms);
                        }
                        return true;
                    }
                    if (80..=194).contains(&lx) {
                        if self.playing {
                            self.playing = false;
                            audio::stop();
                        } else {
                            self.start_playing(now_ms);
                        }
                        return true;
                    }
                    if (202..=262).contains(&lx) {
                        self.stop_playback();
                        return true;
                    }
                    if (270..=316).contains(&lx) {
                        self.track_idx = (self.track_idx + 1) % TRACKS.len();
                        self.note_idx = 0;
                        if self.playing {
                            self.start_playing(now_ms);
                        }
                        return true;
                    }
                    if (328..=cw - 24).contains(&lx) {
                        audio::set_muted(!audio::is_muted());
                        if self.playing && !audio::is_muted() {
                            let f = TRACKS[self.track_idx].notes[self.note_idx].freq;
                            audio::play_freq(f);
                        }
                        return true;
                    }
                }

                // Playlist rows (y >= 224)
                if lx >= 16 && lx <= cw - 16 && ly >= 224 {
                    let row = ((ly - 224) / 36) as usize;
                    if row < TRACKS.len() {
                        self.track_idx = row;
                        self.note_idx = 0;
                        self.start_playing(now_ms);
                        return true;
                    }
                }
            }
            MediaTab::Gallery => {
                if (54..=82).contains(&ly) {
                    if (24..=134).contains(&lx) {
                        self.backdrop = GalleryBackdrop::DarkGlass;
                        return true;
                    }
                    if (142..=256).contains(&lx) {
                        self.backdrop = GalleryBackdrop::WarmEmber;
                        return true;
                    }
                    if (264..=368).contains(&lx) {
                        self.backdrop = GalleryBackdrop::LightCard;
                        return true;
                    }
                }
            }
        }

        false
    }

    fn track_durations(&self) -> (u64, u64) {
        let notes = TRACKS[self.track_idx].notes;
        let total: u64 = notes.iter().map(|n| n.ms).sum();
        let elapsed: u64 = notes.iter().take(self.note_idx + 1).map(|n| n.ms).sum();
        (elapsed, total.max(1))
    }

    pub fn render(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        canvas.fill_rect(cx, cy, cw, ch, theme.window_bg);

        // Tab buttons
        let p_active = self.tab == MediaTab::Player;
        canvas.fill_rounded_rect(
            cx + 16,
            cy + 8,
            136,
            26,
            6,
            if p_active { theme.accent } else { theme.window_surface },
        );
        canvas.draw_text_font(
            cx + 36,
            cy + 12,
            "Music Player",
            if p_active { 0xFFFFFF } else { theme.text_secondary },
            FontFace::UiBold,
        );

        let g_active = self.tab == MediaTab::Gallery;
        canvas.fill_rounded_rect(
            cx + 160,
            cy + 8,
            110,
            26,
            6,
            if g_active { theme.accent } else { theme.window_surface },
        );
        canvas.draw_text_font(
            cx + 188,
            cy + 12,
            "Gallery",
            if g_active { 0xFFFFFF } else { theme.text_secondary },
            FontFace::UiBold,
        );

        match self.tab {
            MediaTab::Player => self.render_player(canvas, theme, cx, cy, cw, ch),
            MediaTab::Gallery => self.render_gallery(canvas, theme, cx, cy, cw, ch),
        }
    }

    fn render_player(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        let track = TRACKS[self.track_idx];
        let x = cx + 16;
        let w = cw - 32;

        // Now Playing Card
        canvas.fill_rounded_rect(x, cy + 44, w, 162, 10, theme.window_surface);
        canvas.draw_rounded_rect_outline(x, cy + 44, w, 162, 10, theme.border_inactive);

        canvas.draw_text_clipped_font(x + 14, cy + 54, w - 130, track.title, theme.text_primary, FontFace::UiBold);
        canvas.draw_text_clipped_font(
            x + 14,
            cy + 74,
            w - 130,
            &format!("{}  |  {} BPM", track.artist, track.bpm),
            theme.accent_soft,
            FontFace::Ui,
        );

        let status = if self.playing { "PLAYING" } else { "PAUSED" };
        let badge_col = if self.playing { theme.term_green } else { theme.text_muted };
        canvas.fill_rounded_rect(x + w - 82, cy + 54, 68, 22, 6, theme.term_bg);
        canvas.draw_text_font(x + w - 72, cy + 56, status, badge_col, FontFace::UiBold);

        // 16-bar Equalizer Visualizer
        let eq_x = x + 14;
        let eq_y = cy + 98;
        let eq_w = w - 28;
        let eq_h = 42;
        canvas.fill_rounded_rect(eq_x, eq_y, eq_w, eq_h, 6, theme.term_bg);

        let bar_slot = (eq_w - 16) / 16;
        let bar_w = (bar_slot - 4).max(4);
        for (i, &val) in self.bars.iter().enumerate() {
            let level = if self.playing { val } else { 14 };
            let bh = ((level as i32) * (eq_h - 8)) / 100;
            let bx = eq_x + 8 + (i as i32) * bar_slot;
            let col = if i % 3 == 0 {
                theme.accent
            } else if i % 3 == 1 {
                theme.accent_soft
            } else {
                theme.accent_sec
            };
            canvas.fill_rect(bx, eq_y + eq_h - 4 - bh, bar_w, bh, col);
        }

        // Progress bar
        let (elapsed_ms, total_ms) = self.track_durations();
        canvas.draw_progress_bar(
            eq_x,
            cy + 146,
            eq_w,
            10,
            elapsed_ms as usize,
            total_ms as usize,
            theme.term_bg,
            theme.border_inactive,
            theme.accent,
        );

        // Transport Controls
        let by = cy + 164;
        canvas.fill_rounded_rect(cx + 26, by, 46, 30, 7, theme.titlebar_active);
        canvas.draw_text_font(cx + 41, by + 6, "|<", theme.text_primary, FontFace::UiBold);

        canvas.fill_rounded_rect(cx + 80, by, 114, 30, 7, theme.accent);
        canvas.draw_text_font(
            cx + 114,
            by + 6,
            if self.playing { "Pause" } else { "Play" },
            0xFFFFFF,
            FontFace::UiBold,
        );

        canvas.fill_rounded_rect(cx + 202, by, 60, 30, 7, theme.titlebar_active);
        canvas.draw_text_font(cx + 216, by + 6, "Stop", theme.text_primary, FontFace::UiBold);

        canvas.fill_rounded_rect(cx + 270, by, 46, 30, 7, theme.titlebar_active);
        canvas.draw_text_font(cx + 285, by + 6, ">|", theme.text_primary, FontFace::UiBold);

        let muted = audio::is_muted();
        let mute_w = (cw - 352).max(86);
        canvas.fill_rounded_rect(cx + 328, by, mute_w, 30, 7, theme.titlebar_active);
        canvas.draw_text_clipped_font(
            cx + 338,
            by + 6,
            mute_w - 16,
            if muted { "Muted" } else { "Audio ON" },
            if muted { theme.err } else { theme.term_green },
            FontFace::UiBold,
        );

        // Playlist rows
        let mut py = cy + 224;
        for (idx, tr) in TRACKS.iter().enumerate() {
            if py + 32 > cy + ch - 6 {
                break;
            }
            let active = idx == self.track_idx;
            canvas.fill_rounded_rect(
                x,
                py,
                w,
                30,
                7,
                if active { theme.titlebar_active } else { theme.window_surface },
            );
            canvas.draw_rounded_rect_outline(
                x,
                py,
                w,
                30,
                7,
                if active { theme.accent } else { theme.border_inactive },
            );
            let secs: u64 = tr.notes.iter().map(|n| n.ms).sum::<u64>() / 1000;
            canvas.draw_text_clipped_font(
                x + 12,
                py + 6,
                w - 90,
                &format!("{}. {}  -  {}", idx + 1, tr.title, tr.artist),
                if active { theme.text_primary } else { theme.text_secondary },
                FontFace::UiBold,
            );
            canvas.draw_text_font(
                x + w - 54,
                py + 6,
                &format!("00:{:02}", secs),
                theme.text_muted,
                FontFace::Mono,
            );
            py += 36;
        }
    }

    fn render_gallery(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        let btns = [
            (24, 110, "Dark Glass", GalleryBackdrop::DarkGlass),
            (142, 114, "Warm Ember", GalleryBackdrop::WarmEmber),
            (264, 104, "Light Card", GalleryBackdrop::LightCard),
        ];
        for (bx, bw, label, kind) in btns {
            let act = self.backdrop == kind;
            canvas.fill_rounded_rect(
                cx + bx,
                cy + 52,
                bw,
                28,
                6,
                if act { theme.accent } else { theme.window_surface },
            );
            canvas.draw_text_font(
                cx + bx + 14,
                cy + 57,
                label,
                if act { 0xFFFFFF } else { theme.text_secondary },
                FontFace::UiBold,
            );
        }

        let card_bg = match self.backdrop {
            GalleryBackdrop::DarkGlass => 0x0B0E14,
            GalleryBackdrop::WarmEmber => 0x261409,
            GalleryBackdrop::LightCard => 0xF1F5F9,
        };
        let card_fg = match self.backdrop {
            GalleryBackdrop::LightCard => 0x1E293B,
            _ => theme.text_primary,
        };

        let px = cx + 24;
        let py = cy + 96;
        let pw = cw - 48;
        let ph = (ch - 116).max(180);

        canvas.fill_rounded_rect(px, py, pw, ph, 12, card_bg);
        canvas.draw_rounded_rect_outline(px, py, pw, ph, 12, theme.accent);

        let logo = assets::logo_120();
        canvas.draw_rgba_sprite(px + (pw - logo.width) / 2, py + 24, logo, 255);

        let cap = "AmbaOS Official Emblem (120x120 RGBA)";
        let tw = font::measure_text(FontFace::UiBold, cap);
        canvas.draw_text_font(px + (pw - tw) / 2, py + 158, cap, card_fg, FontFace::UiBold);

        let sub = "Source: Gemini_Generated_Image_9qs7cd9qs7cd9qs7.jpeg";
        let sw = font::measure_text(FontFace::Ui, sub);
        canvas.draw_text_clipped_font(
            px + ((pw - sw) / 2).max(12),
            py + 180,
            pw - 24,
            sub,
            theme.accent_soft,
            FontFace::Ui,
        );
    }
}
