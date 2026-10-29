//! Settings screen: reading behaviour, typography with live preview, themes, shortcuts,
//! storage and updates, about.

use super::app::{reader_family, reader_font_label};
use super::gfx::*;
use super::icons;
use super::ui::*;
use crate::reader::theme::ReaderThemeType;
use crate::reader::typography::{FontFamilyChoice, PageLayoutMode, ReadingMode, TextAlignment};
use crate::storage::config::AppConfig;
use crate::updater::{UpdateStatus, CURRENT_VERSION, GITHUB_REPO};

const MAX_W: f32 = 860.0;
const FONT_ROW_H: f32 = 34.0;

pub enum SettingsAction {
    ConfigChanged,
    CheckUpdates,
    PickFontFile,
    Open(String),
    ClearCache,
}

pub struct SettingsCtx<'a> {
    pub config: &'a mut AppConfig,
    pub update: &'a UpdateStatus,
    pub fonts: &'a [String],
    pub storage_dir: String,
}

#[derive(Default)]
pub struct SettingsScreen {
    tab: usize,
    font_query: String,
    confirm_clear: bool,
}

const TABS: [(&str, &str); 6] = [
    (icons::READING, "Reading"),
    (icons::FONT, "Typography"),
    (icons::COLOR, "Themes"),
    (icons::KEYBOARD, "Shortcuts"),
    (icons::STORAGE, "Storage & updates"),
    (icons::INFO, "About"),
];
const TAB_TYPOGRAPHY: usize = 1;
const TAB_STORAGE: usize = 4;
const TAB_ABOUT: usize = 5;

const PREVIEW_TEXT: [&str; 2] = [
    "The library was quiet in the way only old libraries are quiet: not silent, but hushed, as though the books themselves were holding their breath between pages.",
    "She ran a finger along the spines until one of them, bound in faded green cloth, seemed to lean out to meet her. “Well,” she said softly, “hello to you too.”",
];

/// Keeps a whole char boundary when shortening text for display.
fn truncate_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", s[..i].trim_end()),
        None => s.to_string(),
    }
}

impl SettingsScreen {
    pub fn needs_fonts(&self) -> bool {
        self.tab == TAB_TYPOGRAPHY
    }

    pub fn show_updates(&mut self) {
        self.tab = TAB_STORAGE;
    }

    pub fn frame(&mut self, ui: &mut Ui, body: Rect, ctx: &mut SettingsCtx) -> Vec<SettingsAction> {
        let mut out = Vec::new();
        let c = ui.c;
        let w = (body.w - 48.0).min(MAX_W);
        let x = body.x + (body.w - w) / 2.0;

        let title = ui.text("Settings", &TextStyle::ui(26.0).semibold(), x, body.y + 22.0, c.text);
        let ver = format!("AO3 Reader v{CURRENT_VERSION} · native preview");
        ui.text(&ver, &TextStyle::ui(SMALL), x + title.w + 14.0, body.y + 22.0 + title.h - 18.0, c.muted);

        let tabs_y = body.y + 22.0 + title.h + 14.0;
        let (tabs_h, changed) = ui.tab_strip(id("settings-tabs"), x - 4.0, tabs_y, w + 8.0, &TABS, &mut self.tab);
        let scroll_id = id("settings-scroll");
        if changed {
            ui.scroll_to_top(scroll_id);
            self.confirm_clear = false;
        }

        let top = tabs_y + tabs_h + 14.0;
        let area = Rect::new(body.x, top, body.w, (body.bottom() - top).max(0.0));
        let tab = self.tab;
        ui.scroll_area(scroll_id, area, |ui, _view, y0| {
            let mut col = Col::new(x, y0 + 2.0, w);
            match tab {
                0 => self.reading_tab(ui, &mut col, ctx, &mut out),
                1 => self.typography_tab(ui, &mut col, ctx, &mut out),
                2 => self.themes_tab(ui, &mut col, ctx, &mut out),
                3 => self.shortcuts_tab(ui, &mut col),
                4 => self.storage_tab(ui, &mut col, ctx, &mut out),
                _ => self.about_tab(ui, &mut col, &mut out),
            }
            col.y + 32.0 - y0
        });
        out
    }

