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
    HighContrast,
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
            ReaderThemeType::HighContrast,
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
            ReaderThemeType::HighContrast => "High Contrast",
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
                text_muted: Rgb(98, 106, 114),
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
                text_muted: Rgb(112, 95, 83),
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
                text_muted: Rgb(184, 192, 208),
                accent: Rgb(176, 82, 92), // Nord Red, deepened for white text
                accent_hover: Rgb(191, 97, 106),
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
            // White on black with bright edges, and the classic high-contrast yellow for the
            // accent (with black text on it): every role is far past the WCAG minimums.
            ReaderThemeType::HighContrast => ThemePalette {
                bg: Rgb(0, 0, 0),
                surface: Rgb(0, 0, 0),
                card: Rgb(0, 0, 0),
                text: Rgb(255, 255, 255),
                text_muted: Rgb(214, 214, 214),
                accent: Rgb(255, 214, 10),
                accent_hover: Rgb(255, 228, 92),
                border: Rgb(170, 170, 170),
                quote_bg: Rgb(18, 18, 18),
                notes_bg: Rgb(14, 14, 14),
                is_dark: true,
            },
        }
    }
}

/// An sRGB colour, independent of any UI toolkit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// WCAG relative luminance.
    pub fn luminance(self) -> f32 {
        let lin = |c: u8| {
            let c = c as f32 / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(self.0) + 0.7152 * lin(self.1) + 0.0722 * lin(self.2)
    }

    /// WCAG contrast ratio between two colours (1–21).
    pub fn contrast(self, o: Rgb) -> f32 {
        let (a, b) = (self.luminance(), o.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    pub fn mix(self, o: Rgb, t: f32) -> Rgb {
        let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgb(m(self.0, o.0), m(self.1, o.1), m(self.2, o.2))
    }

    /// This colour, moved toward `toward` just far enough to reach `ratio` contrast on `on`.
    pub fn with_contrast(self, on: Rgb, toward: Rgb, ratio: f32) -> Rgb {
        (0..=20).map(|i| self.mix(toward, i as f32 / 20.0)).find(|c| c.contrast(on) >= ratio).unwrap_or(toward)
    }

    /// This colour, moved toward `toward` just far enough to read as text (4.5:1) on `on`.
    pub fn readable_on(self, on: Rgb, toward: Rgb) -> Rgb {
        self.with_contrast(on, toward, 4.5)
    }
}

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

impl ThemePalette {
    /// The accent as a text colour: the crimson itself on light themes, a softened tone of it
    /// on dark ones, where the raw accent is too dark to read.
    pub fn accent_text(&self) -> Rgb {
        self.accent.readable_on(self.card, self.text)
    }

    /// The accent for marks that aren't text (the logo, progress bars, the current tab): kept
    /// saturated, and lightened on dark themes just enough to stand out (3:1).
    pub fn accent_mark(&self) -> Rgb {
        self.accent.with_contrast(self.card, Rgb(255, 118, 118), 3.0)
    }

    /// Text on an accent fill: white, or black on a light accent (High Contrast's yellow).
    pub fn on_accent(&self) -> Rgb {
        let white = Rgb(255, 255, 255);
        if white.contrast(self.accent) >= 4.5 { white } else { Rgb(0, 0, 0) }
    }

    /// Hearts on romantic relationship tags: a rose that stands out (3:1) on cards.
    pub fn romance(&self) -> Rgb {
        let toward = if self.is_dark { Rgb(255, 170, 190) } else { Rgb(120, 0, 30) };
        Rgb(225, 29, 72).with_contrast(self.card, toward, 3.0)
    }

    /// Error text that stays readable on cards.
    pub fn danger_text(&self) -> Rgb {
        let base = if self.is_dark { Rgb(248, 113, 113) } else { Rgb(185, 28, 28) };
        base.readable_on(self.card, self.text)
    }

    /// Warning text (amber) that stays readable on cards.
    pub fn warning_text(&self) -> Rgb {
        let base = if self.is_dark { Rgb(245, 180, 60) } else { Rgb(160, 82, 0) };
        base.readable_on(self.card, self.text)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_keeps_text_readable() {
        for t in ReaderThemeType::all() {
            let p = t.palette();
            for (role, fg) in [("text", p.text), ("muted", p.text_muted), ("accent", p.accent_text()), ("danger", p.danger_text()), ("warning", p.warning_text())] {
                for (bg_name, bg) in [("card", p.card), ("surface", p.surface), ("bg", p.bg)] {
                    let ratio = fg.contrast(bg);
                    assert!(ratio >= 4.4, "{}: {role} on {bg_name} is {ratio:.2}:1", t.display_name());
                }
            }
            let mark = p.accent_mark().contrast(p.surface).min(p.accent_mark().contrast(p.card));
            assert!(mark >= 3.0, "{}: accent mark is {mark:.2}:1", t.display_name());
            let on_accent = p.on_accent().contrast(p.accent);
            assert!(on_accent >= 4.5, "{}: text on accent is {on_accent:.2}:1", t.display_name());
            let rose = p.romance().contrast(p.card);
            assert!(rose >= 3.0, "{}: relationship heart is {rose:.2}:1", t.display_name());
            let edge = p.border.contrast(p.card);
            if *t == ReaderThemeType::HighContrast {
                assert!(edge >= 3.0, "{}: card edges are {edge:.2}:1", t.display_name());
            }
        }
    }
}
