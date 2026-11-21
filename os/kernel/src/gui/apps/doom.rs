use alloc::format;
use alloc::vec::Vec;

use crate::drivers::audio;
use crate::drivers::keyboard::{
    self, KEY_A, KEY_D, KEY_DOWN, KEY_E, KEY_LEFT, KEY_Q, KEY_RIGHT, KEY_S, KEY_SPACE, KEY_UP,
    KEY_W, KeyEvent,
};
use crate::gui::assets;
use crate::gui::canvas::Canvas;
use crate::gui::font::{self, FontFace};
use crate::gui::theme::ThemeColors;
use crate::serial_println;

const FP: i32 = 4096;
const RAY_COLS: usize = 280;
const COL_W: i32 = 2;
const VIEW_W: i32 = (RAY_COLS as i32) * COL_W; // 560
const VIEW_H: i32 = 290;
const MAP_W: usize = 16;
const MAP_H: usize = 16;

// Quadrant sine table: round(4096 * sin(2 * pi * i / 256)) for i in 0..=64
const SIN65: [i32; 65] = [
    0, 101, 201, 301, 401, 501, 601, 700, 799, 897, 995, 1092, 1189, 1285, 1380, 1474,
    1567, 1660, 1751, 1842, 1931, 2019, 2106, 2191, 2276, 2359, 2440, 2520, 2598, 2675, 2751, 2824,
    2896, 2967, 3035, 3102, 3166, 3229, 3290, 3349, 3406, 3461, 3513, 3564, 3612, 3659, 3703, 3745,
    3784, 3822, 3857, 3889, 3920, 3948, 3973, 3996, 4017, 4036, 4052, 4065, 4076, 4085, 4091, 4095,
    4096,
];

#[inline]
fn sin256(angle: i32) -> i32 {
    let a = (angle & 255) as usize;
    match a {
        0..=64 => SIN65[a],
        65..=128 => SIN65[128 - a],
        129..=192 => -SIN65[a - 128],
        _ => -SIN65[256 - a],
    }
}

#[inline]
fn cos256(angle: i32) -> i32 {
    sin256(angle + 64)
}

#[rustfmt::skip]
const MAP: [[u8; MAP_W]; MAP_H] = [
    [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],
    [1,0,0,0,0,0,3,0,0,0,0,0,2,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,3,0,2,0,0,1],
    [1,0,0,4,4,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,4,0,0,0,3,0,0,0,2,2,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,2,0,0,1],
    [1,3,0,0,0,2,2,0,0,4,0,0,0,0,0,1],
    [1,0,0,0,0,2,0,0,0,4,0,0,3,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,3,0,0,0,0,0,2,0,0,4,4,0,0,1],
    [1,0,0,0,2,2,0,0,2,0,0,4,0,0,0,1],
    [1,0,0,0,2,0,0,0,0,0,0,0,0,3,0,1],
    [1,0,4,0,0,0,3,0,0,0,2,0,0,0,0,1],
    [1,0,4,0,0,0,0,0,0,0,2,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,3,0,0,0,0,0,0,1],
    [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],
];

