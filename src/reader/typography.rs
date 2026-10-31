use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FontFamilyChoice {
    #[default]
    SansSerif,
    Serif,
    Monospace,
    Custom(String),
    /// Atkinson Hyperlegible (Braille Institute), bundled: clear letter shapes for low vision.
    Atkinson,
    /// OpenDyslexic, bundled: weighted letter bottoms some dyslexic readers find easier.
    OpenDyslexic,
}

impl FontFamilyChoice {
    pub const READER_BOOK_FONT: &'static str = "ReaderBookFont";

    pub fn all_standard() -> &'static [FontFamilyChoice] {
        &[
            FontFamilyChoice::SansSerif,
            FontFamilyChoice::Serif,
            FontFamilyChoice::Monospace,
        ]
    }

    pub fn all() -> &'static [FontFamilyChoice] {
        Self::all_standard()
    }

    pub fn display_name(&self) -> &str {
        match self {
            FontFamilyChoice::SansSerif => "Modern Sans",
            FontFamilyChoice::Serif => "Classic Serif",
            FontFamilyChoice::Monospace => "Code Monospace",
            FontFamilyChoice::Custom(name) => name.as_str(),
            FontFamilyChoice::Atkinson => "Atkinson Hyperlegible",
            FontFamilyChoice::OpenDyslexic => "OpenDyslexic",
        }
    }

    #[cfg(feature = "legacy-ui")]
    pub fn to_egui_family(&self, ctx: &egui::Context) -> egui::FontFamily {
        match self {
            FontFamilyChoice::Monospace => egui::FontFamily::Monospace,
            FontFamilyChoice::SansSerif | FontFamilyChoice::Atkinson | FontFamilyChoice::OpenDyslexic => egui::FontFamily::Proportional,
            FontFamilyChoice::Serif | FontFamilyChoice::Custom(_) => {
                let name = egui::FontFamily::Name(Self::READER_BOOK_FONT.into());
                let is_bound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    ctx.fonts(|f| f.families().contains(&name))
                }))
                .unwrap_or(false);

                if is_bound {
                    name
                } else {
                    egui::FontFamily::Proportional
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TextAlignment {
    #[default]
    Left,
    Justified,
    Center,
}

impl TextAlignment {
    pub fn display_name(&self) -> &'static str {
        match self {
            TextAlignment::Left => "Left Aligned",
            TextAlignment::Justified => "Justified",
            TextAlignment::Center => "Centered",
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReadingMode {
    #[default]
    Paged,
    Scroll,
}

impl ReadingMode {
    pub fn all() -> &'static [ReadingMode] {
        &[ReadingMode::Paged, ReadingMode::Scroll]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ReadingMode::Paged => "📖 E-Reader (Paged / Flip)",
            ReadingMode::Scroll => "📜 Continuous Scroll",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PageLayoutMode {
    #[default]
    Auto,
    Single,
    Dual,
}

impl PageLayoutMode {
    pub fn all() -> &'static [PageLayoutMode] {
        &[PageLayoutMode::Auto, PageLayoutMode::Single, PageLayoutMode::Dual]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            PageLayoutMode::Auto => "Auto (Dual on wide screen)",
            PageLayoutMode::Single => "Single Page",
            PageLayoutMode::Dual => "Two-Page Book Spread",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypographySettings {
    pub font_family: FontFamilyChoice,
    pub font_size: f32,
    pub line_spacing_multiplier: f32,
    pub max_column_width: f32,
    pub side_margin: f32,
    pub paragraph_spacing: f32,
    pub indent_paragraphs: bool,
    pub text_alignment: TextAlignment,
    pub show_chapter_pre_notes: bool,
    pub show_chapter_post_notes: bool,
    pub show_chapter_summary: bool,
    pub show_reading_progress_bar: bool,
    #[serde(default)]
    pub reading_mode: ReadingMode,
    #[serde(default)]
    pub page_layout: PageLayoutMode,
    #[serde(default = "default_true")]
    pub show_page_turn_indicators: bool,
    #[serde(default = "default_true")]
    pub page_flip_mouse_wheel: bool,
    #[serde(default)]
    pub custom_font_name: Option<String>,
    #[serde(default)]
    pub custom_font_path: Option<std::path::PathBuf>,
    /// Font for the app's own interface (menus, buttons, labels); None keeps the system face.
    #[serde(default)]
    pub ui_font: Option<String>,
    /// Extra space between letters, and between words, as a fraction of the text size.
    #[serde(default)]
    pub letter_spacing: f32,
    #[serde(default)]
    pub word_spacing: f32,
    /// Bold the first part of every word ("bold word starts").
    #[serde(default)]
    pub bold_word_starts: bool,
    /// Highlight the line being read and dim the rest of the page.
    #[serde(default)]
    pub reading_ruler: bool,
    /// A soft colour laid over the page (0xRRGGBB), and how strong it is (0–0.5).
    #[serde(default)]
    pub tint: Option<u32>,
    #[serde(default = "default_tint_strength")]
    pub tint_strength: f32,
    /// Only the text: no toolbar, running heads, page numbers or progress bar.
    #[serde(default)]
    pub focus_mode: bool,
}

fn default_tint_strength() -> f32 {
    0.18
}

impl Default for TypographySettings {
    fn default() -> Self {
        Self {
            font_family: FontFamilyChoice::SansSerif,
            font_size: 18.0,
            line_spacing_multiplier: 1.6,
            max_column_width: 780.0,
            side_margin: 28.0,
            paragraph_spacing: 14.0,
            indent_paragraphs: false,
            text_alignment: TextAlignment::Left,
            show_chapter_pre_notes: true,
            show_chapter_post_notes: true,
            show_chapter_summary: true,
            show_reading_progress_bar: true,
            reading_mode: ReadingMode::Paged,
            page_layout: PageLayoutMode::Auto,
            show_page_turn_indicators: true,
            page_flip_mouse_wheel: true,
            custom_font_name: None,
            custom_font_path: None,
            ui_font: None,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            bold_word_starts: false,
            reading_ruler: false,
            tint: None,
            tint_strength: default_tint_strength(),
            focus_mode: false,
        }
    }
}
