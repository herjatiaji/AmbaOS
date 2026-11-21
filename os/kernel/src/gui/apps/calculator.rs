use alloc::format;
use alloc::string::String;

use crate::drivers::keyboard::KeyEvent;
use crate::gui::canvas::Canvas;
use crate::gui::font::{self, FontFace};
use crate::gui::theme::ThemeColors;

#[derive(Clone, Copy)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

impl Op {
    fn symbol(self) -> &'static str {
        match self {
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Div => "/",
            Op::Mod => "%",
        }
    }
}

pub struct CalculatorApp {
    acc: Option<i64>,
    pending: Option<Op>,
    entry: String,
    fresh_entry: bool,
    expr_preview: String,
}

const KEYS: [&str; 20] = [
    "C", "BS", "%", "/",
    "7", "8", "9", "*",
    "4", "5", "6", "-",
    "1", "2", "3", "+",
    "+/-", "0", "00", "=",
];

impl CalculatorApp {
    pub fn new() -> Self {
        Self {
            acc: None,
            pending: None,
            entry: String::from("0"),
            fresh_entry: true,
            expr_preview: String::new(),
        }
    }

    pub fn handle_key(&mut self, ev: KeyEvent) -> bool {
        let token = match ev {
            KeyEvent::Char(c) if c.is_ascii_digit() => match c {
                '0' => "0",
                '1' => "1",
                '2' => "2",
                '3' => "3",
                '4' => "4",
                '5' => "5",
                '6' => "6",
                '7' => "7",
                '8' => "8",
                '9' => "9",
                _ => return false,
            },
            KeyEvent::Char('+') => "+",
            KeyEvent::Char('-') => "-",
            KeyEvent::Char('*') | KeyEvent::Char('x') => "*",
            KeyEvent::Char('/') => "/",
            KeyEvent::Char('%') => "%",
            KeyEvent::Char('=') | KeyEvent::Enter => "=",
            KeyEvent::Char('c') | KeyEvent::Char('C') | KeyEvent::Escape => "C",
            KeyEvent::Backspace => "BS",
            _ => return false,
        };
        self.press_token(token);
        true
    }

    pub fn handle_click(&mut self, local_x: i32, local_y: i32, cw: i32, ch: i32) -> bool {
        let pad = 12;
        let top = 92;
        if local_x < pad || local_x >= cw - pad || local_y < top || local_y >= ch - pad {
            return false;
        }
        let gap = 8;
        let cell_w = (cw - pad * 2 - gap * 3) / 4;
        let cell_h = (ch - top - pad - gap * 4) / 5;
        let col = (local_x - pad) / (cell_w + gap);
        let row = (local_y - top) / (cell_h + gap);
        if !(0..4).contains(&col) || !(0..5).contains(&row) {
            return false;
        }
        let idx = (row * 4 + col) as usize;
        self.press_token(KEYS[idx]);
        true
    }

