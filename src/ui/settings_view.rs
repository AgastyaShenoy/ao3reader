use crate::reader::font_manager::FontManager;
use crate::reader::theme::{ReaderColors, ReaderThemeType};
use crate::reader::typography::{FontFamilyChoice, PageLayoutMode, ReadingMode};
use crate::storage::config::AppConfig;
use crate::updater::{UpdateStatus, CURRENT_VERSION, GITHUB_REPO};
use egui::{Button, Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke, Ui};

pub enum SettingsAction {
    None,
    OpenStorageFolder,
    ClearCache,
    CheckForUpdates,
    OpenUrl(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Reading,
    Typography,
    Theme,
    Shortcuts,
    StorageAndUpdates,
    About,
}

impl SettingsTab {
    pub fn title(&self) -> &'static str {
        match self {
            SettingsTab::Reading => "📖 Reading",
            SettingsTab::Typography => "🔤 Typography & Fonts",
            SettingsTab::Theme => "🎨 Themes",
            SettingsTab::Shortcuts => "⌨ Shortcuts",
            SettingsTab::StorageAndUpdates => "💾 Storage & Updates",
            SettingsTab::About => "ℹ About",
        }
    }
}

pub struct SettingsView {
    pub active_tab: SettingsTab,
    pub confirm_clear_cache: bool,
    pub status_message: Option<String>,
    pub font_search_query: String,
}

impl SettingsView {
    pub fn new() -> Self {
        Self {
            active_tab: SettingsTab::Reading,
            confirm_clear_cache: false,
            status_message: None,
            font_search_query: String::new(),
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, config: &mut AppConfig, update_status: &UpdateStatus) -> SettingsAction {
        let mut action = SettingsAction::None;
        let colors = config.theme.colors();

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(16.0);

            // Centered layout container
            ui.horizontal(|ui| {
                let avail_width = ui.available_width();
                let max_w = 780.0f32.min(avail_width - 32.0);
                let pad = ((avail_width - max_w) / 2.0).max(10.0);

                ui.add_space(pad);

                ui.vertical(|ui| {
                    ui.set_max_width(max_w);

                    // Top Header
                    ui.horizontal(|ui| {
                        ui.heading(RichText::new("⚙ Preferences & Settings").size(22.0).color(colors.text));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("AO3 Reader v{CURRENT_VERSION}"))
                                    .size(12.0)
                                    .color(colors.text_muted),
                            );
                        });
                    });
                    ui.add_space(12.0);

                    // Category Navigation Tabs Bar
                    self.render_tab_navigation(ui, &colors);
                    ui.add_space(16.0);

                    // Active Tab Content
                    match self.active_tab {
                        SettingsTab::Reading => self.render_reading_tab(ui, &colors, config),
                        SettingsTab::Typography => self.render_typography_tab(ui, &colors, config),
                        SettingsTab::Theme => self.render_theme_tab(ui, &colors, config),
                        SettingsTab::Shortcuts => self.render_shortcuts_tab(ui, &colors),
                        SettingsTab::StorageAndUpdates => {
                            self.render_storage_and_updates_tab(ui, &colors, config, update_status, &mut action)
                        }
                        SettingsTab::About => self.render_about_tab(ui, &colors, &mut action),
                    }

                    ui.add_space(40.0);
                });

