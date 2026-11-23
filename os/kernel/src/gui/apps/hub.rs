use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::drivers::keyboard::{self, KeyEvent};
use crate::gui::apps::media;
use crate::gui::canvas::Canvas;
use crate::gui::font::{self, FontFace};
use crate::gui::theme::ThemeColors;
use crate::proc::loader;

const PREVIEW_W: i32 = 96;
const PREVIEW_H: i32 = 54;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HubTab {
    Store,
    Library,
    Downloads,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ActiveGame {
    None,
    Racer3D,
    Blocks,
    Invaders,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HubAction {
    None,
    OpenDoom,
    OpenMedia,
}

struct SetupWizard {
    active: bool,
    pkg_idx: usize,
    progress: u8,
    done: bool,
    status: String,
}

struct CatalogGame {
    id: &'static str,
    title: &'static str,
    bin_type: &'static str,
    size_str: &'static str,
    is_elf: bool,
}

const CATALOG: [CatalogGame; 4] = [
    CatalogGame {
        id: "racer3d",
        title: "Racer3D (.exe)",
        bin_type: "PE32+ Win32 GUI",
        size_str: "14.2 MB",
        is_elf: false,
    },
    CatalogGame {
        id: "blocks",
        title: "Blocks",
        bin_type: "PE32+ Arcade",
        size_str: "4.8 MB",
        is_elf: false,
    },
    CatalogGame {
        id: "invaders",
        title: "Invaders Setup",
        bin_type: "PE32+ Installer",
        size_str: "6.4 MB",
        is_elf: false,
    },
    CatalogGame {
        id: "doom",
        title: "Doom3D (.exe)",
        bin_type: "ELF64 / Raycaster",
        size_str: "18.5 MB",
        is_elf: true,
    },
];

pub struct HubApp {
    tab: HubTab,
    active_game: ActiveGame,
    dl_pct: u8,
    dl_active: bool,
    last_dl_ms: u64,
    wizard: SetupWizard,
    banner_msg: String,
    // Embedded .ambv trailer preview
    trailer_clip: usize,
    trailer_frame: u32,
    trailer_playing: bool,
    last_trailer_ms: u64,
    trailer_buf: Vec<u32>,
    // Racer3D mini-game state
    racer_car_x: i32,
    racer_tick: u32,
    racer_speed: i32,
    racer_score: u32,
    racer_obs_x: i32,
    racer_obs_y: i32,
    // Blocks (Tetris) state
    blk_grid: [[u8; 10]; 14],
    blk_px: i32,
    blk_py: i32,
    blk_kind: usize,
    blk_rot: usize,
    blk_score: u32,
    blk_tick: u64,
    // Space Invaders state
    inv_ship_x: i32,
    inv_aliens: [bool; 18],
    inv_ax: i32,
    inv_dir: i32,
    inv_shot_x: i32,
    inv_shot_y: i32,
    inv_score: u32,
    inv_tick: u64,
}

impl HubApp {
    pub fn new() -> Self {
        let mut trailer_buf = vec![0u32; (PREVIEW_W * PREVIEW_H) as usize];
        media::render_trailer_frame(&mut trailer_buf, PREVIEW_W, PREVIEW_H, 0, 56, false);
        Self {
            tab: HubTab::Store,
            active_game: ActiveGame::None,
            dl_pct: 82,
            dl_active: false,
            last_dl_ms: 0,
            wizard: SetupWizard {
                active: false,
                pkg_idx: 2,
                progress: 0,
                done: false,
                status: String::new(),
            },
            banner_msg: String::new(),
            trailer_clip: 0,
            trailer_frame: 56,
            trailer_playing: false,
            last_trailer_ms: 0,
            trailer_buf,
            racer_car_x: 190,
            racer_tick: 0,
            racer_speed: 145,
            racer_score: 0,
            racer_obs_x: 150,
            racer_obs_y: -30,
            blk_grid: [[0; 10]; 14],
            blk_px: 4,
            blk_py: 0,
            blk_kind: 0,
            blk_rot: 0,
            blk_score: 0,
            blk_tick: 0,
            inv_ship_x: 210,
            inv_aliens: [true; 18],
            inv_ax: 40,
            inv_dir: 1,
            inv_shot_x: -1,
            inv_shot_y: -1,
            inv_score: 0,
            inv_tick: 0,
        }
    }

    pub fn open_install_wizard(&mut self, pkg_idx: usize) {
        let idx = pkg_idx.min(CATALOG.len() - 1);
        self.tab = HubTab::Store;
        self.active_game = ActiveGame::None;
        self.wizard = SetupWizard {
            active: true,
            pkg_idx: idx,
            progress: 15,
            done: false,
            status: format!("Loaded PE32+/ELF64 headers for {}", CATALOG[idx].title),
        };
    }

    pub fn launch_by_id(&mut self, id: &str) -> HubAction {
        match id {
            "racer3d" => {
                self.start_racer3d();
                HubAction::None
            }
            "blocks" => {
                self.start_blocks();
                HubAction::None
            }
            "invaders" => {
                if loader::app_installed("invaders") {
                    self.start_invaders();
                } else {
                    self.open_install_wizard(2);
                }
                HubAction::None
            }
            "doom" => HubAction::OpenDoom,
            _ => HubAction::None,
        }
    }

    fn start_racer3d(&mut self) {
        self.active_game = ActiveGame::Racer3D;
        self.racer_car_x = 190;
        self.racer_speed = 152;
        self.racer_score = 0;
        self.racer_obs_x = 140;
        self.racer_obs_y = 0;
    }

    fn start_blocks(&mut self) {
        self.active_game = ActiveGame::Blocks;
        self.blk_grid = [[0; 10]; 14];
        self.blk_px = 4;
        self.blk_py = 0;
        self.blk_kind = 0;
        self.blk_rot = 0;
        self.blk_score = 0;
    }

    fn start_invaders(&mut self) {
        self.active_game = ActiveGame::Invaders;
        self.inv_ship_x = 210;
        self.inv_aliens = [true; 18];
        self.inv_ax = 44;
        self.inv_dir = 1;
        self.inv_shot_x = -1;
        self.inv_shot_y = -1;
        self.inv_score = 0;
    }

    pub fn handle_menu_key(&mut self, ev: KeyEvent) -> (bool, HubAction) {
        if self.active_game != ActiveGame::None {
            return (self.handle_key(ev), HubAction::None);
        }
        if self.wizard.active {
            match ev {
                KeyEvent::Escape => {
                    self.wizard.active = false;
                    return (true, HubAction::None);
                }
                KeyEvent::Enter => {
                    if !self.wizard.done {
                        self.wizard.progress = 99;
                        return (true, HubAction::None);
                    } else {
                        let id = CATALOG[self.wizard.pkg_idx].id;
                        self.wizard.active = false;
                        return (true, self.launch_by_id(id));
                    }
                }
                _ => return (false, HubAction::None),
            }
        }
        match ev {
            KeyEvent::Char('1') => {
                self.tab = HubTab::Store;
                (true, HubAction::None)
            }
            KeyEvent::Char('2') => {
                self.tab = HubTab::Library;
                (true, HubAction::None)
            }
            KeyEvent::Char('3') => {
                self.tab = HubTab::Downloads;
                (true, HubAction::None)
            }
            KeyEvent::Tab => {
                self.tab = match self.tab {
                    HubTab::Store => HubTab::Library,
                    HubTab::Library => HubTab::Downloads,
                    HubTab::Downloads => HubTab::Store,
                };
                (true, HubAction::None)
            }
            _ => (false, HubAction::None),
        }
    }

    pub fn handle_key(&mut self, ev: KeyEvent) -> bool {
        match self.active_game {
            ActiveGame::None => false,
            ActiveGame::Racer3D => {
                match ev {
                    KeyEvent::Escape => self.active_game = ActiveGame::None,
                    KeyEvent::Left | KeyEvent::Char('a') | KeyEvent::Char('A') => {
                        self.racer_car_x = (self.racer_car_x - 18).clamp(80, 320);
                    }
                    KeyEvent::Right | KeyEvent::Char('d') | KeyEvent::Char('D') => {
                        self.racer_car_x = (self.racer_car_x + 18).clamp(80, 320);
                    }
                    KeyEvent::Up | KeyEvent::Char('w') | KeyEvent::Char('W') => {
                        self.racer_speed = (self.racer_speed + 12).min(240);
                    }
                    KeyEvent::Down | KeyEvent::Char('s') | KeyEvent::Char('S') => {
                        self.racer_speed = (self.racer_speed - 12).max(80);
                    }
                    _ => return false,
                }
                true
            }
            ActiveGame::Blocks => {
                match ev {
                    KeyEvent::Escape => self.active_game = ActiveGame::None,
                    KeyEvent::Left | KeyEvent::Char('a') | KeyEvent::Char('A') => {
                        if self.blocks_fits(self.blk_px - 1, self.blk_py, self.blk_rot) {
                            self.blk_px -= 1;
                        }
                    }
                    KeyEvent::Right | KeyEvent::Char('d') | KeyEvent::Char('D') => {
                        if self.blocks_fits(self.blk_px + 1, self.blk_py, self.blk_rot) {
                            self.blk_px += 1;
                        }
                    }
                    KeyEvent::Up | KeyEvent::Char('w') | KeyEvent::Char('W') => {
                        let nr = (self.blk_rot + 1) % 4;
                        if self.blocks_fits(self.blk_px, self.blk_py, nr) {
                            self.blk_rot = nr;
                        }
                    }
                    KeyEvent::Down | KeyEvent::Char('s') | KeyEvent::Char('S') => {
                        self.blocks_step();
                    }
                    KeyEvent::Char(' ') => {
                        while self.blocks_fits(self.blk_px, self.blk_py + 1, self.blk_rot) {
                            self.blk_py += 1;
                        }
                        self.blocks_step();
                    }
                    _ => return false,
                }
                true
            }
            ActiveGame::Invaders => {
                match ev {
                    KeyEvent::Escape => self.active_game = ActiveGame::None,
                    KeyEvent::Left | KeyEvent::Char('a') | KeyEvent::Char('A') => {
                        self.inv_ship_x = (self.inv_ship_x - 16).clamp(40, 390);
                    }
                    KeyEvent::Right | KeyEvent::Char('d') | KeyEvent::Char('D') => {
                        self.inv_ship_x = (self.inv_ship_x + 16).clamp(40, 390);
                    }
                    KeyEvent::Char(' ') | KeyEvent::Up | KeyEvent::Char('w') | KeyEvent::Char('W') => {
                        if self.inv_shot_y < 0 {
                            self.inv_shot_x = self.inv_ship_x;
                            self.inv_shot_y = 330;
                        }
                    }
                    _ => return false,
                }
                true
            }
        }
    }

    fn piece_cells(kind: usize, rot: usize) -> [(i32, i32); 4] {
        let base = match kind % 4 {
            0 => [(0, 0), (1, 0), (0, 1), (1, 1)],   // O-block
            1 => [(-1, 0), (0, 0), (1, 0), (2, 0)],  // I-block
            2 => [(-1, 0), (0, 0), (1, 0), (0, 1)],  // T-block
            _ => [(-1, 0), (0, 0), (1, 0), (1, 1)],  // L-block
        };
        if kind % 4 == 0 {
            return base;
        }
        let mut out = [(0, 0); 4];
        for (i, (mut x, mut y)) in base.iter().copied().enumerate() {
            for _ in 0..(rot % 4) {
                let nx = -y;
                let ny = x;
                x = nx;
                y = ny;
            }
            out[i] = (x, y);
        }
        out
    }

    fn blocks_fits(&self, px: i32, py: i32, rot: usize) -> bool {
        for (dx, dy) in Self::piece_cells(self.blk_kind, rot) {
            let x = px + dx;
            let y = py + dy;
            if x < 0 || x >= 10 || y >= 14 {
                return false;
            }
            if y >= 0 && self.blk_grid[y as usize][x as usize] != 0 {
                return false;
            }
        }
        true
    }

    fn blocks_step(&mut self) {
        if self.blocks_fits(self.blk_px, self.blk_py + 1, self.blk_rot) {
            self.blk_py += 1;
            return;
        }
        let color_idx = ((self.blk_kind % 4) + 1) as u8;
        for (dx, dy) in Self::piece_cells(self.blk_kind, self.blk_rot) {
            let x = self.blk_px + dx;
            let y = self.blk_py + dy;
            if (0..10).contains(&x) && (0..14).contains(&y) {
                self.blk_grid[y as usize][x as usize] = color_idx;
            }
        }
        let mut r = 13i32;
        while r >= 0 {
            if self.blk_grid[r as usize].iter().all(|&c| c != 0) {
                self.blk_score += 100;
                for rr in (1..=r as usize).rev() {
                    self.blk_grid[rr] = self.blk_grid[rr - 1];
                }
                self.blk_grid[0] = [0; 10];
            } else {
                r -= 1;
            }
        }
        self.blk_kind = (self.blk_kind + 1) % 4;
        self.blk_rot = 0;
        self.blk_px = 4;
        self.blk_py = 0;
        if !self.blocks_fits(self.blk_px, self.blk_py, 0) {
            self.blk_grid = [[0; 10]; 14];
            self.blk_score = 0;
        }
    }

    pub fn tick(&mut self, now_ms: u64) -> bool {
        let mut dirty = false;

        if self.wizard.active && !self.wizard.done && self.wizard.progress < 100 {
            if now_ms % 60 < 20 {
                self.wizard.progress = (self.wizard.progress + 17).min(100);
                if self.wizard.progress >= 100 {
                    let pkg = &CATALOG[self.wizard.pkg_idx];
                    match loader::install_package(pkg.id, pkg.title, pkg.is_elf) {
                        Ok((path, snap)) => {
                            self.wizard.done = true;
                            self.wizard.status = format!("Installed {} (Snapshot #{})", path, snap);
                            self.banner_msg = format!("{} ready in ATA Disk!", pkg.title);
                            if pkg.id == "invaders" {
                                self.dl_pct = 100;
                                self.dl_active = false;
                            }
                        }
                        Err(e) => {
                            self.wizard.done = true;
                            self.wizard.status = format!("Error: {}", e);
                        }
                    }
                }
                dirty = true;
            }
        }

        if self.dl_active && self.dl_pct < 100 {
            if now_ms.saturating_sub(self.last_dl_ms) >= 90 {
                self.last_dl_ms = now_ms;
                self.dl_pct = (self.dl_pct + 6).min(100);
                if self.dl_pct >= 100 {
                    self.dl_active = false;
                    if !loader::app_installed("invaders") {
                        let _ = loader::install_package("invaders", "Invaders Setup", false);
                    }
                    self.banner_msg = String::from("invaders_setup.exe installed to /apps/invaders/");
                }
                dirty = true;
            }
        }

        if self.trailer_playing && now_ms.saturating_sub(self.last_trailer_ms) >= 90 {
            self.last_trailer_ms = now_ms;
            self.trailer_frame = (self.trailer_frame + 2) % media::TOTAL_VIDEO_FRAMES;
            media::render_trailer_frame(
                &mut self.trailer_buf,
                PREVIEW_W,
                PREVIEW_H,
                self.trailer_clip,
                self.trailer_frame,
                false,
            );
            dirty = true;
        }

        match self.active_game {
            ActiveGame::None => {}
            ActiveGame::Racer3D => {
                let held = keyboard::held_mask();
                if (held & (keyboard::KEY_A | keyboard::KEY_LEFT)) != 0 {
                    self.racer_car_x = (self.racer_car_x - 4).clamp(80, 320);
                }
                if (held & (keyboard::KEY_D | keyboard::KEY_RIGHT)) != 0 {
                    self.racer_car_x = (self.racer_car_x + 4).clamp(80, 320);
                }
                if (held & (keyboard::KEY_W | keyboard::KEY_UP)) != 0 {
                    self.racer_speed = (self.racer_speed + 2).min(240);
                }
                self.racer_tick = self.racer_tick.wrapping_add(1);
                self.racer_score = self.racer_score.wrapping_add((self.racer_speed / 30) as u32);
                self.racer_obs_y += 6 + (self.racer_speed / 50);
                if self.racer_obs_y > 320 {
                    self.racer_obs_y = 0;
                    self.racer_obs_x = 120 + ((self.racer_tick as i32 * 53) % 160);
                }
                if (self.racer_obs_y - 220).abs() < 26 && (self.racer_obs_x - self.racer_car_x).abs() < 34 {
                    self.racer_speed = 90;
                    self.racer_score = self.racer_score.saturating_sub(50);
                    self.racer_obs_y = 0;
                }
                dirty = true;
            }
            ActiveGame::Blocks => {
                if now_ms.saturating_sub(self.blk_tick) >= 420 {
                    self.blk_tick = now_ms;
                    self.blocks_step();
                    dirty = true;
                }
            }
            ActiveGame::Invaders => {
                let held = keyboard::held_mask();
                if (held & (keyboard::KEY_A | keyboard::KEY_LEFT)) != 0 {
                    self.inv_ship_x = (self.inv_ship_x - 5).clamp(40, 390);
                }
                if (held & (keyboard::KEY_D | keyboard::KEY_RIGHT)) != 0 {
                    self.inv_ship_x = (self.inv_ship_x + 5).clamp(40, 390);
                }
                if self.inv_shot_y >= 0 {
                    self.inv_shot_y -= 14;
                    for (i, alive) in self.inv_aliens.iter_mut().enumerate() {
                        if !*alive {
                            continue;
                        }
                        let col = (i % 6) as i32;
                        let row = (i / 6) as i32;
                        let ax = self.inv_ax + col * 52;
                        let ay = 82 + row * 38;
                        if (self.inv_shot_x - ax).abs() < 18 && (self.inv_shot_y - ay).abs() < 14 {
                            *alive = false;
                            self.inv_shot_y = -1;
                            self.inv_score += 50;
                            break;
                        }
                    }
                    if self.inv_shot_y < 60 {
                        self.inv_shot_y = -1;
                    }
                }
                if now_ms.saturating_sub(self.inv_tick) >= 120 {
                    self.inv_tick = now_ms;
                    self.inv_ax += self.inv_dir * 6;
                    if self.inv_ax >= 130 || self.inv_ax <= 24 {
                        self.inv_dir = -self.inv_dir;
                    }
                    if self.inv_aliens.iter().all(|&a| !a) {
                        self.inv_aliens = [true; 18];
                    }
                }
                dirty = true;
            }
        }

        dirty
    }

    pub fn handle_click(&mut self, lx: i32, ly: i32, cw: i32, ch: i32) -> (bool, HubAction) {
        if self.active_game != ActiveGame::None {
            if (10..=120).contains(&lx) && (10..=38).contains(&ly) {
                self.active_game = ActiveGame::None;
                return (true, HubAction::None);
            }
            return (false, HubAction::None);
        }

        if self.wizard.active {
            let wx = 44;
            let wy = (ch - 220) / 2;
            let ww = (cw - 88).max(260);
            if ly >= wy + 162 && ly <= wy + 196 {
                if lx >= wx + 20 && lx <= wx + ww / 2 - 6 {
                    if !self.wizard.done {
                        self.wizard.progress = 99;
                    } else {
                        let id = CATALOG[self.wizard.pkg_idx].id;
                        self.wizard.active = false;
                        return (true, self.launch_by_id(id));
                    }
                    return (true, HubAction::None);
                }
                if lx >= wx + ww / 2 + 6 && lx <= wx + ww - 20 {
                    self.wizard.active = false;
                    return (true, HubAction::None);
                }
            }
            return (true, HubAction::None);
        }

        // Top tabs (y: 0..38)
        if ly >= 0 && ly <= 38 {
            let third = cw / 3;
            if lx < third {
                self.tab = HubTab::Store;
                return (true, HubAction::None);
            }
            if lx < third * 2 {
                self.tab = HubTab::Library;
                return (true, HubAction::None);
            }
            self.tab = HubTab::Downloads;
            return (true, HubAction::None);
        }

        // Bottom .ambv Trailer Preview Bar (y: ch - 78 .. ch - 10)
        let bar_y = ch - 76;
        if ly >= bar_y && ly <= ch - 8 && lx >= 14 && lx <= cw - 14 {
            // Left thumbnail click -> play/pause preview or open full AmbaMedia
            if lx <= 122 {
                self.trailer_playing = !self.trailer_playing;
                return (true, HubAction::None);
            }
            // Transport Pause / Next buttons
            if ly >= bar_y + 26 && ly <= bar_y + 52 {
                if (128..=156).contains(&lx) {
                    self.trailer_playing = !self.trailer_playing;
                    return (true, HubAction::None);
                }
                if (160..=188).contains(&lx) {
                    self.trailer_clip = (self.trailer_clip + 1) % 3;
                    self.trailer_frame = 0;
                    media::render_trailer_frame(
                        &mut self.trailer_buf,
                        PREVIEW_W,
                        PREVIEW_H,
                        self.trailer_clip,
                        self.trailer_frame,
                        false,
                    );
                    return (true, HubAction::None);
                }
                // Seek slider
                if lx >= 196 && lx <= cw - 28 {
                    let pct = (((lx - 196) as u32) * media::TOTAL_VIDEO_FRAMES) / ((cw - 224).max(1) as u32);
                    self.trailer_frame = pct.min(media::TOTAL_VIDEO_FRAMES - 1);
                    media::render_trailer_frame(
                        &mut self.trailer_buf,
                        PREVIEW_W,
                        PREVIEW_H,
                        self.trailer_clip,
                        self.trailer_frame,
                        false,
                    );
                    return (true, HubAction::None);
                }
            }
            // Open full AmbaMedia Video Player if title clicked
            if ly < bar_y + 26 && lx > 128 {
                return (true, HubAction::OpenMedia);
            }
            return (true, HubAction::None);
        }

        match self.tab {
            HubTab::Store => {
                // Row 1 cards: y = 46..244
                let card_w = (cw - 44) / 3;
                if (46..=244).contains(&ly) {
                    for col in 0..3 {
                        let cx0 = 14 + (col as i32) * (card_w + 8);
                        if lx >= cx0 && lx <= cx0 + card_w {
                            match col {
                                0 => {
                                    if loader::app_installed("racer3d") {
                                        self.start_racer3d();
                                    } else {
                                        self.open_install_wizard(0);
                                    }
                                    return (true, HubAction::None);
                                }
                                1 => {
                                    if loader::app_installed("blocks") {
                                        self.start_blocks();
                                    } else {
                                        self.open_install_wizard(1);
                                    }
                                    return (true, HubAction::None);
                                }
                                _ => {
                                    if loader::app_installed("invaders") {
                                        self.start_invaders();
                                    } else {
                                        self.open_install_wizard(2);
                                    }
                                    return (true, HubAction::None);
                                }
                            }
                        }
                    }
                }
                // Row 2 card 4 (Doom3D) & Install Custom .exe button
                if (254..=420).contains(&ly) {
                    if lx >= 14 && lx <= 14 + card_w {
                        return (true, HubAction::OpenDoom);
                    }
                    if lx >= 14 + card_w + 12 && lx <= cw - 14 {
                        self.open_install_wizard(0);
                        return (true, HubAction::None);
                    }
                }
            }
            HubTab::Library => {
                for (i, pkg) in CATALOG.iter().enumerate() {
                    let ry = 52 + (i as i32) * 78;
                    if ly >= ry && ly <= ry + 68 {
                        if lx >= cw - 196 && lx <= cw - 110 {
                            return (true, self.launch_by_id(pkg.id));
                        }
                        if lx >= cw - 102 && lx <= cw - 24 {
                            if loader::app_installed(pkg.id) {
                                let _ = loader::uninstall_package(pkg.id);
                                self.banner_msg = format!("Removed /apps/{}/", pkg.id);
                            } else {
                                self.open_install_wizard(i);
                            }
                            return (true, HubAction::None);
                        }
                    }
                }
            }
            HubTab::Downloads => {
                if ly >= 56 && ly <= 220 {
                    if !self.dl_active && self.dl_pct < 100 {
                        self.dl_active = true;
                    } else {
                        self.open_install_wizard(2);
                    }
                    return (true, HubAction::None);
                }
            }
        }

        (false, HubAction::None)
    }

    pub fn render(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        canvas.fill_rect(cx, cy, cw, ch, theme.window_bg);

        if self.active_game != ActiveGame::None {
            self.render_active_game(canvas, theme, cx, cy, cw, ch);
            return;
        }

        // Clean top tab bar with white active underline
        canvas.fill_rect(cx, cy, cw, 38, theme.titlebar_inactive);
        canvas.fill_rect(cx, cy + 38, cw, 1, theme.border_inactive);
        let third = cw / 3;
        let tabs = [
            ("Store Catalog", HubTab::Store),
            ("My Library", HubTab::Library),
            ("Downloads", HubTab::Downloads),
        ];
        for (idx, (lbl, t)) in tabs.iter().enumerate() {
            let act = self.tab == *t;
            let tx0 = cx + (idx as i32) * third;
            let tw = font::measure_text(FontFace::UiBold, lbl);
            canvas.draw_text_font(
                tx0 + (third - tw) / 2,
                cy + 10,
                lbl,
                if act { 0xF8FAFC } else { theme.text_secondary },
                FontFace::UiBold,
            );
            if act {
                canvas.fill_rect(tx0 + 18, cy + 36, third - 36, 2, 0xF8FAFC);
            }
        }

        match self.tab {
            HubTab::Store => self.render_store(canvas, theme, cx, cy, cw, ch),
            HubTab::Library => self.render_library(canvas, theme, cx, cy, cw, ch),
            HubTab::Downloads => self.render_downloads(canvas, theme, cx, cy, cw, ch),
        }

        // Bottom embedded .ambv Trailer Preview Bar
        self.render_trailer_bar(canvas, theme, cx, cy, cw, ch);

        if self.wizard.active {
            self.render_wizard(canvas, theme, cx, cy, cw, ch);
        }
    }

    fn draw_racer_thumb(&self, canvas: &mut Canvas, x: i32, y: i32, w: i32, h: i32) {
        let horizon = (h * 45) / 100;
        canvas.fill_gradient_v(x, y, w, horizon, 0x2B4263, 0xE57A34);
        canvas.fill_circle(x + (w * 65) / 100, y + horizon - 4, 12, 0xFBBF24);
        canvas.fill_rect(x, y + horizon, w, h - horizon, 0x1F2531);
        // Highway trapezoid lines
        let mid = x + w / 2;
        canvas.draw_line(mid - 8, y + horizon, x + 6, y + h, 0xCBD5E1);
        canvas.draw_line(mid + 8, y + horizon, x + w - 6, y + h, 0xCBD5E1);
        canvas.draw_line(mid, y + horizon + 6, mid, y + h - 4, 0xFACC15);
        // Orange Sports Car
        let cx = mid - 22;
        let cy = y + h - 34;
        canvas.fill_rounded_rect(cx + 6, cy, 32, 12, 4, 0x1E293B);
        canvas.fill_rounded_rect(cx, cy + 8, 44, 16, 5, 0xEA580C);
        canvas.fill_rect(cx + 4, cy + 14, 8, 4, 0xFEF08A);
        canvas.fill_rect(cx + 32, cy + 14, 8, 4, 0xFEF08A);
    }

    fn draw_blocks_thumb(&self, canvas: &mut Canvas, x: i32, y: i32, w: i32, h: i32) {
        canvas.fill_rect(x, y, w, h, 0x10141D);
        let cell = 14i32;
        let cols = (w / cell).min(8);
        let ox = x + (w - cols * cell) / 2;
        let colors: [[u32; 8]; 5] = [
            [0x000000, 0x000000, 0x38BDF8, 0x38BDF8, 0x000000, 0xE879F9, 0x000000, 0x000000],
            [0x000000, 0xF97316, 0x38BDF8, 0x38BDF8, 0xE879F9, 0xE879F9, 0xE879F9, 0x000000],
            [0xF97316, 0xF97316, 0xF97316, 0x4ADE80, 0x4ADE80, 0x2563EB, 0x000000, 0xFACC15],
            [0xFACC15, 0xFACC15, 0x2563EB, 0x4ADE80, 0x4ADE80, 0x2563EB, 0x2563EB, 0xFACC15],
            [0xFACC15, 0xFACC15, 0x2563EB, 0xF43F5E, 0xF43F5E, 0xF43F5E, 0x2563EB, 0xFACC15],
        ];
        for r in 0..5 {
            for c in 0..cols as usize {
                let col = colors[r][c];
                if col != 0 {
                    let bx = ox + (c as i32) * cell + 1;
                    let by = y + h - ((5 - r as i32) * cell) - 6;
                    canvas.fill_rounded_rect(bx, by, cell - 2, cell - 2, 2, col);
                }
            }
        }
    }

    fn draw_invaders_thumb(&self, canvas: &mut Canvas, x: i32, y: i32, w: i32, h: i32) {
        canvas.fill_rect(x, y, w, h, 0x06080D);
        let tw = font::measure_text(FontFace::UiBold, "INVADERS");
        canvas.draw_text_font(x + (w - tw) / 2, y + 6, "INVADERS", 0xF8FAFC, FontFace::UiBold);
        const SPRITE: [&str; 6] = [
            "..#...#..",
            ".#######.",
            "##.###.##",
            "#########",
            "#.#####.#",
            "..#...#..",
        ];
        let s = 5i32;
        let sx0 = x + (w - 9 * s) / 2;
        let sy0 = y + 30;
        for (ry, r) in SPRITE.iter().enumerate() {
            for (rx, b) in r.bytes().enumerate() {
                if b == b'#' {
                    canvas.fill_rect(sx0 + (rx as i32) * s, sy0 + (ry as i32) * s, s - 1, s - 1, 0x22C55E);
                }
            }
        }
        for i in 0..3 {
            let rx = x + 22 + i * 36;
            canvas.fill_rounded_rect(rx, y + h - 22, 16, 10, 3, 0xEF4444);
        }
    }

    fn render_store(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, _ch: i32) {
        let card_w = (cw - 44) / 3;
        let r3d_ok = loader::app_installed("racer3d");
        let blk_ok = loader::app_installed("blocks");
        let inv_ok = loader::app_installed("invaders");

        for col in 0..3 {
            let bx = cx + 14 + (col as i32) * (card_w + 8);
            let by = cy + 48;
            canvas.fill_rounded_rect(bx, by, card_w, 196, 10, theme.window_surface);
            canvas.draw_rounded_rect_outline(bx, by, card_w, 196, 10, theme.border_inactive);

            let ax = bx + 6;
            let ay = by + 6;
            let aw = card_w - 12;
            let ah = 104;
            match col {
                0 => self.draw_racer_thumb(canvas, ax, ay, aw, ah),
                1 => self.draw_blocks_thumb(canvas, ax, ay, aw, ah),
                _ => self.draw_invaders_thumb(canvas, ax, ay, aw, ah),
            }

            let title = CATALOG[col].title;
            canvas.draw_text_clipped_font(bx + 10, by + 118, card_w - 20, title, theme.text_primary, FontFace::UiBold);

            if col == 0 {
                let lbl = if r3d_ok { "Play" } else { "Install" };
                let btn_col = if r3d_ok { theme.accent } else { 0x3B82F6 };
                canvas.fill_rounded_rect(bx + 12, by + 148, card_w - 24, 32, 14, btn_col);
                let lw = font::measure_text(FontFace::UiBold, lbl);
                canvas.draw_text_font(bx + (card_w - lw) / 2, by + 155, lbl, 0xFFFFFF, FontFace::UiBold);
            } else if col == 1 {
                let lbl = if blk_ok { "Installed" } else { "Install" };
                let btn_col = if blk_ok { 0x2E7D4F } else { 0x3B82F6 };
                canvas.fill_rounded_rect(bx + 12, by + 148, card_w - 24, 32, 14, btn_col);
                let lw = font::measure_text(FontFace::UiBold, lbl);
                canvas.draw_text_font(bx + (card_w - lw) / 2, by + 155, lbl, 0xFFFFFF, FontFace::UiBold);
            } else if inv_ok {
                canvas.fill_rounded_rect(bx + 12, by + 148, card_w - 24, 32, 14, theme.accent);
                let lw = font::measure_text(FontFace::UiBold, "Play");
                canvas.draw_text_font(bx + (card_w - lw) / 2, by + 155, "Play", 0xFFFFFF, FontFace::UiBold);
            } else {
                canvas.draw_text_clipped_font(
                    bx + 8,
                    by + 144,
                    card_w - 16,
                    &format!("HTTP... {}%", self.dl_pct),
                    theme.text_secondary,
                    FontFace::Ui,
                );
                canvas.draw_progress_bar(
                    bx + 10,
                    by + 166,
                    card_w - 20,
                    10,
                    self.dl_pct as usize,
                    100,
                    theme.term_bg,
                    theme.border_inactive,
                    theme.accent,
                );
            }
        }

        // Row 2: Doom3D Card + Setup.exe Quick Panel
        let r2_y = cy + 254;
        let bx = cx + 14;
        canvas.fill_rounded_rect(bx, r2_y, card_w, 160, 10, theme.window_surface);
        canvas.draw_rounded_rect_outline(bx, r2_y, card_w, 160, 10, theme.border_inactive);
        canvas.fill_gradient_v(bx + 6, r2_y + 6, card_w - 12, 86, 0x7C2D12, 0x1E293B);
        canvas.fill_rounded_rect(bx + 42, r2_y + 48, 52, 22, 5, 0xEA580C);
        canvas.draw_text_font(bx + 10, r2_y + 98, "Doom3D (.exe)", theme.text_primary, FontFace::UiBold);
        canvas.fill_rounded_rect(bx + 12, r2_y + 122, card_w - 24, 28, 12, theme.titlebar_active);
        canvas.draw_text_font(bx + (card_w - 50) / 2, r2_y + 128, "Launch", 0xFFFFFF, FontFace::UiBold);

        // Right companion card: PE32+/ELF64 Installer Status
        let px = bx + card_w + 10;
        let pw = cw - card_w - 38;
        canvas.fill_rounded_rect(px, r2_y, pw, 160, 10, theme.window_surface);
        canvas.draw_rounded_rect_outline(px, r2_y, pw, 160, 10, theme.border_inactive);
        canvas.draw_text_font(px + 14, r2_y + 14, "PE32+ (.exe) & ELF64 Runtime", theme.accent_soft, FontFace::UiBold);
        canvas.draw_text_clipped_font(
            px + 14,
            r2_y + 40,
            pw - 28,
            "Downloads save to ATA Disk (/apps/*)",
            theme.text_secondary,
            FontFace::Ui,
        );
        canvas.draw_text_clipped_font(
            px + 14,
            r2_y + 62,
            pw - 28,
            "Protected by SIP + Capability Sandbox",
            theme.term_green,
            FontFace::Ui,
        );
        if !self.banner_msg.is_empty() {
            canvas.draw_text_clipped_font(px + 14, r2_y + 88, pw - 28, &self.banner_msg, theme.accent, FontFace::Mono);
        }
        canvas.fill_rounded_rect(px + 14, r2_y + 118, pw - 28, 30, 8, theme.accent);
        canvas.draw_text_font(px + 42, r2_y + 124, "Run Setup.exe Wizard...", 0xFFFFFF, FontFace::UiBold);
    }

    fn render_library(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, _ch: i32) {
        canvas.draw_text_font(cx + 16, cy + 48, "Installed Games & Portable Binaries (/apps/*)", theme.accent_soft, FontFace::UiBold);
        for (i, pkg) in CATALOG.iter().enumerate() {
            let ry = cy + 74 + (i as i32) * 76;
            let installed = pkg.id == "doom" || loader::app_installed(pkg.id);
            canvas.fill_rounded_rect(cx + 14, ry, cw - 28, 66, 9, theme.window_surface);
            canvas.draw_rounded_rect_outline(cx + 14, ry, cw - 28, 66, 9, theme.border_inactive);
            canvas.draw_text_font(cx + 26, ry + 10, pkg.title, theme.text_primary, FontFace::UiBold);
            let sub = if installed {
                format!("{}  |  {}  |  READY", pkg.bin_type, pkg.size_str)
            } else {
                format!("{}  |  Not installed", pkg.bin_type)
            };
            canvas.draw_text_clipped_font(
                cx + 26,
                ry + 34,
                cw - 240,
                &sub,
                if installed { theme.term_green } else { theme.text_muted },
                FontFace::Ui,
            );
            canvas.fill_rounded_rect(cx + cw - 196, ry + 17, 82, 30, 8, if installed { theme.accent } else { theme.titlebar_active });
            canvas.draw_text_font(
                cx + cw - 172,
                ry + 23,
                if installed { "Play" } else { "Setup" },
                0xFFFFFF,
                FontFace::UiBold,
            );
            canvas.fill_rounded_rect(cx + cw - 104, ry + 17, 78, 30, 8, 0x334155);
            canvas.draw_text_font(
                cx + cw - 92,
                ry + 23,
                if installed { "Remove" } else { "Install" },
                theme.text_secondary,
                FontFace::Ui,
            );
        }
    }

    fn render_downloads(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, _ch: i32) {
        canvas.draw_text_font(cx + 16, cy + 50, "HTTP Gateway Package Downloads (10.0.2.2:18088)", theme.accent_soft, FontFace::UiBold);
        canvas.fill_rounded_rect(cx + 14, cy + 80, cw - 28, 128, 10, theme.window_surface);
        canvas.draw_rounded_rect_outline(cx + 14, cy + 80, cw - 28, 128, 10, theme.border_inactive);
        canvas.draw_text_font(cx + 28, cy + 94, "invaders_setup.exe (PE32+ AMD64 Installer)", theme.text_primary, FontFace::UiBold);
        canvas.draw_text_font(
            cx + 28,
            cy + 118,
            &format!("Status: HTTP GET /pkg?name=invaders_setup.exe ({}%)", self.dl_pct),
            theme.term_green,
            FontFace::Mono,
        );
        canvas.draw_progress_bar(
            cx + 28,
            cy + 146,
            cw - 56,
            12,
            self.dl_pct as usize,
            100,
            theme.term_bg,
            theme.border_inactive,
            theme.accent,
        );
        canvas.fill_rounded_rect(cx + 28, cy + 168, 190, 28, 7, theme.accent);
        canvas.draw_text_font(cx + 42, cy + 174, "Complete & Run Setup", 0xFFFFFF, FontFace::UiBold);
    }

    fn render_trailer_bar(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        let bx = cx + 14;
        let by = cy + ch - 78;
        let bw = cw - 28;
        let bh = 66;
        canvas.fill_rounded_rect(bx, by, bw, bh, 10, theme.titlebar_inactive);
        canvas.draw_rounded_rect_outline(bx, by, bw, bh, 10, theme.border_inactive);

        // Live .ambv thumbnail frame
        let vx = bx + 6;
        let vy = by + 6;
        canvas.blit_rgb_u32(vx, vy, PREVIEW_W, PREVIEW_H, PREVIEW_W, PREVIEW_H, &self.trailer_buf);
        canvas.draw_rounded_rect_outline(vx, vy, PREVIEW_W, PREVIEW_H, 4, theme.border_inactive);
        canvas.fill_circle(vx + PREVIEW_W / 2, vy + PREVIEW_H / 2, 13, 0x0F172A);
        canvas.draw_text_font(
            vx + PREVIEW_W / 2 - 5,
            vy + PREVIEW_H / 2 - 8,
            if self.trailer_playing { "||" } else { "|>" },
            0xFFFFFF,
            FontFace::UiBold,
        );

        let rx = bx + PREVIEW_W + 18;
        canvas.draw_text_font(rx, by + 10, ".ambv Trailer Preview", theme.text_primary, FontFace::UiBold);

        // Controls row: Pause, Next, orange slider with white knob
        let cy_btn = by + 35;
        canvas.fill_rounded_rect(rx, cy_btn, 24, 22, 5, theme.window_surface);
        canvas.draw_text_font(rx + 6, cy_btn + 2, if self.trailer_playing { "||" } else { "|>" }, theme.text_primary, FontFace::UiBold);

        canvas.fill_rounded_rect(rx + 30, cy_btn, 24, 22, 5, theme.window_surface);
        canvas.draw_text_font(rx + 34, cy_btn + 2, ">|", theme.text_primary, FontFace::UiBold);

        let sx = rx + 64;
        let sw = (bx + bw - sx - 16).max(60);
        canvas.draw_progress_bar(
            sx,
            cy_btn + 7,
            sw,
            8,
            self.trailer_frame as usize,
            media::TOTAL_VIDEO_FRAMES as usize,
            theme.term_bg,
            theme.border_inactive,
            theme.accent,
        );
        let kx = sx + (((self.trailer_frame as i32) * (sw - 8)) / (media::TOTAL_VIDEO_FRAMES as i32));
        canvas.fill_circle(kx + 4, cy_btn + 11, 5, 0xFFFFFF);
    }

    fn render_wizard(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        canvas.fill_rect_alpha(cx, cy, cw, ch, 0x000000, 150);
        let wx = cx + 44;
        let wy = cy + (ch - 220) / 2;
        let ww = (cw - 88).max(260);
        let wh = 210;
        let pkg = &CATALOG[self.wizard.pkg_idx];

        canvas.draw_soft_shadow(wx, wy, ww, wh, 10);
        canvas.fill_rounded_rect(wx, wy, ww, wh, 10, theme.window_bg);
        canvas.draw_rounded_rect_outline(wx, wy, ww, wh, 10, theme.accent);

        canvas.draw_text_font(wx + 18, wy + 16, &format!("AmbaSetup Wizard - {}", pkg.title), theme.text_primary, FontFace::UiBold);
        canvas.draw_text_font(
            wx + 18,
            wy + 44,
            &format!("Binary Format: {} | ATA Target: /apps/{}/", pkg.bin_type, pkg.id),
            theme.accent_soft,
            FontFace::Ui,
        );
        canvas.draw_text_clipped_font(wx + 18, wy + 70, ww - 36, &self.wizard.status, theme.term_green, FontFace::Mono);

        canvas.draw_progress_bar(
            wx + 18,
            wy + 104,
            ww - 36,
            16,
            self.wizard.progress as usize,
            100,
            theme.term_bg,
            theme.border_inactive,
            theme.accent,
        );
        canvas.draw_text_font(
            wx + 18,
            wy + 128,
            &format!("Extracting PE32+ sections & creating Snapshot... {}%", self.wizard.progress),
            theme.text_secondary,
            FontFace::Ui,
        );

        let bw = (ww - 48) / 2;
        canvas.fill_rounded_rect(wx + 18, wy + 162, bw, 32, 8, theme.accent);
        canvas.draw_text_font(
            wx + 36,
            wy + 169,
            if self.wizard.done { "Launch Game" } else { "Install Now" },
            0xFFFFFF,
            FontFace::UiBold,
        );
        canvas.fill_rounded_rect(wx + 24 + bw, wy + 162, bw, 32, 8, theme.window_surface);
        canvas.draw_text_font(wx + 56 + bw, wy + 169, "Close", theme.text_secondary, FontFace::UiBold);
    }

    fn render_active_game(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        canvas.fill_rect(cx, cy, cw, ch, 0x0B0E14);
        canvas.fill_rounded_rect(cx + 10, cy + 10, 112, 28, 7, theme.window_surface);
        canvas.draw_text_font(cx + 22, cy + 15, "< Back (Esc)", theme.text_primary, FontFace::UiBold);

        match self.active_game {
            ActiveGame::Racer3D => {
                canvas.draw_text_font(cx + 138, cy + 15, &format!("Racer3D (.exe) | {} KM/H | Score: {}", self.racer_speed, self.racer_score), theme.accent_soft, FontFace::UiBold);
                let gx = cx + 16;
                let gy = cy + 48;
                let gw = cw - 32;
                let gh = ch - 82;
                canvas.fill_gradient_v(gx, gy, gw, 102, 0x1E293B, 0xE67E22);
                canvas.fill_circle(gx + (gw * 7) / 10, gy + 86, 22, 0xFBBF24);
                let hy = gy + 102;
                let rh = gh - 102;
                canvas.fill_rect(gx, hy, gw, rh, 0x16221C);
                // Perspective 3D Highway Trapezoid
                let mid = gx + gw / 2;
                let phase = (self.racer_tick as i32) * 3;
                for dy in 0..rh {
                    let half = 48 + (dy * (gw / 2 - 58)) / rh.max(1);
                    let row_y = hy + dy;
                    let stripe = ((dy + phase) / 18) % 2 == 0;
                    canvas.fill_rect(mid - half, row_y, half * 2, 1, 0x262C38);
                    canvas.fill_rect(mid - half, row_y, 6, 1, if stripe { 0xE67E22 } else { 0xE2E8F0 });
                    canvas.fill_rect(mid + half - 6, row_y, 6, 1, if stripe { 0xE67E22 } else { 0xE2E8F0 });
                    if stripe && (dy % 18) < 10 {
                        canvas.fill_rect(mid - 2, row_y, 4, 1, 0xFACC15);
                    }
                }
                // Opponent car
                canvas.fill_rounded_rect(cx + self.racer_obs_x - 20, hy + self.racer_obs_y, 40, 24, 5, 0x3B82F6);
                canvas.fill_rect(cx + self.racer_obs_x - 14, hy + self.racer_obs_y + 18, 8, 4, 0xF87171);
                canvas.fill_rect(cx + self.racer_obs_x + 6, hy + self.racer_obs_y + 18, 8, 4, 0xF87171);
                // Player orange sports car
                canvas.fill_rounded_rect(cx + self.racer_car_x - 20, hy + 206, 40, 12, 4, 0x1E293B);
                canvas.fill_rounded_rect(cx + self.racer_car_x - 26, hy + 214, 52, 24, 6, 0xEA580C);
                canvas.fill_rect(cx + self.racer_car_x - 22, hy + 226, 10, 5, 0xEF4444);
                canvas.fill_rect(cx + self.racer_car_x + 12, hy + 226, 10, 5, 0xEF4444);
                canvas.draw_text_font(cx + 24, cy + ch - 24, "A/D or Arrows: Steer | W: Boost | Esc: Exit", theme.text_muted, FontFace::Ui);
            }
            ActiveGame::Blocks => {
                canvas.draw_text_font(cx + 138, cy + 15, &format!("Blocks (.exe) | Score: {}", self.blk_score), theme.term_green, FontFace::UiBold);
                let cell = 26i32;
                let bx0 = cx + (cw - 10 * cell) / 2;
                let by0 = cy + 64;
                canvas.fill_rounded_rect(bx0 - 6, by0 - 6, 10 * cell + 12, 14 * cell + 12, 8, 0x161B26);
                let palette = [0x000000, 0xFACC15, 0x38BDF8, 0xE879F9, 0x4ADE80];
                for r in 0..14 {
                    for c in 0..10 {
                        let v = self.blk_grid[r][c] as usize;
                        if v != 0 {
                            canvas.fill_rounded_rect(bx0 + (c as i32) * cell + 1, by0 + (r as i32) * cell + 1, cell - 2, cell - 2, 4, palette[v % 5]);
                        }
                    }
                }
                let pcol = palette[((self.blk_kind % 4) + 1) % 5];
                for (dx, dy) in Self::piece_cells(self.blk_kind, self.blk_rot) {
                    let gx = self.blk_px + dx;
                    let gy = self.blk_py + dy;
                    if (0..10).contains(&gx) && (0..14).contains(&gy) {
                        canvas.fill_rounded_rect(bx0 + gx * cell + 1, by0 + gy * cell + 1, cell - 2, cell - 2, 4, pcol);
                    }
                }
                canvas.draw_text_font(cx + 24, cy + ch - 24, "A/D: Move | W: Rotate | Space: Drop | Esc: Exit", theme.text_muted, FontFace::Ui);
            }
            ActiveGame::Invaders => {
                canvas.draw_text_font(cx + 138, cy + 15, &format!("Invaders (.exe) | Score: {}", self.inv_score), theme.term_green, FontFace::UiBold);
                for (i, &alive) in self.inv_aliens.iter().enumerate() {
                    if alive {
                        let col = (i % 6) as i32;
                        let row = (i / 6) as i32;
                        let ax = cx + self.inv_ax + col * 52;
                        let ay = cy + 82 + row * 38;
                        canvas.fill_rounded_rect(ax - 14, ay - 10, 28, 18, 4, if row == 0 { 0x22C55E } else { 0xEF4444 });
                    }
                }
                if self.inv_shot_y >= 0 {
                    canvas.fill_rect(cx + self.inv_shot_x - 2, cy + self.inv_shot_y, 4, 14, 0xFACC15);
                }
                canvas.fill_rounded_rect(cx + self.inv_ship_x - 20, cy + 360, 40, 16, 4, 0x38BDF8);
                canvas.draw_text_font(cx + 24, cy + ch - 24, "A/D: Move Cannon | Space/W: Fire Laser | Esc: Exit", theme.text_muted, FontFace::Ui);
            }
            ActiveGame::None => {}
        }
    }
}
