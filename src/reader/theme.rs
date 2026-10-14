use egui::Color32;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReaderThemeType {
    #[default]
    Dark,
    OledBlack,
    PaperLight,
    WarmSepia,
    Nord,
    Midnight,
}

impl ReaderThemeType {
    pub fn all() -> &'static [ReaderThemeType] {
        &[
            ReaderThemeType::Dark,
            ReaderThemeType::OledBlack,
            ReaderThemeType::PaperLight,
            ReaderThemeType::WarmSepia,
            ReaderThemeType::Nord,
            ReaderThemeType::Midnight,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ReaderThemeType::Dark => "Dark Slate",
            ReaderThemeType::OledBlack => "OLED Black",
            ReaderThemeType::PaperLight => "Paper Light",
            ReaderThemeType::WarmSepia => "Warm Sepia",
            ReaderThemeType::Nord => "Nord Arctic",
            ReaderThemeType::Midnight => "Midnight Blue",
        }
    }

    pub fn colors(&self) -> ReaderColors {
        match self {
            ReaderThemeType::Dark => ReaderColors {
                bg: Color32::from_rgb(24, 25, 28),
                surface: Color32::from_rgb(34, 36, 42),
                card: Color32::from_rgb(42, 45, 52),
                text: Color32::from_rgb(226, 232, 240),
                text_muted: Color32::from_rgb(148, 163, 184),
                accent: Color32::from_rgb(153, 34, 34), // AO3 Crimson
                accent_hover: Color32::from_rgb(180, 45, 45),
                border: Color32::from_rgb(51, 56, 66),
                quote_bg: Color32::from_rgb(30, 32, 38),
                notes_bg: Color32::from_rgb(29, 31, 37),
                is_dark: true,
            },
            ReaderThemeType::OledBlack => ReaderColors {
                bg: Color32::from_rgb(0, 0, 0),
                surface: Color32::from_rgb(16, 16, 16),
                card: Color32::from_rgb(24, 24, 24),
                text: Color32::from_rgb(241, 245, 249),
                text_muted: Color32::from_rgb(130, 130, 130),
                accent: Color32::from_rgb(175, 40, 40),
                accent_hover: Color32::from_rgb(205, 50, 50),
                border: Color32::from_rgb(38, 38, 38),
                quote_bg: Color32::from_rgb(12, 12, 12),
                notes_bg: Color32::from_rgb(14, 14, 14),
                is_dark: true,
            },
            ReaderThemeType::PaperLight => ReaderColors {
                bg: Color32::from_rgb(250, 250, 252),
                surface: Color32::from_rgb(241, 243, 246),
                card: Color32::from_rgb(255, 255, 255),
                text: Color32::from_rgb(33, 37, 41),
                text_muted: Color32::from_rgb(108, 117, 125),
                accent: Color32::from_rgb(153, 0, 0), // AO3 Crimson
                accent_hover: Color32::from_rgb(180, 20, 20),
                border: Color32::from_rgb(222, 226, 230),
                quote_bg: Color32::from_rgb(243, 244, 247),
                notes_bg: Color32::from_rgb(238, 242, 246),
                is_dark: false,
            },
            ReaderThemeType::WarmSepia => ReaderColors {
                bg: Color32::from_rgb(251, 240, 217),
                surface: Color32::from_rgb(242, 229, 201),
                card: Color32::from_rgb(247, 236, 211),
                text: Color32::from_rgb(74, 59, 50),
                text_muted: Color32::from_rgb(138, 120, 107),
                accent: Color32::from_rgb(160, 64, 48),
                accent_hover: Color32::from_rgb(180, 80, 60),
                border: Color32::from_rgb(222, 207, 178),
                quote_bg: Color32::from_rgb(243, 231, 204),
                notes_bg: Color32::from_rgb(239, 225, 196),
                is_dark: false,
            },
            ReaderThemeType::Nord => ReaderColors {
                bg: Color32::from_rgb(46, 52, 64),
                surface: Color32::from_rgb(59, 66, 82),
                card: Color32::from_rgb(67, 76, 94),
                text: Color32::from_rgb(236, 239, 244),
                text_muted: Color32::from_rgb(163, 190, 140),
                accent: Color32::from_rgb(191, 97, 106), // Nord Red
                accent_hover: Color32::from_rgb(208, 114, 123),
                border: Color32::from_rgb(76, 86, 106),
                quote_bg: Color32::from_rgb(51, 58, 72),
                notes_bg: Color32::from_rgb(54, 61, 76),
                is_dark: true,
            },
            ReaderThemeType::Midnight => ReaderColors {
                bg: Color32::from_rgb(15, 23, 42),
                surface: Color32::from_rgb(30, 41, 59),
                card: Color32::from_rgb(40, 53, 76),
                text: Color32::from_rgb(241, 245, 249),
                text_muted: Color32::from_rgb(148, 163, 184),
                accent: Color32::from_rgb(225, 29, 72),
                accent_hover: Color32::from_rgb(244, 63, 94),
                border: Color32::from_rgb(51, 65, 85),
                quote_bg: Color32::from_rgb(22, 33, 56),
                notes_bg: Color32::from_rgb(24, 36, 61),
                is_dark: true,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReaderColors {
    pub bg: Color32,
    pub surface: Color32,
    pub card: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub border: Color32,
    pub quote_bg: Color32,
    pub notes_bg: Color32,
    pub is_dark: bool,
}
