use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use crate::drivers::audio;
use crate::drivers::keyboard::KeyEvent;
use crate::gui::assets;
use crate::gui::canvas::Canvas;
use crate::gui::font::{self, FontFace};
use crate::gui::theme::ThemeColors;
use crate::proc::compute;

pub const VIDEO_W: i32 = 236;
pub const VIDEO_H: i32 = 128;
pub const TOTAL_VIDEO_FRAMES: u32 = 180;

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

#[derive(Clone, Copy)]
pub struct VideoClip {
    pub title: &'static str,
    pub path: &'static str,
    pub codec: &'static str,
}

pub const VIDEO_CLIPS: [VideoClip; 3] = [
    VideoClip {
        title: "Racer3D (.exe) Gameplay Trailer",
        path: "/home/videos/racer3d_trailer.ambv",
        codec: "AMBV1 RLE565 | 30 FPS",
    },
    VideoClip {
        title: "AmbaOS v0.7.0 vNPU Architecture Reel",
        path: "mem://ambaos_vnpu_demo.ambv",
        codec: "AMBV1 Zero-Copy DMA",
    },
    VideoClip {
        title: "Space Invaders Arcade Preview",
        path: "mem://invaders_arcade.ambv",
        codec: "AMBV1 SIP Cartridge",
    },
];

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
    Video,
    Music,
    Gallery,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GalleryBackdrop {
    DarkGlass,
    WarmEmber,
    LightCard,
}

