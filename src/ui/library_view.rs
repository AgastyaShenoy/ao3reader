use crate::ao3::models::{LibraryEntry, ShelfStatus};
use crate::reader::theme::ReaderColors;
use crate::storage::config::AppConfig;
use crate::storage::library::LibraryManager;
use crate::ui::components::UiComponents;
use egui::{
    Align, Button, Color32, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke, Ui,
};

pub enum LibraryAction {
    None,
    OpenWork(String), // Work ID
    SwitchToFetch,
    ExportWork(String, ExportFormat), // Work ID, Format
    ToggleFavorite(String),
    SetShelf(String, ShelfStatus),
    DeleteWork(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Html,
    Markdown,
    Txt,
}

pub struct LibraryView {
    pub search_query: String,
    pub selected_shelf: Option<ShelfStatus>,
    pub only_favorites: bool,
    pub confirm_delete_id: Option<String>,
}

impl LibraryView {
    pub fn new() -> Self {
        Self {
            search_query: String::new(),
            selected_shelf: None,
            only_favorites: false,
            confirm_delete_id: None,
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        library: &mut LibraryManager,
        config: &AppConfig,
    ) -> LibraryAction {
        let mut action = LibraryAction::None;
        let colors = config.theme.colors();

        // Top Header & Filters
        self.render_header_and_filters(ui, &colors, library, &mut action);

        ui.add_space(10.0);

        // Delete confirmation modal if triggered
        if let Some(del_id) = &self.confirm_delete_id {
            let to_delete = del_id.clone();
            Frame::NONE
                .fill(colors.card)
                .stroke(Stroke::new(1.0, Color32::from_rgb(239, 68, 68)))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("⚠️ Are you sure you want to delete this work from your library?").color(colors.text).strong());
                        if ui.button("Yes, Delete").clicked() {
                            action = LibraryAction::DeleteWork(to_delete);
                            self.confirm_delete_id = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_delete_id = None;
                        }
                    });
                });
            ui.add_space(10.0);
        }

        // Works List
        let entries = library.query_entries(
            &self.search_query,
            self.selected_shelf,
            self.only_favorites,
        );

        if entries.is_empty() {
            self.render_empty_state(ui, &colors, &mut action);
        } else {
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(6.0);
                for entry in entries {
                    self.render_work_card(ui, entry, &colors, &mut action);
                    ui.add_space(12.0);
                }
            });
        }

        // Apply mutations to library if requested
        match &action {
            LibraryAction::ToggleFavorite(id) => library.toggle_favorite(id),
            LibraryAction::SetShelf(id, shelf) => library.set_shelf(id, *shelf),
            LibraryAction::DeleteWork(id) => library.delete_work(id),
            _ => {}
        }

        action
    }

    fn render_header_and_filters(
        &mut self,
        ui: &mut Ui,
        colors: &ReaderColors,
        library: &LibraryManager,
        action: &mut LibraryAction,
    ) {
        Frame::NONE
            .fill(colors.surface)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(14))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    // Search bar
                    ui.label(RichText::new("🔍").size(14.0).color(colors.text_muted));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search_query)
                            .desired_width(220.0)
                            .hint_text("Search title, author, fandom, tag..."),
                    );

                    if !self.search_query.is_empty() {
                        if ui.button("✖").clicked() {
                            self.search_query.clear();
                        }
                    }

                    ui.separator();

                    // Shelf Tabs
                    let all_active = self.selected_shelf.is_none() && !self.only_favorites;
                    if ui.selectable_label(all_active, format!("All ({})", library.entries().len())).clicked() {
                        self.selected_shelf = None;
                        self.only_favorites = false;
                    }

                    let reading_count = library.entries().values().filter(|e| e.shelf == ShelfStatus::Reading).count();
                    let is_reading = self.selected_shelf == Some(ShelfStatus::Reading) && !self.only_favorites;
                    if ui.selectable_label(is_reading, format!("Reading ({reading_count})")).clicked() {
                        self.selected_shelf = Some(ShelfStatus::Reading);
                        self.only_favorites = false;
                    }

                    let completed_count = library.entries().values().filter(|e| e.shelf == ShelfStatus::Completed).count();
                    let is_completed = self.selected_shelf == Some(ShelfStatus::Completed) && !self.only_favorites;
                    if ui.selectable_label(is_completed, format!("Completed ({completed_count})")).clicked() {
                        self.selected_shelf = Some(ShelfStatus::Completed);
                        self.only_favorites = false;
                    }

                    let later_count = library.entries().values().filter(|e| e.shelf == ShelfStatus::MarkedForLater).count();
                    let is_later = self.selected_shelf == Some(ShelfStatus::MarkedForLater) && !self.only_favorites;
                    if ui.selectable_label(is_later, format!("For Later ({later_count})")).clicked() {
                        self.selected_shelf = Some(ShelfStatus::MarkedForLater);
                        self.only_favorites = false;
                    }

                    let fav_count = library.entries().values().filter(|e| e.is_favorite).count();
                    if ui.selectable_label(self.only_favorites, format!("♥ Favorites ({fav_count})")).clicked() {
                        self.only_favorites = !self.only_favorites;
                        self.selected_shelf = None;
                    }

                    // Push Fetch AO3 Work button right if there is space, otherwise wrap cleanly
                    let avail = ui.available_width();
                    let btn_w = 160.0;
                    if avail > btn_w {
                        ui.add_space(avail - btn_w);
                    }

                    let add_btn = Button::new(RichText::new("➕ Fetch AO3 Work").color(Color32::WHITE).strong())
                        .fill(colors.accent);
                    if ui.add(add_btn).clicked() {
                        *action = LibraryAction::SwitchToFetch;
                    }
                });
            });
    }

    fn render_work_card(
        &mut self,
        ui: &mut Ui,
        entry: &LibraryEntry,
        colors: &ReaderColors,
        action: &mut LibraryAction,
    ) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                let card_w = ui.available_width();
                let is_wide = card_w >= 680.0;
                let right_col_w = 380.0;

                if is_wide {
                    // Two-column responsive layout: details on left, action buttons on right
                    let left_w = (card_w - right_col_w - 16.0).max(260.0);
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(left_w, ui.available_height()),
                            egui::Layout::top_down(egui::Align::LEFT),
                            |ui| {
                                ui.set_max_width(left_w);
                                Self::render_card_details(ui, entry, colors, action);
                            },
                        );

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            self.render_card_buttons_rtl(ui, entry, colors, action);
                        });
                    });
                } else {
                    // Narrow layout: details on top, buttons on bottom wrapping smoothly
                    ui.vertical(|ui| {
                        Self::render_card_details(ui, entry, colors, action);

                        ui.add_space(10.0);

                        ui.horizontal_wrapped(|ui| {
                            self.render_card_buttons_ltr(ui, entry, colors, action);
                        });
                    });
                }
            });
    }

    fn render_card_details(
        ui: &mut Ui,
        entry: &LibraryEntry,
        colors: &ReaderColors,
        action: &mut LibraryAction,
    ) {
        let work_id = &entry.meta.id;

        // 1. Title & Authors
        ui.horizontal_wrapped(|ui| {
            let title_link = ui.link(
                RichText::new(&entry.meta.title)
                    .size(17.0)
                    .strong()
                    .color(colors.accent),
            );
            if title_link.clicked() {
                *action = LibraryAction::OpenWork(work_id.clone());
            }

            ui.label(RichText::new("by").color(colors.text_muted).size(13.0));
            let authors_str = entry.meta.authors.join(", ");
            ui.label(RichText::new(authors_str).color(colors.text).size(13.0).strong());
        });

        ui.add_space(4.0);

        // 2. Badges Row
        ui.horizontal_wrapped(|ui| {
            UiComponents::rating_badge(ui, entry.meta.rating);

            for cat in &entry.meta.categories {
                UiComponents::category_badge(ui, cat, colors);
            }

            for warn in &entry.meta.warnings {
                UiComponents::warning_badge(ui, warn);
            }

            for fandom in entry.meta.fandoms.iter().take(3) {
                UiComponents::tag_badge(ui, fandom, colors);
            }

            for rel in entry.meta.relationships.iter().take(3) {
                UiComponents::relationship_badge(ui, rel, colors);
            }
        });

        ui.add_space(6.0);

        // 3. Summary (wrapped, with full visibility)
        UiComponents::render_meta_summary(ui, &entry.meta, colors, 3);

        ui.add_space(8.0);

        // 4. Progress Stats Bar
        ui.horizontal_wrapped(|ui| {
            let pct = entry.progress.percentage;
            let ch_str = format!("Ch. {} / {}", entry.progress.current_chapter, entry.meta.stats.chapters_published);

            ui.label(RichText::new(format!("{ch_str} ({:.0}%)", pct)).color(colors.accent).size(12.0).strong());

            ui.separator();

            let words = if entry.meta.stats.words > 0 {
                entry.meta.stats.words
            } else {
                0
            };
            ui.label(RichText::new(format!("{words} words")).color(colors.text_muted).size(11.5));

            if entry.meta.stats.kudos > 0 {
                ui.label(RichText::new(format!("♥ {} kudos", entry.meta.stats.kudos)).color(colors.text_muted).size(11.5));
            }
        });
    }

    fn render_card_buttons_rtl(
        &mut self,
        ui: &mut Ui,
        entry: &LibraryEntry,
        colors: &ReaderColors,
        action: &mut LibraryAction,
    ) {
        let work_id = &entry.meta.id;
        let is_favorite = entry.is_favorite;
        let is_reading = entry.progress.current_chapter > 1 || entry.progress.percentage > 0.0;
        let btn_label = if is_reading { "Resume ▶" } else { "Read ▶" };

        // Delete button
        if ui.add(Button::new(RichText::new("🗑").color(Color32::from_rgb(239, 68, 68)).size(13.0))).clicked() {
            self.confirm_delete_id = Some(work_id.clone());
        }

        ui.add_space(6.0);

        // Export dropdown button
        ui.menu_button("Export ▾", |ui| {
            if ui.button("Export HTML (.html)").clicked() {
                *action = LibraryAction::ExportWork(work_id.clone(), ExportFormat::Html);
                ui.close();
            }
            if ui.button("Export Markdown (.md)").clicked() {
                *action = LibraryAction::ExportWork(work_id.clone(), ExportFormat::Markdown);
                ui.close();
            }
            if ui.button("Export Plain Text (.txt)").clicked() {
                *action = LibraryAction::ExportWork(work_id.clone(), ExportFormat::Txt);
                ui.close();
            }
        });

        ui.add_space(6.0);

        // Shelf selector menu
        let mut current_shelf = entry.shelf;
        egui::ComboBox::from_id_salt(format!("shelf_{work_id}"))
            .selected_text(current_shelf.display_name())
            .width(120.0)
            .show_ui(ui, |ui| {
                for s in [ShelfStatus::Reading, ShelfStatus::Completed, ShelfStatus::MarkedForLater, ShelfStatus::Favorites] {
                    if ui.selectable_value(&mut current_shelf, s, s.display_name()).clicked() {
                        *action = LibraryAction::SetShelf(work_id.clone(), s);
                    }
                }
            });

        ui.add_space(6.0);

        // Favorite Heart
        let heart_icon = if is_favorite { "♥" } else { "♡" };
        let heart_color = if is_favorite { Color32::from_rgb(225, 29, 72) } else { colors.text_muted };
        if ui.add(Button::new(RichText::new(heart_icon).color(heart_color).size(15.0))).clicked() {
            *action = LibraryAction::ToggleFavorite(work_id.clone());
        }

        ui.add_space(6.0);

        // Read / Resume Button
        let read_btn = Button::new(RichText::new(btn_label).color(Color32::WHITE).strong().size(13.0))
            .fill(colors.accent);
        if ui.add(read_btn).clicked() {
            *action = LibraryAction::OpenWork(work_id.clone());
        }
    }

    fn render_card_buttons_ltr(
        &mut self,
        ui: &mut Ui,
        entry: &LibraryEntry,
        colors: &ReaderColors,
        action: &mut LibraryAction,
    ) {
        let work_id = &entry.meta.id;
        let is_favorite = entry.is_favorite;
        let is_reading = entry.progress.current_chapter > 1 || entry.progress.percentage > 0.0;
        let btn_label = if is_reading { "Resume ▶" } else { "Read ▶" };

        // Read / Resume Button
        let read_btn = Button::new(RichText::new(btn_label).color(Color32::WHITE).strong().size(13.0))
            .fill(colors.accent);
        if ui.add(read_btn).clicked() {
            *action = LibraryAction::OpenWork(work_id.clone());
        }

        ui.add_space(6.0);

        // Favorite Heart
        let heart_icon = if is_favorite { "♥" } else { "♡" };
        let heart_color = if is_favorite { Color32::from_rgb(225, 29, 72) } else { colors.text_muted };
        if ui.add(Button::new(RichText::new(heart_icon).color(heart_color).size(15.0))).clicked() {
            *action = LibraryAction::ToggleFavorite(work_id.clone());
        }

        ui.add_space(6.0);

        // Shelf selector menu
        let mut current_shelf = entry.shelf;
        egui::ComboBox::from_id_salt(format!("shelf_{work_id}"))
            .selected_text(current_shelf.display_name())
            .width(120.0)
            .show_ui(ui, |ui| {
                for s in [ShelfStatus::Reading, ShelfStatus::Completed, ShelfStatus::MarkedForLater, ShelfStatus::Favorites] {
                    if ui.selectable_value(&mut current_shelf, s, s.display_name()).clicked() {
                        *action = LibraryAction::SetShelf(work_id.clone(), s);
                    }
                }
            });

        ui.add_space(6.0);

        // Export dropdown button
        ui.menu_button("Export ▾", |ui| {
            if ui.button("Export HTML (.html)").clicked() {
                *action = LibraryAction::ExportWork(work_id.clone(), ExportFormat::Html);
                ui.close();
            }
            if ui.button("Export Markdown (.md)").clicked() {
                *action = LibraryAction::ExportWork(work_id.clone(), ExportFormat::Markdown);
                ui.close();
            }
            if ui.button("Export Plain Text (.txt)").clicked() {
                *action = LibraryAction::ExportWork(work_id.clone(), ExportFormat::Txt);
                ui.close();
            }
        });

        ui.add_space(6.0);

        // Delete button
        if ui.add(Button::new(RichText::new("🗑").color(Color32::from_rgb(239, 68, 68)).size(13.0))).clicked() {
            self.confirm_delete_id = Some(work_id.clone());
        }
    }

    fn render_empty_state(
        &self,
        ui: &mut Ui,
        colors: &ReaderColors,
        action: &mut LibraryAction,
    ) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.label(RichText::new("📚").size(48.0));
            ui.add_space(10.0);
            ui.label(RichText::new("Your Library is Empty").size(22.0).strong().color(colors.text));
            ui.add_space(8.0);
            ui.label(
                RichText::new("Fetch any work from Archive of Our Own (AO3) by URL or ID\nto read offline with custom typography and themes.")
                    .size(14.0)
                    .color(colors.text_muted),
            );
            ui.add_space(20.0);

            let fetch_btn = Button::new(RichText::new("➕ Fetch AO3 Work Now").color(Color32::WHITE).size(15.0).strong())
                .fill(colors.accent);

            if ui.add(fetch_btn).clicked() {
                *action = LibraryAction::SwitchToFetch;
            }
        });
    }
}
