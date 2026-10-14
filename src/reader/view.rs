use crate::ao3::models::{Bookmark, ReadingProgress, Work};
use crate::reader::search::ReaderSearchState;
use crate::reader::theme::{ReaderColors, ReaderThemeType};
use crate::reader::typography::TypographySettings;
use crate::storage::config::AppConfig;
use chrono::Utc;
use egui::{
    Align, Button, Color32, CornerRadius, Frame, Key, Layout, Margin, RichText, ScrollArea,
    Stroke, Ui,
};

pub enum ReaderAction {
    None,
    CloseReader,
    UpdateProgress(ReadingProgress),
    AddBookmark(Bookmark),
    LeaveKudos(String), // Work ID
    ChangeChapter(usize),
}

pub struct ReaderView {
    pub work: Work,
    pub current_chapter_idx: usize, // 1-based
    pub search_state: ReaderSearchState,
    pub show_toc: bool,
    pub show_settings: bool,
    pub show_bookmark_dialog: bool,
    pub bookmark_note_input: String,
    pub status_toast: Option<(String, chrono::DateTime<Utc>)>,
    pub scroll_to_top: bool,
}

impl ReaderView {
    pub fn new(work: Work, start_chapter: usize) -> Self {
        let chapter_idx = if start_chapter > 0 && start_chapter <= work.chapters.len() {
            start_chapter
        } else {
            1
        };

        Self {
            work,
            current_chapter_idx: chapter_idx,
            search_state: ReaderSearchState::new(),
            show_toc: false,
            show_settings: false,
            show_bookmark_dialog: false,
            bookmark_note_input: String::new(),
            status_toast: None,
            scroll_to_top: true,
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_toast = Some((msg.into(), Utc::now()));
    }

    pub fn current_chapter(&self) -> Option<&crate::ao3::models::Chapter> {
        self.work.chapters.get(self.current_chapter_idx.saturating_sub(1))
    }

    pub fn total_chapters(&self) -> usize {
        self.work.chapters.len()
    }

    pub fn calculate_progress(&self) -> ReadingProgress {
        let total = self.total_chapters();
        let pct = if total > 0 {
            ((self.current_chapter_idx as f32) / (total as f32)) * 100.0
        } else {
            100.0
        };

        ReadingProgress {
            current_chapter: self.current_chapter_idx,
            scroll_offset: 0.0,
            percentage: pct.clamp(0.0, 100.0),
            last_read: Utc::now(),
            completed: self.current_chapter_idx >= total,
        }
    }

    pub fn estimated_time_left_minutes(&self, wpm: u32) -> (u32, u32) {
        let words_per_min = if wpm > 0 { wpm as usize } else { 230 };

        // Chapter time remaining
        let ch_words = self.current_chapter().map(|c| c.word_count).unwrap_or(0);
        let ch_mins = ((ch_words as f32) / (words_per_min as f32)).ceil() as u32;

        // Work time remaining
        let remaining_words: usize = self
            .work
            .chapters
            .iter()
            .skip(self.current_chapter_idx.saturating_sub(1))
            .map(|c| c.word_count)
            .sum();
        let work_mins = ((remaining_words as f32) / (words_per_min as f32)).ceil() as u32;

        (ch_mins, work_mins)
    }

    pub fn ui(&mut self, ui: &mut Ui, config: &mut AppConfig) -> ReaderAction {
        let mut action = ReaderAction::None;
        let colors = config.theme.colors();

        // Handle keyboard shortcuts
        self.handle_shortcuts(ui, &mut action);

        // Top Navigation & Reader Toolbar
        self.render_top_toolbar(ui, &colors, config, &mut action);

        // Toast alert if present (auto fade after 4 seconds)
        if let Some((msg, time)) = &self.status_toast {
            if (Utc::now() - *time).num_seconds() < 4 {
                Frame::NONE
                    .fill(colors.card)
                    .stroke(Stroke::new(1.0, colors.accent))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(14, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(colors.accent, "ℹ");
                            ui.colored_label(colors.text, msg);
                        });
                    });
            } else {
                self.status_toast = None;
            }
        }

        // In-Reader Search Bar (Ctrl+F)
        if self.search_state.is_open {
            self.render_search_bar(ui, &colors);
        }

        // Table of Contents Modal/Drawer
        if self.show_toc {
            self.render_toc_drawer(ui, &colors, &mut action);
        }

        // Quick Settings Drawer
        if self.show_settings {
            self.render_settings_panel(ui, &colors, config);
        }

        // Bookmark Dialog
        if self.show_bookmark_dialog {
            self.render_bookmark_dialog(ui, &colors, &mut action);
        }

        // Main Reading Scroll Area
        let mut scroll_area = ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(false);

        if self.scroll_to_top {
            scroll_area = scroll_area.vertical_scroll_offset(0.0);
            self.scroll_to_top = false;
        }

        scroll_area.show(ui, |ui| {
            ui.add_space(config.typography.side_margin / 2.0);

            // Center reading column
            ui.horizontal(|ui| {
                let avail_width = ui.available_width();
                let col_width = config.typography.max_column_width.min(avail_width - 32.0).max(300.0);
                let side_padding = ((avail_width - col_width) / 2.0).max(12.0);

                ui.add_space(side_padding);

                ui.vertical(|ui| {
                    ui.set_max_width(col_width);
                    ui.set_min_width(col_width);

                    self.render_chapter_content(ui, &colors, &config.typography, &mut action);

                    ui.add_space(40.0);
                });

                ui.add_space(side_padding);
            });
        });

        // Bottom Progress Bar
        if config.typography.show_reading_progress_bar {
            self.render_bottom_progress(ui, &colors, config, &mut action);
        }

        action
    }

    fn handle_shortcuts(&mut self, ui: &mut Ui, action: &mut ReaderAction) {
        ui.input(|i| {
            if i.key_pressed(Key::ArrowLeft) || (i.modifiers.is_none() && i.key_pressed(Key::J)) {
                if self.current_chapter_idx > 1 {
                    self.current_chapter_idx -= 1;
                    self.scroll_to_top = true;
                    *action = ReaderAction::ChangeChapter(self.current_chapter_idx);
                }
            } else if i.key_pressed(Key::ArrowRight) || (i.modifiers.is_none() && i.key_pressed(Key::K)) {
                if self.current_chapter_idx < self.total_chapters() {
                    self.current_chapter_idx += 1;
                    self.scroll_to_top = true;
                    *action = ReaderAction::ChangeChapter(self.current_chapter_idx);
                }
            } else if i.modifiers.command && i.key_pressed(Key::F) {
                self.search_state.is_open = !self.search_state.is_open;
            } else if i.modifiers.command && i.key_pressed(Key::B) {
                self.show_bookmark_dialog = true;
            } else if i.key_pressed(Key::Escape) {
                self.show_toc = false;
                self.show_settings = false;
                self.show_bookmark_dialog = false;
                self.search_state.is_open = false;
            }
        });
    }

    fn render_top_toolbar(
        &mut self,
        ui: &mut Ui,
        colors: &ReaderColors,
        config: &mut AppConfig,
        action: &mut ReaderAction,
    ) {
        Frame::NONE
            .fill(colors.surface)
            .stroke(Stroke::new(1.0, colors.border))
            .inner_margin(Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Back to Library button
                    if ui.add(Button::new(RichText::new("⬅ Library").color(colors.text).size(13.0))).clicked() {
                        *action = ReaderAction::CloseReader;
                    }

                    ui.separator();

                    // Table of Contents toggle
                    let toc_label = format!("📖 Chapters ({}/{})", self.current_chapter_idx, self.total_chapters());
                    if ui.add(Button::new(RichText::new(toc_label).color(colors.text).size(13.0))).clicked() {
                        self.show_toc = !self.show_toc;
                    }

                    // Title truncated
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        let title = &self.work.meta.title;
                        let display_title = if title.len() > 40 {
                            format!("{}...", &title[..37])
                        } else {
                            title.clone()
                        };
                        ui.label(RichText::new(display_title).strong().color(colors.text).size(13.0));
                    });

                    // Right side controls
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Quick Settings (Font / Theme)
                        let settings_btn = Button::new(RichText::new("⚙ Appearance").color(colors.text).size(13.0));
                        if ui.add(settings_btn).clicked() {
                            self.show_settings = !self.show_settings;
                        }

                        // Bookmark current chapter
                        let bm_btn = Button::new(RichText::new("🔖 Bookmark").color(colors.text).size(13.0));
                        if ui.add(bm_btn).clicked() {
                            self.show_bookmark_dialog = true;
                        }

                        // Kudos Button
                        let kudos_btn = Button::new(RichText::new("❤️ Kudos").color(Color32::from_rgb(225, 29, 72)).size(13.0));
                        if ui.add(kudos_btn).clicked() {
                            *action = ReaderAction::LeaveKudos(self.work.meta.id.clone());
                        }

                        // Search Button
                        let search_btn = Button::new(RichText::new("🔍 Search").color(colors.text).size(13.0));
                        if ui.add(search_btn).clicked() {
                            self.search_state.is_open = !self.search_state.is_open;
                        }

                        // Font size quick toggles (- / +)
                        if ui.add(Button::new(RichText::new("A+").size(12.0).color(colors.text))).clicked() {
                            config.typography.font_size = (config.typography.font_size + 1.0).min(36.0);
                            let _ = config.save();
                        }
                        if ui.add(Button::new(RichText::new("A-").size(12.0).color(colors.text))).clicked() {
                            config.typography.font_size = (config.typography.font_size - 1.0).max(12.0);
                            let _ = config.save();
                        }
                    });
                });
            });
    }

    fn render_search_bar(&mut self, ui: &mut Ui, colors: &ReaderColors) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .inner_margin(Margin::symmetric(14, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Find:").color(colors.text_muted));
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.search_state.query)
                            .desired_width(220.0)
                            .hint_text("Search work text..."),
                    );

                    if edit.changed() {
                        self.search_state.execute_search(&self.work, self.current_chapter_idx);
                    }

                    let match_count = self.search_state.matches.len();
                    if match_count > 0 {
                        let cur_idx = self.search_state.current_match_idx + 1;
                        ui.label(RichText::new(format!("{cur_idx} of {match_count}")).color(colors.text_muted).size(12.0));

                        if ui.button("◀").clicked() {
                            if let Some(m) = self.search_state.prev_match() {
                                if m.chapter_index != self.current_chapter_idx {
                                    self.current_chapter_idx = m.chapter_index;
                                    self.scroll_to_top = true;
                                }
                            }
                        }
                        if ui.button("▶").clicked() {
                            if let Some(m) = self.search_state.next_match() {
                                if m.chapter_index != self.current_chapter_idx {
                                    self.current_chapter_idx = m.chapter_index;
                                    self.scroll_to_top = true;
                                }
                            }
                        }
                    } else if !self.search_state.query.is_empty() {
                        ui.label(RichText::new("No matches").color(Color32::from_rgb(239, 68, 68)).size(12.0));
                    }

                    ui.checkbox(&mut self.search_state.search_all_chapters, "All chapters");

                    if ui.button("✖").clicked() {
                        self.search_state.is_open = false;
                    }
                });
            });
    }

    fn render_toc_drawer(&mut self, ui: &mut Ui, colors: &ReaderColors, action: &mut ReaderAction) {
        Frame::NONE
            .fill(colors.surface)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("Table of Contents").color(colors.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("✖").clicked() {
                            self.show_toc = false;
                        }
                    });
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                ScrollArea::vertical().max_height(350.0).show(ui, |ui| {
                    for chapter in &self.work.chapters {
                        let is_current = chapter.index == self.current_chapter_idx;
                        let text = format!("{}. {}", chapter.index, chapter.title);

                        let rich = if is_current {
                            RichText::new(&text).color(colors.accent).strong()
                        } else {
                            RichText::new(&text).color(colors.text)
                        };

                        ui.horizontal(|ui| {
                            if ui.add(Button::new(rich).fill(if is_current { colors.card } else { Color32::TRANSPARENT })).clicked() {
                                self.current_chapter_idx = chapter.index;
                                self.scroll_to_top = true;
                                self.show_toc = false;
                                *action = ReaderAction::ChangeChapter(self.current_chapter_idx);
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(format!("{} words", chapter.word_count)).color(colors.text_muted).size(11.0));
                            });
                        });
                        ui.add_space(2.0);
                    }
                });
            });
    }

    fn render_settings_panel(&mut self, ui: &mut Ui, colors: &ReaderColors, config: &mut AppConfig) {
        Frame::NONE
            .fill(colors.surface)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("Reading Appearance").color(colors.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("✖").clicked() {
                            self.show_settings = false;
                        }
                    });
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // Theme selector
                ui.label(RichText::new("Theme Palette:").color(colors.text_muted).size(12.0));
                ui.horizontal_wrapped(|ui| {
                    for theme in ReaderThemeType::all() {
                        let is_selected = config.theme == *theme;
                        let t_colors = theme.colors();
                        let btn = Button::new(RichText::new(theme.display_name()).color(t_colors.text))
                            .fill(t_colors.bg)
                            .stroke(Stroke::new(if is_selected { 2.0 } else { 1.0 }, if is_selected { colors.accent } else { colors.border }));

                        if ui.add(btn).clicked() {
                            config.theme = *theme;
                            let _ = config.save();
                        }
                    }
                });

                ui.add_space(12.0);

                // Font Family
                ui.label(RichText::new("Font Family:").color(colors.text_muted).size(12.0));
                ui.horizontal(|ui| {
                    for family in crate::reader::typography::FontFamilyChoice::all() {
                        let is_selected = config.typography.font_family == *family;
                        if ui.selectable_label(is_selected, family.display_name()).clicked() {
                            config.typography.font_family = *family;
                            let _ = config.save();
                        }
                    }
                });

                ui.add_space(12.0);

                // Sliders: Font Size, Line Spacing, Column Width, Paragraph Spacing
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Font Size:").color(colors.text));
                    if ui.add(egui::Slider::new(&mut config.typography.font_size, 12.0..=32.0).suffix(" pt")).changed() {
                        let _ = config.save();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Line Spacing:").color(colors.text));
                    if ui.add(egui::Slider::new(&mut config.typography.line_spacing_multiplier, 1.2..=2.4).step_by(0.1)).changed() {
                        let _ = config.save();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Reading Width:").color(colors.text));
                    if ui.add(egui::Slider::new(&mut config.typography.max_column_width, 500.0..=1200.0).suffix(" px")).changed() {
                        let _ = config.save();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Paragraph Gap:").color(colors.text));
                    if ui.add(egui::Slider::new(&mut config.typography.paragraph_spacing, 6.0..=28.0).suffix(" px")).changed() {
                        let _ = config.save();
                    }
                });

                ui.add_space(8.0);

                // Toggles
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut config.typography.indent_paragraphs, "Indent first line").changed() {
                        let _ = config.save();
                    }
                    if ui.checkbox(&mut config.typography.show_chapter_pre_notes, "Show pre-notes").changed() {
                        let _ = config.save();
                    }
                    if ui.checkbox(&mut config.typography.show_chapter_post_notes, "Show end notes").changed() {
                        let _ = config.save();
                    }
                });
            });
    }

    fn render_bookmark_dialog(&mut self, ui: &mut Ui, colors: &ReaderColors, action: &mut ReaderAction) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.accent))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.heading(RichText::new(format!("Bookmark Chapter {}", self.current_chapter_idx)).color(colors.text));
                ui.add_space(8.0);
                ui.label(RichText::new("Add an optional personal note or reminder:").color(colors.text_muted).size(12.0));

                ui.add(
                    egui::TextEdit::multiline(&mut self.bookmark_note_input)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .hint_text("e.g. Loved this scene, resume reading here later..."),
                );

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let save_btn = Button::new(RichText::new("Save Bookmark").color(Color32::WHITE)).fill(colors.accent);
                    if ui.add(save_btn).clicked() {
                        let bm = Bookmark {
                            id: format!("bm_{}_{}", self.current_chapter_idx, Utc::now().timestamp_millis()),
                            chapter_index: self.current_chapter_idx,
                            paragraph_index: 0,
                            note: self.bookmark_note_input.clone(),
                            excerpt: self.current_chapter().and_then(|c| c.paragraphs.first().cloned()).unwrap_or_default(),
                            created_at: Utc::now(),
                        };
                        *action = ReaderAction::AddBookmark(bm);
                        self.show_bookmark_dialog = false;
                        self.bookmark_note_input.clear();
                        self.set_status("Bookmark saved!");
                    }

                    if ui.button("Cancel").clicked() {
                        self.show_bookmark_dialog = false;
                        self.bookmark_note_input.clear();
                    }
                });
            });
    }

    fn render_chapter_content(
        &mut self,
        ui: &mut Ui,
        colors: &ReaderColors,
        typography: &TypographySettings,
        action: &mut ReaderAction,
    ) {
        let is_first_chapter = self.current_chapter_idx == 1;

        // Render Work Header on Chapter 1
        if is_first_chapter {
            self.render_work_preface(ui, colors);
            ui.add_space(30.0);
        }

        if let Some(chapter) = self.current_chapter().cloned() {
            // Chapter Title
            ui.add_space(10.0);
            ui.label(
                RichText::new(&chapter.title)
                    .size(typography.font_size * 1.35)
                    .strong()
                    .color(colors.text),
            );
            ui.add_space(14.0);

            // Chapter Summary if available
            if typography.show_chapter_summary {
                if let Some(summary) = &chapter.summary {
                    Frame::NONE
                        .fill(colors.notes_bg)
                        .stroke(Stroke::new(1.0, colors.border))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::same(14))
                        .show(ui, |ui| {
                            ui.label(RichText::new("Chapter Summary:").strong().size(12.0).color(colors.accent));
                            ui.add_space(4.0);
                            ui.label(RichText::new(summary).size(typography.font_size * 0.9).color(colors.text));
                        });
                    ui.add_space(16.0);
                }
            }

            // Chapter Pre-notes
            if typography.show_chapter_pre_notes {
                if let Some(pre) = &chapter.pre_notes {
                    Frame::NONE
                        .fill(colors.notes_bg)
                        .stroke(Stroke::new(1.0, colors.border))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::same(14))
                        .show(ui, |ui| {
                            ui.label(RichText::new("Chapter Notes:").strong().size(12.0).color(colors.accent));
                            ui.add_space(4.0);
                            ui.label(RichText::new(pre).size(typography.font_size * 0.9).color(colors.text));
                        });
                    ui.add_space(20.0);
                }
            }

            // Chapter Paragraphs
            for p in &chapter.paragraphs {
                if p == "---" {
                    ui.add_space(typography.paragraph_spacing);
                    ui.separator();
                    ui.add_space(typography.paragraph_spacing);
                } else if p.starts_with("> ") {
                    // Blockquote
                    let quote_text = p.trim_start_matches("> ").trim();
                    Frame::NONE
                        .fill(colors.quote_bg)
                        .stroke(Stroke::new(2.0, colors.accent))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(Margin::symmetric(14, 8))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(quote_text)
                                    .italics()
                                    .family(typography.font_family.to_egui_family())
                                    .size(typography.font_size)
                                    .color(colors.text),
                            );
                        });
                    ui.add_space(typography.paragraph_spacing);
                } else {
                    let formatted_p = if typography.indent_paragraphs {
                        format!("    {}", p)
                    } else {
                        p.clone()
                    };

                    ui.label(
                        RichText::new(formatted_p)
                            .family(typography.font_family.to_egui_family())
                            .size(typography.font_size)
                            .line_height(Some(typography.font_size * typography.line_spacing_multiplier))
                            .color(colors.text),
                    );
                    ui.add_space(typography.paragraph_spacing);
                }
            }

            // Chapter Post-notes
            if typography.show_chapter_post_notes {
                if let Some(post) = &chapter.post_notes {
                    ui.add_space(16.0);
                    Frame::NONE
                        .fill(colors.notes_bg)
                        .stroke(Stroke::new(1.0, colors.border))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::same(14))
                        .show(ui, |ui| {
                            ui.label(RichText::new("Chapter End Notes:").strong().size(12.0).color(colors.accent));
                            ui.add_space(4.0);
                            ui.label(RichText::new(post).size(typography.font_size * 0.9).color(colors.text));
                        });
                }
            }

            ui.add_space(30.0);
            ui.separator();
            ui.add_space(20.0);

            // Bottom Chapter Navigation Buttons
            self.render_chapter_navigation_buttons(ui, colors, action);
        }
    }

    fn render_work_preface(&self, ui: &mut Ui, colors: &ReaderColors) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(20))
            .show(ui, |ui| {
                // Title
                ui.label(RichText::new(&self.work.meta.title).size(26.0).strong().color(colors.accent));
                ui.add_space(4.0);

                // Author
                let authors = self.work.meta.authors.join(", ");
                ui.label(RichText::new(format!("by {authors}")).size(15.0).color(colors.text_muted));
                ui.add_space(14.0);

                // Rating & Warning badges
                ui.horizontal_wrapped(|ui| {
                    let r_badge = self.work.meta.rating;
                    let c = r_badge.badge_color();
                    let rating_color = Color32::from_rgb(c[0], c[1], c[2]);

                    Frame::NONE
                        .fill(rating_color)
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.label(RichText::new(r_badge.display_name()).color(Color32::WHITE).size(11.0).strong());
                        });

                    for warning in &self.work.meta.warnings {
                        Frame::NONE
                            .fill(Color32::from_rgb(180, 40, 40))
                            .corner_radius(CornerRadius::same(4))
                            .inner_margin(Margin::symmetric(8, 3))
                            .show(ui, |ui| {
                                ui.label(RichText::new(warning).color(Color32::WHITE).size(11.0));
                            });
                    }
                });

                ui.add_space(10.0);

                // Fandoms
                if !self.work.meta.fandoms.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Fandoms:").strong().size(12.0).color(colors.text_muted));
                        for fandom in &self.work.meta.fandoms {
                            ui.label(RichText::new(fandom).size(12.0).color(colors.text).strong());
                        }
                    });
                    ui.add_space(6.0);
                }

                // Relationships / Ships
                if !self.work.meta.relationships.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Relationships:").strong().size(12.0).color(colors.text_muted));
                        for rel in &self.work.meta.relationships {
                            Frame::NONE
                                .fill(colors.surface)
                                .corner_radius(CornerRadius::same(4))
                                .inner_margin(Margin::symmetric(6, 2))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(format!("♥ {rel}")).size(11.0).color(colors.accent));
                                });
                        }
                    });
                    ui.add_space(6.0);
                }

                // Summary
                if !self.work.meta.summary_text.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Summary:").strong().size(12.0).color(colors.text_muted));
                    ui.add_space(2.0);
                    ui.label(RichText::new(&self.work.meta.summary_text).size(13.5).color(colors.text));
                }

                // Stats
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);

                let words = self.work.total_words();
                let chaps = format!("{}/{}", self.work.meta.stats.chapters_published, self.work.meta.stats.chapters_total.map(|c| c.to_string()).unwrap_or_else(|| "?".to_string()));
                let stats_str = format!("Words: {words} | Chapters: {chaps} | Kudos: {} | Bookmarks: {}", self.work.meta.stats.kudos, self.work.meta.stats.bookmarks);
                ui.label(RichText::new(stats_str).size(11.5).color(colors.text_muted));
            });
    }

    fn render_chapter_navigation_buttons(
        &mut self,
        ui: &mut Ui,
        colors: &ReaderColors,
        action: &mut ReaderAction,
    ) {
        ui.horizontal(|ui| {
            // Previous Chapter Button
            let has_prev = self.current_chapter_idx > 1;
            let prev_btn = Button::new(RichText::new("◀ Previous Chapter [←]").size(14.0).color(if has_prev { colors.text } else { colors.text_muted }))
                .fill(if has_prev { colors.card } else { Color32::TRANSPARENT });

            if ui.add_enabled(has_prev, prev_btn).clicked() {
                self.current_chapter_idx -= 1;
                self.scroll_to_top = true;
                *action = ReaderAction::ChangeChapter(self.current_chapter_idx);
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // Next Chapter Button
                let has_next = self.current_chapter_idx < self.total_chapters();
                if has_next {
                    let next_btn = Button::new(RichText::new("Next Chapter [→] ▶").size(14.0).color(Color32::WHITE))
                        .fill(colors.accent);

                    if ui.add(next_btn).clicked() {
                        self.current_chapter_idx += 1;
                        self.scroll_to_top = true;
                        *action = ReaderAction::ChangeChapter(self.current_chapter_idx);
                    }
                } else {
                    // Completed work banner
                    let done_btn = Button::new(RichText::new("✔ Finished Work! Leave Kudos ❤️").size(14.0).color(Color32::WHITE))
                        .fill(Color32::from_rgb(225, 29, 72));

                    if ui.add(done_btn).clicked() {
                        *action = ReaderAction::LeaveKudos(self.work.meta.id.clone());
                    }
                }
            });
        });
    }

    fn render_bottom_progress(
        &mut self,
        ui: &mut Ui,
        colors: &ReaderColors,
        config: &AppConfig,
        action: &mut ReaderAction,
    ) {
        Frame::NONE
            .fill(colors.surface)
            .stroke(Stroke::new(1.0, colors.border))
            .inner_margin(Margin::symmetric(16, 6))
            .show(ui, |ui| {
                let progress = self.calculate_progress();
                let (ch_mins, work_mins) = self.estimated_time_left_minutes(config.words_per_minute);

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "Chapter {} of {} ({:.0}%)",
                            self.current_chapter_idx,
                            self.total_chapters(),
                            progress.percentage
                        ))
                        .size(11.5)
                        .color(colors.text_muted),
                    );

                    ui.separator();

                    let time_str = if work_mins > 60 {
                        format!("~{}h {}m left in work ({}m in chapter)", work_mins / 60, work_mins % 60, ch_mins)
                    } else {
                        format!("~{}m left in work ({}m in chapter)", work_mins, ch_mins)
                    };
                    ui.label(RichText::new(time_str).size(11.5).color(colors.text_muted));

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let total_words = self.work.total_words();
                        ui.label(RichText::new(format!("{total_words} total words")).size(11.0).color(colors.text_muted));
                    });
                });
            });

        // Trigger progress save
        *action = ReaderAction::UpdateProgress(self.calculate_progress());
    }
}
