use alloc::format;
use alloc::string::String;

use crate::drivers::keyboard::KeyEvent;
use crate::fs::vfs;
use crate::gui::canvas::Canvas;
use crate::gui::font::{self, FontFace};
use crate::gui::theme::ThemeColors;
use crate::proc::caps::{self, AppId, CAP_FS_WRITE};

pub struct NotesApp {
    text: String,
    saved: bool,
    cap_err: Option<&'static str>,
}

impl NotesApp {
    pub fn new() -> Self {
        let initial = vfs::read_file("/home/note.txt")
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_else(|| String::from("Type your notes here..."));
        Self {
            text: initial,
            saved: true,
            cap_err: None,
        }
    }

    pub fn handle_key(&mut self, ev: KeyEvent) -> bool {
        match ev {
            KeyEvent::Char(ch) if ch.is_ascii() && !ch.is_ascii_control() => {
                if self.text.len() < 1500 {
                    self.text.push(ch);
                    self.saved = false;
                    return true;
                }
                false
            }
            KeyEvent::Enter => {
                if self.text.len() < 1500 {
                    self.text.push('\n');
                    self.saved = false;
                    return true;
                }
                false
            }
            KeyEvent::Backspace => {
                if self.text.pop().is_some() {
                    self.saved = false;
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    pub fn handle_click(&mut self, local_x: i32, local_y: i32) -> bool {
        if (6..=32).contains(&local_y) {
            if (10..=164).contains(&local_x) {
                match caps::check(AppId::Notes, CAP_FS_WRITE) {
                    Ok(_) => {
                        self.cap_err = None;
                        if vfs::write_file("/home/note.txt", self.text.as_bytes().to_vec()).is_ok() {
                            self.saved = true;
                        }
                    }
                    Err(e) => {
                        self.cap_err = Some(e);
                    }
                }
                return true;
            }
            if (174..=238).contains(&local_x) {
                self.text.clear();
                self.saved = false;
                self.cap_err = None;
                return true;
            }
        }
        false
    }

    pub fn render(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32, focused: bool) {
        canvas.fill_rect(cx, cy, cw, ch, theme.window_bg);

        // Toolbar
        canvas.fill_rect(cx, cy, cw, 38, theme.window_surface);
        canvas.fill_rect(cx, cy + 38, cw, 1, theme.border_inactive);

        canvas.fill_rounded_rect(cx + 10, cy + 6, 154, 26, 6, theme.accent);
        canvas.draw_text_font(cx + 20, cy + 10, "Save (/home/note.txt)", 0xFFFFFF, FontFace::UiBold);

        canvas.fill_rounded_rect(cx + 174, cy + 6, 64, 26, 6, theme.titlebar_active);
        canvas.draw_rounded_rect_outline(cx + 174, cy + 6, 64, 26, 6, theme.border_inactive);
        canvas.draw_text_font(cx + 188, cy + 10, "Clear", theme.text_primary, FontFace::Ui);

        let status = if let Some(err) = self.cap_err {
            String::from(err)
        } else if self.saved {
            format!("Saved ({} chars)", self.text.len())
        } else {
            format!("Unsaved ({} chars)", self.text.len())
        };
        let sw = font::measure_text(FontFace::Ui, &status);
        canvas.draw_text_clipped_font(
            cx + cw - sw - 14,
            cy + 10,
            cw - 250,
            &status,
            if self.cap_err.is_some() {
                theme.err
            } else if self.saved {
                theme.term_green
            } else {
                theme.accent_soft
            },
            FontFace::Ui,
        );

        // Editor body
        let mut y = cy + 48;
        let max_cols = ((cw - 24) / 8).max(16) as usize;
        let line_h = 20;

        let mut last_x = cx + 14;
        let mut last_y = y;

        for raw_line in self.text.split('\n') {
            if raw_line.is_empty() {
                last_x = cx + 14;
                last_y = y;
                y += line_h;
                continue;
            }
            let mut line_buf = String::new();
            for c in raw_line.chars() {
                if line_buf.len() >= max_cols {
                    if y + 18 <= cy + ch - 8 {
                        canvas.draw_text_clipped_font(cx + 14, y, cw - 28, &line_buf, theme.text_primary, FontFace::Mono);
                    }
                    y += line_h;
                    line_buf.clear();
                }
                line_buf.push(c);
            }
            if y + 18 <= cy + ch - 8 {
                canvas.draw_text_clipped_font(cx + 14, y, cw - 28, &line_buf, theme.text_primary, FontFace::Mono);
                last_x = cx + 14 + font::measure_text(FontFace::Mono, &line_buf);
                last_y = y;
            }
            y += line_h;
        }

        if focused && last_y + 16 <= cy + ch - 8 {
            canvas.fill_rect(last_x + 1, last_y + 2, 2, 15, theme.accent);
        }
    }
}
