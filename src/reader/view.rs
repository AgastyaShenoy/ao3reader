use crate::ao3::models::{Bookmark, ReadingProgress, Work};
use crate::reader::font_manager::FontManager;
use crate::reader::paginator::{Page, PageItem, Paginator};
use crate::reader::search::ReaderSearchState;
use crate::reader::theme::{ReaderColors, ReaderThemeType};
use crate::reader::typography::{FontFamilyChoice, PageLayoutMode, ReadingMode, TypographySettings};
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
    pub current_page_idx: usize,    // 1-based page in current chapter
    pub cached_pages: Vec<Page>,
    pub last_pagination_key: Option<(usize, u32, u32, u32, u32)>,
    pub last_mouse_wheel_turn: Option<chrono::DateTime<Utc>>,
    pub is_zen_mode: bool,
    pub search_state: ReaderSearchState,
    pub show_toc: bool,
    pub show_settings: bool,
    pub show_bookmark_dialog: bool,
    pub bookmark_note_input: String,
    pub status_toast: Option<(String, chrono::DateTime<Utc>)>,
    pub scroll_to_top: bool,
    pub font_search_query: String,
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
            current_page_idx: 1,
            cached_pages: Vec::new(),
            last_pagination_key: None,
            last_mouse_wheel_turn: None,
            is_zen_mode: false,
            search_state: ReaderSearchState::new(),
            show_toc: false,
            show_settings: false,
            show_bookmark_dialog: false,
            bookmark_note_input: String::new(),
            status_toast: None,
            scroll_to_top: true,
            font_search_query: String::new(),
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
            let ch_pct = ((self.current_chapter_idx.saturating_sub(1)) as f32) / (total as f32);
            let page_fraction = if !self.cached_pages.is_empty() {
                ((self.current_page_idx as f32) / (self.cached_pages.len() as f32)) / (total as f32)
            } else {
                1.0 / (total as f32)
            };
            ((ch_pct + page_fraction) * 100.0).clamp(0.0, 100.0)
        } else {
            100.0
        };

        ReadingProgress {
            current_chapter: self.current_chapter_idx,
            scroll_offset: self.current_page_idx as f32,
            position: 0.0,
            percentage: pct,
            last_read: Utc::now(),
            completed: self.current_chapter_idx >= total && (!self.cached_pages.is_empty() && self.current_page_idx >= self.cached_pages.len()),
        }
    }

    pub fn estimated_time_left_minutes(&self, wpm: u32) -> (u32, u32) {
        let words_per_min = if wpm > 0 { wpm as usize } else { 230 };

        // Chapter time remaining (factoring in current page)
        let ch_words = self.current_chapter().map(|c| c.word_count).unwrap_or(0);
        let ch_words_left = if !self.cached_pages.is_empty() {
            let pages_left = self.cached_pages.len().saturating_sub(self.current_page_idx);
            let words_per_page = ch_words / self.cached_pages.len().max(1);
            pages_left * words_per_page
        } else {
            ch_words
        };
        let ch_mins = ((ch_words_left as f32) / (words_per_min as f32)).ceil() as u32;

        // Work time remaining
        let future_chapters_words: usize = self
            .work
            .chapters
            .iter()
            .skip(self.current_chapter_idx)
            .map(|c| c.word_count)
            .sum();
        let work_mins = (((ch_words_left + future_chapters_words) as f32) / (words_per_min as f32)).ceil() as u32;

        (ch_mins, work_mins)
    }

    pub fn turn_page_next_spread(&mut self, is_dual: bool, action: &mut ReaderAction) {
        let step = if is_dual { 2 } else { 1 };
        let total = self.cached_pages.len();

        if total > 0 && self.current_page_idx + step <= total {
            self.current_page_idx += step;
        } else if total > 0 && self.current_page_idx < total {
            self.current_page_idx = total;
        } else if self.current_chapter_idx < self.total_chapters() {
            // Advance to next chapter, page 1
            self.current_chapter_idx += 1;
            self.current_page_idx = 1;
            self.last_pagination_key = None;
            self.scroll_to_top = true;
            *action = ReaderAction::ChangeChapter(self.current_chapter_idx);
            self.set_status(format!("Chapter {} of {}", self.current_chapter_idx, self.total_chapters()));
        }
    }

    pub fn turn_page_prev_spread(&mut self, is_dual: bool, action: &mut ReaderAction) {
        let step = if is_dual { 2 } else { 1 };

        if self.current_page_idx > step {
            self.current_page_idx -= step;
        } else if self.current_page_idx > 1 {
            self.current_page_idx = 1;
        } else if self.current_chapter_idx > 1 {
            // Retreat to previous chapter, last page
            self.current_chapter_idx -= 1;
            self.current_page_idx = usize::MAX; // Will clamp to total_pages on pagination
            self.last_pagination_key = None;
            self.scroll_to_top = true;
            *action = ReaderAction::ChangeChapter(self.current_chapter_idx);
            self.set_status(format!("Chapter {} of {}", self.current_chapter_idx, self.total_chapters()));
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, config: &mut AppConfig) -> ReaderAction {
        let mut action = ReaderAction::None;
        let colors = config.theme.colors();

        // Safety: Ensure ReaderBookFont is always bound on context before rendering text
        let font_fam = egui::FontFamily::Name(crate::reader::typography::FontFamilyChoice::READER_BOOK_FONT.into());
        let has_reader_font = ui.ctx().fonts(|f| f.families().contains(&font_fam));
        if !has_reader_font {
            crate::reader::font_manager::FontManager::init_fonts(
                ui.ctx(),
                config.typography.custom_font_path.as_deref(),
                config.typography.custom_font_name.as_deref(),
            );
        }

        // Handle keyboard shortcuts
        self.handle_shortcuts(ui, config, &mut action);

        // Top Navigation & Reader Toolbar (hidden in Zen mode)
        if !self.is_zen_mode {
            self.render_top_toolbar(ui, &colors, config, &mut action);
        }

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

        // Render Reader Canvas: Paged vs Continuous Scroll
        if config.typography.reading_mode == ReadingMode::Paged {
            self.render_paged_view(ui, &colors, config, &mut action);
        } else {
            self.render_scroll_view(ui, &colors, config, &mut action);
        }

        // Bottom Progress Bar (hidden in Zen mode)
        if !self.is_zen_mode && config.typography.show_reading_progress_bar {
            self.render_bottom_progress(ui, &colors, config, &mut action);
        }

        action
    }

    fn handle_shortcuts(&mut self, ui: &mut Ui, config: &AppConfig, action: &mut ReaderAction) {
        let is_paged = config.typography.reading_mode == ReadingMode::Paged;
        let is_dual = match config.typography.page_layout {
            PageLayoutMode::Dual => true,
            PageLayoutMode::Single => false,
            PageLayoutMode::Auto => ui.available_width() >= 1100.0,
        };

        ui.input(|i| {
            if is_paged {
                // Page flipping shortcuts
                if i.key_pressed(Key::ArrowRight)
                    || i.key_pressed(Key::PageDown)
                    || (i.modifiers.is_none() && (i.key_pressed(Key::K) || i.key_pressed(Key::Space)))
                {
                    self.turn_page_next_spread(is_dual, action);
                } else if i.key_pressed(Key::ArrowLeft)
                    || i.key_pressed(Key::PageUp)
                    || (i.modifiers.is_none() && i.key_pressed(Key::J))
                    || (i.modifiers.shift && i.key_pressed(Key::Space))
                {
                    self.turn_page_prev_spread(is_dual, action);
                } else if i.key_pressed(Key::Home) {
                    self.current_page_idx = 1;
                } else if i.key_pressed(Key::End) {
                    self.current_page_idx = self.cached_pages.len().max(1);
                } else if i.modifiers.command && i.key_pressed(Key::ArrowLeft) {
                    // Previous chapter
                    if self.current_chapter_idx > 1 {
                        self.current_chapter_idx -= 1;
                        self.current_page_idx = 1;
                        self.last_pagination_key = None;
                        *action = ReaderAction::ChangeChapter(self.current_chapter_idx);
                    }
                } else if i.modifiers.command && i.key_pressed(Key::ArrowRight) {
                    // Next chapter
                    if self.current_chapter_idx < self.total_chapters() {
                        self.current_chapter_idx += 1;
                        self.current_page_idx = 1;
                        self.last_pagination_key = None;
                        *action = ReaderAction::ChangeChapter(self.current_chapter_idx);
                    }
                }
            } else {
                // Continuous Scroll mode chapter shortcuts
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
                }
            }

            if i.key_pressed(Key::F11) {
                self.is_zen_mode = !self.is_zen_mode;
            } else if i.modifiers.command && i.key_pressed(Key::F) {
                self.search_state.is_open = !self.search_state.is_open;
            } else if i.modifiers.command && i.key_pressed(Key::B) {
                self.show_bookmark_dialog = true;
            } else if i.key_pressed(Key::Escape) {
                if self.is_zen_mode {
                    self.is_zen_mode = false;
                }
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
                    let back_btn = Button::new(RichText::new("⬅ Library").color(colors.text).size(13.5).strong());
                    if ui.add(back_btn).clicked() {
                        *action = ReaderAction::CloseReader;
                    }

                    ui.separator();

                    // Table of Contents toggle
                    let toc_label = format!("📖 Chapters ({}/{})", self.current_chapter_idx, self.total_chapters());
                    let toc_btn = Button::new(RichText::new(toc_label).color(colors.text).size(13.0));
                    if ui.add(toc_btn).clicked() {
                        self.show_toc = !self.show_toc;
                    }

                    ui.separator();

                    // Title truncated
                    let title = &self.work.meta.title;
                    let display_title = if title.len() > 24 {
                        format!("{}...", &title[..21])
                    } else {
                        title.clone()
                    };
                    ui.label(RichText::new(display_title).strong().color(colors.text).size(13.0));

                    // Right side controls
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Zen Mode toggle
                        let zen_text = if self.is_zen_mode { "🔲 Exit Zen" } else { "🔲 Zen [F11]" };
                        if ui.add(Button::new(RichText::new(zen_text).color(colors.text).size(12.5))).clicked() {
                            self.is_zen_mode = !self.is_zen_mode;
                        }

                        // Reading Mode quick toggle
                        let is_paged = config.typography.reading_mode == ReadingMode::Paged;
                        let mode_label = if is_paged { "📖 Paged" } else { "📜 Scroll" };
                        let mode_btn = Button::new(RichText::new(mode_label).color(colors.accent).strong().size(12.5))
                            .fill(colors.card)
                            .stroke(Stroke::new(1.0, colors.accent));
                        if ui.add(mode_btn).on_hover_text("Switch between E-Reader Page Flipping and Continuous Scrolling").clicked() {
                            config.typography.reading_mode = if is_paged {
                                ReadingMode::Scroll
                            } else {
                                ReadingMode::Paged
                            };
                            let _ = config.save();
                        }

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
                            self.last_pagination_key = None;
                            let _ = config.save();
                        }
                        if ui.add(Button::new(RichText::new("A-").size(12.0).color(colors.text))).clicked() {
                            config.typography.font_size = (config.typography.font_size - 1.0).max(12.0);
                            self.last_pagination_key = None;
                            let _ = config.save();
                        }
                    });
                });
            });
    }

    fn render_paged_view(
        &mut self,
        ui: &mut Ui,
        colors: &ReaderColors,
        config: &AppConfig,
        action: &mut ReaderAction,
    ) {
        let avail_size = ui.available_size();
        let avail_w = avail_size.x;
        let avail_h = avail_size.y;

        let is_dual = match config.typography.page_layout {
            PageLayoutMode::Dual => true,
            PageLayoutMode::Single => false,
            PageLayoutMode::Auto => avail_w >= 1100.0,
        };

        let col_w = if is_dual {
            ((avail_w - 90.0) / 2.0).min(config.typography.max_column_width).max(320.0)
        } else {
            config.typography.max_column_width.min(avail_w - 70.0).max(320.0)
        };

        // Cache key for pagination
        let cache_key = (
            self.current_chapter_idx,
            col_w as u32,
            avail_h as u32,
            (config.typography.font_size * 10.0) as u32,
            (config.typography.line_spacing_multiplier * 10.0) as u32,
        );

        if self.last_pagination_key != Some(cache_key) || self.cached_pages.is_empty() {
            if let Some(ch) = self.current_chapter() {
                let is_first = self.current_chapter_idx == 1;
                self.cached_pages = Paginator::paginate(
                    ch,
                    is_first,
                    &self.work.meta,
                    col_w,
                    avail_h,
                    &config.typography,
                );
                self.last_pagination_key = Some(cache_key);

                if self.current_page_idx > self.cached_pages.len() {
                    self.current_page_idx = self.cached_pages.len().max(1);
                }
            }
        }

        // Handle Mouse Wheel page flips with debounce
        if config.typography.page_flip_mouse_wheel {
            let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_y.abs() > 8.0 {
                let now = Utc::now();
                let can_flip = self.last_mouse_wheel_turn
                    .map(|t| (now - t).num_milliseconds() > 220)
                    .unwrap_or(true);

                if can_flip {
                    if scroll_y < -8.0 {
                        self.turn_page_next_spread(is_dual, action);
                        self.last_mouse_wheel_turn = Some(now);
                    } else if scroll_y > 8.0 {
                        self.turn_page_prev_spread(is_dual, action);
                        self.last_mouse_wheel_turn = Some(now);
                    }
                }
            }
        }

        let chapter_title = self
            .current_chapter()
            .map(|c| c.title.clone())
            .unwrap_or_else(|| "Chapter".to_string());
        let total_pages = self.cached_pages.len().max(1);
        let cur_page = self.current_page_idx.clamp(1, total_pages);

        // Center reading canvas horizontally and vertically
        let page_card_h = (avail_h - 24.0).max(300.0);

        ui.horizontal(|ui| {
            // Left margin & Chevron button
            let has_prev = cur_page > 1 || self.current_chapter_idx > 1;
            ui.add_space(8.0);

            if config.typography.show_page_turn_indicators {
                let prev_btn = Button::new(RichText::new("‹").size(24.0).strong().color(if has_prev { colors.accent } else { colors.border }))
                    .fill(colors.card)
                    .stroke(Stroke::new(1.0, colors.border))
                    .corner_radius(CornerRadius::same(20));

                if ui.add_enabled(has_prev, prev_btn).on_hover_text("Previous Page (← or J)").clicked() {
                    self.turn_page_prev_spread(is_dual, action);
                }
            }

            // Available width for pages container
            let container_w = if is_dual { col_w * 2.0 + 24.0 } else { col_w };
            let side_margin = ((ui.available_width() - container_w - 50.0) / 2.0).max(6.0);
            ui.add_space(side_margin);

            if is_dual {
                // Two-Page Spread
                let left_page = self.cached_pages.get(cur_page - 1);
                let right_page = self.cached_pages.get(cur_page);

                if let Some(page_a) = left_page {
                    self.render_page_card(ui, page_a, colors, &config.typography, col_w, page_card_h, &chapter_title);
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                if let Some(page_b) = right_page {
                    self.render_page_card(ui, page_b, colors, &config.typography, col_w, page_card_h, &chapter_title);
                } else {
                    // Right page blank end-ornament card
                    self.render_blank_page_card(ui, colors, col_w, page_card_h, cur_page + 1, total_pages);
                }
            } else {
                // Single Page
                if let Some(page) = self.cached_pages.get(cur_page - 1) {
                    self.render_page_card(ui, page, colors, &config.typography, col_w, page_card_h, &chapter_title);
                }
            }

            ui.add_space(side_margin);

            // Right margin & Chevron button
            let has_next = cur_page < total_pages || self.current_chapter_idx < self.total_chapters();
            if config.typography.show_page_turn_indicators {
                let next_btn = Button::new(RichText::new("›").size(24.0).strong().color(if has_next { colors.accent } else { colors.border }))
                    .fill(colors.card)
                    .stroke(Stroke::new(1.0, colors.border))
                    .corner_radius(CornerRadius::same(20));

                if ui.add_enabled(has_next, next_btn).on_hover_text("Next Page (→ or K)").clicked() {
                    self.turn_page_next_spread(is_dual, action);
                }
            }
            ui.add_space(8.0);
        });
    }

    fn render_page_card(
        &self,
        ui: &mut Ui,
        page: &Page,
        colors: &ReaderColors,
        typography: &TypographySettings,
        width: f32,
        height: f32,
        chapter_title: &str,
    ) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::symmetric(24, 14))
            .show(ui, |ui| {
                ui.set_width(width);
                ui.set_height(height);

                ui.vertical(|ui| {
                    // 1. Subtle Page Header
                    ui.horizontal(|ui| {
                        let title = &self.work.meta.title;
                        let t_trunc = if title.len() > 24 { format!("{}...", &title[..21]) } else { title.clone() };
                        ui.label(RichText::new(t_trunc).size(11.0).color(colors.text_muted));

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let ch_trunc = if chapter_title.len() > 28 { format!("{}...", &chapter_title[..25]) } else { chapter_title.to_string() };
                            ui.label(RichText::new(ch_trunc).size(11.0).color(colors.text_muted));
                        });
                    });
                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // 2. Page Content Items
                    for item in &page.items {
                        match item {
                            PageItem::CoverPreface => {
                                self.render_work_preface(ui, colors);
                            }
                            PageItem::ChapterHeader { title, .. } => {
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new(title)
                                        .family(typography.font_family.to_egui_family(ui.ctx()))
                                        .size(typography.font_size * 1.35)
                                        .strong()
                                        .color(colors.text),
                                );
                                ui.add_space(10.0);
                            }
                            PageItem::ChapterSummary(summary) => {
                                Frame::NONE
                                    .fill(colors.notes_bg)
                                    .stroke(Stroke::new(1.0, colors.border))
                                    .corner_radius(CornerRadius::same(6))
                                    .inner_margin(Margin::same(12))
                                    .show(ui, |ui| {
                                        ui.label(RichText::new("Chapter Summary:").strong().size(11.5).color(colors.accent));
                                        ui.add_space(2.0);
                                        ui.label(
                                            RichText::new(summary)
                                                .family(typography.font_family.to_egui_family(ui.ctx()))
                                                .size(typography.font_size * 0.9)
                                                .color(colors.text),
                                        );
                                    });
                                ui.add_space(12.0);
                            }
                            PageItem::ChapterNotes { title, text } => {
                                Frame::NONE
                                    .fill(colors.notes_bg)
                                    .stroke(Stroke::new(1.0, colors.border))
                                    .corner_radius(CornerRadius::same(6))
                                    .inner_margin(Margin::same(12))
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(title).strong().size(11.5).color(colors.accent));
                                        ui.add_space(2.0);
                                        ui.label(
                                            RichText::new(text)
                                                .family(typography.font_family.to_egui_family(ui.ctx()))
                                                .size(typography.font_size * 0.9)
                                                .color(colors.text),
                                        );
                                    });
                                ui.add_space(12.0);
                            }
                            PageItem::Paragraph { text, is_continuation } => {
                                let formatted = if typography.indent_paragraphs && !*is_continuation {
                                    format!("    {}", text)
                                } else {
                                    text.clone()
                                };
                                ui.label(
                                    RichText::new(formatted)
                                        .family(typography.font_family.to_egui_family(ui.ctx()))
                                        .size(typography.font_size)
                                        .line_height(Some(typography.font_size * typography.line_spacing_multiplier))
                                        .color(colors.text),
                                );
                                ui.add_space(typography.paragraph_spacing);
                            }
                            PageItem::Blockquote(quote) => {
                                Frame::NONE
                                    .fill(colors.quote_bg)
                                    .stroke(Stroke::new(2.0, colors.accent))
                                    .corner_radius(CornerRadius::same(4))
                                    .inner_margin(Margin::symmetric(12, 6))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(quote)
                                                .italics()
                                                .family(typography.font_family.to_egui_family(ui.ctx()))
                                                .size(typography.font_size)
                                                .color(colors.text),
                                        );
                                    });
                                ui.add_space(typography.paragraph_spacing);
                            }
                            PageItem::Divider => {
                                ui.add_space(typography.paragraph_spacing / 2.0);
                                ui.separator();
                                ui.add_space(typography.paragraph_spacing / 2.0);
                            }
                            PageItem::ChapterEndMark => {
                                ui.add_space(14.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new("✦   ✦   ✦").size(14.0).color(colors.accent));
                                });
                                ui.add_space(10.0);
                            }
                        }
                    }

                    // 3. Subtle Page Footer
                    ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("— Page {} of {} —", page.page_index, page.total_pages))
                                    .size(11.0)
                                    .color(colors.text_muted),
                            );
                        });
                    });
                });
            });
    }

    fn render_blank_page_card(
        &self,
        ui: &mut Ui,
        colors: &ReaderColors,
        width: f32,
        height: f32,
        page_idx: usize,
        total_pages: usize,
    ) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::symmetric(24, 14))
            .show(ui, |ui| {
                ui.set_width(width);
                ui.set_height(height);

                ui.vertical_centered(|ui| {
                    ui.add_space(height / 2.5);
                    ui.label(RichText::new("✦").size(20.0).color(colors.border));
                    ui.add_space(8.0);
                    ui.label(RichText::new("End of Chapter").size(12.0).color(colors.text_muted));

                    ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(format!("— Page {} of {} —", page_idx, total_pages))
                                .size(11.0)
                                .color(colors.text_muted),
                        );
                    });
                });
            });
    }

    fn render_scroll_view(
        &mut self,
        ui: &mut Ui,
        colors: &ReaderColors,
        config: &AppConfig,
        action: &mut ReaderAction,
    ) {
        let mut scroll_area = ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(false);

        if self.scroll_to_top {
            scroll_area = scroll_area.vertical_scroll_offset(0.0);
            self.scroll_to_top = false;
        }

        scroll_area.show(ui, |ui| {
            ui.add_space(config.typography.side_margin / 2.0);

            ui.horizontal(|ui| {
                let avail_width = ui.available_width();
                let col_width = config.typography.max_column_width.min(avail_width - 32.0).max(300.0);
                let side_padding = ((avail_width - col_width) / 2.0).max(12.0);

                ui.add_space(side_padding);

                ui.vertical(|ui| {
                    ui.set_max_width(col_width);
                    ui.set_min_width(col_width);

                    self.render_chapter_content(ui, colors, &config.typography, action);

                    ui.add_space(40.0);
                });

                ui.add_space(side_padding);
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
                                    self.last_pagination_key = None;
                                }
                            }
                        }
                        if ui.button("▶").clicked() {
                            if let Some(m) = self.search_state.next_match() {
                                if m.chapter_index != self.current_chapter_idx {
                                    self.current_chapter_idx = m.chapter_index;
                                    self.scroll_to_top = true;
                                    self.last_pagination_key = None;
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
                                self.current_page_idx = 1;
                                self.last_pagination_key = None;
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

                // Reading Mode Toggle
                ui.label(RichText::new("Reading Flow Mode:").color(colors.text_muted).size(12.0));
                ui.horizontal(|ui| {
                    for mode in ReadingMode::all() {
                        let is_sel = config.typography.reading_mode == *mode;
                        if ui.selectable_label(is_sel, mode.display_name()).clicked() {
                            config.typography.reading_mode = *mode;
                            self.last_pagination_key = None;
                            let _ = config.save();
                        }
                    }
                });

                ui.add_space(8.0);

                // If Paged Mode: Spread layout
                if config.typography.reading_mode == ReadingMode::Paged {
                    ui.label(RichText::new("Page Layout Spread:").color(colors.text_muted).size(12.0));
                    ui.horizontal(|ui| {
                        for layout in PageLayoutMode::all() {
                            let is_sel = config.typography.page_layout == *layout;
                            if ui.selectable_label(is_sel, layout.display_name()).clicked() {
                                config.typography.page_layout = *layout;
                                self.last_pagination_key = None;
                                let _ = config.save();
                            }
                        }
                    });
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut config.typography.show_page_turn_indicators, "Show edge turn indicators (‹ / ›)").changed() {
                            let _ = config.save();
                        }
                        if ui.checkbox(&mut config.typography.page_flip_mouse_wheel, "Flip pages with mouse wheel").changed() {
                            let _ = config.save();
                        }
                    });
                    ui.add_space(8.0);
                }

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

                ui.add_space(10.0);

                // Font Family & System Fonts
                ui.label(RichText::new("Font Family & Book Typeface:").color(colors.text).strong());

                let active_name = config
                    .typography
                    .custom_font_name
                    .clone()
                    .unwrap_or_else(|| config.typography.font_family.display_name().to_string());

                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("Current: {active_name}")).size(12.0).color(colors.accent).strong());

                    if config.typography.custom_font_path.is_some() {
                        if ui.button("↺ Reset to Default").on_hover_text("Reset book typeface to standard Modern Sans").clicked() {
                            config.typography.font_family = FontFamilyChoice::SansSerif;
                            config.typography.custom_font_name = None;
                            config.typography.custom_font_path = None;
                            self.last_pagination_key = None;
                            let _ = config.save();
                        }
                    }

                    if ui.button("📂 Open Font File...").on_hover_text("Load any .ttf or .otf file from your disk").clicked() {
                        if let Some((name, path)) = FontManager::pick_custom_font_file() {
                            config.typography.font_family = FontFamilyChoice::Custom(name.clone());
                            config.typography.custom_font_name = Some(name);
                            config.typography.custom_font_path = Some(path);
                            self.last_pagination_key = None;
                            let _ = config.save();
                        }
                    }
                });

                ui.add_space(4.0);

                // 1. Built-in font presets
                ui.horizontal_wrapped(|ui| {
                    let is_sans = config.typography.custom_font_path.is_none() && config.typography.font_family == FontFamilyChoice::SansSerif;
                    if ui.selectable_label(is_sans, "Modern Sans (Default)").clicked() {
                        config.typography.font_family = FontFamilyChoice::SansSerif;
                        config.typography.custom_font_name = None;
                        config.typography.custom_font_path = None;
                        self.last_pagination_key = None;
                        let _ = config.save();
                    }

                    let is_mono = config.typography.custom_font_path.is_none() && config.typography.font_family == FontFamilyChoice::Monospace;
                    if ui.selectable_label(is_mono, "Code Monospace").clicked() {
                        config.typography.font_family = FontFamilyChoice::Monospace;
                        config.typography.custom_font_name = None;
                        config.typography.custom_font_path = None;
                        self.last_pagination_key = None;
                        let _ = config.save();
                    }
                });

                ui.add_space(4.0);

                // 2. Curated e-reader book fonts installed on this system
                let curated = FontManager::cached_curated_fonts();
                if !curated.is_empty() {
                    ui.label(RichText::new("Book-Optimized System Fonts:").size(11.0).color(colors.text_muted));
                    ui.horizontal_wrapped(|ui| {
                        for font in curated {
                            let is_active = config.typography.custom_font_name.as_deref() == Some(&font.name);
                            if ui.selectable_label(is_active, &font.name).clicked() {
                                config.typography.font_family = FontFamilyChoice::Custom(font.name.clone());
                                config.typography.custom_font_name = Some(font.name.clone());
                                config.typography.custom_font_path = Some(font.path.clone());
                                self.last_pagination_key = None;
                                let _ = config.save();
                            }
                        }
                    });
                    ui.add_space(4.0);
                }

                // 3. Search All System Fonts directly without popups closing on click
                let all_fonts = FontManager::cached_system_fonts();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("All System Fonts:").size(11.0).color(colors.text_muted));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.font_search_query)
                            .desired_width(170.0)
                            .hint_text("Search fonts..."),
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
                    .inner_margin(Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                            let mut count = 0;
                            for font in all_fonts {
                                if query.is_empty() || font.name.to_lowercase().contains(&query) {
                                    count += 1;
                                    let is_active = config.typography.custom_font_name.as_deref() == Some(&font.name);
                                    if ui.selectable_label(is_active, &font.name).clicked() {
                                        config.typography.font_family = FontFamilyChoice::Custom(font.name.clone());
                                        config.typography.custom_font_name = Some(font.name.clone());
                                        config.typography.custom_font_path = Some(font.path.clone());
                                        self.last_pagination_key = None;
                                        let _ = config.save();
                                    }
                                }
                            }
                            if count == 0 {
                                ui.label(RichText::new("No fonts matching query").size(11.0).color(colors.text_muted));
                            }
                        });
                    });

                ui.add_space(10.0);

                // Sliders: Font Size, Line Spacing, Column Width, Paragraph Spacing
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Font Size:").color(colors.text));
                    if ui.add(egui::Slider::new(&mut config.typography.font_size, 12.0..=32.0).suffix(" pt")).changed() {
                        self.last_pagination_key = None;
                        let _ = config.save();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Line Spacing:").color(colors.text));
                    if ui.add(egui::Slider::new(&mut config.typography.line_spacing_multiplier, 1.2..=2.4).step_by(0.1)).changed() {
                        self.last_pagination_key = None;
                        let _ = config.save();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Reading Width:").color(colors.text));
                    if ui.add(egui::Slider::new(&mut config.typography.max_column_width, 500.0..=1200.0).suffix(" px")).changed() {
                        self.last_pagination_key = None;
                        let _ = config.save();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Paragraph Gap:").color(colors.text));
                    if ui.add(egui::Slider::new(&mut config.typography.paragraph_spacing, 6.0..=28.0).suffix(" px")).changed() {
                        self.last_pagination_key = None;
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
                        self.last_pagination_key = None;
                        let _ = config.save();
                    }
                    if ui.checkbox(&mut config.typography.show_chapter_post_notes, "Show end notes").changed() {
                        self.last_pagination_key = None;
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
                            paragraph_index: self.current_page_idx,
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

        if is_first_chapter {
            self.render_work_preface(ui, colors);
            ui.add_space(30.0);
        }

        if let Some(chapter) = self.current_chapter().cloned() {
            ui.add_space(10.0);
            ui.label(
                RichText::new(&chapter.title)
                    .family(typography.font_family.to_egui_family(ui.ctx()))
                    .size(typography.font_size * 1.35)
                    .strong()
                    .color(colors.text),
            );
            ui.add_space(14.0);

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
                            ui.label(
                                RichText::new(summary)
                                    .family(typography.font_family.to_egui_family(ui.ctx()))
                                    .size(typography.font_size * 0.9)
                                    .color(colors.text),
                            );
                        });
                    ui.add_space(16.0);
                }
            }

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
                            ui.label(
                                RichText::new(pre)
                                    .family(typography.font_family.to_egui_family(ui.ctx()))
                                    .size(typography.font_size * 0.9)
                                    .color(colors.text),
                            );
                        });
                    ui.add_space(20.0);
                }
            }

            for p in &chapter.paragraphs {
                if p == "---" {
                    ui.add_space(typography.paragraph_spacing);
                    ui.separator();
                    ui.add_space(typography.paragraph_spacing);
                } else if p.starts_with("> ") {
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
                                    .family(typography.font_family.to_egui_family(ui.ctx()))
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
                            .family(typography.font_family.to_egui_family(ui.ctx()))
                            .size(typography.font_size)
                            .line_height(Some(typography.font_size * typography.line_spacing_multiplier))
                            .color(colors.text),
                    );
                    ui.add_space(typography.paragraph_spacing);
                }
            }

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
                            ui.label(
                                RichText::new(post)
                                    .family(typography.font_family.to_egui_family(ui.ctx()))
                                    .size(typography.font_size * 0.9)
                                    .color(colors.text),
                            );
                        });
                }
            }

            ui.add_space(30.0);
            ui.separator();
            ui.add_space(20.0);

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
                ui.label(RichText::new(&self.work.meta.title).size(26.0).strong().color(colors.accent));
                ui.add_space(4.0);

                let authors = self.work.meta.authors.join(", ");
                ui.label(RichText::new(format!("by {authors}")).size(15.0).color(colors.text_muted));
                ui.add_space(14.0);

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

                if !self.work.meta.fandoms.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Fandoms:").strong().size(12.0).color(colors.text_muted));
                        for fandom in &self.work.meta.fandoms {
                            ui.label(RichText::new(fandom).size(12.0).color(colors.text).strong());
                        }
                    });
                    ui.add_space(6.0);
                }

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

                if !self.work.meta.summary_text.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Summary:").strong().size(12.0).color(colors.text_muted));
                    ui.add_space(2.0);
                    ui.add(egui::Label::new(RichText::new(&self.work.meta.summary_text).size(13.5).color(colors.text)).wrap());
                }

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
            let has_prev = self.current_chapter_idx > 1;
            let prev_btn = Button::new(RichText::new("◀ Previous Chapter [←]").size(14.0).color(if has_prev { colors.text } else { colors.text_muted }))
                .fill(if has_prev { colors.card } else { Color32::TRANSPARENT });

            if ui.add_enabled(has_prev, prev_btn).clicked() {
                self.current_chapter_idx -= 1;
                self.current_page_idx = 1;
                self.last_pagination_key = None;
                self.scroll_to_top = true;
                *action = ReaderAction::ChangeChapter(self.current_chapter_idx);
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let has_next = self.current_chapter_idx < self.total_chapters();
                if has_next {
                    let next_btn = Button::new(RichText::new("Next Chapter [→] ▶").size(14.0).color(Color32::WHITE))
                        .fill(colors.accent);

                    if ui.add(next_btn).clicked() {
                        self.current_chapter_idx += 1;
                        self.current_page_idx = 1;
                        self.last_pagination_key = None;
                        self.scroll_to_top = true;
                        *action = ReaderAction::ChangeChapter(self.current_chapter_idx);
                    }
                } else {
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
        config: &mut AppConfig,
        action: &mut ReaderAction,
    ) {
        Frame::NONE
            .fill(colors.surface)
            .stroke(Stroke::new(1.0, colors.border))
            .inner_margin(Margin::symmetric(16, 6))
            .show(ui, |ui| {
                let progress = self.calculate_progress();
                let (ch_mins, work_mins) = self.estimated_time_left_minutes(config.words_per_minute);
                let is_paged = config.typography.reading_mode == ReadingMode::Paged;
                let is_dual = match config.typography.page_layout {
                    PageLayoutMode::Dual => true,
                    PageLayoutMode::Single => false,
                    PageLayoutMode::Auto => ui.available_width() >= 1100.0,
                };

                ui.horizontal(|ui| {
                    if is_paged {
                        // Paged Mode Navigation Bar
                        let total_pages = self.cached_pages.len().max(1);
                        let has_prev = self.current_page_idx > 1 || self.current_chapter_idx > 1;
                        let has_next = self.current_page_idx < total_pages || self.current_chapter_idx < self.total_chapters();

                        if ui.add_enabled(has_prev, Button::new("◀ Prev Page")).clicked() {
                            self.turn_page_prev_spread(is_dual, action);
                        }

                        // Page scrub slider
                        ui.label(RichText::new(format!("Page {} of {}", self.current_page_idx, total_pages)).size(12.0).strong().color(colors.text));
                        let slider = egui::Slider::new(&mut self.current_page_idx, 1..=total_pages).show_value(false);
                        ui.add(slider);

                        if ui.add_enabled(has_next, Button::new("Next Page ▶")).clicked() {
                            self.turn_page_next_spread(is_dual, action);
                        }

                        ui.separator();
                    }

                    // Chapter progress info
                    ui.label(
                        RichText::new(format!(
                            "Ch. {}/{} ({:.0}%)",
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

                    // Quick Mode & Spread toggles on the right
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Mode toggle pill
                        let mode_label = if is_paged { "📖 Paged Mode" } else { "📜 Scroll Mode" };
                        if ui.selectable_label(is_paged, mode_label).clicked() {
                            config.typography.reading_mode = if is_paged {
                                ReadingMode::Scroll
                            } else {
                                ReadingMode::Paged
                            };
                            let _ = config.save();
                        }

                        if is_paged {
                            let spread_label = match config.typography.page_layout {
                                PageLayoutMode::Auto => "Auto Spread",
                                PageLayoutMode::Single => "Single Page",
                                PageLayoutMode::Dual => "Dual Spread",
                            };
                            if ui.button(spread_label).on_hover_text("Cycle Single Page / Dual Spread / Auto").clicked() {
                                config.typography.page_layout = match config.typography.page_layout {
                                    PageLayoutMode::Auto => PageLayoutMode::Single,
                                    PageLayoutMode::Single => PageLayoutMode::Dual,
                                    PageLayoutMode::Dual => PageLayoutMode::Auto,
                                };
                                self.last_pagination_key = None;
                                let _ = config.save();
                            }
                        }
                    });
                });
            });
    }
}
