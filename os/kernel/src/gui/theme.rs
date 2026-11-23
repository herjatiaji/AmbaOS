#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ThemePreset {
    ObsidianEmber,
    MidnightIndigo,
    CyberEmerald,
    PitchDark,
    HighContrast,
}

impl ThemePreset {
    pub const ALL: [ThemePreset; 5] = [
        ThemePreset::ObsidianEmber,
        ThemePreset::MidnightIndigo,
        ThemePreset::CyberEmerald,
        ThemePreset::PitchDark,
        ThemePreset::HighContrast,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ThemePreset::ObsidianEmber => "Obsidian Ember",
            ThemePreset::MidnightIndigo => "Midnight Indigo",
            ThemePreset::CyberEmerald => "Cyber Emerald",
            ThemePreset::PitchDark => "Pitch Dark",
            ThemePreset::HighContrast => "High Contrast (A11y)",
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            ThemePreset::ObsidianEmber => "Signature orange & obsidian glass",
            ThemePreset::MidnightIndigo => "Deep violet-blue workspace",
            ThemePreset::CyberEmerald => "Clean slate with neon mint",
            ThemePreset::PitchDark => "Pure OLED charcoal minimal",
            ThemePreset::HighContrast => "Max contrast black/white/yellow (WCAG AAA)",
        }
    }

    pub fn to_index(self) -> u8 {
        match self {
            ThemePreset::ObsidianEmber => 0,
            ThemePreset::MidnightIndigo => 1,
            ThemePreset::CyberEmerald => 2,
            ThemePreset::PitchDark => 3,
            ThemePreset::HighContrast => 4,
        }
    }

    pub fn from_index(idx: u8) -> Self {
        match idx {
            1 => ThemePreset::MidnightIndigo,
            2 => ThemePreset::CyberEmerald,
            3 => ThemePreset::PitchDark,
            4 => ThemePreset::HighContrast,
            _ => ThemePreset::ObsidianEmber,
        }
    }

    pub fn colors(self) -> ThemeColors {
        match self {
            ThemePreset::ObsidianEmber => ThemeColors {
                bg_top: 0x161B23,
                bg_bottom: 0x1E242E,
                bg_glow: 0xD97736,
                topbar_bg: 0x11151C,
                dock_bg: 0x222834,
                window_bg: 0x1B2029,
                window_surface: 0x252B36,
                titlebar_active: 0x2B313D,
                titlebar_inactive: 0x20252F,
                border_active: 0xB8632E,
                border_inactive: 0x353D4B,
                accent: 0xE67E22,
                accent_soft: 0xF39C12,
                accent_sec: 0x38BDF8,
                text_primary: 0xF8FAFC,
                text_secondary: 0xCBD5E1,
                text_muted: 0x7E899B,
                term_bg: 0x141820,
                term_prompt: 0x4ADE80,
                term_green: 0x4ADE80,
                err: 0xF87171,
            },
            ThemePreset::MidnightIndigo => ThemeColors {
                bg_top: 0x090C18,
                bg_bottom: 0x151B33,
                bg_glow: 0x7C6FF0,
                topbar_bg: 0x0E1220,
                dock_bg: 0x141A2E,
                window_bg: 0x131828,
                window_surface: 0x1B2238,
                titlebar_active: 0x202945,
                titlebar_inactive: 0x151B2D,
                border_active: 0x7C6FF0,
                border_inactive: 0x2A3452,
                accent: 0x7C6FF0,
                accent_soft: 0xA29BFE,
                accent_sec: 0x00D2D3,
                text_primary: 0xF8FAFC,
                text_secondary: 0xCBD5E1,
                text_muted: 0x6B7280,
                term_bg: 0x0A0E1A,
                term_prompt: 0x00D2D3,
                term_green: 0x4ADE80,
                err: 0xF87171,
            },
            ThemePreset::CyberEmerald => ThemeColors {
                bg_top: 0x081010,
                bg_bottom: 0x112021,
                bg_glow: 0x10B981,
                topbar_bg: 0x0C1617,
                dock_bg: 0x122022,
                window_bg: 0x111B1D,
                window_surface: 0x182629,
                titlebar_active: 0x1D2E32,
                titlebar_inactive: 0x131E20,
                border_active: 0x10B981,
                border_inactive: 0x263A3E,
                accent: 0x10B981,
                accent_soft: 0x34D399,
                accent_sec: 0x38BDF8,
                text_primary: 0xF8FAFC,
                text_secondary: 0xCBD5E1,
                text_muted: 0x64748B,
                term_bg: 0x091112,
                term_prompt: 0x34D399,
                term_green: 0x4ADE80,
                err: 0xF87171,
            },
            ThemePreset::PitchDark => ThemeColors {
                bg_top: 0x070709,
                bg_bottom: 0x111116,
                bg_glow: 0xE2E8F0,
                topbar_bg: 0x0B0B0F,
                dock_bg: 0x121218,
                window_bg: 0x101015,
                window_surface: 0x181820,
                titlebar_active: 0x1E1E28,
                titlebar_inactive: 0x121218,
                border_active: 0xFF6A00,
                border_inactive: 0x272733,
                accent: 0xFF6A00,
                accent_soft: 0xFB923C,
                accent_sec: 0x38BDF8,
                text_primary: 0xFAFAFA,
                text_secondary: 0xD4D4D8,
                text_muted: 0x71717A,
                term_bg: 0x08080B,
                term_prompt: 0xFB923C,
                term_green: 0x4ADE80,
                err: 0xF87171,
            },
            ThemePreset::HighContrast => ThemeColors {
                bg_top: 0x000000,
                bg_bottom: 0x050608,
                bg_glow: 0xFACC15,
                topbar_bg: 0x000000,
                dock_bg: 0x080A10,
                window_bg: 0x000000,
                window_surface: 0x121620,
                titlebar_active: 0x1D283A,
                titlebar_inactive: 0x0B0E14,
                border_active: 0xFACC15,
                border_inactive: 0xCBD5E1,
                accent: 0x1D4ED8,
                accent_soft: 0xFACC15,
                accent_sec: 0x22D3EE,
                text_primary: 0xFFFFFF,
                text_secondary: 0xF8FAFC,
                text_muted: 0xE2E8F0,
                term_bg: 0x000000,
                term_prompt: 0xFACC15,
                term_green: 0x4ADE80,
                err: 0xFF6B6B,
            },
        }
    }
}

#[derive(Clone, Copy)]
pub struct ThemeColors {
    pub bg_top: u32,
    pub bg_bottom: u32,
    pub bg_glow: u32,
    pub topbar_bg: u32,
    pub dock_bg: u32,
    pub window_bg: u32,
    pub window_surface: u32,
    pub titlebar_active: u32,
    pub titlebar_inactive: u32,
    pub border_active: u32,
    pub border_inactive: u32,
    pub accent: u32,
    pub accent_soft: u32,
    pub accent_sec: u32,
    pub text_primary: u32,
    pub text_secondary: u32,
    pub text_muted: u32,
    pub term_bg: u32,
    pub term_prompt: u32,
    pub term_green: u32,
    pub err: u32,
}

pub const BTN_CLOSE: u32 = 0xFF5F56;
pub const BTN_MIN: u32 = 0xFFBD2E;
pub const BTN_MAX: u32 = 0x27C93F;

#[derive(Clone, Copy, Default)]
pub struct AccessibilityConfig {
    pub large_cursor: bool,
    pub magnifier: bool,
    pub large_text: bool,
    pub mouse_keys: bool,
    pub sound_cues: bool,
}