// 16x16 Cyber-Imp sprite bitmap:
// '.'=transparent, 'H'=horn, 'R'=crimson skin, 'D'=dark shade, 'Y'=glow eye, 'O'=orange armor/claw
const IMP_SPRITE: [&str; 16] = [
    "...HH......HH...",
    "..H..H....H..H..",
    "...RRRRRRRRRR...",
    "..RRRRRRRRRRRR..",
    "..RRYYRRRRYYRR..",
    "..RRRRDDDDRRRR..",
    "...RRRROORRRR...",
    "....RRRRRRRR....",
    "..OOORRRRRROOO..",
    ".O..RRRRRRRR..O.",
    "....RRDDDDRR....",
    "...RRR....RRR...",
    "...RRR....RRR...",
    "..DDRR....RRDD..",
    "..DDD......DDD..",
    "................",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum SpriteKind {
    Enemy,
    Medkit,
    Ammo,
}

#[derive(Clone, Copy)]
struct Entity {
    x: i32,
    y: i32,
    kind: SpriteKind,
    alive: bool,
    hp: i32,
    hurt_frames: u8,
    attack_cd: u16,
}

pub struct DoomApp {
    px: i32,
    py: i32,
    angle: i32,
    health: i32,
    ammo: i32,
    score: i32,
    kills: usize,
    total_enemies: usize,
    entities: Vec<Entity>,
    tap_mask: u32,
    fire_requested: bool,
    muzzle_frames: u8,
    recoil: i32,
    hurt_flash: u8,
    walk_phase: u32,
    show_map: bool,
    game_over: bool,
    victory: bool,
    last_tick_ms: u64,
    sfx_end_ms: u64,
}

impl DoomApp {
    pub fn new() -> Self {
        let mut app = Self {
            px: 0,
            py: 0,
            angle: 0,
            health: 100,
            ammo: 24,
            score: 0,
            kills: 0,
            total_enemies: 6,
            entities: Vec::new(),
            tap_mask: 0,
            fire_requested: false,
            muzzle_frames: 0,
            recoil: 0,
            hurt_flash: 0,
            walk_phase: 0,
            show_map: true,
            game_over: false,
            victory: false,
            last_tick_ms: 0,
            sfx_end_ms: 0,
        };
        app.reset();
        app
    }

    pub fn reset(&mut self) {
        self.px = 2 * FP + FP / 2;
        self.py = 2 * FP + FP / 2;
        self.angle = 2; // Facing East (+X) slightly South-East down the corridor
        self.health = 100;
        self.ammo = 24;
        self.score = 0;
        self.kills = 0;
        self.tap_mask = 0;
        self.fire_requested = false;
        self.muzzle_frames = 0;
        self.recoil = 0;
        self.hurt_flash = 0;
        self.walk_phase = 0;
        self.game_over = false;
        self.victory = false;
        self.sfx_end_ms = 0;

        self.entities = alloc::vec![
            Entity { x: 6 * FP + FP / 2, y: 2 * FP + FP / 4, kind: SpriteKind::Enemy, alive: true, hp: 60, hurt_frames: 0, attack_cd: 30 },
            Entity { x: 10 * FP + FP / 2, y: 4 * FP + FP / 2, kind: SpriteKind::Enemy, alive: true, hp: 60, hurt_frames: 0, attack_cd: 40 },
            Entity { x: 7 * FP + FP / 2, y: 7 * FP + FP / 2, kind: SpriteKind::Enemy, alive: true, hp: 80, hurt_frames: 0, attack_cd: 35 },
            Entity { x: 3 * FP + FP / 2, y: 8 * FP + FP / 2, kind: SpriteKind::Enemy, alive: true, hp: 60, hurt_frames: 0, attack_cd: 45 },
            Entity { x: 13 * FP + FP / 2, y: 10 * FP + FP / 2, kind: SpriteKind::Enemy, alive: true, hp: 80, hurt_frames: 0, attack_cd: 45 },
            Entity { x: 9 * FP + FP / 2, y: 13 * FP + FP / 2, kind: SpriteKind::Enemy, alive: true, hp: 100, hurt_frames: 0, attack_cd: 50 },
            Entity { x: 4 * FP + FP / 2, y: 1 * FP + FP / 2, kind: SpriteKind::Medkit, alive: true, hp: 0, hurt_frames: 0, attack_cd: 0 },
            Entity { x: 12 * FP + FP / 2, y: 7 * FP + FP / 2, kind: SpriteKind::Medkit, alive: true, hp: 0, hurt_frames: 0, attack_cd: 0 },
            Entity { x: 5 * FP + FP / 2, y: 2 * FP + (FP * 3) / 4, kind: SpriteKind::Ammo, alive: true, hp: 0, hurt_frames: 0, attack_cd: 0 },
            Entity { x: 13 * FP + FP / 2, y: 13 * FP + FP / 2, kind: SpriteKind::Ammo, alive: true, hp: 0, hurt_frames: 0, attack_cd: 0 },
        ];
        self.total_enemies = self.entities.iter().filter(|e| e.kind == SpriteKind::Enemy).count();
    }

    pub fn handle_key(&mut self, ev: KeyEvent) -> bool {
        match ev {
            KeyEvent::Char('w') | KeyEvent::Char('W') | KeyEvent::Up => {
                self.tap_mask |= KEY_W;
                true
            }
            KeyEvent::Char('s') | KeyEvent::Char('S') | KeyEvent::Down => {
                self.tap_mask |= KEY_S;
                true
            }
            KeyEvent::Char('a') | KeyEvent::Char('A') | KeyEvent::Left => {
                self.tap_mask |= KEY_A;
                true
            }
            KeyEvent::Char('d') | KeyEvent::Char('D') | KeyEvent::Right => {
                self.tap_mask |= KEY_D;
                true
            }
            KeyEvent::Char('q') | KeyEvent::Char('Q') => {
                self.tap_mask |= KEY_Q;
                true
            }
            KeyEvent::Char('e') | KeyEvent::Char('E') => {
                self.tap_mask |= KEY_E;
                true
            }
            KeyEvent::Char(' ') | KeyEvent::Char('f') | KeyEvent::Char('F') => {
                if self.game_over || self.victory {
                    self.reset();
                } else {
                    self.fire_requested = true;
                }
                true
            }
            KeyEvent::Char('m') | KeyEvent::Char('M') => {
                self.show_map = !self.show_map;
                true
            }
            KeyEvent::Char('r') | KeyEvent::Char('R') | KeyEvent::Enter => {
                self.reset();
                true
            }
            _ => false,
        }
    }

    pub fn handle_click(&mut self, lx: i32, ly: i32) -> bool {
        // Status bar Restart button
        if (308..=338).contains(&ly) && (476..=562).contains(&lx) {
            self.reset();
            return true;
        }
        // Clicking inside 3D viewport fires weapon (or restarts if ended)
        if (6..=6 + VIEW_H).contains(&ly) && (9..=9 + VIEW_W).contains(&lx) {
            if self.game_over || self.victory {
                self.reset();
            } else {
                self.fire_requested = true;
            }
            return true;
        }
        false
    }

    fn trigger_sfx(&mut self, freq: u32, dur_ms: u64, now_ms: u64) {
        audio::play_freq(freq);
        self.sfx_end_ms = now_ms + dur_ms;
    }

    pub fn tick(&mut self, now_ms: u64, focused: bool) -> bool {
        if self.sfx_end_ms > 0 && now_ms >= self.sfx_end_ms {
            audio::stop();
            self.sfx_end_ms = 0;
        }

        if now_ms.saturating_sub(self.last_tick_ms) < 33 {
            return false;
        }
        self.last_tick_ms = now_ms;

        if self.game_over || self.victory {
            self.tap_mask = 0;
            self.fire_requested = false;
            return false;
        }

        let mut redraw = false;
        if self.muzzle_frames > 0 {
            self.muzzle_frames -= 1;
            redraw = true;
        }
        if self.recoil > 0 {
            self.recoil = (self.recoil - 4).max(0);
            redraw = true;
        }
        if self.hurt_flash > 0 {
            self.hurt_flash -= 1;
            redraw = true;
        }

        let held = if focused { keyboard::held_mask() } else { 0 };
        let mask = held | self.tap_mask;
        self.tap_mask = 0;

        let dir_x = cos256(self.angle);
        let dir_y = sin256(self.angle);
        let move_step = 420i32; // ~0.10 tile per frame

        if (mask & (KEY_A | KEY_LEFT)) != 0 {
            self.angle = (self.angle - 5) & 255;
            redraw = true;
        }
        if (mask & (KEY_D | KEY_RIGHT)) != 0 {
            self.angle = (self.angle + 5) & 255;
            redraw = true;
        }
        if (mask & (KEY_W | KEY_UP)) != 0 {
            self.try_move((dir_x * move_step) / FP, (dir_y * move_step) / FP);
            self.walk_phase = self.walk_phase.wrapping_add(1);
            redraw = true;
        }
        if (mask & (KEY_S | KEY_DOWN)) != 0 {
            self.try_move(-(dir_x * move_step) / FP, -(dir_y * move_step) / FP);
            self.walk_phase = self.walk_phase.wrapping_add(1);
            redraw = true;
        }
        if (mask & KEY_Q) != 0 {
            self.try_move((dir_y * move_step) / FP, -(dir_x * move_step) / FP);
            self.walk_phase = self.walk_phase.wrapping_add(1);
            redraw = true;
        }
        if (mask & KEY_E) != 0 {
            self.try_move(-(dir_y * move_step) / FP, (dir_x * move_step) / FP);
            self.walk_phase = self.walk_phase.wrapping_add(1);
            redraw = true;
        }

        if (held & KEY_SPACE) != 0 && self.muzzle_frames == 0 && self.recoil == 0 {
            self.fire_requested = true;
        }

        if self.fire_requested {
            self.fire_requested = false;
            self.fire_weapon(now_ms);
            redraw = true;
        }

        // Update pickups, enemy movement & attacks
        if self.update_entities(now_ms) {
            redraw = true;
        }

        redraw
    }

    #[inline]
    fn is_solid(tx: i32, ty: i32) -> bool {
        if tx < 0 || ty < 0 || tx >= MAP_W as i32 || ty >= MAP_H as i32 {
            return true;
        }
        MAP[ty as usize][tx as usize] != 0
    }

    fn try_move(&mut self, dx: i32, dy: i32) {
        let margin = 900;
        let nx = self.px + dx;
        let sx = if dx >= 0 { margin } else { -margin };
        if !Self::is_solid((nx + sx) >> 12, (self.py - margin) >> 12)
            && !Self::is_solid((nx + sx) >> 12, (self.py + margin) >> 12)
        {
            self.px = nx;
        }
        let ny = self.py + dy;
        let sy = if dy >= 0 { margin } else { -margin };
        if !Self::is_solid((self.px - margin) >> 12, (ny + sy) >> 12)
            && !Self::is_solid((self.px + margin) >> 12, (ny + sy) >> 12)
        {
            self.py = ny;
        }
    }

    fn has_line_of_sight(&self, tx: i32, ty: i32) -> bool {
        let dx = tx - self.px;
        let dy = ty - self.py;
        let steps = 24i32;
        for s in 1..steps {
            let cx = self.px + (dx * s) / steps;
            let cy = self.py + (dy * s) / steps;
            if Self::is_solid(cx >> 12, cy >> 12) {
                return false;
            }
        }
        true
    }

    fn fire_weapon(&mut self, now_ms: u64) {
        if self.ammo <= 0 {
            self.trigger_sfx(120, 45, now_ms);
            return;
        }
        self.ammo -= 1;
        self.muzzle_frames = 3;
        self.recoil = 20;
        self.trigger_sfx(210, 65, now_ms);
        serial_println!("[doom] fire! ammo={} pos=({},{})", self.ammo, self.px >> 12, self.py >> 12);

        let dir_x = cos256(self.angle) as i64;
        let dir_y = sin256(self.angle) as i64;

        let mut best_idx: Option<usize> = None;
        let mut best_dist = i64::MAX;

        for (idx, ent) in self.entities.iter().enumerate() {
            if !ent.alive || ent.kind != SpriteKind::Enemy {
                continue;
            }
            let dx = (ent.x - self.px) as i64;
            let dy = (ent.y - self.py) as i64;
            let forward = (dx * dir_x + dy * dir_y) / FP as i64;
            if forward <= 512 || forward > 12 * FP as i64 {
                continue;
            }
            let perp = ((dx * dir_y - dy * dir_x) / FP as i64).abs();
            // Aim cone widens with close range
            if perp < (forward / 5).max(1600) && forward < best_dist && self.has_line_of_sight(ent.x, ent.y) {
                best_dist = forward;
                best_idx = Some(idx);
            }
        }

        if let Some(idx) = best_idx {
            let ent = &mut self.entities[idx];
            ent.hp -= 35;
            ent.hurt_frames = 5;
            if ent.hp <= 0 {
                ent.alive = false;
                self.kills += 1;
                self.score += 150;
                self.trigger_sfx(480, 90, now_ms);
                if self.kills >= self.total_enemies {
                    self.victory = true;
                }
            } else {
                self.score += 25;
                self.trigger_sfx(340, 70, now_ms);
            }
        }
    }

    fn update_entities(&mut self, now_ms: u64) -> bool {
        let mut changed = false;
        let px = self.px;
        let py = self.py;
        let mut pickup_sfx = false;
        let mut hurt_sfx = false;

        for i in 0..self.entities.len() {
            if !self.entities[i].alive {
                continue;
            }
            if self.entities[i].hurt_frames > 0 {
                self.entities[i].hurt_frames -= 1;
                changed = true;
            }

            let dx = px - self.entities[i].x;
            let dy = py - self.entities[i].y;
            let dist_sq = (dx as i64) * (dx as i64) + (dy as i64) * (dy as i64);

            match self.entities[i].kind {
                SpriteKind::Medkit => {
                    if dist_sq < ((FP as i64) * 3 / 4).pow(2) && self.health < 100 {
                        self.entities[i].alive = false;
                        self.health = (self.health + 30).min(100);
                        self.score += 50;
                        pickup_sfx = true;
                        changed = true;
                    }
                }
                SpriteKind::Ammo => {
                    if dist_sq < ((FP as i64) * 3 / 4).pow(2) {
                        self.entities[i].alive = false;
                        self.ammo = (self.ammo + 12).min(99);
                        self.score += 50;
                        pickup_sfx = true;
                        changed = true;
                    }
                }
                SpriteKind::Enemy => {
                    // Chase player slowly when within 7 tiles and line-of-sight
                    if dist_sq < ((FP as i64) * 7).pow(2) && dist_sq > ((FP as i64) * 11 / 10).pow(2) {
                        if self.has_line_of_sight(self.entities[i].x, self.entities[i].y) {
                            let step = 75i32;
                            let mx = if dx > 200 { step } else if dx < -200 { -step } else { 0 };
                            let my = if dy > 200 { step } else if dy < -200 { -step } else { 0 };
                            let nx = self.entities[i].x + mx;
                            let ny = self.entities[i].y + my;
                            if !Self::is_solid(nx >> 12, self.entities[i].y >> 12) {
                                self.entities[i].x = nx;
                            }
                            if !Self::is_solid(self.entities[i].x >> 12, ny >> 12) {
                                self.entities[i].y = ny;
                            }
                            changed = true;
                        }
                    }

                    if dist_sq < ((FP as i64) * 4).pow(2) && self.has_line_of_sight(self.entities[i].x, self.entities[i].y) {
                        if self.entities[i].attack_cd == 0 {
                            self.entities[i].attack_cd = 55;
                            self.health = (self.health - 8).max(0);
                            self.hurt_flash = 4;
                            hurt_sfx = true;
                            if self.health == 0 {
                                self.game_over = true;
                            }
                            changed = true;
                        } else {
                            self.entities[i].attack_cd -= 1;
                        }
                    }
                }
            }
        }

        if pickup_sfx {
            self.trigger_sfx(660, 70, now_ms);
        } else if hurt_sfx {
            self.trigger_sfx(110, 80, now_ms);
        }

        changed
    }

    pub fn render(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        canvas.fill_rect(cx, cy, cw, ch, 0x0A0C10);

        let vx = cx + 9;
        let vy = cy + 6;
        let half_h = VIEW_H / 2;

        // 1. Shaded Ceiling & Floor gradients
        canvas.fill_gradient_v(vx, vy, VIEW_W, half_h, 0x090A0E, 0x1A1D28);
        canvas.fill_gradient_v(vx, vy + half_h, VIEW_W, VIEW_H - half_h, 0x1F1A17, 0x332A24);

        // 2. Raycast 3D Textured Walls
        let dir_x = cos256(self.angle);
        let dir_y = sin256(self.angle);
        let plane_x = (-dir_y * 2700) / FP;
        let plane_y = (dir_x * 2700) / FP;

        let mut z_buf = [i64::MAX; RAY_COLS];

        for col in 0..RAY_COLS {
            let cam_x = ((2 * col as i32 - RAY_COLS as i32) * FP) / (RAY_COLS as i32);
            let ray_dx = dir_x + (plane_x * cam_x) / FP;
            let ray_dy = dir_y + (plane_y * cam_x) / FP;

            let mut map_x = self.px >> 12;
            let mut map_y = self.py >> 12;

            let delta_x: i64 = if ray_dx == 0 {
                1_000_000_000
            } else {
                ((FP as i64 * FP as i64) / (ray_dx.abs() as i64)).max(1)
            };
            let delta_y: i64 = if ray_dy == 0 {
                1_000_000_000
            } else {
                ((FP as i64 * FP as i64) / (ray_dy.abs() as i64)).max(1)
            };

            let (step_x, mut side_x) = if ray_dx < 0 {
                let frac = (self.px & 4095) as i64;
                (-1, (frac * delta_x) >> 12)
            } else {
                let frac = (4096 - (self.px & 4095)) as i64;
                (1, (frac * delta_x) >> 12)
            };

            let (step_y, mut side_y) = if ray_dy < 0 {
                let frac = (self.py & 4095) as i64;
                (-1, (frac * delta_y) >> 12)
            } else {
                let frac = (4096 - (self.py & 4095)) as i64;
                (1, (frac * delta_y) >> 12)
            };

            let mut tile: u8 = 1;
            let mut side = 0u8;
            for _ in 0..48 {
                if side_x < side_y {
                    side_x += delta_x;
                    map_x += step_x;
                    side = 0;
                } else {
                    side_y += delta_y;
                    map_y += step_y;
                    side = 1;
                }
                if map_x < 0 || map_y < 0 || map_x >= MAP_W as i32 || map_y >= MAP_H as i32 {
                    tile = 1;
                    break;
                }
                let t = MAP[map_y as usize][map_x as usize];
                if t != 0 {
                    tile = t;
                    break;
                }
            }

            let perp_dist = if side == 0 {
                (side_x - delta_x).max(256)
            } else {
                (side_y - delta_y).max(256)
            };
            z_buf[col] = perp_dist;

            let line_h = ((VIEW_H as i64 * FP as i64) / perp_dist).clamp(2, 1600) as i32;
            let raw_top = half_h - line_h / 2;
            let y0 = raw_top.max(0);
            let y1 = (half_h + line_h / 2).min(VIEW_H - 1);

            let wall_hit = if side == 0 {
                self.py as i64 + (perp_dist * ray_dy as i64) / (FP as i64)
            } else {
                self.px as i64 + (perp_dist * ray_dx as i64) / (FP as i64)
            };
            let tex_x = ((wall_hit >> 8) & 15) as usize;
            let shade = compute_fog_shade(perp_dist, side == 1);

            let gx = vx + (col as i32) * COL_W;
            let mut cur_y = y0;
            while cur_y <= y1 {
                let tex_y = (((cur_y - raw_top) * 16) / line_h).clamp(0, 15) as usize;
                let base_col = sample_wall_texture(tile, tex_x, tex_y);
                let col_rgb = shade_rgb(base_col, shade);
                let packed = canvas.pack(col_rgb);
                canvas.put_pixel_packed(gx, vy + cur_y, packed);
                canvas.put_pixel_packed(gx + 1, vy + cur_y, packed);
                cur_y += 1;
            }
        }

        // 3. Depth-sorted 3D Sprites (Enemies, Medkits, Ammo)
        let mut order: Vec<(usize, i64)> = self
            .entities
            .iter()
            .enumerate()
            .filter(|(_, e)| e.alive)
            .map(|(i, e)| {
                let dx = (self.px - e.x) as i64;
                let dy = (self.py - e.y) as i64;
                (i, dx * dx + dy * dy)
            })
            .collect();
        order.sort_by(|a, b| b.1.cmp(&a.1));

        let det = (plane_x as i64) * (dir_y as i64) - (dir_x as i64) * (plane_y as i64);
        if det != 0 {
            for (idx, _) in order {
                let ent = self.entities[idx];
                let dx = (ent.x - self.px) as i64;
                let dy = (ent.y - self.py) as i64;
                let transform_x = ((dir_y as i64 * dx - dir_x as i64 * dy) * FP as i64) / det;
                let transform_z = ((-plane_y as i64 * dx + plane_x as i64 * dy) * FP as i64) / det;

                if transform_z < 900 {
                    continue;
                }

                let screen_col = ((RAY_COLS as i64 / 2) * (FP as i64 + (transform_x * FP as i64) / transform_z))
                    / (FP as i64);
                let full_h = ((VIEW_H as i64 * FP as i64) / transform_z).clamp(8, 280) as i32;
                let (spr_h, raw_top) = match ent.kind {
                    SpriteKind::Enemy => (full_h, half_h - full_h / 2),
                    SpriteKind::Medkit | SpriteKind::Ammo => {
                        let sh = (full_h * 45) / 100;
                        let floor_y = half_h + full_h / 2;
                        (sh.max(6), floor_y - sh.max(6))
                    }
                };
                let spr_w_cols = match ent.kind {
                    SpriteKind::Enemy => (spr_h / 2).clamp(4, 140),
                    SpriteKind::Medkit | SpriteKind::Ammo => (spr_h / 2).clamp(3, 60),
                };

                let start_col = (screen_col as i32 - spr_w_cols / 2).max(0);
                let end_col = (screen_col as i32 + spr_w_cols / 2).min(RAY_COLS as i32 - 1);
                let top_y = raw_top.max(0);
                let bot_y = (raw_top + spr_h).min(VIEW_H - 1);
                let shade = compute_fog_shade(transform_z, false);

                for scol in start_col..=end_col {
                    if transform_z >= z_buf[scol as usize] {
                        continue;
                    }
                    let tx = (((scol - (screen_col as i32 - spr_w_cols / 2)) * 16) / spr_w_cols.max(1)).clamp(0, 15) as usize;
                    let gx = vx + scol * COL_W;
                    for sy in top_y..=bot_y {
                        let ty = (((sy - raw_top) * 16) / spr_h.max(1)).clamp(0, 15) as usize;
                        if let Some(rgb) = sample_entity_pixel(ent, tx, ty) {
                            let lit = shade_rgb(rgb, shade);
                            let packed = canvas.pack(lit);
                            canvas.put_pixel_packed(gx, vy + sy, packed);
                            canvas.put_pixel_packed(gx + 1, vy + sy, packed);
                        }
                    }
                }
            }
        }

        // 4. First-Person Blaster + Muzzle Flash
        self.draw_weapon(canvas, vx, vy);

        // 5. Crosshair & Hurt vignette
        let mx = vx + VIEW_W / 2;
        let my = vy + VIEW_H / 2;
        canvas.fill_rect(mx - 6, my, 4, 1, 0x22D3EE);
        canvas.fill_rect(mx + 3, my, 4, 1, 0x22D3EE);
        canvas.fill_rect(mx, my - 6, 1, 4, 0x22D3EE);
        canvas.fill_rect(mx, my + 3, 1, 4, 0x22D3EE);

        if self.hurt_flash > 0 {
            canvas.fill_rect_alpha(vx, vy, VIEW_W, VIEW_H, 0xDC2626, 75);
        }

        // 6. Live Mini Automap Radar (top-right inside viewport)
        if self.show_map {
            self.draw_automap(canvas, vx + VIEW_W - 74, vy + 8);
        }

        // Top-left E1M1 label
        canvas.fill_rect_alpha(vx + 6, vy + 6, 138, 22, 0x000000, 140);
        canvas.draw_text_font(vx + 12, vy + 8, "E1M1: Amba Hangar", 0xFF944D, FontFace::UiBold);

        // Victory / Game Over Overlays
        if self.victory || self.game_over {
            canvas.fill_rounded_rect_alpha(vx + 130, vy + 92, 300, 92, 10, 0x0B0E14, 220);
            canvas.draw_rounded_rect_outline(vx + 130, vy + 92, 300, 92, 10, theme.accent);
            let headline = if self.victory { "STAGE CLEAR!" } else { "YOU DIED" };
            let hcol = if self.victory { theme.term_green } else { theme.err };
            let hw = font::measure_text(FontFace::UiBold, headline);
            canvas.draw_text_font(vx + (VIEW_W - hw) / 2, vy + 108, headline, hcol, FontFace::UiBold);
            let sub = format!("Score: {}  |  Press Space or R", self.score);
            let sw = font::measure_text(FontFace::Ui, &sub);
            canvas.draw_text_font(vx + (VIEW_W - sw) / 2, vy + 140, &sub, theme.text_primary, FontFace::Ui);
        }

        // 7. Classic DOOM Bottom HUD Bar
        let hy = cy + VIEW_H + 10;
        canvas.fill_rounded_rect(vx, hy, VIEW_W, 44, 6, 0x1E2028);
        canvas.draw_rounded_rect_outline(vx, hy, VIEW_W, 44, 6, 0x3A3F52);

        canvas.draw_text_font(vx + 14, hy + 4, "HEALTH", 0x94A3B8, FontFace::Ui);
        canvas.draw_text_font(
            vx + 14,
            hy + 21,
            &format!("{}%", self.health),
            if self.health <= 30 { 0xFF5F56 } else { 0x4ADE80 },
            FontFace::Mono,
        );

        canvas.draw_text_font(vx + 96, hy + 4, "AMMO", 0x94A3B8, FontFace::Ui);
        canvas.draw_text_font(vx + 96, hy + 21, &format!("{}", self.ammo), 0xFF944D, FontFace::Mono);

        // Center AmbaOS emblem face
        canvas.fill_rounded_rect(vx + 258, hy + 4, 40, 36, 6, 0x11131A);
        canvas.draw_rounded_rect_outline(vx + 258, hy + 4, 40, 36, 6, 0xFF6A00);
        canvas.draw_rgba_sprite(vx + 267, hy + 11, assets::logo_22(), 255);

        canvas.draw_text_font(vx + 322, hy + 4, "KILLS", 0x94A3B8, FontFace::Ui);
        canvas.draw_text_font(
            vx + 322,
            hy + 21,
            &format!("{}/{}", self.kills, self.total_enemies),
            0x22D3EE,
            FontFace::Mono,
        );

        canvas.draw_text_font(vx + 392, hy + 4, "SCORE", 0x94A3B8, FontFace::Ui);
        canvas.draw_text_font(vx + 392, hy + 21, &format!("{}", self.score), 0xF8FAFC, FontFace::Mono);

        canvas.fill_rounded_rect(cx + 476, hy + 8, 86, 28, 6, theme.accent);
        canvas.draw_text_font(cx + 494, hy + 12, "Restart", 0xFFFFFF, FontFace::UiBold);
    }

    fn draw_weapon(&self, canvas: &mut Canvas, vx: i32, vy: i32) {
        let bob_x = match self.walk_phase % 8 {
            0..=1 => 0,
            2..=3 => 3,
            4..=5 => 0,
            _ => -3,
        };
        let bob_y = if self.walk_phase % 2 == 1 { 2 } else { 0 };

        let gx = vx + VIEW_W / 2 - 26 + bob_x;
        let gy = vy + VIEW_H - 72 + self.recoil + bob_y;

        if self.muzzle_frames > 0 {
            canvas.fill_circle(gx + 26, gy - 6, 18, 0xFF6A00);
            canvas.fill_circle(gx + 26, gy - 4, 10, 0xFACC15);
            canvas.fill_circle(gx + 26, gy - 2, 5, 0xFFFFFF);
        }

        // Twin barrels & dark steel shotgun body
        canvas.fill_rounded_rect(gx + 14, gy, 10, 44, 2, 0x475569);
        canvas.fill_rounded_rect(gx + 28, gy, 10, 44, 2, 0x475569);
        canvas.fill_rect(gx + 16, gy + 2, 4, 14, 0x94A3B8);
        canvas.fill_rect(gx + 30, gy + 2, 4, 14, 0x94A3B8);
        canvas.fill_rounded_rect(gx + 6, gy + 36, 40, 42, 4, 0x1E293B);
        canvas.fill_rect(gx + 18, gy + 42, 16, 6, 0xFF6A00);
    }

    fn draw_automap(&self, canvas: &mut Canvas, mx: i32, my: i32) {
        let cell = 4;
        let sz = (MAP_W as i32) * cell;
        canvas.fill_rect_alpha(mx - 2, my - 2, sz + 4, sz + 4, 0x000000, 185);
        for row in 0..MAP_H {
            for col in 0..MAP_W {
                let t = MAP[row][col];
                if t != 0 {
                    let c = match t {
                        3 => 0xFF6A00,
                        2 => 0x991B1B,
                        _ => 0x475569,
                    };
                    canvas.fill_rect(mx + (col as i32) * cell, my + (row as i32) * cell, cell, cell, c);
                }
            }
        }
        for ent in &self.entities {
            if ent.alive && ent.kind == SpriteKind::Enemy {
                let ex = mx + ((ent.x * cell) >> 12);
                let ey = my + ((ent.y * cell) >> 12);
                canvas.fill_rect(ex - 1, ey - 1, 3, 3, 0xEF4444);
            }
        }
        let px = mx + ((self.px * cell) >> 12);
        let py = my + ((self.py * cell) >> 12);
        let lx = px + (cos256(self.angle) * 7) / FP;
        let ly = py + (sin256(self.angle) * 7) / FP;
        canvas.draw_line(px, py, lx, ly, 0x22D3EE);
        canvas.fill_rect(px - 1, py - 1, 3, 3, 0x4ADE80);
    }
}

#[inline]
fn compute_fog_shade(dist_fp: i64, is_side: bool) -> u32 {
    let mut s = (320 - ((dist_fp * 18) >> 12)).clamp(55, 255) as u32;
    if is_side {
        s = (s * 195) / 255;
    }
    s
}

#[inline]
fn shade_rgb(rgb: u32, shade: u32) -> u32 {
    let r = (((rgb >> 16) & 0xFF) * shade) / 255;
    let g = (((rgb >> 8) & 0xFF) * shade) / 255;
    let b = ((rgb & 0xFF) * shade) / 255;
    (r << 16) | (g << 8) | b
}

#[inline]
fn sample_wall_texture(tile: u8, tx: usize, ty: usize) -> u32 {
    match tile {
        1 => {
            // Tech Steel Panels
            if tx == 0 || ty == 0 || tx == 15 || ty == 15 {
                0x334155
            } else if (tx == 2 || tx == 13) && (ty == 2 || ty == 13) {
                0x94A3B8
            } else if ty == 8 {
                0x1E293B
            } else {
                0x475569
            }
        }
        2 => {
            // Crimson DOOM Brick
            let row = ty / 4;
            let offset = if row % 2 == 1 { 4 } else { 0 };
            if ty % 4 == 0 || (tx + offset) % 8 == 0 {
                0x2B1212
            } else {
                0x992828
            }
        }
        3 => {
            // Glowing Ember-Orange AmbaOS Pillar
            if (6..=9).contains(&tx) && (2..=13).contains(&ty) {
                if tx == 7 || tx == 8 { 0xFFB366 } else { 0xFF6A00 }
            } else if ty == 0 || ty == 15 || tx == 0 || tx == 15 {
                0x1E2430
            } else {
                0x2A3244
            }
        }
        _ => {
            // Hazard Bulkhead
            if ty >= 12 && ((tx + ty) % 6) < 3 {
                0xEAB308
            } else if ty >= 12 {
                0x18181B
            } else if tx == 0 || tx == 15 {
                0x334155
            } else {
                0x52525B
            }
        }
    }
}

#[inline]
fn sample_entity_pixel(ent: Entity, tx: usize, ty: usize) -> Option<u32> {
    match ent.kind {
        SpriteKind::Medkit => {
            if !(3..=12).contains(&tx) || !(5..=14).contains(&ty) {
                return None;
            }
            if ((6..=9).contains(&tx) && (8..=11).contains(&ty))
                && (tx == 7 || tx == 8 || ty == 9 || ty == 10)
            {
                Some(0xEF4444)
            } else {
                Some(0xF1F5F9)
            }
        }
        SpriteKind::Ammo => {
            if !(3..=12).contains(&tx) || !(6..=14).contains(&ty) {
                return None;
            }
            if ty == 7 || ty == 12 {
                Some(0xF59E0B)
            } else {
                Some(0x3F6212)
            }
        }
        SpriteKind::Enemy => {
            let ch = IMP_SPRITE[ty].as_bytes()[tx];
            if ch == b'.' {
                return None;
            }
            if ent.hurt_frames > 0 {
                return Some(0xFFFFFF);
            }
            Some(match ch {
                b'H' => 0xE2E8F0,
                b'Y' => 0xFACC15,
                b'O' => 0xFF6A00,
                b'D' => 0x6B1D1D,
                _ => 0xDC2626,
            })
        }
    }
}
