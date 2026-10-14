use crate::reader::theme::{ReaderColors, ReaderThemeType};
use crate::reader::typography::FontFamilyChoice;
use crate::storage::config::AppConfig;
use crate::updater::{UpdateInfo, UpdateStatus, CURRENT_VERSION, GITHUB_REPO};
use egui::{Button, Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke, Ui};

pub enum SettingsAction {
    None,
    OpenStorageFolder,
    ClearCache,
    CheckForUpdates,
    OpenUrl(String),
}

pub struct SettingsView {
    pub confirm_clear_cache: bool,
    pub status_message: Option<String>,
}

impl SettingsView {
    pub fn new() -> Self {
        Self {
            confirm_clear_cache: false,
            status_message: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, config: &mut AppConfig, update_status: &UpdateStatus) -> SettingsAction {
        let mut action = SettingsAction::None;
        let colors = config.theme.colors();

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(20.0);

            // Centered layout
            ui.horizontal(|ui| {
                let avail_width = ui.available_width();
                let max_w = 720.0f32.min(avail_width - 32.0);
                let pad = ((avail_width - max_w) / 2.0).max(10.0);

                ui.add_space(pad);

                ui.vertical(|ui| {
                    ui.set_max_width(max_w);

                    // Heading
                    ui.heading(RichText::new("Preferences & Reader Settings").size(22.0).color(colors.text));
                    ui.add_space(14.0);

                    // 1. Reading Theme Section
                    self.render_theme_section(ui, &colors, config);
                    ui.add_space(18.0);

                    // 2. Typography & Layout Defaults
                    self.render_typography_section(ui, &colors, config);
                    ui.add_space(18.0);

                    // 3. Reading Speed & Ergonomics
                    self.render_reading_speed_section(ui, &colors, config);
                    ui.add_space(18.0);

                    // 4. In-App Updates & Releases
                    self.render_updates_section(ui, &colors, config, update_status, &mut action);
                    ui.add_space(18.0);

                    // 5. Storage & Offline Data
                    self.render_storage_section(ui, &colors, &mut action);
                    ui.add_space(18.0);

                    // 6. Keyboard Shortcuts Cheatsheet
                    self.render_shortcuts_section(ui, &colors);
                    ui.add_space(18.0);

                    // 7. About Box
                    self.render_about_section(ui, &colors, &mut action);

                    ui.add_space(40.0);
                });

                ui.add_space(pad);
            });
        });