pub fn render_trailer_frame(
    buf: &mut Vec<u32>,
    w: i32,
    h: i32,
    clip_idx: usize,
    frame: u32,
    npu_hdr: bool,
) {
    let needed = (w.max(1) as usize) * (h.max(1) as usize);
    if buf.len() != needed {
        buf.resize(needed, 0);
    }
    let uw = w as usize;
    let t = frame as i32;

    match clip_idx % 3 {
        0 => {
            // Clip 0: Racer3D Sunset Highway Animation
            let horizon = (h * 44) / 100;
            for y in 0..h {
                let row = (y as usize) * uw;
                if y < horizon {
                    // Sunset sky gradient + sun + mountain ridge
                    let sky_t = (y * 255) / horizon.max(1);
                    let r = (24 + (sky_t * 195) / 255).min(255) as u32;
                    let g = (38 + (sky_t * 82) / 255).min(255) as u32;
                    let b = (78 + ((255 - sky_t) * 45) / 255).min(255) as u32;
                    let sky_col = (r << 16) | (g << 8) | b;
                    for x in 0..w {
                        let cx = x - (w / 2);
                        let sy = y - (horizon - 10);
                        let sun_r = (w / 9).max(8);
                        if cx * cx + sy * sy <= sun_r * sun_r {
                            buf[row + (x as usize)] = 0xFFAE34;
                        } else {
                            let ridge = horizon - ((cx.abs() % 37) / 3 + ((x + 17) % 19) / 3);
                            buf[row + (x as usize)] = if y >= ridge { 0x1E2331 } else { sky_col };
                        }
                    }
                } else {
                    let dy = y - horizon;
                    let depth = (dy * 100) / (h - horizon).max(1);
                    let road_half = (w * (16 + (depth * 42) / 100)) / 100;
                    let curve = ((t * 3 + dy) / 20) % 7 - 3;
                    let center = w / 2 + (curve * (100 - depth)) / 80;
                    let z_stripe = ((140 / (dy + 4)) + t) % 12 < 6;
                    for x in 0..w {
                        let dx = (x - center).abs();
                        let col = if dx <= road_half {
                            if dx < 2 && ((dy + t * 2) % 10 < 5) {
                                0xF8FAFC
                            } else if dx >= road_half - (road_half / 9).max(2) {
                                if z_stripe { 0xE67E22 } else { 0xCBD5E1 }
                            } else {
                                0x242A36
                            }
                        } else if z_stripe {
                            0x1B2E24
                        } else {
                            0x14221B
                        };
                        buf[row + (x as usize)] = col;
                    }
                }
            }
            // Orange Sports Car Silhouette in foreground
            let bob = ((t / 4) % 2) as i32;
            let car_w = (w * 28) / 100;
            let car_h = (h * 22) / 100;
            let car_x = (w - car_w) / 2 + (((t / 6) % 5) - 2);
            let car_y = h - car_h - (h / 12) + bob;
            for py in 0..car_h {
                let gy = car_y + py;
                if gy < 0 || gy >= h {
                    continue;
                }
                let inset = if py < car_h / 3 { car_w / 5 } else { 0 };
                for px in inset..(car_w - inset) {
                    let gx = car_x + px;
                    if gx < 0 || gx >= w {
                        continue;
                    }
                    let col = if py < car_h / 3 {
                        0x334155 // windshield
                    } else if py >= (car_h * 2) / 3 && (px < car_w / 4 || px > (car_w * 3) / 4) {
                        0xEF4444 // glowing taillights
                    } else if py >= car_h - 2 {
                        0x0F172A // wheels/shadow
                    } else {
                        0xF97316 // orange chassis
                    };
                    buf[(gy as usize) * uw + (gx as usize)] = col;
                }
            }
        }
        1 => {
            // Clip 1: AmbaOS v0.7.0 vNPU Telemetry & Plasma Wave
            for y in 0..h {
                let row = (y as usize) * uw;
                for x in 0..w {
                    let v1 = ((x * 5 + t * 4) & 63) - 32;
                    let v2 = ((y * 7 - t * 3) & 63) - 32;
                    let wave = (v1 * v1 + v2 * v2) / 12;
                    let bar_hit = ((x + t * 2) % 28 < 18) && (y > h / 4 && y < (h * 3) / 4) && (((x + y + t) % 13) == 0);
                    if bar_hit {
                        buf[row + (x as usize)] = 0x4ADE80;
                    } else {
                        let r = (24 + (wave.min(180))) as u32;
                        let g = (28 + ((wave / 2).min(110))) as u32;
                        let b = (44 + ((255 - wave.min(200)) / 3)) as u32;
                        buf[row + (x as usize)] = (r << 16) | (g << 8) | b;
                    }
                }
            }
        }
        _ => {
            // Clip 2: Space Invaders Arcade Stream
            buf.fill(0x090C12);
            for s in 0..24i32 {
                let sx = (s * 47 + 13) % w.max(1);
                let sy = (s * 31 + t * 2) % h.max(1);
                buf[(sy as usize) * uw + (sx as usize)] = 0x64748B;
            }
            const ALIEN: [&str; 8] = [
                "..#.....#..",
                "...#...#...",
                "..#######..",
                ".##.###.##.",
                "###########",
                "#.#######.#",
                "#.#.....#.#",
                "...##.##...",
            ];
            let scale = (w / 44).max(2);
            let ax0 = (w / 2 - (11 * scale) / 2) + (((t * 2) % 24) - 12);
            let ay0 = (h / 4) + (((t / 12) % 2) * 3);
            for (ry, row_str) in ALIEN.iter().enumerate() {
                for (rx, b) in row_str.bytes().enumerate() {
                    if b == b'#' {
                        for sy in 0..scale {
                            for sx in 0..scale {
                                let gx = ax0 + (rx as i32) * scale + sx;
                                let gy = ay0 + (ry as i32) * scale + sy;
                                if gx >= 0 && gx < w && gy >= 0 && gy < h {
                                    buf[(gy as usize) * uw + (gx as usize)] = 0x22C55E;
                                }
                            }
                        }
                    }
                }
            }
            // Laser bolt
            let lx = w / 2;
            let ly = (h - ((t * 6) % h.max(1))).clamp(0, h - 8);
            for py in 0..6 {
                if ly + py < h {
                    buf[((ly + py) as usize) * uw + (lx as usize)] = 0xF97316;
                }
            }
        }
    }

    if npu_hdr {
        for px in buf.iter_mut() {
            let r = (((*px >> 16) & 0xFF) * 115 / 100).min(255);
            let g = (((*px >> 8) & 0xFF) * 108 / 100).min(255);
            let b = (((*px) & 0xFF) * 105 / 100).min(255);
            *px = (r << 16) | (g << 8) | b;
        }
    }
}

pub struct MediaApp {
    tab: MediaTab,
    backdrop: GalleryBackdrop,
    // Video player state
    clip_idx: usize,
    video_frame: u32,
    video_playing: bool,
    npu_hdr: bool,
    last_video_ms: u64,
    frame_buf: Vec<u32>,
    // Music synth state
    track_idx: usize,
    note_idx: usize,
    playing: bool,
    last_note_ms: u64,
    last_anim_ms: u64,
    bars: [u8; 16],
}

