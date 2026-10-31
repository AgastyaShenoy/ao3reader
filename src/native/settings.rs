//! Settings screen, in categories down the side (along the top when the window is narrow):
//! appearance, text with a live preview, reading, accessibility, library and files, sources,
//! updates, shortcuts and about.

use super::app::{reader_family, reader_font_label};
use super::gfx::*;
use super::icons;
use super::ui::*;
use crate::reader::rich_text::word_start_spans;
use crate::reader::theme::{ReaderThemeType, Rgb, ThemePalette};
use crate::reader::typography::{FontFamilyChoice, PageLayoutMode, ReadingMode, TextAlignment};
use crate::storage::config::AppConfig;
use crate::updater::{UpdateStatus, CURRENT_VERSION, DISCORD_INVITE, GITHUB_REPO};

const MAX_W: f32 = 960.0;
const FONT_ROW_H: f32 = 34.0;

pub enum SettingsAction {
    ConfigChanged,
    CheckUpdates,
    PickFontFile,
    Open(String),
    ClearCache,
    /// Pick a file to import.
    Import,
    OpenWork(String),
    /// Choose a "Your colours" colour: the text colour (true) or the page colour.
    PickColor(bool),
    /// Go to Browse and add a catalog there.
    AddCatalog,
    /// A catalog was removed (Browse starts again from AO3).
    CatalogsChanged,
    /// Go to the Account page (signing in to AO3).
    Account,
}

/// The Import page's progress.
#[derive(Default)]
pub struct ImportState {
    pub busy: bool,
    pub error: Option<String>,
    /// The work imported last: (id, title).
    pub done: Option<(String, String)>,
}

pub struct SettingsCtx<'a> {
    /// Windows is in dark mode (for "Match Windows").
    pub system_dark: bool,
    pub config: &'a mut AppConfig,
    pub update: &'a UpdateStatus,
    pub fonts: &'a [String],
    pub storage_dir: String,
}

#[derive(Default)]
pub struct SettingsScreen {
    tab: usize,
    font_query: String,
    /// The interface font list is open (Typography → Advanced).
    ui_font_open: bool,
    ui_font_query: String,
    confirm_clear: bool,
    pub import: ImportState,
}

/// The categories, most used first: (icon, name, what's in it).
const TABS: [(&str, &str, &str); 9] = [
    (icons::COLOR, "Appearance", "Colour themes, and your own colours."),
    (icons::FONT, "Text", "The font, size and spacing of text in books."),
    (icons::READING, "Reading", "Pages or scrolling, what chapters show, and reading speed."),
    (icons::ACCESSIBILITY, "Accessibility", "Reading aids, easier text, colour and reading aloud."),
    (icons::LIBRARY, "Library & files", "Bring in books you have, save works as files, and storage."),
    (icons::GLOBE, "Sources", "Where books come from: AO3, Project Gutenberg and your catalogues."),
    (icons::UPDATE, "Updates", "New versions of Flax Reader."),
    (icons::KEYBOARD, "Shortcuts", "Keys for reading and getting around."),
    (icons::INFO, "About", "Flax Reader, its community, privacy and credits."),
];
const TAB_THEMES: usize = 0;
const TAB_TYPOGRAPHY: usize = 1;
const TAB_IMPORT: usize = 4;
const TAB_UPDATES: usize = 6;
const TAB_ABOUT: usize = 8;
/// Wide enough for the categories down the side.
const SIDEBAR_MIN: f32 = 900.0;
const SIDEBAR_W: f32 = 224.0;

/// Page tints (coloured overlays): soft yellow, peach, rose, blue, green, grey.
const TINTS: [(u32, &str); 6] = [
    (0xFFE680, "Yellow"),
    (0xFFB37A, "Peach"),
    (0xF59AB4, "Rose"),
    (0x7FB2F0, "Blue"),
    (0x8ED39A, "Green"),
    (0x9A9A9A, "Grey"),
];

/// Interface sizes offered (1.0 is the system's own size).
const UI_SCALES: [f32; 5] = [0.9, 1.0, 1.1, 1.25, 1.5];