                ui.add_space(pad);
            });
        });

        action
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Tab Navigation Bar
    // ─────────────────────────────────────────────────────────────────────────────

    fn render_tab_navigation(&mut self, ui: &mut Ui, colors: &ReaderColors) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(8))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let tabs = [
                        SettingsTab::Reading,
                        SettingsTab::Typography,
                        SettingsTab::Theme,
                        SettingsTab::Shortcuts,
                        SettingsTab::StorageAndUpdates,
                        SettingsTab::About,
                    ];

                    for tab in tabs {
                        let is_selected = self.active_tab == tab;
                        let btn = Button::new(
                            RichText::new(tab.title())
                                .strong()
                                .size(13.0)
                                .color(if is_selected { Color32::WHITE } else { colors.text }),
                        )
                        .fill(if is_selected { colors.accent } else { Color32::TRANSPARENT })
                        .stroke(Stroke::new(
                            if is_selected { 1.5 } else { 0.0 },
                            if is_selected { colors.accent } else { Color32::TRANSPARENT },
                        ))
                        .corner_radius(CornerRadius::same(6));

                        if ui.add(btn).clicked() {
                            self.active_tab = tab;
                        }
                        ui.add_space(4.0);
                    }
                });
            });
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // 1. Reading & Ergonomics Tab
    // ─────────────────────────────────────────────────────────────────────────────

    fn render_reading_tab(&self, ui: &mut Ui, colors: &ReaderColors, config: &mut AppConfig) {
        // Card A: Reading Experience Mode
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.label(RichText::new("📖 Reading Experience Mode").strong().size(15.0).color(colors.text));
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Choose whether you prefer e-reader page-flipping or continuous vertical scrolling.")
                        .size(12.0)
                        .color(colors.text_muted),
                );
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    for mode in ReadingMode::all() {
                        let is_active = config.typography.reading_mode == *mode;
                        let btn = Button::new(
                            RichText::new(mode.display_name())
                                .size(13.0)
                                .strong()
                                .color(if is_active { Color32::WHITE } else { colors.text }),
                        )
                        .fill(if is_active { colors.accent } else { colors.surface })
                        .stroke(Stroke::new(1.0, if is_active { colors.accent } else { colors.border }))
                        .corner_radius(CornerRadius::same(6));

                        if ui.add(btn).clicked() {
                            config.typography.reading_mode = *mode;
                            let _ = config.save();
                        }
                    }
                });

                if config.typography.reading_mode == ReadingMode::Paged {
                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.label(RichText::new("Page Layout Spread:").strong().size(13.0).color(colors.text));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        for layout in PageLayoutMode::all() {
                            let is_sel = config.typography.page_layout == *layout;
                            if ui.selectable_label(is_sel, layout.display_name()).clicked() {
                                config.typography.page_layout = *layout;
                                let _ = config.save();
                            }
                        }
                    });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut config.typography.show_page_turn_indicators, "Show edge turn indicators (‹ / ›)").changed() {
                            let _ = config.save();
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut config.typography.page_flip_mouse_wheel, "Flip pages with mouse scroll wheel").changed() {
                            let _ = config.save();
                        }
                    });
                }
            });

        ui.add_space(16.0);

        // Card B: Reading Speed & Ergonomics
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.label(RichText::new("⏱ Reading Speed & Ergonomics").strong().size(15.0).color(colors.text));
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Configure speed estimation and quality-of-life reading automations.")
                        .size(12.0)
                        .color(colors.text_muted),
                );
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Reading Speed (WPM):").strong().color(colors.text));
                    if ui.add(egui::Slider::new(&mut config.words_per_minute, 120..=500).suffix(" WPM")).changed() {
                        let _ = config.save();
                    }
                });
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Calculates estimated reading time remaining for current chapter and total work.")
                        .size(11.5)
                        .color(colors.text_muted),
                );

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut config.auto_save_progress, "Auto-save reading position on exit").changed() {
                        let _ = config.save();
                    }
                });

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut config.auto_next_chapter, "Advance to next chapter automatically when finished").changed() {
                        let _ = config.save();
                    }
                });

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut config.confirm_adult_content, "Prompt confirmation before opening Explicit / Mature rated works").changed() {
                        let _ = config.save();
                    }
                });
            });

        ui.add_space(16.0);

        // Card C: Chapter Content Visibility
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.label(RichText::new("📑 Chapter Elements Visibility").strong().size(15.0).color(colors.text));
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Control which supplementary author sections appear in the reading canvas.")
                        .size(12.0)
                        .color(colors.text_muted),
                );
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut config.typography.show_chapter_summary, "Show chapter summary").changed() {
                        let _ = config.save();
                    }
                });

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut config.typography.show_chapter_pre_notes, "Show chapter beginning notes").changed() {
                        let _ = config.save();
                    }
                });

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut config.typography.show_chapter_post_notes, "Show chapter end notes").changed() {
                        let _ = config.save();
                    }
                });

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut config.typography.show_reading_progress_bar, "Show bottom reading progress bar").changed() {
                        let _ = config.save();
                    }
                });
            });
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // 2. Typography & Fonts Tab
    // ─────────────────────────────────────────────────────────────────────────────

    fn render_typography_tab(&mut self, ui: &mut Ui, colors: &ReaderColors, config: &mut AppConfig) {
        // Card A: Book Typeface & Font Family
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.label(RichText::new("🔤 Book Typeface & Custom Fonts").strong().size(15.0).color(colors.text));
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Choose any font installed on your PC or load custom .ttf / .otf files for reading.")
                        .size(12.0)
                        .color(colors.text_muted),
                );
                ui.add_space(12.0);

                // Active font badge & reset
                let active_name = config
                    .typography
                    .custom_font_name
                    .clone()
                    .unwrap_or_else(|| config.typography.font_family.display_name().to_string());

                ui.horizontal_wrapped(|ui| {
                    Frame::NONE
                        .fill(colors.surface)
                        .stroke(Stroke::new(1.0, colors.accent))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(10, 5))
                        .show(ui, |ui| {
                            ui.label(RichText::new(format!("Current Font: {active_name}")).size(12.5).color(colors.accent).strong());
                        });

                    if config.typography.custom_font_path.is_some() || config.typography.font_family != FontFamilyChoice::SansSerif {
                        if ui.button("↺ Reset to Modern Sans").on_hover_text("Reset book font back to standard Modern Sans").clicked() {
                            config.typography.font_family = FontFamilyChoice::SansSerif;
                            config.typography.custom_font_name = None;
                            config.typography.custom_font_path = None;
                            let _ = config.save();
                        }
                    }

                    if ui.button("📂 Open Font File...").on_hover_text("Load any .ttf or .otf file from disk").clicked() {
                        if let Some((name, path)) = FontManager::pick_custom_font_file() {
                            config.typography.font_family = FontFamilyChoice::Custom(name.clone());
                            config.typography.custom_font_name = Some(name);
                            config.typography.custom_font_path = Some(path);
                            let _ = config.save();
                        }
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);

                // 1. Built-in styles
                ui.label(RichText::new("Standard Presets:").size(12.0).color(colors.text_muted).strong());
                ui.horizontal(|ui| {
                    let is_sans = config.typography.custom_font_path.is_none() && config.typography.font_family == FontFamilyChoice::SansSerif;
                    if ui.selectable_label(is_sans, "Modern Sans (Default)").clicked() {
                        config.typography.font_family = FontFamilyChoice::SansSerif;
                        config.typography.custom_font_name = None;
                        config.typography.custom_font_path = None;
                        let _ = config.save();
                    }

                    let is_mono = config.typography.custom_font_path.is_none() && config.typography.font_family == FontFamilyChoice::Monospace;
                    if ui.selectable_label(is_mono, "Code Monospace").clicked() {
                        config.typography.font_family = FontFamilyChoice::Monospace;
                        config.typography.custom_font_name = None;
                        config.typography.custom_font_path = None;
                        let _ = config.save();
                    }
                });

                ui.add_space(10.0);

                // 2. Curated e-reader book fonts
                let curated = FontManager::cached_curated_fonts();
                if !curated.is_empty() {
                    ui.label(RichText::new("Book-Optimized System Fonts:").size(12.0).color(colors.text_muted).strong());
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        for font in curated {
                            let is_active = config.typography.custom_font_name.as_deref() == Some(&font.name);
                            let label = if is_active { format!("✓ {}", font.name) } else { font.name.clone() };
                            if ui.selectable_label(is_active, label).clicked() {
                                config.typography.font_family = FontFamilyChoice::Custom(font.name.clone());
                                config.typography.custom_font_name = Some(font.name.clone());
                                config.typography.custom_font_path = Some(font.path.clone());
                                let _ = config.save();
                            }
                        }
                    });
                    ui.add_space(10.0);
                }

                // 3. Search All Installed System Fonts
                let all_fonts = FontManager::cached_system_fonts();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Search Any Installed Font:").size(12.0).color(colors.text_muted).strong());
                    ui.add(
                        egui::TextEdit::singleline(&mut self.font_search_query)
                            .desired_width(220.0)
                            .hint_text("Search font family name..."),
                    );
                    if !self.font_search_query.is_empty() && ui.button("✖").clicked() {
                        self.font_search_query.clear();
                    }
                });

                let query = self.font_search_query.trim().to_lowercase();
                Frame::NONE
                    .fill(colors.bg)
                    .stroke(Stroke::new(1.0, colors.border))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                            let mut count = 0;
                            for font in all_fonts {
                                if query.is_empty() || font.name.to_lowercase().contains(&query) {
                                    count += 1;
                                    let is_active = config.typography.custom_font_name.as_deref() == Some(&font.name);
                                    let label = if is_active { format!("✓ {}", font.name) } else { font.name.clone() };
                                    if ui.selectable_label(is_active, label).clicked() {
                                        config.typography.font_family = FontFamilyChoice::Custom(font.name.clone());
                                        config.typography.custom_font_name = Some(font.name.clone());
                                        config.typography.custom_font_path = Some(font.path.clone());
                                        let _ = config.save();
                                    }
                                }
                            }
                            if count == 0 {
                                ui.label(RichText::new("No fonts matching search query").size(12.0).color(colors.text_muted));
                            }
                        });
                    });
            });

        ui.add_space(16.0);

        // Card B: Text Metrics, Spacing & Layout
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.label(RichText::new("📏 Text Metrics & Spacing").strong().size(15.0).color(colors.text));
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Fine-tune font size, line spacing, margins, and column width for comfortable reading.")
                        .size(12.0)
                        .color(colors.text_muted),
                );
                ui.add_space(14.0);

                egui::Grid::new("typography_sliders_grid")
                    .num_columns(2)
                    .spacing([20.0, 10.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Base Font Size:").strong().color(colors.text));
                        if ui.add(egui::Slider::new(&mut config.typography.font_size, 12.0..=36.0).suffix(" pt")).changed() {
                            let _ = config.save();
                        }
                        ui.end_row();

                        ui.label(RichText::new("Line Height Multiplier:").strong().color(colors.text));
                        if ui.add(egui::Slider::new(&mut config.typography.line_spacing_multiplier, 1.2..=2.6).step_by(0.1)).changed() {
                            let _ = config.save();
                        }
                        ui.end_row();

                        ui.label(RichText::new("Max Column Width:").strong().color(colors.text));
                        if ui.add(egui::Slider::new(&mut config.typography.max_column_width, 450.0..=1400.0).suffix(" px")).changed() {
                            let _ = config.save();
                        }
                        ui.end_row();

                        ui.label(RichText::new("Paragraph Gap:").strong().color(colors.text));
                        if ui.add(egui::Slider::new(&mut config.typography.paragraph_spacing, 4.0..=32.0).suffix(" px")).changed() {
                            let _ = config.save();
                        }
                        ui.end_row();
                    });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut config.typography.indent_paragraphs, "Indent first line of paragraphs (4 spaces)").changed() {
                        let _ = config.save();
                    }
                });
            });
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // 3. Themes Tab
    // ─────────────────────────────────────────────────────────────────────────────

    fn render_theme_tab(&self, ui: &mut Ui, colors: &ReaderColors, config: &mut AppConfig) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.label(RichText::new("🎨 Color Palettes & Aesthetics").strong().size(15.0).color(colors.text));
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Select a curated color theme optimized for day or nighttime reading comfort.")
                        .size(12.0)
                        .color(colors.text_muted),
                );
                ui.add_space(14.0);

                ui.horizontal_wrapped(|ui| {
                    for theme in ReaderThemeType::all() {
                        let is_active = config.theme == *theme;
                        let t_colors = theme.colors();

                        Frame::NONE
                            .fill(t_colors.bg)
                            .stroke(Stroke::new(
                                if is_active { 2.5 } else { 1.0 },
                                if is_active { colors.accent } else { colors.border },
                            ))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(16, 12))
                            .show(ui, |ui| {
                                let label = if is_active {
                                    format!("✓ {}", theme.display_name())
                                } else {
                                    theme.display_name().to_string()
                                };

                                let btn = Button::new(
                                    RichText::new(label)
                                        .color(t_colors.text)
                                        .strong()
                                        .size(13.5),
                                )
                                .fill(Color32::TRANSPARENT);

                                if ui.add(btn).clicked() {
                                    config.theme = *theme;
                                    let _ = config.save();
                                }
                            });

                        ui.add_space(8.0);
                    }
                });
            });
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // 4. Keyboard Shortcuts Tab
    // ─────────────────────────────────────────────────────────────────────────────

    fn render_shortcuts_tab(&self, ui: &mut Ui, colors: &ReaderColors) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.label(RichText::new("⌨ Keyboard Shortcuts Cheatsheet").strong().size(15.0).color(colors.text));
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Quickly navigate, turn pages, bookmark, and search with keyboard shortcuts.")
                        .size(12.0)
                        .color(colors.text_muted),
                );
                ui.add_space(14.0);

                let groups: &[(&str, &[(&str, &str)])] = &[
                    (
                        "📖 E-Reader Navigation (Paged Mode)",
                        &[
                            ("→  or  PageDown  or  Space  or  K", "Turn to Next Page / Spread"),
                            ("←  or  PageUp  or  Shift+Space  or  J", "Turn to Previous Page / Spread"),
                            ("Home  /  End", "Jump to First / Last Page of Chapter"),
                        ],
                    ),
                    (
                        "📚 Chapter Navigation",
                        &[
                            ("Ctrl + →", "Advance to Next Chapter"),
                            ("Ctrl + ←", "Return to Previous Chapter"),
                        ],
                    ),
                    (
                        "🔍 Tools & Reading Canvas",
                        &[
                            ("F11", "Toggle Zen Mode (Border-free, fullscreen immersion)"),
                            ("Ctrl + F", "Open In-Reader Search (search chapter or entire fic)"),
                            ("Ctrl + B", "Add Bookmark at current reading position"),
                            ("A+  /  A-", "Quickly increase / decrease font size"),
                            ("Esc", "Close search, TOC, appearance drawer, or exit Zen mode"),
                        ],
                    ),
                ];

                for (category, items) in groups {
                    ui.label(RichText::new(*category).strong().color(colors.accent).size(13.5));
                    ui.add_space(4.0);

                    for (key, desc) in *items {
                        ui.horizontal(|ui| {
                            Frame::NONE
                                .fill(colors.surface)
                                .stroke(Stroke::new(1.0, colors.border))
                                .corner_radius(CornerRadius::same(4))
                                .inner_margin(Margin::symmetric(8, 3))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(*key).monospace().strong().color(colors.accent).size(11.5));
                                });

                            ui.label(RichText::new(*desc).color(colors.text).size(12.5));
                        });
                        ui.add_space(4.0);
                    }
                    ui.add_space(8.0);
                }
            });
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // 5. Storage & Updates Tab
    // ─────────────────────────────────────────────────────────────────────────────

    fn render_storage_and_updates_tab(
        &mut self,
        ui: &mut Ui,
        colors: &ReaderColors,
        config: &mut AppConfig,
        update_status: &UpdateStatus,
        action: &mut SettingsAction,
    ) {
        // Card A: App Updates
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🚀 In-App Updates & Releases").strong().size(15.0).color(colors.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("Current: v{CURRENT_VERSION}"))
                                .size(12.0)
                                .color(colors.accent)
                                .strong(),
                        );
                    });
                });
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Check for new releases, performance improvements, and bug fixes directly from GitHub.")
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
                            ui.label(RichText::new("Queries GitHub Releases API").size(12.0).color(colors.text_muted));
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
                            ui.add_space(8.0);
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

                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    if ui.button(RichText::new("🌐 View Release on GitHub").strong().color(colors.accent)).clicked() {
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

        ui.add_space(16.0);

        // Card B: Storage & Offline Data
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.label(RichText::new("💾 Storage & Offline Data").strong().size(15.0).color(colors.text));
                ui.add_space(2.0);
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

    // ─────────────────────────────────────────────────────────────────────────────
    // 6. About Tab
    // ─────────────────────────────────────────────────────────────────────────────

    fn render_about_tab(&self, ui: &mut Ui, colors: &ReaderColors, action: &mut SettingsAction) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(20))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("AO3 Desktop Reader").size(18.0).strong().color(colors.text));
                    ui.label(RichText::new(format!("v{CURRENT_VERSION}")).size(13.0).color(colors.accent).strong());
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new("A lightning-fast, native desktop reader for Archive of Our Own.\nBuilt in 100% Rust with GPU-accelerated immediate-mode graphics (eframe / egui).\nZero webview overhead, complete offline library support, customizable typography, and full privacy.")
                        .size(12.5)
                        .color(colors.text_muted),
                );

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(10.0);

                ui.label(RichText::new("Key Features:").strong().size(13.5).color(colors.text));
                ui.add_space(4.0);
                let features = [
                    "📖 Real E-Reader Page Flipping with Single and Dual-Page spreads",
                    "🔤 Support for Any Installed System Font & Custom .ttf / .otf font loading",
                    "📱 Responsive dynamic work cards with clean line wrapping",
                    "💾 Multi-format export: HTML, Markdown, and Plain Text",
                    "🎨 6 High-contrast & low-eyestrain reading theme palettes",
                    "🚀 Integrated in-app update notifications & checks",
                    "🔒 Safe, offline-first storage with zero telemetry or data tracking",
                ];
                for f in features {
                    ui.label(RichText::new(format!("• {f}")).size(12.0).color(colors.text));
                }

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(10.0);

                ui.label(
                    RichText::new("💡 Ideated and prompted by Agastya Shenoy\n🤖 Developed majorly by AI (Gemini 3.8 Flash via Antigravity)")
                        .size(12.5)
                        .strong()
                        .color(colors.text),
                );

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let repo_url = format!("https://github.com/{GITHUB_REPO}");
                    if ui.button("⭐ View on GitHub").clicked() {
                        *action = SettingsAction::OpenUrl(repo_url);
                    }
                });
            });
    }
}