    // ---- shared row helpers ------------------------------------------------------------

    fn section(ui: &mut Ui, col: &mut Col, text: &str) {
        col.gap(14.0);
        let c = ui.c;
        let t = ui.text(text, &TextStyle::ui(13.0).semibold(), col.x + 2.0, col.y, c.text);
        col.gap(t.h + 8.0);
    }

    /// A settings card with a title, optional description and a right-hand control area.
    fn row(ui: &mut Ui, col: &mut Col, title: &str, sub: &str, ctrl_w: f32, ctrl_h: f32) -> Rect {
        let c = ui.c;
        let text_w = (col.w - ctrl_w - 56.0).max(120.0);
        let tt = ui.measure(title, &TextStyle::ui(BODY), TextOpts::wrap(text_w));
        let st = if sub.is_empty() { None } else { Some(ui.measure(sub, &TextStyle::ui(SMALL), TextOpts::wrap(text_w))) };
        let text_h = tt.h + st.map_or(0.0, |s| s.h + 2.0);
        let h = (text_h + 26.0).max(ctrl_h + 26.0).max(62.0);
        let r = col.take(h);
        col.gap(4.0);
        ui.card(r);
        let ty = r.y + (h - text_h) / 2.0;
        ui.g.draw_text(tt, r.x + 16.0, ty, c.text);
        if let Some(s) = st {
            ui.g.draw_text(s, r.x + 16.0, ty + tt.h + 2.0, c.muted);
        }
        Rect::new(r.right() - 16.0 - ctrl_w, r.y + (h - ctrl_h) / 2.0, ctrl_w, ctrl_h)
    }

    fn toggle_row(ui: &mut Ui, col: &mut Col, key: &str, title: &str, sub: &str, value: &mut bool) -> bool {
        let r = Self::row(ui, col, title, sub, 40.0, 20.0);
        ui.toggle(id(key), r.x, r.y, value)
    }

    fn slider_row(ui: &mut Ui, col: &mut Col, key: &str, title: &str, value: &mut f32, range: (f32, f32, f32), fmt: impl Fn(f32) -> String) -> bool {
        let c = ui.c;
        let ctrl_w = (col.w * 0.42).clamp(200.0, 340.0);
        let r = Self::row(ui, col, title, "", ctrl_w, 32.0);
        let label = fmt(*value);
        ui.text_in(&label, &TextStyle::ui(13.0), Rect::new(r.right() - 64.0, r.y, 64.0, r.h), Align::Right, c.muted);
        ui.slider(id(key), Rect::new(r.x, r.y, r.w - 72.0, r.h), value, range.0, range.1, range.2)
    }

    // ---- tabs --------------------------------------------------------------------------

