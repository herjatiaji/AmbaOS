use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::fs::vfs::{self, Partition};
use crate::gui::canvas::Canvas;
use crate::gui::font::FontFace;
use crate::gui::theme::ThemeColors;

pub struct FileViewerApp {
    selected: usize,
}

impl FileViewerApp {
    pub fn new() -> Self {
        let files = vfs::list_entries();
        let selected = files
            .iter()
            .position(|(p, _, _)| p == "/etc/motd.txt")
            .unwrap_or(0);
        Self { selected }
    }

    pub fn handle_click(&mut self, local_x: i32, local_y: i32, cw: i32) -> bool {
        let split_x = (cw * 40) / 100;
        if local_x >= 8 && local_x < split_x && local_y >= 36 {
            let row = ((local_y - 36) / 26) as usize;
            let count = vfs::file_count();
            if row < count {
                self.selected = row;
                return true;
            }
        }
        false
    }

    pub fn render(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32) {
        canvas.fill_rect(cx, cy, cw, ch, theme.window_bg);

        let files = vfs::list_entries();
        let split_x = (cw * 40) / 100;

        // Top toolbar breadcrumb
        canvas.fill_rect(cx, cy, cw, 30, theme.window_surface);
        canvas.fill_rect(cx, cy + 30, cw, 1, theme.border_inactive);
        let cur = files.get(self.selected);
        let (cur_path, part_lbl) = match cur {
            Some((p, _, Partition::CoreRo)) => (p.as_str(), "CORE [RO]"),
            Some((p, _, Partition::UserRw)) => (p.as_str(), "USER [RW]"),
            None => ("/", "VFS"),
        };
        canvas.draw_text_clipped_font(
            cx + 12,
            cy + 6,
            cw - 24,
            &format!("VFS Explorer ({})  >  {}", part_lbl, cur_path),
            theme.accent_soft,
            FontFace::UiBold,
        );

        // Left sidebar
        canvas.fill_rect(cx, cy + 31, split_x, ch - 31, theme.titlebar_inactive);
        canvas.fill_rect(cx + split_x, cy + 31, 1, ch - 31, theme.border_inactive);

        let mut list_y = cy + 38;
        for (idx, (path, size, part)) in files.iter().enumerate() {
            if list_y + 24 > cy + ch - 4 {
                break;
            }
            let is_sel = idx == self.selected;
            if is_sel {
                canvas.fill_rounded_rect(cx + 6, list_y, split_x - 12, 23, 5, theme.accent);
            }
            let (tag, tag_col) = match part {
                Partition::CoreRo => ("[RO]", if is_sel { 0xFFFFFF } else { 0xFBBF24 }),
                Partition::UserRw => ("[RW]", if is_sel { 0xFFFFFF } else { theme.term_green }),
            };
            canvas.draw_text_font(cx + 10, list_y + 4, tag, tag_col, FontFace::UiBold);
            let label = format!("{} ({}B)", path, size);
            canvas.draw_text_clipped_font(
                cx + 42,
                list_y + 3,
                split_x - 50,
                &label,
                if is_sel { 0xFFFFFF } else { theme.text_secondary },
                FontFace::Ui,
            );
            list_y += 26;
        }

        // Right preview area
        let rx = cx + split_x + 14;
        let rw = cw - split_x - 26;
        if let Some((path, size, part)) = files.get(self.selected) {
            let p_desc = match part {
                Partition::CoreRo => "Immutable Read-Only Core OS",
                Partition::UserRw => "Persistent Read-Write User Data",
            };
            canvas.draw_text_clipped_font(
                rx,
                cy + 38,
                rw,
                &format!("{}  ({} B) - {}", path, size, p_desc),
                theme.text_primary,
                FontFace::UiBold,
            );
            canvas.fill_rect(rx, cy + 60, rw, 1, theme.border_inactive);

            if let Some(bytes) = vfs::read_file(path) {
                let text = core::str::from_utf8(&bytes).unwrap_or("[binary data]");
                let max_cols = ((rw - 8) / 8).max(16) as usize;
                let lines = wrap_lines(text, max_cols);
                let mut ty = cy + 68;
                for line in lines {
                    if ty + 18 > cy + ch - 6 {
                        break;
                    }
                    canvas.draw_text_clipped_font(rx, ty, rw, &line, theme.text_secondary, FontFace::Mono);
                    ty += 19;
                }
            }
        }
    }
}

fn wrap_lines(text: &str, max_cols: usize) -> Vec<String> {
    let mut out = Vec::new();
    for raw_line in text.lines() {
        if raw_line.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut cur = String::new();
        for ch in raw_line.chars() {
            if cur.len() >= max_cols {
                out.push(cur);
                cur = String::new();
            }
            cur.push(ch);
        }
        if !cur.is_empty() {
            out.push(cur);
        }
    }
    out
}