        action
    }

    fn render_theme_section(&self, ui: &mut Ui, colors: &ReaderColors, config: &mut AppConfig) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(20))
            .show(ui, |ui| {
                ui.label(RichText::new("🎨 Reading Theme Palette").strong().size(15.0).color(colors.text));
                ui.add_space(4.0);
                ui.label(RichText::new("Choose your preferred color theme for eye comfort:").size(12.5).color(colors.text_muted));
                ui.add_space(12.0);

                ui.horizontal_wrapped(|ui| {
                    for theme in ReaderThemeType::all() {
                        let is_active = config.theme == *theme;
                        let t_colors = theme.colors();
                        let btn = Button::new(RichText::new(theme.display_name()).color(t_colors.text).strong())
                            .fill(t_colors.bg)
                            .stroke(Stroke::new(if is_active { 2.5 } else { 1.0 }, if is_active { colors.accent } else { colors.border }));

                        if ui.add(btn).clicked() {
                            config.theme = *theme;
                            let _ = config.save();
                        }
                    }
                });
            });
    }

    fn render_typography_section(&self, ui: &mut Ui, colors: &ReaderColors, config: &mut AppConfig) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(20))
            .show(ui, |ui| {
                ui.label(RichText::new("🔤 Typography & Reading Column").strong().size(15.0).color(colors.text));
                ui.add_space(4.0);
                ui.label(RichText::new("Adjust default text rendering to match your favorite ebook reader:").size(12.5).color(colors.text_muted));
                ui.add_space(12.0);

                // Font Family
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Font Family:").color(colors.text));
                    for fam in FontFamilyChoice::all() {
                        let is_sel = config.typography.font_family == *fam;
                        if ui.selectable_label(is_sel, fam.display_name()).clicked() {
                            config.typography.font_family = *fam;
                            let _ = config.save();
                        }
                    }
                });

                ui.add_space(10.0);

                // Font Size
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Base Font Size:").color(colors.text));
                    if ui.add(egui::Slider::new(&mut config.typography.font_size, 12.0..=32.0).suffix(" pt")).changed() {
                        let _ = config.save();
                    }
                });

                // Line Height
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Line Height Multiplier:").color(colors.text));
                    if ui.add(egui::Slider::new(&mut config.typography.line_spacing_multiplier, 1.2..=2.4).step_by(0.1)).changed() {
                        let _ = config.save();
                    }
                });

                // Column Width
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Max Reading Column Width:").color(colors.text));
                    if ui.add(egui::Slider::new(&mut config.typography.max_column_width, 500.0..=1200.0).suffix(" px")).changed() {
                        let _ = config.save();
                    }
                });

                // Paragraph Gap
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Paragraph Spacing:").color(colors.text));
                    if ui.add(egui::Slider::new(&mut config.typography.paragraph_spacing, 6.0..=28.0).suffix(" px")).changed() {
                        let _ = config.save();
                    }
                });

                ui.add_space(8.0);

                // Toggles
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut config.typography.indent_paragraphs, "Indent first line of paragraphs").changed() {
                        let _ = config.save();
                    }
                });

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut config.typography.show_chapter_pre_notes, "Show chapter pre-notes").changed() {
                        let _ = config.save();
                    }
                    if ui.checkbox(&mut config.typography.show_chapter_post_notes, "Show chapter end-notes").changed() {
                        let _ = config.save();
                    }
                });
            });
    }

    fn render_reading_speed_section(&self, ui: &mut Ui, colors: &ReaderColors, config: &mut AppConfig) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(20))
            .show(ui, |ui| {
                ui.label(RichText::new("⏱ Reading Time Estimation").strong().size(15.0).color(colors.text));
                ui.add_space(4.0);
                ui.label(RichText::new("Used to calculate 'minutes remaining' in each chapter and entire work.").size(12.5).color(colors.text_muted));
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Reading Speed:").color(colors.text));
                    if ui.add(egui::Slider::new(&mut config.words_per_minute, 120..=450).suffix(" WPM")).changed() {
                        let _ = config.save();
                    }
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut config.auto_save_progress, "Auto-save reading position").changed() {
                        let _ = config.save();
                    }
                });
            });
    }

    fn render_storage_section(&mut self, ui: &mut Ui, colors: &ReaderColors, action: &mut SettingsAction) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(20))
            .show(ui, |ui| {
                ui.label(RichText::new("💾 Storage & Offline Cache").strong().size(15.0).color(colors.text));
                ui.add_space(4.0);
                let app_dir = AppConfig::get_app_dir();
                ui.label(RichText::new(format!("Local storage path: {}", app_dir.display())).size(12.0).color(colors.text_muted));
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if ui.button("📂 Open AppData Folder").clicked() {
                        *action = SettingsAction::OpenStorageFolder;
                    }

                    if !self.confirm_clear_cache {
                        if ui.button("🗑 Clear Offline Cache").clicked() {
                            self.confirm_clear_cache = true;
                        }
                    } else {
                        ui.label(RichText::new("Confirm clear?").color(Color32::from_rgb(239, 68, 68)).strong());
                        if ui.button("Yes, Clear Cache").clicked() {
                            *action = SettingsAction::ClearCache;
                            self.confirm_clear_cache = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_clear_cache = false;
                        }
                    }
                });
            });
    }

    fn render_shortcuts_section(&self, ui: &mut Ui, colors: &ReaderColors) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(20))
            .show(ui, |ui| {
                ui.label(RichText::new("⌨ Keyboard Shortcuts").strong().size(15.0).color(colors.text));
                ui.add_space(8.0);

                let shortcuts = [
                    ("[←] or [J]", "Go to Previous Chapter"),
                    ("[→] or [K]", "Go to Next Chapter"),
                    ("[Ctrl + F]", "Find / Search in work"),
                    ("[Ctrl + B]", "Add Bookmark at current chapter"),
                    ("[A+] / [A-]", "Quickly increase or decrease font size"),
                    ("[Esc]", "Close open drawers, search, or dialogs"),
                ];

                for (key, desc) in shortcuts {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(key).strong().color(colors.accent).size(12.5));
                        ui.label(RichText::new("—").color(colors.text_muted));
                        ui.label(RichText::new(desc).color(colors.text).size(12.5));
                    });
                    ui.add_space(3.0);
                }
            });
    }

    fn render_updates_section(
        &self,
        ui: &mut Ui,
        colors: &ReaderColors,
        config: &mut AppConfig,
        update_status: &UpdateStatus,
        action: &mut SettingsAction,
    ) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(20))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🚀 App Updates & Version").strong().size(15.0).color(colors.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("Current: v{CURRENT_VERSION}"))
                                .size(12.0)
                                .color(colors.accent)
                                .strong(),
                        );
                    });
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Keep your AO3 Desktop Reader updated with the latest reader features, fixes, and performance improvements.")
                        .size(12.0)
                        .color(colors.text_muted),
                );
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut config.check_updates_on_startup, "Automatically check for updates on startup").changed() {
                        let _ = config.save();
                    }
                });
                ui.add_space(10.0);

                match update_status {
                    UpdateStatus::Idle => {
                        ui.horizontal(|ui| {
                            if ui.button(RichText::new("🔄 Check for Updates Now").strong()).clicked() {
                                *action = SettingsAction::CheckForUpdates;
                            }
                            ui.label(RichText::new("Connects to GitHub Releases to check for new versions.").size(12.0).color(colors.text_muted));
                        });
                    }
                    UpdateStatus::Checking => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new("Checking GitHub for the latest release...").size(13.0).color(colors.text));
                        });
                    }
                    UpdateStatus::UpToDate { checked_version } => {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("✔ You are on the latest version!").strong().color(Color32::from_rgb(34, 197, 94)));
                            ui.label(RichText::new(format!("(v{checked_version})")).size(12.0).color(colors.text_muted));
                            ui.add_space(10.0);
                            if ui.button("Check Again").clicked() {
                                *action = SettingsAction::CheckForUpdates;
                            }
                        });
                    }
                    UpdateStatus::UpdateAvailable(info) => {
                        Frame::NONE
                            .fill(colors.surface)
                            .stroke(Stroke::new(1.5, colors.accent))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(Margin::same(14))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!("🎉 New Version Available: {}", info.latest_version))
                                            .strong()
                                            .size(14.0)
                                            .color(colors.accent),
                                    );
                                    ui.label(RichText::new(format!("(Released {})", info.published_date)).size(12.0).color(colors.text_muted));
                                });
                                ui.add_space(4.0);
                                ui.label(RichText::new(&info.release_name).strong().color(colors.text));

                                if !info.release_notes.is_empty() {
                                    ui.add_space(6.0);
                                    let preview = if info.release_notes.len() > 300 {
                                        format!("{}...", &info.release_notes[..300])
                                    } else {
                                        info.release_notes.clone()
                                    };
                                    ui.label(RichText::new(preview).size(12.0).color(colors.text_muted));
                                }

                                ui.add_space(12.0);
                                ui.horizontal(|ui| {
                                    if ui.button(RichText::new("🌐 Open Release Page").strong().color(colors.accent)).clicked() {
                                        *action = SettingsAction::OpenUrl(info.release_url.clone());
                                    }
                                    if let Some(direct) = &info.direct_download_url {
                                        if ui.button(RichText::new("⬇ Download Binary (.exe)").strong()).clicked() {
                                            *action = SettingsAction::OpenUrl(direct.clone());
                                        }
                                    }
                                });
                            });
                    }
                    UpdateStatus::Error(err) => {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("⚠️ Update check failed: {err}")).color(Color32::from_rgb(239, 68, 68)).size(12.5));
                            if ui.button("Retry").clicked() {
                                *action = SettingsAction::CheckForUpdates;
                            }
                        });
                    }
                }
            });
    }

    fn render_about_section(&self, ui: &mut Ui, colors: &ReaderColors, action: &mut SettingsAction) {
        Frame::NONE
            .fill(colors.surface)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("AO3 Desktop Reader").size(15.0).strong().color(colors.text));
                    ui.label(RichText::new(format!("v{CURRENT_VERSION}")).size(12.0).color(colors.text_muted));
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new("A fast, lightweight native desktop reader for Archive of Our Own.\nBuilt in 100% Rust with GPU-accelerated immediate-mode graphics (eframe / egui).\nZero webview bloat, complete offline reading support, customizable typography, and full privacy.")
                        .size(12.0)
                        .color(colors.text_muted),
                );
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(
                    RichText::new("💡 Ideated and prompted by Agastya Shenoy\n🤖 Developed majorly by AI (Gemini 3.8 Flash via Antigravity)")
                        .size(12.0)
                        .strong()
                        .color(colors.text),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let repo_url = format!("https://github.com/{GITHUB_REPO}");
                    if ui.button("⭐ View on GitHub").clicked() {
                        *action = SettingsAction::OpenUrl(repo_url);
                    }
                });
            });
    }
}