    fn reading_tab(&mut self, ui: &mut Ui, col: &mut Col, ctx: &mut SettingsCtx, out: &mut Vec<SettingsAction>) {
        let t = &mut ctx.config.typography;
        let mut changed = false;

        Self::section(ui, col, "Reading mode");
        let r = Self::row(ui, col, "Layout", "Turn pages like a book, or scroll through each chapter.", 280.0, 34.0);
        let mut mode = if t.reading_mode == ReadingMode::Paged { 0 } else { 1 };
        if ui.segmented(id("mode"), r, &["Pages", "Scroll"], &mut mode) {
            t.reading_mode = if mode == 0 { ReadingMode::Paged } else { ReadingMode::Scroll };
            changed = true;
        }
        if t.reading_mode == ReadingMode::Paged {
            let r = Self::row(ui, col, "Page spread", "Auto shows two pages side by side on wide windows.", 300.0, 34.0);
            let mut spread = match t.page_layout {
                PageLayoutMode::Auto => 0,
                PageLayoutMode::Single => 1,
                PageLayoutMode::Dual => 2,
            };
            if ui.segmented(id("spread"), r, &["Auto", "Single", "Two pages"], &mut spread) {
                t.page_layout = [PageLayoutMode::Auto, PageLayoutMode::Single, PageLayoutMode::Dual][spread];
                changed = true;
            }
            changed |= Self::toggle_row(ui, col, "arrows", "Page turn arrows", "Show ‹ › buttons at the edges of the page.", &mut t.show_page_turn_indicators);
            changed |= Self::toggle_row(ui, col, "wheel", "Mouse wheel turns pages", "", &mut t.page_flip_mouse_wheel);
        }

        Self::section(ui, col, "Chapter content");
        changed |= Self::toggle_row(ui, col, "summary", "Chapter summaries", "", &mut t.show_chapter_summary);
        changed |= Self::toggle_row(ui, col, "pre-notes", "Author notes before chapters", "", &mut t.show_chapter_pre_notes);
        changed |= Self::toggle_row(ui, col, "end-notes", "Author notes after chapters", "", &mut t.show_chapter_post_notes);
        changed |= Self::toggle_row(ui, col, "progress", "Progress bar", "Chapter, page and time left at the bottom of the reader.", &mut t.show_reading_progress_bar);
        changed |= Self::toggle_row(
            ui,
            col,
            "next-chapter",
            "Continue into the next chapter",
            "In scroll mode, keep reading past the end of a chapter.",
            &mut ctx.config.auto_next_chapter,
        );

        Self::section(ui, col, "Pace");
        let mut wpm = ctx.config.words_per_minute as f32;
        if Self::slider_row(ui, col, "wpm", "Reading speed (used for time left)", &mut wpm, (120.0, 500.0, 10.0), |v| format!("{v:.0} wpm")) {
            ctx.config.words_per_minute = wpm as u32;
            changed = true;
        }

        if changed {
            out.push(SettingsAction::ConfigChanged);
        }
    }