impl MediaApp {
    pub fn new() -> Self {
        let mut frame_buf = vec![0u32; (VIDEO_W * VIDEO_H) as usize];
        render_trailer_frame(&mut frame_buf, VIDEO_W, VIDEO_H, 0, 12, false);
        Self {
            tab: MediaTab::Video,
            backdrop: GalleryBackdrop::DarkGlass,
            clip_idx: 0,
            video_frame: 12,
            video_playing: true,
            npu_hdr: true,
            last_video_ms: 0,
            frame_buf,
            track_idx: 0,
            note_idx: 0,
            playing: false,
            last_note_ms: 0,
            last_anim_ms: 0,
            bars: [22, 34, 48, 30, 56, 70, 44, 64, 52, 38, 60, 46, 32, 42, 28, 18],
        }
    }

    pub fn open_video_clip(&mut self, clip_idx: usize) {
        self.tab = MediaTab::Video;
        self.clip_idx = clip_idx % VIDEO_CLIPS.len();
        self.video_frame = 0;
        self.video_playing = true;
        render_trailer_frame(
            &mut self.frame_buf,
            VIDEO_W,
            VIDEO_H,
            self.clip_idx,
            self.video_frame,
            self.npu_hdr,
        );
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
        let mut redraw = false;

        if self.tab == MediaTab::Video && self.video_playing {
            if now_ms.saturating_sub(self.last_video_ms) >= 75 {
                self.last_video_ms = now_ms;
                self.video_frame = (self.video_frame + 1) % TOTAL_VIDEO_FRAMES;
                render_trailer_frame(
                    &mut self.frame_buf,
                    VIDEO_W,
                    VIDEO_H,
                    self.clip_idx,
                    self.video_frame,
                    self.npu_hdr,
                );
                redraw = true;
            }
        }

        if self.playing {
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
        }

        redraw
    }

    pub fn handle_key(&mut self, ev: KeyEvent, now_ms: u64) -> bool {
        match ev {
            KeyEvent::Char('1') => {
                self.tab = MediaTab::Video;
                return true;
            }
            KeyEvent::Char('2') => {
                self.tab = MediaTab::Music;
                return true;
            }
            KeyEvent::Char('3') => {
                self.tab = MediaTab::Gallery;
                return true;
            }
            KeyEvent::Tab => {
                self.tab = match self.tab {
                    MediaTab::Video => MediaTab::Music,
                    MediaTab::Music => MediaTab::Gallery,
                    MediaTab::Gallery => MediaTab::Video,
                };
                return true;
            }
            KeyEvent::Char(' ') => {
                match self.tab {
                    MediaTab::Video => self.video_playing = !self.video_playing,
                    MediaTab::Music => {
                        if self.playing {
                            self.playing = false;
                            audio::stop();
                        } else {
                            self.start_playing(now_ms);
                        }
                    }
                    MediaTab::Gallery => {
                        self.backdrop = match self.backdrop {
                            GalleryBackdrop::DarkGlass => GalleryBackdrop::WarmEmber,
                            GalleryBackdrop::WarmEmber => GalleryBackdrop::LightCard,
                            GalleryBackdrop::LightCard => GalleryBackdrop::DarkGlass,
                        };
                    }
                }
                return true;
            }
            KeyEvent::Left => {
                match self.tab {
                    MediaTab::Video => {
                        self.clip_idx = (self.clip_idx + VIDEO_CLIPS.len() - 1) % VIDEO_CLIPS.len();
                        self.video_frame = 0;
                    }
                    MediaTab::Music => {
                        self.track_idx = (self.track_idx + TRACKS.len() - 1) % TRACKS.len();
                        self.note_idx = 0;
                        if self.playing {
                            self.start_playing(now_ms);
                        }
                    }
                    MediaTab::Gallery => {}
                }
                return true;
            }
            KeyEvent::Right => {
                match self.tab {
                    MediaTab::Video => {
                        self.clip_idx = (self.clip_idx + 1) % VIDEO_CLIPS.len();
                        self.video_frame = 0;
                    }
                    MediaTab::Music => {
                        self.track_idx = (self.track_idx + 1) % TRACKS.len();
                        self.note_idx = 0;
                        if self.playing {
                            self.start_playing(now_ms);
                        }
                    }
                    MediaTab::Gallery => {}
                }
                return true;
            }
            KeyEvent::Char('m') | KeyEvent::Char('M') => {
                audio::set_muted(!audio::is_muted());
                if self.playing && !audio::is_muted() {
                    let f = TRACKS[self.track_idx].notes[self.note_idx].freq;
                    audio::play_freq(f);
                }
                return true;
            }
            KeyEvent::Char('h') | KeyEvent::Char('H') if self.tab == MediaTab::Video => {
                self.npu_hdr = !self.npu_hdr;
                if self.npu_hdr {
                    let _ = compute::schedule_npu_dag();
                }
                render_trailer_frame(
                    &mut self.frame_buf,
                    VIDEO_W,
                    VIDEO_H,
                    self.clip_idx,
                    self.video_frame,
                    self.npu_hdr,
                );
                return true;
            }
            _ => {}
        }
        false
    }

