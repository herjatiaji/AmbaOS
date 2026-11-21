use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use crate::drivers::keyboard::KeyEvent;
use crate::gui::canvas::Canvas;
use crate::gui::font::{self, FontFace};
use crate::gui::theme::ThemeColors;

const GRID_W: i32 = 24;
const GRID_H: i32 = 16;
const CELL: i32 = 16;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

pub struct SnakeApp {
    body: Vec<(i32, i32)>,
    dir: Dir,
    next_dir: Dir,
    food: (i32, i32),
    score: u32,
    best: u32,
    game_over: bool,
    last_step_ms: u64,
    rng_state: u64,
}

impl SnakeApp {
    pub fn new() -> Self {
        let mut app = Self {
            body: Vec::new(),
            dir: Dir::Right,
            next_dir: Dir::Right,
            food: (16, 8),
            score: 0,
            best: 0,
            game_over: false,
            last_step_ms: 0,
            rng_state: 0x5EED_CAFE_1234_5678,
        };
        app.reset();
        app
    }

    pub fn reset(&mut self) {
        self.body = vec![(8, 8), (7, 8), (6, 8), (5, 8)];
        self.dir = Dir::Right;
        self.next_dir = Dir::Right;
        self.score = 0;
        self.game_over = false;
        self.spawn_food();
    }

    fn next_rand(&mut self) -> u64 {
        self.rng_state = self.rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.rng_state
    }

    fn spawn_food(&mut self) {
        for _ in 0..64 {
            let fx = (self.next_rand() as i32).abs() % GRID_W;
            let fy = (self.next_rand() as i32).abs() % GRID_H;
            if !self.body.contains(&(fx, fy)) {
                self.food = (fx, fy);
                return;
            }
        }
        self.food = (0, 0);
    }

    pub fn handle_key(&mut self, ev: KeyEvent) -> bool {
        match ev {
            KeyEvent::Up | KeyEvent::Char('w') | KeyEvent::Char('W') if self.dir != Dir::Down => {
                self.next_dir = Dir::Up;
                true
            }
            KeyEvent::Down | KeyEvent::Char('s') | KeyEvent::Char('S') if self.dir != Dir::Up => {
                self.next_dir = Dir::Down;
                true
            }
            KeyEvent::Left | KeyEvent::Char('a') | KeyEvent::Char('A') if self.dir != Dir::Right => {
                self.next_dir = Dir::Left;
                true
            }
            KeyEvent::Right | KeyEvent::Char('d') | KeyEvent::Char('D') if self.dir != Dir::Left => {
                self.next_dir = Dir::Right;
                true
            }
            KeyEvent::Enter | KeyEvent::Char(' ') | KeyEvent::Char('r') | KeyEvent::Char('R') if self.game_over => {
                self.reset();
                true
            }
            _ => false,
        }
    }

    pub fn handle_click(&mut self, lx: i32, ly: i32, cw: i32) -> bool {
        if (6..=32).contains(&ly) && lx >= cw - 92 && lx <= cw - 14 {
            self.reset();
            return true;
        }
        false
    }

    pub fn tick(&mut self, now_ms: u64) -> bool {
        if self.game_over || now_ms.saturating_sub(self.last_step_ms) < 135 {
            return false;
        }
        self.last_step_ms = now_ms;
        self.rng_state ^= now_ms;
        self.dir = self.next_dir;

        let (hx, hy) = self.body[0];
        let next = match self.dir {
            Dir::Up => (hx, hy - 1),
            Dir::Down => (hx, hy + 1),
            Dir::Left => (hx - 1, hy),
            Dir::Right => (hx + 1, hy),
        };

        if next.0 < 0 || next.0 >= GRID_W || next.1 < 0 || next.1 >= GRID_H || self.body.contains(&next) {
            self.game_over = true;
            return true;
        }

        self.body.insert(0, next);
        if next == self.food {
            self.score += 10;
            if self.score > self.best {
                self.best = self.score;
            }
            self.spawn_food();
        } else {
            self.body.pop();
        }
        true
    }

    pub fn render(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        canvas.fill_rect(cx, cy, cw, ch, theme.window_bg);

        // Score Header
        canvas.fill_rect(cx, cy, cw, 38, theme.window_surface);
        canvas.fill_rect(cx, cy + 38, cw, 1, theme.border_inactive);
        canvas.draw_text_font(
            cx + 14,
            cy + 10,
            &format!("Score: {}   |   Best: {}", self.score, self.best),
            theme.text_primary,
            FontFace::UiBold,
        );
        canvas.fill_rounded_rect(cx + cw - 92, cy + 7, 78, 24, 6, theme.accent);
        canvas.draw_text_font(cx + cw - 78, cy + 10, "Restart", 0xFFFFFF, FontFace::UiBold);

        // Board centered
        let bw = GRID_W * CELL;
        let bh = GRID_H * CELL;
        let bx = cx + (cw - bw) / 2;
        let by = cy + 48 + ((ch - 56 - bh).max(0) / 2);

        canvas.fill_rounded_rect(bx, by, bw, bh, 6, theme.term_bg);
        canvas.draw_rounded_rect_outline(bx - 1, by - 1, bw + 2, bh + 2, 6, theme.border_inactive);

        // Food
        let (fx, fy) = self.food;
        canvas.fill_circle(bx + fx * CELL + CELL / 2, by + fy * CELL + CELL / 2, 6, theme.accent_sec);

        // Snake segments
        for (i, &(sx, sy)) in self.body.iter().enumerate() {
            let col = if i == 0 { theme.accent } else { theme.accent_soft };
            canvas.fill_rounded_rect(bx + sx * CELL + 1, by + sy * CELL + 1, CELL - 2, CELL - 2, 4, col);
        }

        if self.game_over {
            canvas.fill_rounded_rect_alpha(bx + 72, by + bh / 2 - 34, bw - 144, 68, 10, 0x000000, 200);
            canvas.draw_rounded_rect_outline(bx + 72, by + bh / 2 - 34, bw - 144, 68, 10, theme.accent);
            let msg = "GAME OVER";
            let mw = font::measure_text(FontFace::UiBold, msg);
            canvas.draw_text_font(bx + (bw - mw) / 2, by + bh / 2 - 22, msg, theme.err, FontFace::UiBold);
            let hint = "Press Space or Restart";
            let hw = font::measure_text(FontFace::Ui, hint);
            canvas.draw_text_font(bx + (bw - hw) / 2, by + bh / 2 + 4, hint, theme.text_secondary, FontFace::Ui);
        }
    }
}
