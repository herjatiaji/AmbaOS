use crate::drivers::keyboard::KeyEvent;
use crate::gui::canvas::Canvas;
use crate::gui::font::FontFace;
use crate::gui::theme::{AccessibilityConfig, ThemeColors, ThemePreset};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Themes,
    Accessibility,
}

#[derive(Clone, Copy)]
pub enum SettingsAction {
    None,
    Redraw,
    SelectTheme(ThemePreset),
    ToggleWatermark,
    ToggleHighContrast,
    ToggleLargeCursor,
    ToggleMagnifier,
    ToggleLargeText,
    ToggleMouseKeys,
    ToggleSoundCues,
}

pub struct SettingsApp {
    tab: SettingsTab,
    selected_row: usize,
}

impl SettingsApp {
    pub fn new() -> Self {
        Self {
            tab: SettingsTab::Themes,
            selected_row: 0,
        }
    }

    pub fn open_a11y_tab(&mut self) {
        self.tab = SettingsTab::Accessibility;
        self.selected_row = 0;
    }

    pub fn handle_key(&mut self, ev: KeyEvent) -> SettingsAction {
        match ev {
            KeyEvent::Tab | KeyEvent::Left | KeyEvent::Right => {
                self.tab = if self.tab == SettingsTab::Themes {
                    SettingsTab::Accessibility
                } else {
                    SettingsTab::Themes
                };
                self.selected_row = 0;
                SettingsAction::Redraw
            }
            KeyEvent::Char('1') => {
                self.tab = SettingsTab::Themes;
                self.selected_row = 0;
                SettingsAction::Redraw
            }
            KeyEvent::Char('2') => {
                self.tab = SettingsTab::Accessibility;
                self.selected_row = 0;
                SettingsAction::Redraw
            }
            KeyEvent::Up => {
                self.selected_row = self.selected_row.saturating_sub(1);
                SettingsAction::Redraw
            }
            KeyEvent::Down => {
                let max_idx = 5;
                if self.selected_row < max_idx {
                    self.selected_row += 1;
                }
                SettingsAction::Redraw
            }
            KeyEvent::Enter | KeyEvent::Char(' ') => match self.tab {
                SettingsTab::Themes => {
                    if let Some(&preset) = ThemePreset::ALL.get(self.selected_row) {
                        SettingsAction::SelectTheme(preset)
                    } else {
                        SettingsAction::ToggleWatermark
                    }
                }
                SettingsTab::Accessibility => match self.selected_row {
                    0 => SettingsAction::ToggleHighContrast,
                    1 => SettingsAction::ToggleLargeCursor,
                    2 => SettingsAction::ToggleMagnifier,
                    3 => SettingsAction::ToggleLargeText,
                    4 => SettingsAction::ToggleMouseKeys,
                    _ => SettingsAction::ToggleSoundCues,
                },
            },
            _ => SettingsAction::None,
        }
    }

    pub fn handle_click(&mut self, local_x: i32, local_y: i32, cw: i32) -> SettingsAction {
        // Tab bar (y: 8..34)
        if (8..=34).contains(&local_y) {
            if (16..=226).contains(&local_x) {
                self.tab = SettingsTab::Themes;
                self.selected_row = 0;
                return SettingsAction::Redraw;
            }
            if (234..=cw - 16).contains(&local_x) {
                self.tab = SettingsTab::Accessibility;
                self.selected_row = 0;
                return SettingsAction::Redraw;
            }
        }

        let card_x = 16;
        let card_w = cw - 32;
        if local_x < card_x || local_x > card_x + card_w {
            return SettingsAction::None;
        }

        match self.tab {
            SettingsTab::Themes => {
                for (idx, preset) in ThemePreset::ALL.iter().enumerate() {
                    let cy = 48 + (idx as i32) * 50;
                    if local_y >= cy && local_y <= cy + 44 {
                        self.selected_row = idx;
                        return SettingsAction::SelectTheme(*preset);
                    }
                }
                let ty = 48 + (ThemePreset::ALL.len() as i32) * 50 + 4;
                if local_y >= ty && local_y <= ty + 40 {
                    self.selected_row = ThemePreset::ALL.len();
                    return SettingsAction::ToggleWatermark;
                }
            }
            SettingsTab::Accessibility => {
                for idx in 0..6usize {
                    let cy = 48 + (idx as i32) * 52;
                    if local_y >= cy && local_y <= cy + 46 {
                        self.selected_row = idx;
                        return match idx {
                            0 => SettingsAction::ToggleHighContrast,
                            1 => SettingsAction::ToggleLargeCursor,
                            2 => SettingsAction::ToggleMagnifier,
                            3 => SettingsAction::ToggleLargeText,
                            4 => SettingsAction::ToggleMouseKeys,
                            _ => SettingsAction::ToggleSoundCues,
                        };
                    }
                }
            }
        }
        SettingsAction::None
    }