    fn press_token(&mut self, tok: &str) {
        match tok {
            "C" => {
                self.acc = None;
                self.pending = None;
                self.entry = String::from("0");
                self.fresh_entry = true;
                self.expr_preview.clear();
            }
            "BS" => {
                if !self.fresh_entry {
                    self.entry.pop();
                    if self.entry.is_empty() || self.entry == "-" {
                        self.entry = String::from("0");
                        self.fresh_entry = true;
                    }
                }
            }
            "+/-" => {
                if let Ok(v) = self.entry.parse::<i64>() {
                    self.entry = format!("{}", v.saturating_neg());
                }
            }
            "00" => {
                if !self.fresh_entry && self.entry != "0" && self.entry.len() < 14 {
                    self.entry.push_str("00");
                }
            }
            "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
                if self.fresh_entry || self.entry == "0" {
                    self.entry = String::from(tok);
                    self.fresh_entry = false;
                } else if self.entry.len() < 15 {
                    self.entry.push_str(tok);
                }
            }
            "+" | "-" | "*" | "/" | "%" => {
                let op = match tok {
                    "+" => Op::Add,
                    "-" => Op::Sub,
                    "*" => Op::Mul,
                    "/" => Op::Div,
                    _ => Op::Mod,
                };
                self.apply_pending();
                let val = self.entry.parse::<i64>().unwrap_or(0);
                self.acc = Some(val);
                self.pending = Some(op);
                self.expr_preview = format!("{} {}", val, op.symbol());
                self.fresh_entry = true;
            }
            "=" => {
                let rhs = self.entry.parse::<i64>().unwrap_or(0);
                if let (Some(lhs), Some(op)) = (self.acc, self.pending) {
                    self.expr_preview = format!("{} {} {} =", lhs, op.symbol(), rhs);
                }
                self.apply_pending();
                self.pending = None;
                self.acc = None;
                self.fresh_entry = true;
            }
            _ => {}
        }
    }

    fn apply_pending(&mut self) {
        if let (Some(lhs), Some(op)) = (self.acc, self.pending) {
            let rhs = self.entry.parse::<i64>().unwrap_or(0);
            let res = match op {
                Op::Add => lhs.saturating_add(rhs),
                Op::Sub => lhs.saturating_sub(rhs),
                Op::Mul => lhs.saturating_mul(rhs),
                Op::Div => if rhs == 0 { 0 } else { lhs / rhs },
                Op::Mod => if rhs == 0 { 0 } else { lhs % rhs },
            };
            self.entry = format!("{}", res);
            self.acc = Some(res);
        }
    }

    pub fn render(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        canvas.fill_rect(cx, cy, cw, ch, theme.window_bg);

        let pad = 12;
        // Display box
        canvas.fill_rounded_rect(cx + pad, cy + 10, cw - pad * 2, 70, 8, theme.term_bg);
        canvas.draw_rounded_rect_outline(cx + pad, cy + 10, cw - pad * 2, 70, 8, theme.border_inactive);

        let ew = font::measure_text(FontFace::Ui, &self.expr_preview);
        canvas.draw_text_clipped_font(
            cx + cw - pad - ew.min(cw - 40) - 10,
            cy + 18,
            cw - pad * 2 - 20,
            &self.expr_preview,
            theme.text_muted,
            FontFace::Ui,
        );

        let vw = font::measure_text(FontFace::Mono, &self.entry);
        canvas.draw_text_clipped_font(
            cx + cw - pad - vw.min(cw - 40) - 10,
            cy + 44,
            cw - pad * 2 - 20,
            &self.entry,
            theme.text_primary,
            FontFace::Mono,
        );

        // Keypad grid
        let top = 92;
        let gap = 8;
        let cell_w = (cw - pad * 2 - gap * 3) / 4;
        let cell_h = (ch - top - pad - gap * 4) / 5;

        for (idx, label) in KEYS.iter().enumerate() {
            let row = (idx / 4) as i32;
            let col = (idx % 4) as i32;
            let bx = cx + pad + col * (cell_w + gap);
            let by = cy + top + row * (cell_h + gap);

            let is_accent = col == 3;
            let is_func = row == 0 && col < 3;
            let bg = if is_accent {
                theme.accent
            } else if is_func {
                theme.titlebar_active
            } else {
                theme.window_surface
            };
            let fg = if is_accent { 0xFFFFFF } else { theme.text_primary };

            canvas.fill_rounded_rect(bx, by, cell_w, cell_h, 8, bg);
            if !is_accent {
                canvas.draw_rounded_rect_outline(bx, by, cell_w, cell_h, 8, theme.border_inactive);
            }
            let tw = font::measure_text(FontFace::UiBold, label);
            canvas.draw_text_font(bx + (cell_w - tw) / 2, by + (cell_h - 16) / 2, label, fg, FontFace::UiBold);
        }
    }
}
