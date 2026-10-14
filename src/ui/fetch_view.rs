use crate::ao3::models::Work;
use crate::reader::theme::ReaderColors;
use crate::storage::config::AppConfig;
use crate::ui::components::UiComponents;
use egui::{
    Align, Button, Color32, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke, Ui,
};

pub enum FetchAction {
    None,
    StartFetch { input: String, auto_open: bool },
    DownloadEpub(String), // Work ID
    OpenWork(Work),
    SwitchToSettings,
    ImportLocalFile,
}

pub struct FetchView {
    pub input_url_or_id: String,
    pub is_fetching: bool,
    pub error_message: Option<String>,
    pub fetched_preview: Option<Work>,
}

impl FetchView {
    pub fn new() -> Self {
        Self {
            input_url_or_id: String::new(),
            is_fetching: false,
            error_message: None,
            fetched_preview: None,
        }
    }

    pub fn set_fetching(&mut self, fetching: bool) {
        self.is_fetching = fetching;
        if fetching {
            self.error_message = None;
            self.fetched_preview = None;
        }
    }

    pub fn set_error(&mut self, err: String) {
        self.is_fetching = false;
        self.error_message = Some(err);
        self.fetched_preview = None;
    }

    pub fn set_success(&mut self, work: Work) {
        self.is_fetching = false;
        self.error_message = None;
        self.fetched_preview = Some(work);
    }

