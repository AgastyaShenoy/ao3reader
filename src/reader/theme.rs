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

    pub fn palette(&self) -> ThemePalette {
        match self {
            ReaderThemeType::Dark => ThemePalette {
                bg: Rgb(24, 25, 28),
                surface: Rgb(34, 36, 42),
                card: Rgb(42, 45, 52),
                text: Rgb(226, 232, 240),
                text_muted: Rgb(148, 163, 184),
                accent: Rgb(153, 34, 34), // AO3 Crimson
                accent_hover: Rgb(180, 45, 45),
                border: Rgb(51, 56, 66),
                quote_bg: Rgb(30, 32, 38),
                notes_bg: Rgb(29, 31, 37),
                is_dark: true,
            },
            ReaderThemeType::OledBlack => ThemePalette {
                bg: Rgb(0, 0, 0),
                surface: Rgb(16, 16, 16),
                card: Rgb(24, 24, 24),
                text: Rgb(241, 245, 249),
                text_muted: Rgb(130, 130, 130),
                accent: Rgb(175, 40, 40),
                accent_hover: Rgb(205, 50, 50),
                border: Rgb(38, 38, 38),
                quote_bg: Rgb(12, 12, 12),
                notes_bg: Rgb(14, 14, 14),
                is_dark: true,
            },
            ReaderThemeType::PaperLight => ThemePalette {
                bg: Rgb(250, 250, 252),
                surface: Rgb(241, 243, 246),
                card: Rgb(255, 255, 255),
                text: Rgb(33, 37, 41),
                text_muted: Rgb(108, 117, 125),
                accent: Rgb(153, 0, 0), // AO3 Crimson
                accent_hover: Rgb(180, 20, 20),
                border: Rgb(222, 226, 230),
                quote_bg: Rgb(243, 244, 247),
                notes_bg: Rgb(238, 242, 246),
                is_dark: false,
            },
            ReaderThemeType::WarmSepia => ThemePalette {
                bg: Rgb(251, 240, 217),
                surface: Rgb(242, 229, 201),
                card: Rgb(247, 236, 211),
                text: Rgb(74, 59, 50),
                text_muted: Rgb(138, 120, 107),
                accent: Rgb(160, 64, 48),
                accent_hover: Rgb(180, 80, 60),
                border: Rgb(222, 207, 178),
                quote_bg: Rgb(243, 231, 204),
                notes_bg: Rgb(239, 225, 196),
                is_dark: false,
            },
            ReaderThemeType::Nord => ThemePalette {
                bg: Rgb(46, 52, 64),
                surface: Rgb(59, 66, 82),
                card: Rgb(67, 76, 94),
                text: Rgb(236, 239, 244),
                text_muted: Rgb(163, 190, 140),
                accent: Rgb(191, 97, 106), // Nord Red
                accent_hover: Rgb(208, 114, 123),
                border: Rgb(76, 86, 106),
                quote_bg: Rgb(51, 58, 72),
                notes_bg: Rgb(54, 61, 76),
                is_dark: true,
            },
            ReaderThemeType::Midnight => ThemePalette {
                bg: Rgb(15, 23, 42),
                surface: Rgb(30, 41, 59),
                card: Rgb(40, 53, 76),
                text: Rgb(241, 245, 249),
                text_muted: Rgb(148, 163, 184),
                accent: Rgb(225, 29, 72),
                accent_hover: Rgb(244, 63, 94),
                border: Rgb(51, 65, 85),
                quote_bg: Rgb(22, 33, 56),
                notes_bg: Rgb(24, 36, 61),
                is_dark: true,
            },
        }
    }
}

/// An sRGB colour, independent of any UI toolkit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemePalette {
    pub bg: Rgb,
    pub surface: Rgb,
    pub card: Rgb,
    pub text: Rgb,
    pub text_muted: Rgb,
    pub accent: Rgb,
    pub accent_hover: Rgb,
    pub border: Rgb,
    pub quote_bg: Rgb,
    pub notes_bg: Rgb,
    pub is_dark: bool,
}

#[cfg(feature = "legacy-ui")]
pub use legacy::ReaderColors;

#[cfg(feature = "legacy-ui")]
mod legacy {
    use super::{ReaderThemeType, Rgb, ThemePalette};
    use egui::Color32;

    impl From<Rgb> for Color32 {
        fn from(c: Rgb) -> Self {
            Color32::from_rgb(c.0, c.1, c.2)
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

    impl From<ThemePalette> for ReaderColors {
        fn from(p: ThemePalette) -> Self {
            Self {
                bg: p.bg.into(),
                surface: p.surface.into(),
                card: p.card.into(),
                text: p.text.into(),
                text_muted: p.text_muted.into(),
                accent: p.accent.into(),
                accent_hover: p.accent_hover.into(),
                border: p.border.into(),
                quote_bg: p.quote_bg.into(),
                notes_bg: p.notes_bg.into(),
                is_dark: p.is_dark,
            }
        }
    }

    impl ReaderThemeType {
        pub fn colors(&self) -> ReaderColors {
            self.palette().into()
        }
    }
}