fn rgb(c: u32) -> Rgb {
    Rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

const PREVIEW_TEXT: [&str; 2] = [
    "The library was quiet in the way only old libraries are quiet: not silent, but hushed, as though the books themselves were holding their breath between pages.",
    "She ran a finger along the spines until one of them, bound in faded green cloth, seemed to lean out to meet her. “Well,” she said softly, “hello to you too.”",
];

/// "45 minutes", "3 h 20 min".
fn reading_time(minutes: u32) -> String {
    match minutes {
        0..=1 => "a minute".into(),
        2..=59 => format!("{minutes} minutes"),
        _ if minutes % 60 == 0 => format!("{} h", minutes / 60),
        _ => format!("{} h {} min", minutes / 60, minutes % 60),
    }
}

/// The face a font list entry is drawn in.
fn choice_family(choice: &FontFamilyChoice) -> Family {
    match choice {
        FontFamilyChoice::Custom(n) => Family::Named { name: n.clone(), file: None },
        FontFamilyChoice::Serif => Family::Named { name: "Georgia".into(), file: None },
        FontFamilyChoice::Monospace => Family::Mono,
        FontFamilyChoice::SansSerif => Family::Named { name: "Segoe UI".into(), file: None },
        FontFamilyChoice::Atkinson => Family::Named { name: "Atkinson Hyperlegible".into(), file: super::fonts::atkinson() },
        FontFamilyChoice::OpenDyslexic => Family::Named { name: "OpenDyslexic".into(), file: super::fonts::open_dyslexic() },
    }
}

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
        self.tab = TAB_UPDATES;
    }

    pub fn show_typography(&mut self) {
        self.tab = TAB_TYPOGRAPHY;
    }

    pub fn show_import(&mut self) {
        self.tab = TAB_IMPORT;
    }

    pub fn frame(&mut self, ui: &mut Ui, body: Rect, ctx: &mut SettingsCtx) -> Vec<SettingsAction> {
        let mut out = Vec::new();
        let c = ui.c;
        let scroll_id = id("settings-scroll");
        let before = self.tab;
        let wide = body.w >= SIDEBAR_MIN;

        // Where the page goes: beside the categories, or under them.
        let (x, w, top) = if wide {
            let total = (body.w - 48.0).min(MAX_W + SIDEBAR_W + 32.0);
            let x0 = body.x + (body.w - total) / 2.0;
            let title = ui.text("Settings", &TextStyle::ui(26.0).semibold(), x0, body.y + 22.0, c.text);
            self.sidebar(ui, x0 - 8.0, body.y + 22.0 + title.h + 18.0);
            let x = x0 + SIDEBAR_W + 32.0;
            (x, (x0 + total - x).min(MAX_W), body.y + 14.0)
        } else {
            let w = (body.w - 48.0).min(MAX_W);
            let x = body.x + (body.w - w) / 2.0;
            let title = ui.text("Settings", &TextStyle::ui(26.0).semibold(), x, body.y + 22.0, c.text);
            let tabs_y = body.y + 22.0 + title.h + 14.0;
            // The strip wraps onto more rows when narrow; every category keeps its name.
            let items: Vec<(&str, &str)> = TABS.iter().map(|(icon, name, _)| (*icon, *name)).collect();
            let (tabs_h, _) = ui.tab_strip(id("settings-tabs"), x - 4.0, tabs_y, w + 8.0, &items, &[], &mut self.tab);
            (x, w, tabs_y + tabs_h + 6.0)
        };
        if self.tab != before {
            ui.scroll_to_top(scroll_id);
            self.confirm_clear = false;
        }

        let area = Rect::new(x - 8.0, top, (body.right() - x + 8.0).max(0.0), (body.bottom() - top).max(0.0));
        let tab = self.tab;
        ui.scroll_area(scroll_id, area, |ui, _view, y0| {
            let mut col = Col::new(x, y0 + 8.0, w);
            // The category's name and what's in it.
            let (_, name, what) = TABS[tab.min(TABS.len() - 1)];
            if wide {
                let t = ui.text(name, &TextStyle::ui(22.0).semibold(), col.x + 2.0, col.y, c.text);
                col.gap(t.h + 2.0);
            }
            let t = ui.text_opts(what, &TextStyle::ui(SMALL), TextOpts::wrap(col.w - 4.0), col.x + 2.0, col.y, c.muted);
            col.gap(t.h + 4.0);
            match tab {
                TAB_THEMES => self.themes_tab(ui, &mut col, ctx, &mut out),
                TAB_TYPOGRAPHY => self.typography_tab(ui, &mut col, ctx, &mut out),
                2 => self.reading_tab(ui, &mut col, ctx, &mut out),
                3 => self.accessibility_tab(ui, &mut col, ctx, &mut out),
                TAB_IMPORT => self.library_tab(ui, &mut col, ctx, &mut out),
                5 => self.sources_tab(ui, &mut col, ctx, &mut out),
                TAB_UPDATES => self.updates_tab(ui, &mut col, ctx, &mut out),
                7 => self.shortcuts_tab(ui, &mut col),
                _ => self.about_tab(ui, &mut col, &mut out),
            }
            col.y + 32.0 - y0
        });
        out
    }

    /// The categories down the side, About set apart at the end.
    fn sidebar(&mut self, ui: &mut Ui, x: f32, mut y: f32) {
        let c = ui.c;
        for (i, (icon, name, _)) in TABS.iter().enumerate() {
            if i == TAB_ABOUT {
                ui.separator(x + 12.0, y + 5.0, SIDEBAR_W - 24.0);
                y += 11.0;
            }
            let r = Rect::new(x, y, SIDEBAR_W, 38.0);
            let resp = ui.interact_as(id2(id("settings-nav"), i), r, Role::Tab(self.tab == i), name);
            if resp.clicked {
                self.tab = i;
            }
            let sel = self.tab == i;
            if sel {
                ui.g.fill_round(r, 8.0, c.selected());
                // A short accent mark at the left of the open category; the fill stays neutral.
                ui.g.fill_round(Rect::new(r.x, r.y + 11.0, 3.0, 16.0), 1.5, c.accent_mark);
            } else if resp.hovered {
                ui.g.fill_round(r, 8.0, c.hover());
            }
            let fg = if sel { c.text } else { c.muted };
            let it = ui.measure(icon, &TextStyle::icons(15.0), TextOpts::default());
            ui.g.draw_text(it, r.x + 16.0, r.y + (r.h - it.h) / 2.0, fg);
            let style = if sel { TextStyle::ui(14.0).semibold() } else { TextStyle::ui(14.0) };
            let t = ui.measure(name, &style, TextOpts::default());
            ui.g.draw_text(t, r.x + 46.0, r.y + (r.h - t.h) / 2.0, if sel { c.text } else { c.text.alpha(0.86) });
            y += 40.0;
        }
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
        ui.name_next(title, "");
        ui.toggle(id(key), r.x, r.y, value)
    }

    fn slider_row(ui: &mut Ui, col: &mut Col, key: &str, title: &str, value: &mut f32, range: (f32, f32, f32), fmt: impl Fn(f32) -> String) -> bool {
        let c = ui.c;
        let ctrl_w = (col.w * 0.42).clamp(200.0, 340.0);
        let r = Self::row(ui, col, title, "", ctrl_w, 32.0);
        let label = fmt(*value);
        ui.text_in(&label, &TextStyle::ui(13.0), Rect::new(r.right() - 64.0, r.y, 64.0, r.h), Align::Right, c.muted);
        ui.name_next(title, &label);
        ui.slider(id(key), Rect::new(r.x, r.y, r.w - 72.0, r.h), value, range.0, range.1, range.2)
    }

    // ---- tabs --------------------------------------------------------------------------

    fn reading_tab(&mut self, ui: &mut Ui, col: &mut Col, ctx: &mut SettingsCtx, out: &mut Vec<SettingsAction>) {
        let t = &mut ctx.config.typography;
        let mut changed = false;

        Self::section(ui, col, "Pages");
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

        Self::section(ui, col, "Reading speed");
        let speed = ctx.config.reading_speed;
        let sub = match speed.wpm() {
            Some(w) => format!("About {w} words a minute, learned from {} of your reading. Used for the time left in a work.", reading_time(speed.minutes())),
            None => format!("Learned from how fast you turn pages, after a few minutes of reading. Until then, {} words a minute is used.", ctx.config.words_per_minute),
        };
        changed |= Self::toggle_row(ui, col, "auto-wpm", "Work out my reading speed", &sub, &mut ctx.config.auto_reading_speed);
        if !ctx.config.auto_reading_speed {
            let mut wpm = ctx.config.words_per_minute as f32;
            if Self::slider_row(ui, col, "wpm", "Reading speed", &mut wpm, (120.0, 500.0, 10.0), |v| format!("{v:.0} wpm")) {
                ctx.config.words_per_minute = wpm as u32;
                changed = true;
            }
        } else if speed.secs > 0.0 {
            let r = Self::row(ui, col, "Start over", "Forget the reading measured so far, e.g. if someone else has been reading.", 96.0, 32.0);
            if ui.button(id("wpm-reset"), r, "Reset") {
                ctx.config.reading_speed = Default::default();
                changed = true;
            }
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

        let q = Self::font_search(ui, col, "font-search", &mut self.font_query);
        let mut entries: Vec<(String, FontFamilyChoice)> = Vec::new();
        if q.is_empty() {
            entries.push(("Segoe UI (default)".into(), FontFamilyChoice::SansSerif));
            entries.push(("Georgia".into(), FontFamilyChoice::Serif));
            entries.push(("Monospace".into(), FontFamilyChoice::Monospace));
        }
        // The two bundled easy-to-read fonts come next, and turn up in searches too.
        for (name, choice) in [("Atkinson Hyperlegible", FontFamilyChoice::Atkinson), ("OpenDyslexic", FontFamilyChoice::OpenDyslexic)] {
            if q.is_empty() || name.to_lowercase().contains(&q) {
                entries.push((name.into(), choice));
            }
        }
        entries.extend(ctx.fonts.iter().filter(|f| q.is_empty() || f.to_lowercase().contains(&q)).map(|f| (f.clone(), FontFamilyChoice::Custom(f.clone()))));
        let current = ctx.config.typography.font_family.clone();
        let rows: Vec<(String, Family, bool)> = entries.iter().map(|(n, ch)| (n.clone(), choice_family(ch), *ch == current)).collect();
        let picked = Self::font_list(ui, col, "font-list", &rows, ctx.fonts.is_empty()).map(|i| entries[i].1.clone());
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
        let pal = ctx.config.palette();
        let t = &ctx.config.typography;
        let page_bg = Color::rgb(pal.bg);
        let page_fg = Color::rgb(pal.text);
        let text_w = t.max_column_width.min(col.w - 64.0);
        let style = TextStyle { family: reader_family(t), ..TextStyle::ui(t.font_size) };
        let align = if t.text_alignment == TextAlignment::Justified { Align::Justify } else { Align::Left };
        let opts = TextOpts::wrap(text_w)
            .align(align)
            .line_height(t.line_spacing_multiplier)
            .spacing(t.letter_spacing * t.font_size, t.word_spacing * t.font_size);
        let paras: Vec<String> = PREVIEW_TEXT.iter().map(|p| if t.indent_paragraphs { format!("\u{2003}\u{2003}{p}") } else { p.to_string() }).collect();
        let layouts: Vec<Text> =
            paras.iter().map(|p| ui.g.layout_rich(p, &if t.bold_word_starts { word_start_spans(p) } else { Vec::new() }, &style, opts)).collect();
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

        // The app's own interface font, for those who want it to match their reading font
        // or need a plainer one; most people never change it.
        Self::section(ui, col, "Advanced");
        let current = ctx.config.typography.ui_font.clone();
        let sub = format!("Menus, buttons and labels. Now {}.", current.as_deref().unwrap_or("Segoe UI, the Windows default"));
        let r = Self::row(ui, col, "Interface font", &sub, 196.0, 32.0);
        let open_label = if self.ui_font_open { "Done" } else { "Change…" };
        if ui.button_ex(id("ui-font-open"), Rect::new(r.x, r.y, 108.0, r.h), open_label, None, ButtonKind::Secondary, true) {
            self.ui_font_open = !self.ui_font_open;
        }
        if ui.button_ex(id("ui-font-reset"), Rect::new(r.x + 116.0, r.y, 80.0, r.h), "Reset", None, ButtonKind::Secondary, current.is_some()) {
            ctx.config.typography.ui_font = None;
            changed = true;
        }
        if self.ui_font_open {
            let q = Self::font_search(ui, col, "ui-font-search", &mut self.ui_font_query);
            let mut rows: Vec<(String, Family, bool)> = Vec::new();
            let mut names: Vec<Option<String>> = Vec::new();
            if q.is_empty() {
                rows.push(("Segoe UI (default)".into(), Family::Named { name: "Segoe UI".into(), file: None }, current.is_none()));
                names.push(None);
            }
            for f in ctx.fonts.iter().filter(|f| q.is_empty() || f.to_lowercase().contains(&q)) {
                rows.push((f.clone(), Family::Named { name: f.clone(), file: None }, current.as_deref() == Some(f.as_str())));
                names.push(Some(f.clone()));
            }
            if let Some(i) = Self::font_list(ui, col, "ui-font-list", &rows, ctx.fonts.is_empty()) {
                ctx.config.typography.ui_font = names[i].clone();
                changed = true;
            }
        }

        if changed {
            out.push(SettingsAction::ConfigChanged);
        }
    }

    /// Search box above a font list; returns the lower-cased query.
    fn font_search(ui: &mut Ui, col: &mut Col, key: &str, query: &mut String) -> String {
        let c = ui.c;
        let search = col.take(36.0);
        col.gap(6.0);
        let it = ui.measure(icons::SEARCH, &TextStyle::icons(14.0), TextOpts::default());
        ui.g.draw_text(it, search.x + 6.0, search.y + (search.h - it.h) / 2.0, c.muted);
        ui.text_input(id(key), Rect::new(search.x + 30.0, search.y, search.w - 30.0, search.h), query, "Search installed fonts", false);
        query.trim().to_lowercase()
    }

    /// A scrolling list of fonts, each name drawn in its own face: (name, face, selected).
    /// Returns the row clicked.
    fn font_list(ui: &mut Ui, col: &mut Col, key: &str, rows: &[(String, Family, bool)], loading: bool) -> Option<usize> {
        let c = ui.c;
        let list_r = col.take(280.0);
        col.gap(4.0);
        ui.card(list_r);
        if rows.is_empty() {
            let msg = if loading { "Loading fonts…" } else { "No installed font matches" };
            ui.text_in(msg, &TextStyle::ui(13.0), list_r, Align::Center, c.muted);
        }
        let mut picked = None;
        ui.scroll_area(id(key), list_r.shrink(1.0), |ui, view, y0| {
            let first = (((view.y - y0) / FONT_ROW_H).floor().max(0.0)) as usize;
            let last = ((((view.bottom() - y0) / FONT_ROW_H).ceil()) as usize).min(rows.len());
            for (i, (name, family, selected)) in rows.iter().enumerate().take(last).skip(first) {
                let r = Rect::new(view.x + 4.0, y0 + i as f32 * FONT_ROW_H, view.w - 16.0, FONT_ROW_H);
                let resp = ui.interact_as(id2(id(key), name), r, Role::Choice(*selected), name);
                if *selected {
                    ui.g.fill_round(r.shrink2(0.0, 2.0), 4.0, c.selected());
                } else if resp.hovered {
                    ui.g.fill_round(r.shrink2(0.0, 2.0), 4.0, c.hover());
                }
                let style = TextStyle { family: family.clone(), ..TextStyle::ui(15.0) };
                ui.text_in(name, &style, Rect::new(r.x + 12.0, r.y, r.w - 44.0, r.h), Align::Left, c.text);
                if *selected {
                    let ck = ui.measure(icons::CHECK, &TextStyle::icons(13.0), TextOpts::default());
                    ui.g.draw_text(ck, r.right() - 24.0, r.y + (r.h - ck.h) / 2.0, c.accent_text);
                }
                if resp.clicked {
                    picked = Some(i);
                }
            }
            rows.len() as f32 * FONT_ROW_H
        });
        picked
    }

    fn themes_tab(&mut self, ui: &mut Ui, col: &mut Col, ctx: &mut SettingsCtx, out: &mut Vec<SettingsAction>) {
        let follow = ctx.config.follow_system;
        let mut on = follow;
        let mode = if ctx.system_dark { "dark" } else { "light" };
        let sub = if follow {
            format!("Pick a light theme and a dark theme below; Windows is in {mode} mode now. High contrast in Windows uses High Contrast here.")
        } else {
            "Switch between a light and a dark theme with Windows' own setting.".to_string()
        };
        if Self::toggle_row(ui, col, "follow-system", "Match Windows", &sub, &mut on) {
            ctx.config.follow_system = on;
            if on {
                // Start from the theme in use for whichever mode it suits.
                if ctx.config.palette().is_dark {
                    ctx.config.dark_theme = ctx.config.theme;
                } else {
                    ctx.config.light_theme = ctx.config.theme;
                }
            }
            out.push(SettingsAction::ConfigChanged);
        }

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
            let p = ctx.config.palette_of(*theme);
            let resp = ui.interact_as(id2(id("theme"), i), r, Role::Choice(false), theme.display_name());
            // Following Windows, each card can be the light or the dark theme.
            let slot = match (follow, p.is_dark) {
                (false, _) => None,
                (true, false) => Some((ctx.config.light_theme == *theme, "Light mode")),
                (true, true) => Some((ctx.config.dark_theme == *theme, "Dark mode")),
            };
            let selected = match slot {
                Some((s, _)) => s,
                None => ctx.config.theme == *theme,
            };
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
            if let Some((true, label)) = slot {
                ui.text_in(label, &TextStyle::ui(12.0), Rect::new(r.x + 104.0, r.bottom() - 32.0, r.w - 120.0, 18.0), Align::Right, Color::rgb(p.text_muted));
            }
            if selected {
                let ck = Rect::new(r.right() - 34.0, r.y + 12.0, 22.0, 22.0);
                ui.g.fill_circle(ck.x + 11.0, ck.y + 11.0, 11.0, Color::rgb(p.accent));
                ui.text_in(icons::CHECK, &TextStyle::icons(11.0), ck, Align::Center, Color::rgb(p.on_accent()));
            }
            if resp.clicked && !selected {
                match slot {
                    Some(_) if p.is_dark => ctx.config.dark_theme = *theme,
                    Some(_) => ctx.config.light_theme = *theme,
                    None => ctx.config.theme = *theme,
                }
                out.push(SettingsAction::ConfigChanged);
            }
        }

        // "Your colours": any page and text colour, with the contrast between them.
        Self::section(ui, col, "Your colours");
        let c = ui.c;
        for (text, title, value) in [(false, "Page colour", ctx.config.custom_bg), (true, "Text colour", ctx.config.custom_text)] {
            let r = Self::row(ui, col, title, &format!("#{value:06X}"), 150.0, 32.0);
            let sw = Rect::new(r.x, r.y + 2.0, 28.0, 28.0);
            ui.g.fill_round(sw, 6.0, Color::rgb(rgb(value)));
            ui.g.stroke_round(sw, 6.0, 1.0, c.border);
            if ui.button_ex(id2(id("pick-color"), text as usize), Rect::new(r.x + 38.0, r.y, 112.0, r.h), "Choose…", None, ButtonKind::Secondary, true) {
                out.push(SettingsAction::PickColor(text));
            }
        }
        let bg = rgb(ctx.config.custom_bg);
        let ratio = bg.contrast(rgb(ctx.config.custom_text));
        let (note, colour) = if ratio >= 7.0 {
            (format!("Contrast {ratio:.1}:1, very easy to read."), c.muted)
        } else if ratio >= 4.5 {
            (format!("Contrast {ratio:.1}:1, easy to read."), c.muted)
        } else {
            let way = if ThemePalette::custom_toward(bg) == Rgb(255, 255, 255) { "lighter" } else { "darker" };
            (format!("Contrast {ratio:.1}:1 is too low to read, so the text is made {way} until it reaches 4.5:1."), c.warning_text)
        };
        ui.text_opts(&note, &TextStyle::ui(SMALL), TextOpts::wrap(col.w - 8.0), col.x + 4.0, col.y + 2.0, colour);
        col.gap(26.0);
    }

    fn accessibility_tab(&mut self, ui: &mut Ui, col: &mut Col, ctx: &mut SettingsCtx, out: &mut Vec<SettingsAction>) {
        let c = ui.c;
        let mut changed = false;
        let t = &mut ctx.config.typography;

        Self::section(ui, col, "Reading aids");
        changed |= Self::toggle_row(
            ui,
            col,
            "ruler",
            "Reading ruler",
            "Highlights the line you're on and dims the rest. Move it with ↑ ↓ or the mouse.",
            &mut t.reading_ruler,
        );
        changed |= Self::toggle_row(ui, col, "bold-starts", "Bold word starts", "Bolds the first letters of each word, to lead the eye along the line.", &mut t.bold_word_starts);
        changed |= Self::toggle_row(
            ui,
            col,
            "focus",
            "Focus mode",
            "Just the text: no toolbar, page numbers or progress bar. Press F in the reader to switch, Esc to leave.",
            &mut t.focus_mode,
        );

        Self::section(ui, col, "Easier text");
        let r = Self::row(
            ui,
            col,
            "Easy-to-read font",
            "Atkinson Hyperlegible keeps similar letters distinct; OpenDyslexic weights the bottom of each letter.",
            316.0,
            32.0,
        );
        for (i, (label, choice)) in [("Atkinson", FontFamilyChoice::Atkinson), ("OpenDyslexic", FontFamilyChoice::OpenDyslexic)].into_iter().enumerate() {
            let active = t.font_family == choice;
            let br = Rect::new(r.x + i as f32 * 162.0, r.y, 154.0, r.h);
            if ui.button_ex(id2(id("easy-font"), i), br, label, active.then_some(icons::CHECK), ButtonKind::Secondary, true) {
                // A second click goes back to the default font.
                t.font_family = if active { FontFamilyChoice::SansSerif } else { choice };
                t.custom_font_name = None;
                t.custom_font_path = None;
                changed = true;
            }
        }
        let pct = |v: f32| if v < 0.005 { "Normal".to_string() } else { format!("+{:.0}%", v * 100.0) };
        changed |= Self::slider_row(ui, col, "letter-spacing", "Space between letters", &mut t.letter_spacing, (0.0, 0.3, 0.01), pct);
        changed |= Self::slider_row(ui, col, "word-spacing", "Space between words", &mut t.word_spacing, (0.0, 1.0, 0.05), pct);
        changed |= Self::slider_row(ui, col, "line-a11y", "Line height", &mut t.line_spacing_multiplier, (1.2, 2.6, 0.05), |v| format!("{v:.2}×"));

        Self::section(ui, col, "Colour");
        let r = Self::row(ui, col, "Page tint", "A soft colour over the page, like a coloured overlay.", 6.0 * 34.0 + 64.0, 30.0);
        let none_r = Rect::new(r.x, r.y, 56.0, r.h);
        if ui.button_ex(id("tint-none"), none_r, "None", None, if t.tint.is_none() { ButtonKind::Secondary } else { ButtonKind::Ghost }, true) {
            t.tint = None;
            changed = true;
        }
        for (i, (rgbv, name)) in TINTS.iter().enumerate() {
            let sr = Rect::new(r.x + 64.0 + i as f32 * 34.0, r.y, 30.0, 30.0);
            let resp = ui.interact_as(id2(id("tint"), i), sr, Role::Choice(t.tint == Some(*rgbv)), &format!("{name} tint"));
            let active = t.tint == Some(*rgbv);
            ui.g.fill_circle(sr.x + 15.0, sr.y + 15.0, 12.0, Color::rgb(rgb(*rgbv)));
            if active {
                ui.g.stroke_round(sr.shrink(0.5), 14.5, 2.0, c.text);
            } else if resp.hovered {
                ui.g.stroke_round(sr.shrink(0.5), 14.5, 1.0, c.muted);
            }
            ui.tip(id2(id("tint"), i), sr, name);
            if resp.clicked {
                t.tint = Some(*rgbv);
                changed = true;
            }
        }
        if t.tint.is_some() {
            changed |= Self::slider_row(ui, col, "tint-strength", "Tint strength", &mut t.tint_strength, (0.05, 0.5, 0.01), |v| format!("{:.0}%", v * 100.0));
        }
        let r = Self::row(ui, col, "Page and text colours", "High Contrast, your own colours, and matching Windows are in Appearance.", 150.0, 32.0);
        if ui.button(id("to-themes"), r, "Open Appearance") {
            self.tab = TAB_THEMES;
        }

        Self::section(ui, col, "Interface");
        let r = Self::row(ui, col, "Interface size", "Menus, buttons and labels. Text in books has its own size.", 330.0, 34.0);
        let labels = ["90%", "100%", "110%", "125%", "150%"];
        let mut pick = UI_SCALES.iter().position(|s| (s - ctx.config.ui_scale).abs() < 0.01).unwrap_or(1);
        if ui.segmented(id("ui-scale"), r, &labels, &mut pick) {
            ctx.config.ui_scale = UI_SCALES[pick];
            changed = true;
        }

        Self::section(ui, col, "Read aloud");
        changed |= Self::slider_row(ui, col, "speech-rate", "Reading voice speed", &mut ctx.config.speech_rate, (0.5, 2.0, 0.05), |v| format!("{v:.2}×"));
        ui.text_opts(
            "Start reading aloud from the reader's toolbar, or press R. Voices come from Windows (Settings → Time & language → Speech).",
            &TextStyle::ui(SMALL),
            TextOpts::wrap(col.w - 8.0),
            col.x + 4.0,
            col.y + 2.0,
            c.muted,
        );
        col.gap(40.0);

        if changed {
            out.push(SettingsAction::ConfigChanged);
        }
    }

    fn library_tab(&mut self, ui: &mut Ui, col: &mut Col, ctx: &mut SettingsCtx, out: &mut Vec<SettingsAction>) {
        self.import_tab(ui, col, out);
        self.storage_section(ui, col, ctx, out);
    }

    fn import_tab(&mut self, ui: &mut Ui, col: &mut Col, out: &mut Vec<SettingsAction>) {
        let c = ui.c;
        Self::section(ui, col, "Import a file");
        let intro = "Add a work you already have as a file. It goes into your library and reads like any downloaded work, offline.";
        let it = ui.measure(intro, &TextStyle::ui(13.5), TextOpts::wrap(col.w - 36.0).line_height(1.45));
        let formats: [(&str, &str); 4] = [
            ("EPUB books", "Including the EPUB files from AO3's Download button."),
            ("PDFs", "Shown page by page, as they were made."),
            ("AO3 web pages", "AO3's HTML downloads, or a work's page saved from a browser."),
            ("Flax Reader works", "Works saved by this app (.json), including from AO3 Reader."),
        ];
        let card = col.take(it.h + 18.0 + formats.len() as f32 * 44.0 + 82.0);
        col.gap(4.0);
        ui.card(card);
        ui.g.draw_text(it, card.x + 18.0, card.y + 16.0, c.muted);
        let mut y = card.y + 16.0 + it.h + 14.0;
        for (name, what) in formats {
            let ck = ui.measure(icons::CHECK, &TextStyle::icons(12.0), TextOpts::default());
            ui.g.draw_text(ck, card.x + 20.0, y + 3.0, c.success);
            ui.text(name, &TextStyle::ui(14.0).semibold(), card.x + 44.0, y, c.text);
            ui.text(what, &TextStyle::ui(12.5), card.x + 44.0, y + 20.0, c.muted);
            y += 44.0;
        }
        let by = card.bottom() - 54.0;
        let bw = ui.button_width("Choose a file…", Some(icons::OPEN_FILE));
        if ui.button_ex(id("import-pick"), Rect::new(card.x + 18.0, by, bw, 36.0), "Choose a file…", Some(icons::OPEN_FILE), ButtonKind::Primary, !self.import.busy) {
            out.push(SettingsAction::Import);
        }
        if self.import.busy {
            ui.spinner(card.x + 34.0 + bw, by + 18.0, 7.0);
            ui.text("Importing…", &TextStyle::ui(13.5), card.x + 50.0 + bw, by + 9.0, c.muted);
        }

        if let Some(err) = self.import.error.clone() {
            let t = ui.measure(&err, &TextStyle::ui(13.5), TextOpts::wrap(col.w - 58.0).line_height(1.35));
            col.gap(8.0);
            let r = col.take(t.h + 52.0);
            col.gap(4.0);
            ui.notice(r, "Couldn't import that file");
            ui.g.draw_text(t, r.x + 40.0, r.y + 36.0, c.text.alpha(0.85));
        } else if let Some((wid, title)) = self.import.done.clone() {
            col.gap(8.0);
            let r = col.take(64.0);
            col.gap(4.0);
            ui.card(r);
            let bw = ui.button_width("Open", Some(icons::READING)).max(96.0);
            ui.text("Imported", &TextStyle::ui(12.5).semibold(), r.x + 18.0, r.y + 12.0, c.success);
            ui.text_opts(&title, &TextStyle::ui(14.5).semibold(), TextOpts::ellipsize(r.w - bw - 52.0), r.x + 18.0, r.y + 32.0, c.text);
            if ui.button_ex(id("import-open"), Rect::new(r.right() - 16.0 - bw, r.y + 14.0, bw, 36.0), "Open", Some(icons::READING), ButtonKind::Tonal, true) {
                out.push(SettingsAction::OpenWork(wid));
            }
        }

        Self::section(ui, col, "Export");
        Self::row(ui, col, "Save a work as a file", "Use ⋯ on a work in your Library to export it as a web page, Markdown or plain text.", 0.0, 0.0);
    }

    fn shortcuts_tab(&mut self, ui: &mut Ui, col: &mut Col) {
        let c = ui.c;
        let groups: [(&str, &[(&str, &str)]); 3] = [
            (
                "Reader",
                &[
                    ("→  PgDn  Space", "Next page"),
                    ("←  PgUp  Shift+Space", "Previous page"),
                    ("Home  End", "First / last page of the chapter"),
                    ("Ctrl+→  Ctrl+←", "Next / previous chapter"),
                    ("↑  ↓", "Move the reading ruler a line (when it's on)"),
                    ("Ctrl+F", "Search in the work"),
                    ("Ctrl+B", "Bookmark this page"),
                    ("Ctrl+=  Ctrl+-", "Larger / smaller text"),
                    ("R", "Read aloud, or stop"),
                    ("F", "Focus mode"),
                    ("F11", "Full screen"),
                    ("Esc", "Stop reading aloud, close a panel, or leave focus mode or full screen"),
                ],
            ),
            (
                "Everywhere",
                &[
                    ("Tab  Shift+Tab", "Move between buttons and fields"),
                    ("Enter  Space", "Press the highlighted button"),
                    ("←  →", "Move a highlighted slider"),
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

    fn updates_tab(&mut self, ui: &mut Ui, col: &mut Col, ctx: &mut SettingsCtx, out: &mut Vec<SettingsAction>) {
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
        let r = Self::row(ui, col, "What's new", "Release notes for every version, on GitHub.", 130.0, 32.0);
        if ui.button_ex(id("releases"), r, "Releases", Some(icons::HISTORY), ButtonKind::Secondary, true) {
            out.push(SettingsAction::Open(format!("https://github.com/{GITHUB_REPO}/releases")));
        }
    }

    fn storage_section(&mut self, ui: &mut Ui, col: &mut Col, ctx: &mut SettingsCtx, out: &mut Vec<SettingsAction>) {
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

    fn sources_tab(&mut self, ui: &mut Ui, col: &mut Col, ctx: &mut SettingsCtx, out: &mut Vec<SettingsAction>) {
        let s = &ctx.config.session;
        let user = s.username.clone().filter(|_| s.is_logged_in && (s.session_cookie.is_some() || s.remember_token.is_some()));
        Self::section(ui, col, "Archive of Our Own");
        let sub = match &user {
            Some(name) => format!("Signed in as {name}. Works only members can see, and kudos, use your account."),
            None => "Not signed in. Sign in to read works only members can see, and to leave kudos.".to_string(),
        };
        let r = Self::row(ui, col, "AO3 account", &sub, 120.0, 32.0);
        if ui.button_ex(id("src-account"), r, if user.is_some() { "Account" } else { "Sign in" }, Some(icons::CONTACT), ButtonKind::Secondary, true) {
            out.push(SettingsAction::Account);
        }

        Self::section(ui, col, "Catalogues");
        for cat in crate::sources::builtin() {
            Self::row(ui, col, &cat.name, "Built in: tens of thousands of free books whose copyright has run out.", 0.0, 0.0);
        }
        let mut remove = None;
        for (i, cat) in ctx.config.catalogs.iter().enumerate() {
            let sub = if cat.user.is_empty() { cat.url.clone() } else { format!("{}  ·  signs in as {}", cat.url, cat.user) };
            let r = Self::row(ui, col, &cat.name, &sub, 110.0, 32.0);
            if ui.button_ex(id2(id("src-remove"), i), r, "Remove", Some(icons::DELETE), ButtonKind::Ghost, true) {
                remove = Some(i);
            }
        }
        if let Some(i) = remove {
            ctx.config.catalogs.remove(i);
            out.push(SettingsAction::ConfigChanged);
            out.push(SettingsAction::CatalogsChanged);
        }
        let r = Self::row(ui, col, "Add a catalogue", "Calibre's content server, Kavita, Komga, a library's catalogue: any OPDS feed.", 110.0, 32.0);
        if ui.button_ex(id("src-add"), r, "Add…", Some("\u{E710}"), ButtonKind::Secondary, true) {
            out.push(SettingsAction::AddCatalog);
        }

        Self::section(ui, col, "Dictionary");
        Self::row(ui, col, "Looking up words", "Double-click a word in a book (or select up to three), then Define. Definitions come from Wiktionary, so this needs the internet.", 0.0, 0.0);
    }

    fn about_tab(&mut self, ui: &mut Ui, col: &mut Col, out: &mut Vec<SettingsAction>) {
        let c = ui.c;
        let open = |out: &mut Vec<SettingsAction>, url: String| out.push(SettingsAction::Open(url));

        // Who we are, and where to find us.
        col.gap(10.0);
        let blurb = "A small, quick e-reader for your own books and free ones. Read EPUBs and PDFs, browse Project Gutenberg and your own catalogues, and read Archive of Our Own, with themes, fonts and reading aids made for long, comfortable reading.";
        let t = ui.measure(blurb, &TextStyle::ui(13.5), TextOpts::wrap(col.w - 36.0).line_height(1.45));
        let note = "The Discord is aga2522's general server: come and chat about anything, and ask about Flax Reader there too.";
        let nt = ui.measure(note, &TextStyle::ui(SMALL), TextOpts::wrap(col.w - 36.0));
        let card = col.take(t.h + nt.h + 162.0);
        col.gap(4.0);
        ui.card(card);
        ui.g.fill_path(super::mark::PATH, super::mark::SIZE, Rect::new(card.x + 18.0, card.y + 18.0, 48.0, 48.0), c.accent_mark);
        let n = ui.text("Flax Reader", &TextStyle::ui(22.0).semibold(), card.x + 80.0, card.y + 16.0, c.text);
        ui.text(&format!("Version {CURRENT_VERSION}  ·  Made by aga2522  ·  Free and open source (MIT)"), &TextStyle::ui(SMALL), card.x + 80.0, card.y + 18.0 + n.h, c.muted);
        ui.g.draw_text(t, card.x + 18.0, card.y + 84.0, c.text);
        let by = card.y + 84.0 + t.h + 16.0;
        ui.g.draw_text(nt, card.x + 18.0, by + 48.0, c.muted);
        let dw = ui.button_width("Join the Discord", Some(icons::CHAT));
        if ui.button_ex(id("about-discord"), Rect::new(card.x + 18.0, by, dw, 36.0), "Join the Discord", Some(icons::CHAT), ButtonKind::Primary, true) {
            open(out, DISCORD_INVITE.to_string());
        }
        let gw = ui.button_width("GitHub", Some(icons::CODE));
        if ui.button_ex(id("about-github"), Rect::new(card.x + 26.0 + dw, by, gw, 36.0), "GitHub", Some(icons::CODE), ButtonKind::Secondary, true) {
            open(out, format!("https://github.com/{GITHUB_REPO}"));
        }

        Self::section(ui, col, "Help");
        let r = Self::row(ui, col, "Report a problem", "Something broken or confusing? Tell us on GitHub, or ask in the Discord.", 120.0, 32.0);
        if ui.button_ex(id("about-issue"), r, "Report", Some(icons::BUG), ButtonKind::Secondary, true) {
            open(out, format!("https://github.com/{GITHUB_REPO}/issues/new"));
        }
        let r = Self::row(ui, col, "What's new", "Release notes for every version.", 120.0, 32.0);
        if ui.button_ex(id("about-releases"), r, "Releases", Some(icons::HISTORY), ButtonKind::Secondary, true) {
            open(out, format!("https://github.com/{GITHUB_REPO}/releases"));
        }

        Self::section(ui, col, "Privacy");
        Self::row(ui, col, "Your books stay yours", "Your library, highlights and notes stay on this PC. There are no ads, no tracking and no Flax account. The app goes online only for what you ask: AO3, catalogues, word definitions, and (if you allow it) checking GitHub for updates.", 0.0, 0.0);

        Self::section(ui, col, "This app");
        let (private_ws, ws) = memory_mb();
        let sub = format!("{private_ws:.1} MB private (what Task Manager shows)  ·  {ws:.1} MB working set");
        Self::row(ui, col, "Memory in use", &sub, 0.0, 0.0);
        ui.wake_after(1000);

        Self::section(ui, col, "Credits");
        Self::row(ui, col, "Thanks to", "Project Gutenberg for its free books, and Wiktionary for definitions (CC BY-SA). Atkinson Hyperlegible is by the Braille Institute and OpenDyslexic by Abbie Gonzalez, both under the SIL Open Font License.", 0.0, 0.0);
        Self::row(ui, col, "Not affiliated with AO3", "Flax Reader is unofficial: it isn't made by Archive of Our Own or the Organization for Transformative Works.", 0.0, 0.0);
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
