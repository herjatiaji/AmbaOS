use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::drivers::keyboard::KeyEvent;
use crate::fs::vfs::{self, Partition};
use crate::gui::canvas::Canvas;
use crate::gui::font::FontFace;
use crate::gui::theme::ThemeColors;
use crate::proc::loader;

#[derive(Clone)]
pub enum FileAction {
    None,
    LaunchBinary(String),
}

pub struct FileViewerApp {
    selected: usize,
    scroll_row: usize,
}

impl FileViewerApp {
    pub fn new() -> Self {
        let files = vfs::list_entries();
        let selected = files
            .iter()
            .position(|(p, _, _)| p.ends_with(".exe"))
            .or_else(|| files.iter().position(|(p, _, _)| p == "/etc/motd.txt"))
            .unwrap_or(0);
        Self {
            selected,
            scroll_row: 0,
        }
    }

    pub fn handle_wheel(&mut self, dz: i32) -> bool {
        let count = vfs::file_count();
        if count == 0 || dz == 0 {
            return false;
        }
        if dz > 0 {
            self.selected = (self.selected + 1).min(count - 1);
        } else {
            self.selected = self.selected.saturating_sub(1);
        }
        if self.selected < self.scroll_row {
            self.scroll_row = self.selected;
        }
        true
    }

    pub fn handle_key(&mut self, ev: KeyEvent) -> (bool, FileAction) {
        let count = vfs::file_count();
        if count == 0 {
            return (false, FileAction::None);
        }
        match ev {
            KeyEvent::Up => {
                self.selected = self.selected.saturating_sub(1);
                if self.selected < self.scroll_row {
                    self.scroll_row = self.selected;
                }
                (true, FileAction::None)
            }
            KeyEvent::Down => {
                self.selected = (self.selected + 1).min(count - 1);
                if self.selected >= self.scroll_row + 12 {
                    self.scroll_row = self.selected.saturating_sub(11);
                }
                (true, FileAction::None)
            }
            KeyEvent::Home => {
                self.selected = 0;
                self.scroll_row = 0;
                (true, FileAction::None)
            }
            KeyEvent::End => {
                self.selected = count - 1;
                self.scroll_row = count.saturating_sub(12);
                (true, FileAction::None)
            }
            KeyEvent::Enter => {
                let files = vfs::list_entries();
                if let Some((path, _, _)) = files.get(self.selected) {
                    if path.ends_with(".exe") || path.ends_with(".elf") || path.ends_with(".ambv") {
                        return (true, FileAction::LaunchBinary(path.clone()));
                    }
                }
                (false, FileAction::None)
            }
            _ => (false, FileAction::None),
        }
    }

    pub fn handle_click(&mut self, local_x: i32, local_y: i32, cw: i32, ch: i32) -> (bool, FileAction) {
        let split_x = (cw * 40) / 100;
        if local_x >= 8 && local_x < split_x && local_y >= 36 {
            let row = self.scroll_row + ((local_y - 36) / 26) as usize;
            let count = vfs::file_count();
            if row < count {
                self.selected = row;
                return (true, FileAction::None);
            }
        }
        // Check [Run Executable (.exe / .elf)] button in preview pane
        let rx = split_x + 14;
        if local_x >= rx && local_x <= rx + 240 && local_y >= ch - 46 && local_y <= ch - 14 {
            let files = vfs::list_entries();
            if let Some((path, _, _)) = files.get(self.selected) {
                if path.ends_with(".exe") || path.ends_with(".elf") || path.ends_with(".ambv") {
                    return (true, FileAction::LaunchBinary(path.clone()));
                }
            }
        }
        (false, FileAction::None)
    }

    pub fn render(&self, canvas: &mut Canvas, theme: ThemeColors, cx: i32, cy: i32, cw: i32, ch: i32, large_text: bool) {
        canvas.fill_rect(cx, cy, cw, ch, theme.window_bg);

        let files = vfs::list_entries();
        let split_x = (cw * 40) / 100;

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

        canvas.fill_rect(cx, cy + 31, split_x, ch - 31, theme.titlebar_inactive);
        canvas.fill_rect(cx + split_x, cy + 31, 1, ch - 31, theme.border_inactive);

        let mut list_y = cy + 38;
        for (idx, (path, size, part)) in files.iter().enumerate().skip(self.scroll_row) {
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

        let rx = cx + split_x + 14;
        let rw = cw - split_x - 26;
        if let Some((path, size, part)) = files.get(self.selected) {
            let p_desc = match part {
                Partition::CoreRo => "Immutable Read-Only Core OS",
                Partition::UserRw => "Persistent ATA User Data",
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
                if let Some(hdr) = loader::inspect_binary(&bytes) {
                    canvas.draw_text_font(rx, cy + 72, &format!("Binary Type : {}", hdr.format.label()), theme.accent, FontFace::UiBold);
                    canvas.draw_text_clipped_font(rx, cy + 96, rw, &format!("Header Magic: {}", hdr.magic_str), theme.text_primary, FontFace::Mono);
                    canvas.draw_text_clipped_font(rx, cy + 118, rw, &format!("Subsystem   : {}", hdr.subsystem), theme.term_green, FontFace::Mono);
                    canvas.draw_text_clipped_font(rx, cy + 140, rw, &format!("Entry RVA   : {:#010x}", hdr.entry_rva), theme.text_secondary, FontFace::Mono);
                    canvas.draw_text_clipped_font(rx, cy + 162, rw, &format!("ImageBase   : {:#018x}", hdr.image_base), theme.text_secondary, FontFace::Mono);
                    canvas.draw_text_clipped_font(rx, cy + 184, rw, &format!("Imports     : {}", hdr.imports), theme.accent_sec, FontFace::Mono);
                    canvas.draw_text_clipped_font(rx, cy + 206, rw, &format!("SIP Target  : {} ({})", hdr.app_id, hdr.version), theme.text_secondary, FontFace::Mono);

                    canvas.fill_rounded_rect(rx, cy + ch - 46, 236, 32, 8, theme.accent);
                    canvas.draw_text_font(rx + 18, cy + ch - 39, "Run Executable in SIP Sandbox", 0xFFFFFF, FontFace::UiBold);
                } else {
                    let text = core::str::from_utf8(&bytes).unwrap_or("[binary data]");
                    let col_div = if large_text { 12 } else { 8 };
                    let line_step = if large_text { 26 } else { 19 };
                    let max_cols = ((rw - 8) / col_div).max(12) as usize;
                    let lines = wrap_lines(text, max_cols);
                    let mut ty = cy + 68;
                    for line in lines {
                        if ty + line_step > cy + ch - 6 {
                            break;
                        }
                        canvas.draw_text_scaled_clipped_font(rx, ty, rw, &line, theme.text_secondary, FontFace::Mono, large_text);
                        ty += line_step;
                    }
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
