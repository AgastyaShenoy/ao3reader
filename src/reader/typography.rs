use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FontFamilyChoice {
    #[default]
    SansSerif,
    Serif,
    Monospace,
}

impl FontFamilyChoice {
    pub fn all() -> &'static [FontFamilyChoice] {
        &[
            FontFamilyChoice::SansSerif,
            FontFamilyChoice::Serif,
            FontFamilyChoice::Monospace,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            FontFamilyChoice::SansSerif => "Modern Sans",
            FontFamilyChoice::Serif => "Classic Serif",
            FontFamilyChoice::Monospace => "Code Monospace",
        }
    }

    pub fn to_egui_family(&self) -> egui::FontFamily {
        match self {
            FontFamilyChoice::SansSerif | FontFamilyChoice::Serif => egui::FontFamily::Proportional,
            FontFamilyChoice::Monospace => egui::FontFamily::Monospace,
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
        }
    }
}