    fn typography_tab(&mut self, ui: &mut Ui, col: &mut Col, ctx: &mut SettingsCtx, out: &mut Vec<SettingsAction>) {
        let c = ui.c;
        let mut changed = false;

        Self::section(ui, col, "Font");
        let label = reader_font_label(&ctx.config.typography);
        let r = Self::row(ui, col, "Reading font", &label, 238.0, 32.0);
        if ui.button_ex(id("font-file"), Rect::new(r.x, r.y, 150.0, r.h), "Font file…", Some(icons::OPEN_FILE), ButtonKind::Secondary, true) {
            out.push(SettingsAction::PickFontFile);
        }
        let is_default = ctx.config.typography.font_family == FontFamilyChoice::SansSerif;
        if ui.button_ex(id("font-reset"), Rect::new(r.x + 158.0, r.y, 80.0, r.h), "Reset", None, ButtonKind::Secondary, !is_default) {
            let t = &mut ctx.config.typography;
            t.font_family = FontFamilyChoice::SansSerif;
            t.custom_font_name = None;
            t.custom_font_path = None;
            changed = true;
        }

        // Search box + font list, each name drawn in its own face.
        let search = col.take(36.0);
        col.gap(6.0);
        let it = ui.measure(icons::SEARCH, &TextStyle::icons(14.0), TextOpts::default());
        let input_r = Rect::new(search.x + 30.0, search.y, search.w - 30.0, search.h);
        ui.g.draw_text(it, search.x + 6.0, search.y + (search.h - it.h) / 2.0, c.muted);
        ui.text_input(id("font-search"), input_r, &mut self.font_query, "Search installed fonts", false);

        let q = self.font_query.trim().to_lowercase();
        let mut entries: Vec<(String, FontFamilyChoice)> = Vec::new();
        if q.is_empty() {
            entries.push(("Segoe UI (default)".into(), FontFamilyChoice::SansSerif));
            entries.push(("Georgia".into(), FontFamilyChoice::Serif));
            entries.push(("Monospace".into(), FontFamilyChoice::Monospace));
        }
        entries.extend(ctx.fonts.iter().filter(|f| q.is_empty() || f.to_lowercase().contains(&q)).map(|f| (f.clone(), FontFamilyChoice::Custom(f.clone()))));

        let list_r = col.take(280.0);
        col.gap(4.0);
        ui.card(list_r);
        if entries.is_empty() {
            let msg = if ctx.fonts.is_empty() { "Loading fonts…" } else { "No installed font matches" };
            ui.text_in(msg, &TextStyle::ui(13.0), list_r, Align::Center, c.muted);
        }
        let current = ctx.config.typography.font_family.clone();
        let mut picked: Option<FontFamilyChoice> = None;
        ui.scroll_area(id("font-list"), list_r.shrink(1.0), |ui, view, y0| {
            let total = entries.len() as f32 * FONT_ROW_H;
            let first = (((view.y - y0) / FONT_ROW_H).floor().max(0.0)) as usize;
            let last = ((((view.bottom() - y0) / FONT_ROW_H).ceil()) as usize).min(entries.len());
            for i in first..last {
                let (name, choice) = &entries[i];
                let r = Rect::new(view.x + 4.0, y0 + i as f32 * FONT_ROW_H, view.w - 16.0, FONT_ROW_H);
                let resp = ui.interact(id2(id("font-row"), name), r);
                let selected = *choice == current;
                if selected {
                    ui.g.fill_round(r.shrink2(0.0, 2.0), 4.0, c.accent.alpha(0.16));
                } else if resp.hovered {
                    ui.g.fill_round(r.shrink2(0.0, 2.0), 4.0, c.hover());
                }
                let family = match choice {
                    FontFamilyChoice::Custom(n) => Family::Named { name: n.clone(), file: None },
                    FontFamilyChoice::Serif => Family::Named { name: "Georgia".into(), file: None },
                    FontFamilyChoice::Monospace => Family::Mono,
                    FontFamilyChoice::SansSerif => Family::Named { name: "Segoe UI".into(), file: None },
                };
                let style = TextStyle { family, ..TextStyle::ui(15.0) };
                ui.text_in(name, &style, Rect::new(r.x + 12.0, r.y, r.w - 44.0, r.h), Align::Left, c.text);
                if selected {
                    let ck = ui.measure(icons::CHECK, &TextStyle::icons(13.0), TextOpts::default());
                    ui.g.draw_text(ck, r.right() - 24.0, r.y + (r.h - ck.h) / 2.0, c.accent);
                }
                if resp.clicked {
                    picked = Some(choice.clone());
                }
            }
            total
        });
        if let Some(choice) = picked {
            let t = &mut ctx.config.typography;
            if let FontFamilyChoice::Custom(name) = &choice {
                t.custom_font_name = Some(name.clone());
            } else {
                t.custom_font_name = None;
            }
            t.custom_font_path = None;
            t.font_family = choice;
            changed = true;
        }

        Self::section(ui, col, "Text");
        let t = &mut ctx.config.typography;
        changed |= Self::slider_row(ui, col, "size", "Text size", &mut t.font_size, (12.0, 36.0, 1.0), |v| format!("{v:.0} px"));
        changed |= Self::slider_row(ui, col, "line", "Line height", &mut t.line_spacing_multiplier, (1.2, 2.6, 0.05), |v| format!("{v:.2}×"));
        changed |= Self::slider_row(ui, col, "width", "Column width", &mut t.max_column_width, (450.0, 1400.0, 10.0), |v| format!("{v:.0} px"));
        changed |= Self::slider_row(ui, col, "para", "Space between paragraphs", &mut t.paragraph_spacing, (4.0, 32.0, 1.0), |v| format!("{v:.0} px"));
        let r = Self::row(ui, col, "Alignment", "", 220.0, 34.0);
        let mut align = if t.text_alignment == TextAlignment::Justified { 1 } else { 0 };
        if ui.segmented(id("align"), r, &["Left", "Justified"], &mut align) {
            t.text_alignment = if align == 1 { TextAlignment::Justified } else { TextAlignment::Left };
            changed = true;
        }
        changed |= Self::toggle_row(ui, col, "indent", "Indent first line of paragraphs", "", &mut t.indent_paragraphs);

        Self::section(ui, col, "Preview");
        let pal = ctx.config.theme.palette();
        let page_bg = Color::rgb(pal.bg);
        let page_fg = Color::rgb(pal.text);
        let text_w = t.max_column_width.min(col.w - 64.0);
        let style = TextStyle { family: reader_family(t), ..TextStyle::ui(t.font_size) };
        let align = if t.text_alignment == TextAlignment::Justified { Align::Justify } else { Align::Left };
        let opts = TextOpts::wrap(text_w).align(align).line_height(t.line_spacing_multiplier);
        let paras: Vec<String> = PREVIEW_TEXT.iter().map(|p| if t.indent_paragraphs { format!("\u{2003}\u{2003}{p}") } else { p.to_string() }).collect();
        let layouts: Vec<Text> = paras.iter().map(|p| ui.measure(p, &style, opts)).collect();
        let body_h: f32 = layouts.iter().map(|l| l.h).sum::<f32>() + t.paragraph_spacing;
        let r = col.take(body_h + 64.0);
        ui.g.fill_round(r, 8.0, page_bg);
        ui.g.stroke_round(r, 8.0, 1.0, c.border);
        let mut y = r.y + 32.0;
        let px = r.x + (r.w - text_w) / 2.0;
        for l in layouts {
            ui.g.draw_text(l, px, y, page_fg);
            y += l.h + t.paragraph_spacing;
        }

        if changed {
            out.push(SettingsAction::ConfigChanged);
        }
    }