    pub fn handle_click(&mut self, lx: i32, ly: i32, cw: i32, now_ms: u64) -> bool {
        if (8..=34).contains(&ly) {
            if (16..=152).contains(&lx) {
                self.tab = MediaTab::Video;
                return true;
            }
            if (160..=292).contains(&lx) {
                self.tab = MediaTab::Music;
                return true;
            }
            if (300..=406).contains(&lx) {
                self.tab = MediaTab::Gallery;
                return true;
            }
        }

        match self.tab {
            MediaTab::Video => {
                // Seek timeline bar (y: 222..240)
                if (220..=242).contains(&ly) && lx >= 24 && lx <= cw - 24 {
                    let span = (cw - 48).max(1);
                    let pct = (((lx - 24) as u32) * TOTAL_VIDEO_FRAMES) / (span as u32);
                    self.video_frame = pct.min(TOTAL_VIDEO_FRAMES - 1);
                    render_trailer_frame(
                        &mut self.frame_buf,
                        VIDEO_W,
                        VIDEO_H,
                        self.clip_idx,
                        self.video_frame,
                        self.npu_hdr,
                    );
                    return true;
                }
                // Video transport buttons (y: 248..278)
                if (248..=278).contains(&ly) {
                    if (24..=74).contains(&lx) {
                        self.clip_idx = (self.clip_idx + VIDEO_CLIPS.len() - 1) % VIDEO_CLIPS.len();
                        self.video_frame = 0;
                        return true;
                    }
                    if (82..=180).contains(&lx) {
                        self.video_playing = !self.video_playing;
                        return true;
                    }
                    if (188..=238).contains(&lx) {
                        self.clip_idx = (self.clip_idx + 1) % VIDEO_CLIPS.len();
                        self.video_frame = 0;
                        return true;
                    }
                    if (248..=cw - 24).contains(&lx) {
                        self.npu_hdr = !self.npu_hdr;
                        if self.npu_hdr {
                            let _ = compute::schedule_npu_dag();
                        }
                        render_trailer_frame(
                            &mut self.frame_buf,
                            VIDEO_W,
                            VIDEO_H,
                            self.clip_idx,
                            self.video_frame,
                            self.npu_hdr,
                        );
                        return true;
                    }
                }
                // Video clip list (y >= 294)
                if lx >= 16 && lx <= cw - 16 && ly >= 294 {
                    let row = ((ly - 294) / 36) as usize;
                    if row < VIDEO_CLIPS.len() {
                        self.open_video_clip(row);
                        return true;
                    }
                }
            }
            MediaTab::Music => {
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

        let tabs = [
            (16, 136, "Video Player", MediaTab::Video),
            (160, 132, "Music Synth", MediaTab::Music),
            (300, 106, "Gallery", MediaTab::Gallery),
        ];
        for (bx, bw, lbl, t) in tabs {
            let act = self.tab == t;
            canvas.fill_rounded_rect(
                cx + bx,
                cy + 8,
                bw,
                26,
                6,
                if act { theme.accent } else { theme.window_surface },
            );
            canvas.draw_text_font(
                cx + bx + 18,
                cy + 12,
                lbl,
                if act { 0xFFFFFF } else { theme.text_secondary },
                FontFace::UiBold,
            );
        }

        match self.tab {
            MediaTab::Video => self.render_video(canvas, theme, cx, cy, cw, ch),
            MediaTab::Music => self.render_player(canvas, theme, cx, cy, cw, ch),
            MediaTab::Gallery => self.render_gallery(canvas, theme, cx, cy, cw, ch),
        }
    }

    fn render_video(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        let clip = VIDEO_CLIPS[self.clip_idx];
        let x = cx + 16;
        let w = cw - 32;

        // Main Video Stage Card
        canvas.fill_rounded_rect(x, cy + 44, w, 240, 10, theme.window_surface);
        canvas.draw_rounded_rect_outline(x, cy + 44, w, 240, 10, theme.border_inactive);

        canvas.draw_text_clipped_font(x + 14, cy + 49, w - 170, clip.title, theme.text_primary, FontFace::UiBold);
        canvas.draw_text_clipped_font(x + 14, cy + 65, w - 170, clip.path, theme.text_muted, FontFace::Mono);
        let badge = if self.video_playing { "AMBV1 30FPS" } else { "PAUSED" };
        let bcol = if self.video_playing { theme.term_green } else { theme.text_muted };
        canvas.fill_rounded_rect(x + w - 116, cy + 49, 104, 22, 6, theme.term_bg);
        canvas.draw_text_font(x + w - 106, cy + 51, badge, bcol, FontFace::UiBold);

        // Video Viewport (236x128 centered)
        let vx = x + (w - VIDEO_W) / 2;
        let vy = cy + 87;
        canvas.fill_rounded_rect(vx - 3, vy - 3, VIDEO_W + 6, VIDEO_H + 6, 8, 0x0B0E14);
        canvas.blit_rgb_u32(vx, vy, VIDEO_W, VIDEO_H, VIDEO_W, VIDEO_H, &self.frame_buf);
        canvas.draw_rounded_rect_outline(vx - 1, vy - 1, VIDEO_W + 2, VIDEO_H + 2, 4, theme.border_inactive);

        // Seek Timeline Bar
        let tx = cx + 24;
        let ty = cy + 227;
        let tw = cw - 48;
        canvas.draw_progress_bar(
            tx,
            ty,
            tw,
            10,
            self.video_frame as usize,
            TOTAL_VIDEO_FRAMES as usize,
            theme.term_bg,
            theme.border_inactive,
            theme.accent,
        );
        let knob_x = tx + (((self.video_frame as i32) * (tw - 8)) / (TOTAL_VIDEO_FRAMES as i32));
        canvas.fill_circle(knob_x + 4, ty + 5, 6, 0xFFFFFF);

        // Controls Row
        let by = cy + 246;
        canvas.fill_rounded_rect(cx + 24, by, 50, 30, 7, theme.titlebar_active);
        canvas.draw_text_font(cx + 41, by + 6, "|<", theme.text_primary, FontFace::UiBold);

        canvas.fill_rounded_rect(cx + 82, by, 98, 30, 7, theme.accent);
        canvas.draw_text_font(
            cx + 110,
            by + 6,
            if self.video_playing { "Pause" } else { "Play" },
            0xFFFFFF,
            FontFace::UiBold,
        );

        canvas.fill_rounded_rect(cx + 188, by, 50, 30, 7, theme.titlebar_active);
        canvas.draw_text_font(cx + 205, by + 6, ">|", theme.text_primary, FontFace::UiBold);

        let npu_w = (cw - 272).max(120);
        canvas.fill_rounded_rect(
            cx + 248,
            by,
            npu_w,
            30,
            7,
            if self.npu_hdr { 0x1E3A2F } else { theme.titlebar_active },
        );
        canvas.draw_text_clipped_font(
            cx + 260,
            by + 6,
            npu_w - 20,
            if self.npu_hdr { "vNPU HDR: ON" } else { "vNPU HDR: OFF" },
            if self.npu_hdr { theme.term_green } else { theme.text_secondary },
            FontFace::UiBold,
        );

        // Video playlist rows
        let mut py = cy + 294;
        for (idx, vc) in VIDEO_CLIPS.iter().enumerate() {
            if py + 32 > cy + ch - 6 {
                break;
            }
            let act = idx == self.clip_idx;
            canvas.fill_rounded_rect(
                x,
                py,
                w,
                30,
                7,
                if act { theme.titlebar_active } else { theme.window_surface },
            );
            canvas.draw_rounded_rect_outline(
                x,
                py,
                w,
                30,
                7,
                if act { theme.accent } else { theme.border_inactive },
            );
            canvas.draw_text_clipped_font(
                x + 12,
                py + 6,
                w - 178,
                &format!("{}. {}", idx + 1, vc.title),
                if act { theme.text_primary } else { theme.text_secondary },
                FontFace::UiBold,
            );
            canvas.draw_text_clipped_font(
                x + w - 160,
                py + 6,
                150,
                vc.codec,
                theme.accent_soft,
                FontFace::Mono,
            );
            py += 36;
        }
    }

    fn render_player(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        let track = TRACKS[self.track_idx];
        let x = cx + 16;
        let w = cw - 32;

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

        let sub = "Zero-Trust SIP + PE32+ Engine v0.7.0";
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
