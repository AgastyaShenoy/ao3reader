use crate::ao3::models::{Category, WorkMeta, WorkRating};
use crate::reader::theme::ReaderColors;
use egui::{Color32, CornerRadius, Frame, Margin, RichText, Ui};

pub struct UiComponents;

impl UiComponents {
    /// Render color-coded rating badge matching official AO3 colors
    pub fn rating_badge(ui: &mut Ui, rating: WorkRating) {
        let rgb = rating.badge_color();
        let bg_color = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);

        Frame::NONE
            .fill(bg_color)
            .corner_radius(CornerRadius::same(4))
            .inner_margin(Margin::symmetric(7, 3))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(rating.display_name())
                        .color(Color32::WHITE)
                        .size(11.0)
                        .strong(),
                );
            });
    }

    /// Render category badges (F/M, M/M, Gen, etc.)
    pub fn category_badge(ui: &mut Ui, category: &Category, colors: &ReaderColors) {
        let (bg, text) = match category {
            Category::MM => (Color32::from_rgb(46, 117, 182), Color32::WHITE),
            Category::FM => (Color32::from_rgb(155, 89, 182), Color32::WHITE),
            Category::FF => (Color32::from_rgb(231, 76, 60), Color32::WHITE),
            Category::Gen => (Color32::from_rgb(39, 174, 96), Color32::WHITE),
            Category::Multi => (Color32::from_rgb(230, 126, 34), Color32::WHITE),
            Category::Other | Category::Unknown(_) => (colors.card, colors.text_muted),
        };

        Frame::NONE
            .fill(bg)
            .corner_radius(CornerRadius::same(4))
            .inner_margin(Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.label(RichText::new(category.display_name()).color(text).size(10.5).strong());
            });
    }

    /// Render tag badge
    pub fn tag_badge(ui: &mut Ui, text: &str, colors: &ReaderColors) {
        Frame::NONE
            .fill(colors.surface)
            .corner_radius(CornerRadius::same(4))
            .inner_margin(Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.label(RichText::new(text).color(colors.text_muted).size(11.0));
            });
    }

    /// Render relationship badge
    pub fn relationship_badge(ui: &mut Ui, rel: &str, colors: &ReaderColors) {
        Frame::NONE
            .fill(colors.card)
            .corner_radius(CornerRadius::same(4))
            .inner_margin(Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.label(RichText::new(format!("♥ {rel}")).color(colors.accent).size(11.0).strong());
            });
    }

    /// Render warning badge
    pub fn warning_badge(ui: &mut Ui, warning: &str) {
        let is_none_apply = warning.to_lowercase().contains("no archive warnings apply");
        let bg = if is_none_apply {
            Color32::from_rgb(34, 139, 34) // Forest Green
        } else {
            Color32::from_rgb(180, 40, 40) // Warning Red
        };

        Frame::NONE
            .fill(bg)
            .corner_radius(CornerRadius::same(4))
            .inner_margin(Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.label(RichText::new(warning).color(Color32::WHITE).size(10.5));
            });
    }

    /// Render summary snippet
    pub fn render_meta_summary(ui: &mut Ui, meta: &WorkMeta, colors: &ReaderColors, max_lines: usize) {
        if !meta.summary_text.is_empty() {
            let mut lines: Vec<&str> = meta.summary_text.lines().collect();
            if lines.len() > max_lines {
                lines.truncate(max_lines);
            }
            let truncated = lines.join(" ");
            let clean = if truncated.len() > 240 {
                format!("{}...", &truncated[..237])
            } else {
                truncated
            };

            ui.label(RichText::new(clean).color(colors.text_muted).size(12.0).italics());
        }
    }
}
