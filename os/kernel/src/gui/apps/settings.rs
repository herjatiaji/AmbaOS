use crate::gui::canvas::Canvas;
use crate::gui::font::FontFace;
use crate::gui::theme::{ThemeColors, ThemePreset};

#[derive(Clone, Copy)]
pub enum SettingsAction {
    None,
    SelectTheme(ThemePreset),
    ToggleWatermark,
}

pub struct SettingsApp;

impl SettingsApp {
    pub fn new() -> Self {
        Self
    }

    pub fn handle_click(&self, local_x: i32, local_y: i32, cw: i32) -> SettingsAction {
        let card_x = 16;
        let card_w = cw - 32;
        if local_x < card_x || local_x > card_x + card_w {
            return SettingsAction::None;
        }
        // Theme cards at y: 40..256 (each 48px + 6px gap)
        for (idx, preset) in ThemePreset::ALL.iter().enumerate() {
            let cy = 40 + (idx as i32) * 54;
            if local_y >= cy && local_y <= cy + 46 {
                return SettingsAction::SelectTheme(*preset);
            }
        }
        // Watermark toggle row
        if local_y >= 268 && local_y <= 308 {
            return SettingsAction::ToggleWatermark;
        }
        SettingsAction::None
    }

    pub fn render(
        &self,
        canvas: &mut Canvas,
        theme: ThemeColors,
        active_preset: ThemePreset,
        show_watermark: bool,
        cx: i32,
        cy: i32,
        cw: i32,
        ch: i32,
    ) {
        canvas.fill_rect(cx, cy, cw, ch, theme.window_bg);

        canvas.draw_text_clipped_font(cx + 16, cy + 12, cw - 32, "Desktop Theme Preset", theme.text_primary, FontFace::UiBold);

        let card_x = cx + 16;
        let card_w = cw - 32;
        for (idx, preset) in ThemePreset::ALL.iter().enumerate() {
            let py = cy + 40 + (idx as i32) * 54;
            if py + 46 > cy + ch - 10 {
                break;
            }
            let selected = *preset == active_preset;
            let pcol = preset.colors();

            canvas.fill_rounded_rect(card_x, py, card_w, 46, 8, theme.window_surface);
            canvas.draw_rounded_rect_outline(
                card_x,
                py,
                card_w,
                46,
                8,
                if selected { theme.accent } else { theme.border_inactive },
            );

            // Color swatch circle
            canvas.fill_circle(card_x + 24, py + 23, 10, pcol.accent);
            canvas.fill_circle(card_x + 36, py + 23, 6, pcol.accent_sec);

            canvas.draw_text_clipped_font(card_x + 54, py + 6, card_w - 130, preset.name(), theme.text_primary, FontFace::UiBold);
            canvas.draw_text_clipped_font(card_x + 54, py + 24, card_w - 130, preset.desc(), theme.text_muted, FontFace::Ui);

            if selected {
                canvas.fill_rounded_rect(card_x + card_w - 72, py + 12, 60, 22, 6, theme.accent);
                canvas.draw_text_font(card_x + card_w - 62, py + 14, "ACTIVE", 0xFFFFFF, FontFace::UiBold);
            }
        }

        // Watermark toggle
        let ty = cy + 268;
        if ty + 40 <= cy + ch - 6 {
            canvas.fill_rounded_rect(card_x, ty, card_w, 40, 8, theme.window_surface);
            canvas.draw_rounded_rect_outline(card_x, ty, card_w, 40, 8, theme.border_inactive);
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
}