    pub fn render(
        &self,
        canvas: &mut Canvas,
        theme: ThemeColors,
        active_preset: ThemePreset,
        show_watermark: bool,
        a11y: AccessibilityConfig,
        cx: i32,
        cy: i32,
        cw: i32,
        ch: i32,
    ) {
        canvas.fill_rect(cx, cy, cw, ch, theme.window_bg);

        let tabs = [
            (16, 210, "Themes & Desktop", SettingsTab::Themes),
            (234, (cw - 250).max(160), "Accessibility (A11y)", SettingsTab::Accessibility),
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
            canvas.draw_rounded_rect_outline(
                cx + bx,
                cy + 8,
                bw,
                26,
                6,
                if act { theme.border_active } else { theme.border_inactive },
            );
            canvas.draw_text_clipped_font(
                cx + bx + 14,
                cy + 12,
                bw - 24,
                lbl,
                if act { 0xFFFFFF } else { theme.text_secondary },
                FontFace::UiBold,
            );
        }

        let card_x = cx + 16;
        let card_w = cw - 32;

        match self.tab {
            SettingsTab::Themes => {
                for (idx, preset) in ThemePreset::ALL.iter().enumerate() {
                    let py = cy + 48 + (idx as i32) * 50;
                    if py + 44 > cy + ch - 8 {
                        break;
                    }
                    let selected = *preset == active_preset;
                    let focused = idx == self.selected_row;
                    let pcol = preset.colors();

                    canvas.fill_rounded_rect(card_x, py, card_w, 44, 8, theme.window_surface);
                    canvas.draw_rounded_rect_outline(
                        card_x,
                        py,
                        card_w,
                        44,
                        8,
                        if selected || focused { theme.border_active } else { theme.border_inactive },
                    );

                    canvas.fill_circle(card_x + 22, py + 22, 10, pcol.accent);
                    canvas.fill_circle(card_x + 34, py + 22, 6, pcol.accent_sec);

                    canvas.draw_text_clipped_font(card_x + 52, py + 5, card_w - 132, preset.name(), theme.text_primary, FontFace::UiBold);
                    canvas.draw_text_clipped_font(card_x + 52, py + 23, card_w - 132, preset.desc(), theme.text_muted, FontFace::Ui);

                    if selected {
                        canvas.fill_rounded_rect(card_x + card_w - 72, py + 11, 60, 22, 6, theme.accent);
                        canvas.draw_text_font(card_x + card_w - 62, py + 13, "ACTIVE", 0xFFFFFF, FontFace::UiBold);
                    }
                }

                let ty = cy + 48 + (ThemePreset::ALL.len() as i32) * 50 + 4;
                if ty + 40 <= cy + ch - 6 {
                    let focused = self.selected_row == ThemePreset::ALL.len();
                    canvas.fill_rounded_rect(card_x, ty, card_w, 40, 8, theme.window_surface);
                    canvas.draw_rounded_rect_outline(
                        card_x,
                        ty,
                        card_w,
                        40,
                        8,
                        if focused { theme.border_active } else { theme.border_inactive },
                    );
                    canvas.draw_text_clipped_font(
                        card_x + 14,
                        ty + 11,
                        card_w - 90,
                        "Desktop Logo Watermark",
                        theme.text_primary,
                        FontFace::UiBold,
                    );
                    let pill_bg = if show_watermark { theme.accent } else { theme.titlebar_active };
                    canvas.fill_rounded_rect(card_x + card_w - 64, ty + 8, 52, 24, 12, pill_bg);
                    canvas.draw_text_font(
                        card_x + card_w - 50,
                        ty + 11,
                        if show_watermark { "ON" } else { "OFF" },
                        0xFFFFFF,
                        FontFace::UiBold,
                    );
                }
            }
            SettingsTab::Accessibility => {
                let items = [
                    ("High Contrast Theme (Alt+C)", "Black/white/yellow contrast for low vision", active_preset == ThemePreset::HighContrast),
                    ("Large High-Vis Cursor 2x", "Double-size cursor easier to track on screen", a11y.large_cursor),
                    ("Screen Magnifier 2x (Alt+Z)", "Real-time zoom lens following pointer", a11y.magnifier),
                    ("Readable Large Text (Alt+L)", "1.5x text size in Notes, Files, Terminal & Reader", a11y.large_text),
                    ("Keyboard Mouse Keys (Alt+K)", "Move cursor with Arrows & click with Enter", a11y.mouse_keys),
                    ("Audio Feedback Chimes", "Play subtle PC speaker tones for A11y events", a11y.sound_cues),
                ];
                for (idx, (title, desc, on)) in items.iter().enumerate() {
                    let py = cy + 48 + (idx as i32) * 52;
                    if py + 46 > cy + ch - 6 {
                        break;
                    }
                    let focused = idx == self.selected_row;
                    canvas.fill_rounded_rect(card_x, py, card_w, 46, 8, theme.window_surface);
                    canvas.draw_rounded_rect_outline(
                        card_x,
                        py,
                        card_w,
                        46,
                        8,
                        if *on || focused { theme.border_active } else { theme.border_inactive },
                    );
                    canvas.draw_text_clipped_font(card_x + 14, py + 6, card_w - 92, title, theme.text_primary, FontFace::UiBold);
                    canvas.draw_text_clipped_font(card_x + 14, py + 25, card_w - 92, desc, theme.text_muted, FontFace::Ui);

                    let pill_bg = if *on { theme.accent } else { theme.titlebar_active };
                    canvas.fill_rounded_rect(card_x + card_w - 66, py + 11, 52, 24, 12, pill_bg);
                    canvas.draw_text_font(
                        card_x + card_w - 52,
                        py + 14,
                        if *on { "ON" } else { "OFF" },
                        0xFFFFFF,
                        FontFace::UiBold,
                    );
                }
            }
        }
    }
}