    fn themes_tab(&mut self, ui: &mut Ui, col: &mut Col, ctx: &mut SettingsCtx, out: &mut Vec<SettingsAction>) {
        Self::section(ui, col, "Colour theme");
        let cols = if col.w >= 600.0 { 3 } else { 2 };
        let gap = 10.0;
        let cw = (col.w - gap * (cols as f32 - 1.0)) / cols as f32;
        let ch = 112.0;
        let themes = ReaderThemeType::all();
        let rows = themes.len().div_ceil(cols);
        let grid = col.take(rows as f32 * (ch + gap));
        for (i, theme) in themes.iter().enumerate() {
            let r = Rect::new(grid.x + (i % cols) as f32 * (cw + gap), grid.y + (i / cols) as f32 * (ch + gap), cw, ch);
            let p = theme.palette();
            let resp = ui.interact(id2(id("theme"), i), r);
            let selected = ctx.config.theme == *theme;
            ui.g.fill_round(r, 10.0, Color::rgb(p.bg));
            let border = if selected { Color::rgb(p.accent) } else if resp.hovered { ui.c.text.alpha(0.5) } else { ui.c.border };
            ui.g.stroke_round(r, 10.0, if selected { 2.5 } else { 1.0 }, border);
            ui.text(theme.display_name(), &TextStyle::ui(15.0).semibold(), r.x + 16.0, r.y + 16.0, Color::rgb(p.text));
            ui.text_opts(
                "The quick brown fox jumps over the lazy dog.",
                &TextStyle::ui(12.5),
                TextOpts::ellipsize(r.w - 32.0),
                r.x + 16.0,
                r.y + 44.0,
                Color::rgb(p.text_muted),
            );
            ui.g.fill_round(Rect::new(r.x + 16.0, r.bottom() - 26.0, 44.0, 6.0), 3.0, Color::rgb(p.accent));
            ui.g.fill_round(Rect::new(r.x + 66.0, r.bottom() - 26.0, 28.0, 6.0), 3.0, Color::rgb(p.card));
            if selected {
                let ck = Rect::new(r.right() - 34.0, r.y + 12.0, 22.0, 22.0);
                ui.g.fill_circle(ck.x + 11.0, ck.y + 11.0, 11.0, Color::rgb(p.accent));
                ui.text_in(icons::CHECK, &TextStyle::icons(11.0), ck, Align::Center, Color::from_u8(255, 255, 255));
            }
            if resp.clicked && !selected {
                ctx.config.theme = *theme;
                out.push(SettingsAction::ConfigChanged);
            }
        }
    }