    pub fn ui(&mut self, ui: &mut Ui, config: &AppConfig) -> FetchAction {
        let mut action = FetchAction::None;
        let colors = config.theme.colors();

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(20.0);

            // Centered layout
            ui.horizontal(|ui| {
                let avail_width = ui.available_width();
                let max_w = 760.0f32.min(avail_width - 32.0);
                let pad = ((avail_width - max_w) / 2.0).max(10.0);

                ui.add_space(pad);

                ui.vertical(|ui| {
                    ui.set_max_width(max_w);

                    // Hero Card
                    self.render_input_card(ui, &colors, config, &mut action);

                    ui.add_space(20.0);

                    // Error display
                    if let Some(err) = &self.error_message {
                        self.render_error_card(ui, err, &colors, &mut action);
                        ui.add_space(16.0);
                    }

                    // Fetched Work Preview
                    if let Some(work) = &self.fetched_preview {
                        self.render_preview_card(ui, work, &colors, &mut action);
                        ui.add_space(20.0);
                    }

                    // Quick Test Suggestions & Features info
                    self.render_suggestions_and_info(ui, &colors, &mut action);

                    ui.add_space(40.0);
                });

                ui.add_space(pad);
            });
        });

        action
    }

    fn render_input_card(
        &mut self,
        ui: &mut Ui,
        colors: &ReaderColors,
        _config: &AppConfig,
        action: &mut FetchAction,
    ) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(24))
            .show(ui, |ui| {
                ui.heading(RichText::new("Fetch AO3 Work").size(22.0).color(colors.text));
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Enter an Archive of Our Own link (or numeric Work ID) to download it for offline reading.")
                        .size(13.5)
                        .color(colors.text_muted),
                );

                ui.add_space(18.0);

                // Input line
                ui.horizontal(|ui| {
                    ui.label(RichText::new("URL or ID:").strong().color(colors.text));

                    let edit = ui.add_enabled(
                        !self.is_fetching,
                        egui::TextEdit::singleline(&mut self.input_url_or_id)
                            .desired_width(ui.available_width() - 110.0)
                            .hint_text("e.g. https://archiveofourown.org/works/123456 or 123456"),
                    );

                    // Paste button
                    if ui.add_enabled(!self.is_fetching, Button::new("📋 Paste")).clicked() {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            if let Ok(text) = clipboard.get_text() {
                                self.input_url_or_id = text.trim().to_string();
                            }
                        }
                    }

                    if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if !self.input_url_or_id.trim().is_empty() {
                            *action = FetchAction::StartFetch {
                                input: self.input_url_or_id.trim().to_string(),
                                auto_open: true,
                            };
                        }
                    }
                });

                ui.add_space(16.0);

                // Action buttons row
                ui.horizontal(|ui| {
                    let can_fetch = !self.input_url_or_id.trim().is_empty() && !self.is_fetching;

                    let fetch_open_btn = Button::new(RichText::new("📥 Download & Read").color(Color32::WHITE).strong().size(14.0))
                        .fill(colors.accent);

                    if ui.add_enabled(can_fetch, fetch_open_btn).clicked() {
                        *action = FetchAction::StartFetch {
                            input: self.input_url_or_id.trim().to_string(),
                            auto_open: true,
                        };
                    }

                    let fetch_only_btn = Button::new(RichText::new("💾 Save to Library").color(colors.text).size(14.0))
                        .fill(colors.surface);

                    if ui.add_enabled(can_fetch, fetch_only_btn).clicked() {
                        *action = FetchAction::StartFetch {
                            input: self.input_url_or_id.trim().to_string(),
                            auto_open: false,
                        };
                    }

                    let epub_btn = Button::new(RichText::new("📦 Download EPUB").color(colors.text).size(14.0))
                        .fill(colors.surface);

                    if ui.add_enabled(can_fetch, epub_btn).clicked() {
                        *action = FetchAction::DownloadEpub(self.input_url_or_id.trim().to_string());
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("📁 Import File").clicked() {
                            *action = FetchAction::ImportLocalFile;
                        }
                    });
                });

                if self.is_fetching {
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Downloading work and parsing chapters from AO3...").color(colors.accent).italics());
                    });
                }
            });
    }

    fn render_error_card(
        &self,
        ui: &mut Ui,
        err: &str,
        colors: &ReaderColors,
        action: &mut FetchAction,
    ) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, Color32::from_rgb(239, 68, 68)))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("❌").size(18.0));
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Download Failed").strong().color(Color32::from_rgb(239, 68, 68)));
                        ui.label(RichText::new(err).color(colors.text));

                        if err.contains("restricted") || err.contains("log in") {
                            ui.add_space(6.0);
                            if ui.button("🔑 Go to AO3 Account Login").clicked() {
                                *action = FetchAction::SwitchToSettings;
                            }
                        }
                    });
                });
            });
    }

    fn render_preview_card(
        &self,
        ui: &mut Ui,
        work: &Work,
        colors: &ReaderColors,
        action: &mut FetchAction,
    ) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, Color32::from_rgb(34, 197, 94)))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(20))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("✔ Work Downloaded Successfully!").color(Color32::from_rgb(34, 197, 94)).strong().size(15.0));

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let read_btn = Button::new(RichText::new("Open in Reader ▶").color(Color32::WHITE).strong().size(14.0))
                            .fill(colors.accent);

                        if ui.add(read_btn).clicked() {
                            *action = FetchAction::OpenWork(work.clone());
                        }
                    });
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                ui.label(RichText::new(&work.meta.title).size(20.0).strong().color(colors.text));
                ui.label(RichText::new(format!("by {}", work.meta.authors.join(", "))).size(14.0).color(colors.text_muted));

                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    UiComponents::rating_badge(ui, work.meta.rating);
                    for fandom in &work.meta.fandoms {
                        UiComponents::tag_badge(ui, fandom, colors);
                    }
                    for rel in &work.meta.relationships {
                        UiComponents::relationship_badge(ui, rel, colors);
                    }
                });

                ui.add_space(8.0);
                let words = work.total_words();
                let chaps = work.chapters.len();
                ui.label(RichText::new(format!("📊 {chaps} chapters | {words} words")).color(colors.text_muted).size(12.5));

                if !work.meta.summary_text.is_empty() {
                    ui.add_space(8.0);
                    UiComponents::render_meta_summary(ui, &work.meta, colors, 4);
                }
            });
    }

    fn render_suggestions_and_info(
        &mut self,
        ui: &mut Ui,
        colors: &ReaderColors,
        action: &mut FetchAction,
    ) {
        Frame::NONE
            .fill(colors.surface)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.label(RichText::new("💡 Tips & Quick Links").strong().color(colors.text).size(14.0));
                ui.add_space(6.0);
                ui.label(
                    RichText::new("• You can paste either the full URL or simply the work ID (e.g. 54106).\n• Adult content is confirmed automatically.\n• If a work is restricted to AO3 account holders, go to the 'Account' tab to sign in.\n• All fetched works are automatically saved for complete offline reading.")
                        .size(12.5)
                        .color(colors.text_muted),
                );

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(10.0);

                ui.label(RichText::new("Quick Test Samples (Public AO3 Works):").strong().color(colors.text_muted).size(12.0));
                ui.add_space(6.0);

                ui.horizontal_wrapped(|ui| {
                    if ui.button("📖 Sample Fic #1 (Multi-Chapter)").clicked() {
                        self.input_url_or_id = "https://archiveofourown.org/works/1000000".to_string();
                        *action = FetchAction::StartFetch {
                            input: self.input_url_or_id.clone(),
                            auto_open: true,
                        };
                    }

                    if ui.button("📖 Sample Fic #2 (Short Story)").clicked() {
                        self.input_url_or_id = "https://archiveofourown.org/works/2000000".to_string();
                        *action = FetchAction::StartFetch {
                            input: self.input_url_or_id.clone(),
                            auto_open: true,
                        };
                    }
                });
            });
    }
}