    fn shortcuts_tab(&mut self, ui: &mut Ui, col: &mut Col) {
        let c = ui.c;
        let groups: [(&str, &[(&str, &str)]); 2] = [
            (
                "Reader",
                &[
                    ("→  PgDn  Space", "Next page"),
                    ("←  PgUp  Shift+Space", "Previous page"),
                    ("Home  End", "First / last page of the chapter"),
                    ("Ctrl+→  Ctrl+←", "Next / previous chapter"),
                    ("Ctrl+F", "Search in the work"),
                    ("Ctrl+B", "Bookmark this page"),
                    ("Ctrl+=  Ctrl+-", "Larger / smaller text"),
                    ("F11", "Full screen"),
                    ("Esc", "Close panel or leave full screen"),
                ],
            ),
            ("Text fields", &[("Enter", "Submit"), ("Esc", "Cancel"), ("Ctrl+C  Ctrl+V  Ctrl+Z", "Copy, paste, undo")]),
        ];
        for (title, rows) in groups {
            Self::section(ui, col, title);
            let h = rows.len() as f32 * 40.0 + 8.0;
            let card = col.take(h);
            col.gap(4.0);
            ui.card(card);
            for (i, (keys, what)) in rows.iter().enumerate() {
                let y = card.y + 4.0 + i as f32 * 40.0;
                if i > 0 {
                    ui.separator(card.x + 12.0, y, card.w - 24.0);
                }
                let mut kx = card.x + 16.0;
                for k in keys.split("  ") {
                    let t = ui.measure(k, &TextStyle { family: Family::Mono, ..TextStyle::ui(12.5) }, TextOpts::default());
                    let pill = Rect::new(kx, y + 9.0, t.w + 16.0, 22.0);
                    ui.g.fill_round(pill, 5.0, c.bg);
                    ui.g.stroke_round(pill, 5.0, 1.0, c.border);
                    ui.g.draw_text(t, pill.x + 8.0, pill.y + (22.0 - t.h) / 2.0, c.text);
                    kx += pill.w + 6.0;
                }
                let dx = (card.x + 250.0).max(kx + 12.0);
                ui.text_in(what, &TextStyle::ui(13.5), Rect::new(dx, y, card.right() - dx - 16.0, 40.0), Align::Left, c.muted);
            }
        }
    }

    fn storage_tab(&mut self, ui: &mut Ui, col: &mut Col, ctx: &mut SettingsCtx, out: &mut Vec<SettingsAction>) {
        let c = ui.c;
        Self::section(ui, col, "Updates");
        if Self::toggle_row(ui, col, "auto-update", "Check for updates on startup", "Asks GitHub for the latest release when the app opens.", &mut ctx.config.check_updates_on_startup) {
            out.push(SettingsAction::ConfigChanged);
        }

        let (sub, button): (String, Option<&str>) = match ctx.update {
            UpdateStatus::Idle => (format!("You have version {CURRENT_VERSION}."), Some("Check now")),
            UpdateStatus::Checking => ("Checking GitHub for a newer release…".into(), None),
            UpdateStatus::UpToDate { checked_version } => (format!("You're on the latest version ({checked_version})."), Some("Check again")),
            UpdateStatus::UpdateAvailable(info) => (format!("{} is available (released {}).", info.latest_version, info.published_date), None),
            UpdateStatus::Error(e) => (format!("Couldn't check for updates: {e}"), Some("Retry")),
        };
        let r = Self::row(ui, col, "App version", &sub, 150.0, 32.0);
        match (ctx.update, button) {
            (UpdateStatus::Checking, _) => ui.spinner(r.right() - 16.0, r.y + 16.0, 9.0),
            (UpdateStatus::UpdateAvailable(info), _) => {
                if ui.button_ex(id("update-open"), r, "Get update", Some(icons::DOWNLOAD), ButtonKind::Primary, true) {
                    out.push(SettingsAction::Open(info.direct_download_url.clone().unwrap_or_else(|| info.release_url.clone())));
                }
            }
            (_, Some(label)) => {
                if ui.button_ex(id("update-check"), r, label, Some(icons::SYNC), ButtonKind::Secondary, true) {
                    out.push(SettingsAction::CheckUpdates);
                }
            }
            _ => {}
        }
        if let UpdateStatus::UpdateAvailable(info) = ctx.update {
            let notes = truncate_chars(info.release_notes.trim(), 700);
            if !notes.is_empty() {
                let t = ui.measure(&notes, &TextStyle::ui(13.0), TextOpts::wrap(col.w - 32.0).line_height(1.45));
                let card = col.take(t.h + 72.0);
                col.gap(4.0);
                ui.card(card);
                ui.text(&info.release_name, &TextStyle::ui(14.0).semibold(), card.x + 16.0, card.y + 14.0, c.text);
                ui.g.draw_text(t, card.x + 16.0, card.y + 40.0, c.muted);
                let link = Rect::new(card.right() - 150.0, card.y + 8.0, 134.0, 30.0);
                if ui.button_ex(id("release-notes"), link, "Release page", Some(icons::GLOBE), ButtonKind::Ghost, true) {
                    out.push(SettingsAction::Open(info.release_url.clone()));
                }
            }
        }

        Self::section(ui, col, "Storage");
        let r = Self::row(ui, col, "Library folder", &ctx.storage_dir, 140.0, 32.0);
        if ui.button_ex(id("open-folder"), r, "Open folder", Some(icons::FOLDER), ButtonKind::Secondary, true) {
            out.push(SettingsAction::Open(ctx.storage_dir.clone()));
        }
        let (title, sub) = if self.confirm_clear {
            ("Remove all downloaded works?", "Saved chapter text is deleted. Library entries and reading progress stay.")
        } else {
            ("Clear downloaded works", "Frees disk space. Library entries and reading progress are kept.")
        };
        let r = Self::row(ui, col, title, sub, if self.confirm_clear { 196.0 } else { 110.0 }, 32.0);
        if self.confirm_clear {
            if ui.button(id("clear-cancel"), Rect::new(r.x, r.y, 90.0, r.h), "Cancel") {
                self.confirm_clear = false;
            }
            if ui.button_ex(id("clear-yes"), Rect::new(r.x + 98.0, r.y, 98.0, r.h), "Remove", Some(icons::DELETE), ButtonKind::Danger, true) {
                self.confirm_clear = false;
                out.push(SettingsAction::ClearCache);
            }
        } else if ui.button(id("clear"), r, "Clear…") {
            self.confirm_clear = true;
        }
    }

    fn about_tab(&mut self, ui: &mut Ui, col: &mut Col, out: &mut Vec<SettingsAction>) {
        let c = ui.c;
        col.gap(14.0);
        let blurb = "A lightweight reader for Archive of Our Own. This native build draws its interface with Direct2D on the CPU and DirectWrite, so no graphics driver is loaded into the app.";
        let t = ui.measure(blurb, &TextStyle::ui(13.5), TextOpts::wrap(col.w - 32.0).line_height(1.45));
        let card = col.take(t.h + 96.0);
        col.gap(4.0);
        ui.card(card);
        let badge = Rect::new(card.x + 16.0, card.y + 18.0, 46.0, 28.0);
        ui.g.fill_round(badge, 6.0, c.accent);
        ui.text_in("AO3", &TextStyle::ui(14.0).bold(), badge, Align::Center, c.on_accent);
        ui.text("AO3 Reader", &TextStyle::ui(18.0).semibold(), card.x + 74.0, card.y + 16.0, c.text);
        ui.text(&format!("Version {CURRENT_VERSION}"), &TextStyle::ui(SMALL), card.x + 74.0, card.y + 40.0, c.muted);
        ui.g.draw_text(t, card.x + 16.0, card.y + 70.0, c.text);

        let (private_ws, ws) = memory_mb();
        let sub = format!("{private_ws:.1} MB private (what Task Manager shows) · {ws:.1} MB working set");
        Self::row(ui, col, "Memory in use", &sub, 0.0, 0.0);
        ui.wake_after(1000);

        let r = Self::row(ui, col, "Source code", &format!("github.com/{GITHUB_REPO}"), 120.0, 32.0);
        if ui.button_ex(id("github"), r, "GitHub", Some(icons::GLOBE), ButtonKind::Secondary, true) {
            out.push(SettingsAction::Open(format!("https://github.com/{GITHUB_REPO}")));
        }
    }
}

/// (private working set, working set) of this process in MB.
fn memory_mb() -> (f64, f64) {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2};
    use windows::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let mut m = PROCESS_MEMORY_COUNTERS_EX2 { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32, ..Default::default() };
        let ok = GetProcessMemoryInfo(GetCurrentProcess(), &mut m as *mut _ as *mut PROCESS_MEMORY_COUNTERS, m.cb).is_ok();
        if ok {
            (m.PrivateWorkingSetSize as f64 / 1048576.0, m.WorkingSetSize as f64 / 1048576.0)
        } else {
            (0.0, 0.0)
        }
    }
}
