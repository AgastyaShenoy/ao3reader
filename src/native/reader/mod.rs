//! Reader screen: paged (single or two-page spread) and scrolling modes, contents and
//! bookmarks drawer, in-work search with highlights, appearance panel, shortcuts, and
//! position tracking that survives font, window and mode changes.

mod layout;
mod preface;

use crate::ao3::models::{Bookmark, ReadingProgress, Work};
use crate::native::app::{reader_family, reader_font_label};
use crate::native::gfx::*;
use crate::native::icons;
use crate::native::ui::*;
use crate::reader::theme::ReaderThemeType;
use crate::reader::typography::{PageLayoutMode, ReadingMode, TextAlignment, TypographySettings};
use crate::storage::config::AppConfig;
use layout::{is_landmark, paragraph_rich, ChapterLayout, Kind, CARD_PAD, INDENT};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

const TOOLBAR_H: f32 = 48.0;
const BOTTOM_H: f32 = 44.0;
const MAX_MATCHES: usize = 2000;
/// Minimum time between wheel-driven page turns (touchpads send many small deltas).
const FLIP_DEBOUNCE: Duration = Duration::from_millis(220);

const VK_SPACE: u16 = 0x20;
const VK_PRIOR: u16 = 0x21;
const VK_NEXT: u16 = 0x22;
const VK_END: u16 = 0x23;
const VK_HOME: u16 = 0x24;
const VK_LEFT: u16 = 0x25;
const VK_UP: u16 = 0x26;
const VK_RIGHT: u16 = 0x27;
const VK_DOWN: u16 = 0x28;
const VK_F11: u16 = 0x7A;
const VK_ESCAPE: u16 = 0x1B;
const VK_ADD: u16 = 0x6B;
const VK_SUBTRACT: u16 = 0x6D;
const VK_OEM_PLUS: u16 = 0xBB;
const VK_OEM_MINUS: u16 = 0xBD;
const VK_F: u16 = 0x46;
const VK_B: u16 = 0x42;

pub enum ReaderAction {
    Close,
    Kudos,
    AddBookmark(Bookmark),
    RemoveBookmark(String),
    ConfigChanged,
    ToggleFullscreen,
    OpenTypographySettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    None,
    Contents,
    Appearance,
    Search,
    Bookmark,
}

#[derive(Debug, Clone, Copy)]
struct Match {
    chapter: usize,
    para: usize,
    start: usize,
    end: usize,
}

enum EndAction {
    NextChapter,
    Kudos,
}

pub struct ReaderScreen {
    pub work: Work,
    pub chapter: usize,
    page: usize,
    anchor: f32,
    /// Paragraph anchor to apply once the next layout exists.
    restore: Option<f32>,
    layout: Option<ChapterLayout>,
    chapter_words: Vec<usize>,
    para_words: Vec<usize>,
    panel: Panel,
    contents_tab: usize,
    focus_pending: bool,
    query: String,
    search_all: bool,
    searched: (String, bool),
    matches: Vec<Match>,
    current_match: Option<usize>,
    bookmark_note: String,
    pub fullscreen: bool,
    /// Two pages were on screen in the last paged frame.
    spread_shown: bool,
    wheel_acc: f32,
    last_flip: Option<Instant>,
    pub progress_dirty: bool,
}

fn hash_of<T: Hash>(v: &T) -> u64 {
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

fn layout_key(chapter: usize, t: &TypographySettings, family: &Family, width: f32, page_h: Option<f32>) -> u64 {
    hash_of(&(
        chapter,
        family,
        width.to_bits(),
        page_h.map(f32::to_bits),
        t.font_size.to_bits(),
        t.line_spacing_multiplier.to_bits(),
        t.paragraph_spacing.to_bits(),
        t.indent_paragraphs,
        t.text_alignment == TextAlignment::Justified,
        (t.show_chapter_summary, t.show_chapter_pre_notes, t.show_chapter_post_notes),
    ))
}

fn fmt_minutes(mins: f32) -> String {
    let m = mins.round().max(1.0) as u32;
    if m >= 60 { format!("{} h {} min", m / 60, m % 60) } else { format!("{m} min") }
}

impl ReaderScreen {
    pub fn new(mut work: Work, progress: Option<&ReadingProgress>) -> Self {
        // Only the parsed paragraphs are displayed; don't keep the source HTML in memory.
        for c in &mut work.chapters {
            c.raw_html = String::new();
        }
        let chapter_words: Vec<usize> = (0..work.chapters.len()).map(|i| layout::paragraph_words(&work, i).iter().sum()).collect();
        let n = work.chapters.len().max(1);
        let chapter = progress.map_or(0, |p| p.current_chapter.saturating_sub(1)).min(n - 1);
        let anchor = progress.map_or(0.0, |p| p.position);
        let para_words = layout::paragraph_words(&work, chapter);
        Self {
            work,
            chapter,
            page: 0,
            anchor,
            restore: Some(anchor),
            layout: None,
            chapter_words,
            para_words,
            panel: Panel::None,
            contents_tab: 0,
            focus_pending: false,
            query: String::new(),
            search_all: true,
            searched: (String::new(), true),
            matches: Vec::new(),
            current_match: None,
            bookmark_note: String::new(),
            fullscreen: false,
            spread_shown: false,
            wheel_acc: 0.0,
            last_flip: None,
            progress_dirty: false,
        }
    }

    pub fn work_id(&self) -> &str {
        &self.work.meta.id
    }

    fn chapter_count(&self) -> usize {
        self.work.chapters.len()
    }

    fn go_chapter(&mut self, idx: usize, anchor: f32) {
        if idx >= self.chapter_count() {
            return;
        }
        if idx != self.chapter {
            self.chapter = idx;
            self.layout = None;
            self.para_words = layout::paragraph_words(&self.work, idx);
        }
        self.page = 0;
        self.restore = Some(anchor);
        self.anchor = anchor.min(self.para_words.len() as f32);
        self.progress_dirty = true;
    }

    fn at_end(&self) -> bool {
        self.chapter + 1 >= self.chapter_count() && self.anchor >= self.para_words.len() as f32 - 0.05
    }

    /// Words read so far in (the work, the current chapter).
    fn words_read(&self) -> (f32, f32) {
        let before: usize = self.chapter_words[..self.chapter.min(self.chapter_words.len())].iter().sum();
        let idx = self.anchor.floor().max(0.0) as usize;
        let mut in_ch: f32 = self.para_words.iter().take(idx).sum::<usize>() as f32;
        if let Some(w) = self.para_words.get(idx) {
            in_ch += *w as f32 * (self.anchor - self.anchor.floor());
        }
        (before as f32 + in_ch, in_ch)
    }

    pub fn progress(&self) -> ReadingProgress {
        let total: usize = self.chapter_words.iter().sum();
        let (read, _) = self.words_read();
        let completed = self.at_end();
        let pct = if completed { 100.0 } else { (read / total.max(1) as f32 * 100.0).clamp(0.0, 100.0) };
        ReadingProgress {
            current_chapter: self.chapter + 1,
            scroll_offset: self.page as f32,
            position: self.anchor,
            percentage: pct,
            last_read: chrono::Utc::now(),
            completed,
        }
    }

    fn set_anchor(&mut self, a: f32) {
        if (a - self.anchor).abs() > 0.001 {
            self.anchor = a;
            self.progress_dirty = true;
        }
    }

    fn ensure_layout(&mut self, ui: &mut Ui, t: &TypographySettings, width: f32, page_h: Option<f32>) {
        let family = reader_family(t);
        let key = layout_key(self.chapter, t, &family, width, page_h);
        if self.layout.as_ref().is_some_and(|l| l.key == key) {
            return;
        }
        if self.restore.is_none() {
            self.restore = Some(self.anchor);
        }
        self.layout = Some(ChapterLayout::build(ui, key, &self.work, self.chapter, t, family, width, page_h));
    }

    // ---- frame ---------------------------------------------------------------------

    pub fn frame(&mut self, ui: &mut Ui, r: Rect, config: &mut AppConfig, bookmarks: &[Bookmark]) -> Vec<ReaderAction> {
        let mut out = Vec::new();
        self.global_keys(ui, config, &mut out);
        let t = config.typography.clone();
        let c = ui.c;
        let chrome = !self.fullscreen;
        let top = if chrome { TOOLBAR_H } else { 0.0 };
        let bottom = if chrome && t.show_reading_progress_bar { BOTTOM_H } else { 0.0 };
        let area = Rect::new(r.x, r.y + top, r.w, (r.h - top - bottom).max(100.0));

        let panel_rect = self.panel_rect(r, area);
        if self.panel != Panel::None {
            ui.block(area);
            if ui.input.pressed && area.contains(ui.input.mouse) && !panel_rect.is_some_and(|p| p.contains(ui.input.mouse)) {
                self.close_panel();
                ui.request_repaint();
            }
        }

        ui.g.fill_rect(area, c.bg);
        let end = match t.reading_mode {
            ReadingMode::Paged => self.paged(ui, area, &t),
            ReadingMode::Scroll => self.scrolled(ui, area, &t, config.auto_next_chapter),
        };
        match end {
            Some(EndAction::NextChapter) => self.go_chapter(self.chapter + 1, 0.0),
            Some(EndAction::Kudos) => out.push(ReaderAction::Kudos),
            None => {}
        }

        if chrome {
            self.toolbar(ui, Rect::new(r.x, r.y, r.w, TOOLBAR_H), config, &mut out);
            if bottom > 0.0 {
                self.bottom_bar(ui, Rect::new(r.x, area.bottom(), r.w, bottom), &t, config.words_per_minute);
            }
        }

        if let Some(pr) = panel_rect {
            ui.g.fill_rect(area, Color { a: 0.25, ..Color::from_u8(0, 0, 0) });
            ui.overlay(|ui| match self.panel {
                Panel::Contents => self.contents_panel(ui, pr, bookmarks, &mut out),
                Panel::Appearance => self.appearance_panel(ui, pr, config, &mut out),
                Panel::Search => self.search_panel(ui, pr),
                Panel::Bookmark => self.bookmark_panel(ui, pr, &mut out),
                Panel::None => {}
            });
        }
        out
    }

    fn panel_rect(&self, r: Rect, area: Rect) -> Option<Rect> {
        let w = |want: f32| want.min(r.w - 24.0);
        match self.panel {
            Panel::None => None,
            Panel::Contents => Some(Rect::new(r.x, area.y, w(380.0), area.h)),
            Panel::Appearance => Some(Rect::new(r.right() - w(360.0), area.y, w(360.0), area.h)),
            Panel::Search => Some(Rect::new(r.right() - w(440.0) - 12.0, area.y + 10.0, w(440.0), 108.0)),
            Panel::Bookmark => Some(Rect::new(r.right() - w(420.0) - 12.0, area.y + 10.0, w(420.0), 210.0)),
        }
    }

    /// Closing search also clears its highlights.
    fn close_panel(&mut self) {
        if self.panel == Panel::Search {
            self.matches.clear();
            self.current_match = None;
            self.query.clear();
            self.searched = (String::new(), self.search_all);
        }
        self.panel = Panel::None;
    }

    fn open_panel(&mut self, p: Panel) {
        if self.panel == p {
            self.close_panel();
            return;
        }
        if self.panel == Panel::Search {
            self.close_panel();
        }
        self.panel = p;
        self.focus_pending = matches!(self.panel, Panel::Search | Panel::Bookmark);
        if self.panel == Panel::Bookmark {
            self.bookmark_note.clear();
        }
    }

    fn global_keys(&mut self, ui: &mut Ui, config: &mut AppConfig, out: &mut Vec<ReaderAction>) {
        if ui.key_combo(VK_ESCAPE, false, false) {
            if self.panel != Panel::None {
                self.close_panel();
            } else if self.fullscreen {
                out.push(ReaderAction::ToggleFullscreen);
            }
        }
        if ui.key_combo(VK_F11, false, false) {
            out.push(ReaderAction::ToggleFullscreen);
        }
        if ui.key_combo(VK_F, true, false) {
            self.open_panel(Panel::Search);
        }
        if ui.key_combo(VK_B, true, false) {
            self.open_panel(Panel::Bookmark);
        }
        let t = &mut config.typography;
        if ui.key_combo(VK_OEM_PLUS, true, false) || ui.key_combo(VK_ADD, true, false) {
            t.font_size = (t.font_size + 1.0).min(36.0);
            out.push(ReaderAction::ConfigChanged);
        }
        if ui.key_combo(VK_OEM_MINUS, true, false) || ui.key_combo(VK_SUBTRACT, true, false) {
            t.font_size = (t.font_size - 1.0).max(12.0);
            out.push(ReaderAction::ConfigChanged);
        }
        if ui.key_combo(VK_RIGHT, true, false) && self.chapter + 1 < self.chapter_count() {
            self.go_chapter(self.chapter + 1, 0.0);
        }
        if ui.key_combo(VK_LEFT, true, false) && self.chapter > 0 {
            self.go_chapter(self.chapter - 1, 0.0);
        }
    }

    // ---- paged mode --------------------------------------------------------------------

    fn turn(&mut self, forward: bool, dual: bool) {
        let step = if dual { 2 } else { 1 };
        let n = self.layout.as_ref().map_or(1, |l| l.pages.len());
        if forward {
            if self.page + step < n {
                self.page += step;
            } else if self.chapter + 1 < self.chapter_count() {
                self.go_chapter(self.chapter + 1, 0.0);
            }
        } else if self.page > 0 {
            self.page = self.page.saturating_sub(step);
        } else if self.chapter > 0 {
            self.go_chapter(self.chapter - 1, f32::MAX);
        }
        self.progress_dirty = true;
    }

    fn paged(&mut self, ui: &mut Ui, area: Rect, t: &TypographySettings) -> Option<EndAction> {
        let c = ui.c;
        let dual = t.page_layout == PageLayoutMode::Dual || (t.page_layout == PageLayoutMode::Auto && area.w >= 1100.0);
        let (margin, gutter) = (56.0, 80.0);
        let col_w = if dual { t.max_column_width.min((area.w - 2.0 * margin - gutter) / 2.0) } else { t.max_column_width.min(area.w - 2.0 * margin) }.max(240.0);
        let (head, foot) = (40.0, 44.0);
        let page_top = area.y + head;
        let page_h = (area.h - head - foot).max(120.0);

        // Navigation keys and the wheel act on the layout from the previous frame.
        if self.panel == Panel::None && self.layout.is_some() {
            let fwd = ui.key_combo(VK_RIGHT, false, false) || ui.key_combo(VK_NEXT, false, false) || ui.key_combo(VK_SPACE, false, false) || ui.key_combo(VK_DOWN, false, false);
            let back = ui.key_combo(VK_LEFT, false, false) || ui.key_combo(VK_PRIOR, false, false) || ui.key_combo(VK_SPACE, false, true) || ui.key_combo(VK_UP, false, false);
            if fwd {
                self.turn(true, dual);
            } else if back {
                self.turn(false, dual);
            }
            if ui.key_combo(VK_HOME, false, false) {
                self.page = 0;
                self.progress_dirty = true;
            }
            if ui.key_combo(VK_END, false, false) {
                self.page = usize::MAX;
                self.progress_dirty = true;
            }
            if t.page_flip_mouse_wheel && ui.input.wheel != 0.0 && ui.hit(area) {
                let ready = self.last_flip.is_none_or(|at| at.elapsed() >= FLIP_DEBOUNCE);
                self.wheel_acc = if ready { self.wheel_acc + ui.input.wheel } else { 0.0 };
                if self.wheel_acc.abs() >= 0.5 {
                    self.turn(self.wheel_acc < 0.0, dual);
                    self.wheel_acc = 0.0;
                    self.last_flip = Some(Instant::now());
                }
                ui.request_repaint();
            }
        }

        self.ensure_layout(ui, t, col_w, Some(page_h));
        let layout = self.layout.as_ref().unwrap();
        let n = layout.pages.len();
        if let Some(a) = self.restore.take() {
            self.page = layout.page_for_y(layout.y_for_anchor(a));
        }
        self.page = self.page.min(n - 1);
        if dual {
            self.page -= self.page % 2;
        }

        let total_w = if dual { col_w * 2.0 + gutter } else { col_w };
        let x0 = area.x + (area.w - total_w) / 2.0;
        let small = TextStyle::ui(12.0);
        let mut end = None;
        for k in 0..if dual { 2 } else { 1 } {
            let p = self.page + k;
            if p >= n {
                break;
            }
            let px = x0 + k as f32 * (col_w + gutter);
            let head_text = if dual && k == 0 { self.work.meta.title.clone() } else { self.work.chapters[self.chapter].title.clone() };
            ui.text_in(&head_text, &small, Rect::new(px, area.y + 12.0, col_w, 20.0), Align::Center, c.muted.alpha(0.8));
            let mut y = page_top;
            for s in &layout.pages[p].slices {
                let h = s.to - s.from;
                ui.g.push_clip(Rect::new(px - 30.0, y, col_w + 60.0, h));
                if let Some(e) = self.draw_block(ui, layout, s.block, px, y - s.from) {
                    end = Some(e);
                }
                ui.g.pop_clip();
                y += h;
            }
            ui.text_in(&format!("{}", p + 1), &small, Rect::new(px, area.bottom() - foot + 12.0, col_w, 20.0), Align::Center, c.muted);
        }
        if dual {
            let mx = x0 + col_w + gutter / 2.0;
            ui.g.line(mx, page_top, mx, page_top + page_h, 1.0, c.border);
        }
        let pages_y = layout.pages[self.page].y;
        let last_visible = (self.page + if dual { 1 } else { 0 }).min(n - 1);
        self.spread_shown = last_visible > self.page;

        if t.show_page_turn_indicators {
            let size = 40.0;
            let cy = area.y + area.h / 2.0 - size / 2.0;
            let can_back = self.page > 0 || self.chapter > 0;
            let can_fwd = last_visible + 1 < n || self.chapter + 1 < self.chapter_count();
            let left = Rect::new(area.x + 12.0, cy, size, size);
            let right = Rect::new(area.right() - 12.0 - size, cy, size, size);
            if can_back && ui.button_ex(id("page-back"), left, "", Some("\u{E76B}"), ButtonKind::Ghost, true) {
                self.turn(false, dual);
            }
            if can_fwd && ui.button_ex(id("page-fwd"), right, "", Some("\u{E76C}"), ButtonKind::Ghost, true) {
                self.turn(true, dual);
            }
        }

        let anchor = if last_visible + 1 >= n {
            // Showing the chapter's last page counts as having read it.
            self.layout.as_ref().map_or(0.0, |l| l.para_count as f32)
        } else {
            self.layout.as_ref().map_or(0.0, |l| l.anchor_at(pages_y))
        };
        self.set_anchor(anchor);
        end
    }

    // ---- scroll mode -------------------------------------------------------------------

    fn scrolled(&mut self, ui: &mut Ui, area: Rect, t: &TypographySettings, auto_next: bool) -> Option<EndAction> {
        let col_w = t.max_column_width.min(area.w - 72.0).max(240.0);
        self.ensure_layout(ui, t, col_w, None);
        let sid = id2(id("reader-scroll"), self.chapter);
        let pad_top = 36.0;
        let total = self.layout.as_ref().map_or(0.0, |l| l.total_h) + pad_top + 24.0;

        if let Some(a) = self.restore.take() {
            let y = self.layout.as_ref().map_or(0.0, |l| l.y_for_anchor(a));
            let s = ui.mem.scroll.entry(sid).or_default();
            s.content_h = total;
            s.offset = if a >= f32::MAX / 2.0 { total } else { y };
        }
        let max = (total - area.h).max(0.0);
        if self.panel == Panel::None {
            let page = (area.h - 64.0).max(40.0);
            let delta = if ui.key_combo(VK_DOWN, false, false) {
                Some(56.0)
            } else if ui.key_combo(VK_UP, false, false) {
                Some(-56.0)
            } else if ui.key_combo(VK_NEXT, false, false) || ui.key_combo(VK_SPACE, false, false) {
                Some(page)
            } else if ui.key_combo(VK_PRIOR, false, false) || ui.key_combo(VK_SPACE, false, true) {
                Some(-page)
            } else if ui.key_combo(VK_HOME, false, false) {
                Some(-f32::MAX / 4.0)
            } else if ui.key_combo(VK_END, false, false) {
                Some(f32::MAX / 4.0)
            } else {
                None
            };
            if let Some(d) = delta {
                let s = ui.mem.scroll.entry(sid).or_default();
                s.offset = (s.offset + d).clamp(0.0, max);
            }
            // Wheeling down past the end continues into the next chapter.
            let at_end = ui.mem.scroll.get(&sid).is_some_and(|s| s.offset >= max - 1.0);
            if auto_next && at_end && ui.input.wheel < 0.0 && ui.hit(area) && self.chapter + 1 < self.chapter_count() {
                self.wheel_acc += ui.input.wheel;
                if self.wheel_acc <= -4.0 {
                    self.wheel_acc = 0.0;
                    self.go_chapter(self.chapter + 1, 0.0);
                    return None;
                }
            } else if ui.input.wheel > 0.0 {
                self.wheel_acc = 0.0;
            }
        }

        let layout = self.layout.as_ref().unwrap();
        let x = area.x + (area.w - col_w) / 2.0;
        let mut end = None;
        ui.scroll_area(sid, area, |ui, view, y0| {
            let top = y0 + pad_top;
            for (i, b) in layout.blocks.iter().enumerate() {
                if top + b.y + b.h < view.y || top + b.y > view.bottom() {
                    continue;
                }
                if let Some(e) = self.draw_block(ui, layout, i, x, top + b.y) {
                    end = Some(e);
                }
            }
            total
        });
        let offset = ui.mem.scroll.get(&sid).map_or(0.0, |s| s.offset);
        let anchor = if offset >= max - 1.0 && max > 0.0 { layout.para_count as f32 } else { layout.anchor_at(offset) };
        self.set_anchor(anchor);
        end
    }

    // ---- blocks ------------------------------------------------------------------------

    fn draw_block(&self, ui: &mut Ui, layout: &ChapterLayout, bi: usize, x: f32, y: f32) -> Option<EndAction> {
        let c = ui.c;
        let b = &layout.blocks[bi];
        let w = layout.width;
        let card_label = |kind: Kind| match kind {
            Kind::Summary => "Chapter summary",
            Kind::PreNotes => "Notes",
            _ => "End notes",
        };
        match b.kind {
            Kind::Preface => {
                preface::draw(ui, &self.work.meta, x, y, w, layout.page_h);
                None
            }
            Kind::Divider => {
                let t = ui.measure("\u{2042}", &TextStyle::ui(18.0), TextOpts::default());
                ui.g.draw_text(t, x + (w - t.w) / 2.0, y + (b.text_h - t.h) / 2.0, c.muted);
                None
            }
            Kind::End => {
                let last = self.chapter + 1 >= self.chapter_count();
                let label = if last { "The end" } else { "End of chapter" };
                ui.text_in(label, &TextStyle::ui(13.0), Rect::new(x, y + 24.0, w, 20.0), Align::Center, c.muted);
                let (text, icon) = if last { ("Leave kudos", "\u{EB51}") } else { ("Next chapter", "\u{E76C}") };
                let bw = ui.button_width(text, Some(icon));
                let r = Rect::new(x + (w - bw) / 2.0, y + 56.0, bw, 36.0);
                if ui.button_ex(id2(id("end-action"), self.chapter), r, text, Some(icon), ButtonKind::Primary, true) {
                    return Some(if last { EndAction::Kudos } else { EndAction::NextChapter });
                }
                None
            }
            kind => {
                if matches!(kind, Kind::Summary | Kind::PreNotes | Kind::EndNotes) {
                    let top = if kind == Kind::EndNotes { 16.0 } else { 0.0 };
                    let card = Rect::new(x, y + top, w, b.text_y + b.text_h + CARD_PAD - top);
                    ui.g.fill_round(card, 8.0, c.notes_bg);
                    ui.g.stroke_round(card, 8.0, 1.0, c.border);
                    ui.text(card_label(kind), &TextStyle::ui(12.5).semibold(), x + CARD_PAD, y + top + CARD_PAD - 2.0, c.accent);
                }
                if kind == Kind::Quote {
                    let r = Rect::new(x + 6.0, y, w - 6.0, b.text_y + b.text_h + 12.0);
                    ui.g.fill_round(r, 6.0, c.quote_bg);
                    ui.g.fill_round(Rect::new(r.x, r.y, 3.0, r.h), 1.5, c.accent.alpha(0.7));
                }
                let (style, opts) = layout.styles.get(b.style);
                let opts = TextOpts { wrap: Some(b.text_w), ..opts };
                let th = ui.g.layout_rich(&b.rich.text, &b.rich.spans, style, opts);
                let (tx, ty) = (x + b.text_x, y + b.text_y);
                if matches!(kind, Kind::Para | Kind::Quote) && !self.matches.is_empty() {
                    let shift = if kind == Kind::Para && b.rich.text.starts_with(INDENT) { INDENT.len() } else { 0 };
                    for (mi, m) in self.matches.iter().enumerate() {
                        if m.chapter != layout.chapter || m.para != b.para {
                            continue;
                        }
                        let current = self.current_match == Some(mi);
                        let fill = c.accent.alpha(if current { 0.55 } else { 0.22 });
                        for rr in ui.g.range_rects(th, &b.rich.text, m.start + shift, m.end + shift) {
                            ui.g.fill_round(Rect::new(tx + rr.x - 1.0, ty + rr.y, rr.w + 2.0, rr.h), 3.0, fill);
                        }
                    }
                }
                let color = if matches!(kind, Kind::Summary | Kind::PreNotes | Kind::EndNotes) { c.text.alpha(0.9) } else { c.text };
                ui.g.draw_text(th, tx, ty, color);
                None
            }
        }
    }

    // ---- chrome ------------------------------------------------------------------------

    fn toolbar(&mut self, ui: &mut Ui, r: Rect, config: &mut AppConfig, out: &mut Vec<ReaderAction>) {
        let c = ui.c;
        ui.g.fill_rect(r, c.surface);
        ui.separator(r.x, r.bottom() - 1.0, r.w);
        let b = |x: f32| Rect::new(x, r.y + 8.0, 34.0, 32.0);
        if ui.button_ex(id("rd-back"), Rect::new(r.x + 8.0, r.y + 8.0, 96.0, 32.0), "Library", Some("\u{E72B}"), ButtonKind::Ghost, true) {
            out.push(ReaderAction::Close);
        }
        if ui.icon_button(id("rd-contents"), b(r.x + 108.0), "\u{E8FD}", self.panel == Panel::Contents) {
            self.open_panel(Panel::Contents);
        }

        let mut x = r.right() - 8.0 - 34.0;
        let paged = config.typography.reading_mode == ReadingMode::Paged;
        let fs_icon = if self.fullscreen { "\u{E73F}" } else { "\u{E740}" };
        if ui.icon_button(id("rd-full"), b(x), fs_icon, false) {
            out.push(ReaderAction::ToggleFullscreen);
        }
        x -= 38.0;
        if ui.icon_button(id("rd-mode"), b(x), if paged { "\u{E8A1}" } else { "\u{E8A9}" }, false) {
            config.typography.reading_mode = if paged { ReadingMode::Scroll } else { ReadingMode::Paged };
            out.push(ReaderAction::ConfigChanged);
        }
        x -= 38.0;
        if ui.icon_button(id("rd-appearance"), b(x), icons::FONT, self.panel == Panel::Appearance) {
            self.open_panel(Panel::Appearance);
        }
        x -= 38.0;
        if ui.icon_button(id("rd-kudos"), b(x), "\u{EB51}", false) {
            out.push(ReaderAction::Kudos);
        }
        x -= 38.0;
        if ui.icon_button(id("rd-bookmark"), b(x), "\u{E734}", self.panel == Panel::Bookmark) {
            self.open_panel(Panel::Bookmark);
        }
        x -= 38.0;
        if ui.icon_button(id("rd-search"), b(x), icons::SEARCH, self.panel == Panel::Search) {
            self.open_panel(Panel::Search);
        }

        let title_x = r.x + 156.0;
        let title_w = (x - 12.0 - title_x).max(0.0);
        let ch = &self.work.chapters[self.chapter];
        let title = if self.chapter_count() > 1 { format!("{}  ·  {}", self.work.meta.title, ch.title) } else { self.work.meta.title.clone() };
        ui.text_in(&title, &TextStyle::ui(14.0).semibold(), Rect::new(title_x, r.y, title_w, r.h), Align::Center, c.text);
    }

    fn bottom_bar(&mut self, ui: &mut Ui, r: Rect, t: &TypographySettings, wpm: u32) {
        let c = ui.c;
        ui.g.fill_rect(r, c.surface);
        let p = self.progress();
        ui.g.fill_rect(Rect::new(r.x, r.y, r.w, 3.0), c.border);
        ui.g.fill_rect(Rect::new(r.x, r.y, r.w * p.percentage / 100.0, 3.0), c.accent);

        let n = self.chapter_count();
        let has_prev = self.chapter > 0;
        let has_next = self.chapter + 1 < n;
        if ui.button_ex(id("ch-prev"), Rect::new(r.x + 8.0, r.y + 7.0, 34.0, 32.0), "", Some("\u{E76B}"), ButtonKind::Ghost, has_prev) {
            self.go_chapter(self.chapter - 1, 0.0);
        }
        if ui.button_ex(id("ch-next"), Rect::new(r.right() - 42.0, r.y + 7.0, 34.0, 32.0), "", Some("\u{E76C}"), ButtonKind::Ghost, has_next) {
            self.go_chapter(self.chapter + 1, 0.0);
        }

        let mut left = format!("Chapter {} of {}", self.chapter + 1, n);
        if t.reading_mode == ReadingMode::Paged {
            if let Some(l) = &self.layout {
                let n = l.pages.len();
                if self.spread_shown {
                    left.push_str(&format!("  ·  Pages {}–{} of {}", self.page + 1, self.page + 2, n));
                } else {
                    left.push_str(&format!("  ·  Page {} of {}", self.page + 1, n));
                }
            }
        }
        let style = TextStyle::ui(12.5);
        ui.text_in(&left, &style, Rect::new(r.x + 50.0, r.y + 3.0, r.w / 2.0 - 50.0, r.h - 3.0), Align::Left, c.muted);

        let total: usize = self.chapter_words.iter().sum();
        let (read, in_ch) = self.words_read();
        let wpm = wpm.max(60) as f32;
        let left_work = (total as f32 - read).max(0.0) / wpm;
        let left_ch = (self.chapter_words.get(self.chapter).copied().unwrap_or(0) as f32 - in_ch).max(0.0) / wpm;
        let right = if p.completed {
            "Finished".to_string()
        } else if n > 1 {
            format!("{:.0}%  ·  {} left  ·  {} in chapter", p.percentage, fmt_minutes(left_work), fmt_minutes(left_ch))
        } else {
            format!("{:.0}%  ·  {} left", p.percentage, fmt_minutes(left_work))
        };
        ui.text_in(&right, &style, Rect::new(r.x + r.w / 2.0, r.y + 3.0, r.w / 2.0 - 50.0, r.h - 3.0), Align::Right, c.muted);
    }

    // ---- panels ------------------------------------------------------------------------

    fn panel_frame(ui: &mut Ui, r: Rect) {
        let c = ui.c;
        ui.g.fill_round(r, 8.0, c.surface);
        ui.g.stroke_round(r, 8.0, 1.0, c.border);
    }

    fn contents_panel(&mut self, ui: &mut Ui, r: Rect, bookmarks: &[Bookmark], out: &mut Vec<ReaderAction>) {
        let c = ui.c;
        Self::panel_frame(ui, r);
        let bm_label = format!("Bookmarks ({})", bookmarks.len());
        ui.segmented(id("contents-tab"), Rect::new(r.x + 14.0, r.y + 14.0, r.w - 28.0, 34.0), &["Chapters", bm_label.as_str()], &mut self.contents_tab);
        let list = Rect::new(r.x + 4.0, r.y + 60.0, r.w - 8.0, r.h - 64.0);
        let mut jump: Option<(usize, f32)> = None;

        if self.contents_tab == 0 {
            let row_h = 44.0;
            let chapters = &self.work.chapters;
            let current = self.chapter;
            let words = &self.chapter_words;
            ui.scroll_area(id("contents-list"), list, |ui, view, y0| {
                for (i, ch) in chapters.iter().enumerate() {
                    let rr = Rect::new(view.x + 6.0, y0 + i as f32 * row_h, view.w - 22.0, row_h - 2.0);
                    if rr.bottom() < view.y || rr.y > view.bottom() {
                        continue;
                    }
                    let resp = ui.interact(id2(id("toc-row"), i), rr);
                    if i == current {
                        ui.g.fill_round(rr, 6.0, c.accent.alpha(0.16));
                    } else if resp.hovered {
                        ui.g.fill_round(rr, 6.0, c.hover());
                    }
                    let wtxt = format!("{} words", preface::thousands(words.get(i).copied().unwrap_or(0) as u64));
                    let wt = ui.text_in(&wtxt, &TextStyle::ui(11.5), Rect::new(rr.right() - 100.0, rr.y, 92.0, rr.h), Align::Right, c.muted);
                    let style = if i == current { TextStyle::ui(13.5).semibold() } else { TextStyle::ui(13.5) };
                    let label = if ch.title.trim().is_empty() { format!("Chapter {}", i + 1) } else { ch.title.trim().to_string() };
                    ui.text_in(&label, &style, Rect::new(rr.x + 12.0, rr.y, rr.w - wt.w - 32.0, rr.h), Align::Left, c.text);
                    if resp.clicked {
                        jump = Some((i, 0.0));
                    }
                }
                chapters.len() as f32 * row_h + 8.0
            });
        } else {
            let mut sorted: Vec<&Bookmark> = bookmarks.iter().collect();
            sorted.sort_by(|a, b| (a.chapter_index, a.paragraph_index).cmp(&(b.chapter_index, b.paragraph_index)));
            if sorted.is_empty() {
                let msg = "No bookmarks yet. Press Ctrl+B or the bookmark button to add one.";
                let t = ui.measure(msg, &TextStyle::ui(13.0), TextOpts::wrap(list.w - 48.0).align(Align::Center));
                ui.g.draw_text(t, list.x + 24.0, list.y + 32.0, c.muted);
            }
            let row_h = 78.0;
            let mut remove = None;
            ui.scroll_area(id("bookmark-list"), list, |ui, view, y0| {
                for (i, bm) in sorted.iter().enumerate() {
                    let rr = Rect::new(view.x + 6.0, y0 + i as f32 * row_h, view.w - 22.0, row_h - 4.0);
                    if rr.bottom() < view.y || rr.y > view.bottom() {
                        continue;
                    }
                    let del = Rect::new(rr.right() - 38.0, rr.y + 8.0, 32.0, 30.0);
                    if ui.button_ex(id2(id("bm-del"), &bm.id), del, "", Some(icons::DELETE), ButtonKind::Ghost, true) {
                        remove = Some(bm.id.clone());
                    }
                    let resp = ui.interact(id2(id("bm-row"), &bm.id), Rect::new(rr.x, rr.y, rr.w - 44.0, rr.h));
                    ui.g.stroke_round(rr, 6.0, 1.0, c.border);
                    if resp.hovered {
                        ui.g.fill_round(rr, 6.0, c.hover());
                    }
                    ui.text(&format!("Chapter {}", bm.chapter_index), &TextStyle::ui(12.5).semibold(), rr.x + 12.0, rr.y + 9.0, c.accent);
                    let body = if bm.note.trim().is_empty() { bm.excerpt.as_str() } else { bm.note.as_str() };
                    let t = ui.measure(body, &TextStyle::ui(13.0), TextOpts::wrap(rr.w - 60.0).max_lines(2));
                    ui.g.draw_text(t, rr.x + 12.0, rr.y + 30.0, c.text);
                    if resp.clicked {
                        jump = Some((bm.chapter_index.saturating_sub(1), bm.paragraph_index as f32));
                    }
                }
                sorted.len() as f32 * row_h + 8.0
            });
            if let Some(bid) = remove {
                out.push(ReaderAction::RemoveBookmark(bid));
            }
        }
        if let Some((ch, anchor)) = jump {
            self.go_chapter(ch, anchor);
            self.panel = Panel::None;
        }
    }

    fn appearance_panel(&mut self, ui: &mut Ui, r: Rect, config: &mut AppConfig, out: &mut Vec<ReaderAction>) {
        let c = ui.c;
        Self::panel_frame(ui, r);
        let x = r.x + 18.0;
        let w = r.w - 36.0;
        let mut y = r.y + 16.0;
        let mut changed = false;
        let label = |ui: &mut Ui, y: &mut f32, s: &str| {
            let t = ui.text(s, &TextStyle::ui(12.5).semibold(), x, *y, c.muted);
            *y += t.h + 8.0;
        };

        label(ui, &mut y, "Theme");
        let themes = ReaderThemeType::all();
        let sw = (w / themes.len() as f32).min(48.0);
        for (i, th) in themes.iter().enumerate() {
            let p = th.palette();
            let cx = x + sw * i as f32 + sw / 2.0;
            let rr = Rect::new(cx - 17.0, y, 34.0, 34.0);
            let resp = ui.interact(id2(id("ap-theme"), i), rr);
            if config.theme == *th {
                ui.g.fill_circle(cx, y + 17.0, 17.0, c.accent);
            } else if resp.hovered {
                ui.g.fill_circle(cx, y + 17.0, 17.0, c.text.alpha(0.3));
            }
            ui.g.fill_circle(cx, y + 17.0, 14.0, Color::rgb(p.bg));
            ui.g.fill_circle(cx + 5.0, y + 22.0, 4.0, Color::rgb(p.accent));
            if resp.clicked {
                config.theme = *th;
                changed = true;
            }
        }
        y += 48.0;

        let t = &mut config.typography;
        label(ui, &mut y, "Text size");
        let row = Rect::new(x, y, w, 34.0);
        if ui.button(id("ap-smaller"), Rect::new(row.x, row.y, 56.0, row.h), "A−") && t.font_size > 12.0 {
            t.font_size -= 1.0;
            changed = true;
        }
        if ui.button(id("ap-larger"), Rect::new(row.right() - 56.0, row.y, 56.0, row.h), "A+") && t.font_size < 36.0 {
            t.font_size += 1.0;
            changed = true;
        }
        ui.text_in(&format!("{:.0} px", t.font_size), &TextStyle::ui(14.0), row, Align::Center, c.text);
        y += 46.0;

        label(ui, &mut y, &format!("Line height  {:.2}×", t.line_spacing_multiplier));
        changed |= ui.slider(id("ap-line"), Rect::new(x - 6.0, y, w + 12.0, 26.0), &mut t.line_spacing_multiplier, 1.2, 2.6, 0.05);
        y += 34.0;
        label(ui, &mut y, &format!("Column width  {:.0} px", t.max_column_width));
        changed |= ui.slider(id("ap-width"), Rect::new(x - 6.0, y, w + 12.0, 26.0), &mut t.max_column_width, 450.0, 1400.0, 10.0);
        y += 34.0;
        label(ui, &mut y, &format!("Paragraph spacing  {:.0} px", t.paragraph_spacing));
        changed |= ui.slider(id("ap-para"), Rect::new(x - 6.0, y, w + 12.0, 26.0), &mut t.paragraph_spacing, 4.0, 32.0, 1.0);
        y += 38.0;

        label(ui, &mut y, "Alignment");
        let mut align = if t.text_alignment == TextAlignment::Justified { 1 } else { 0 };
        if ui.segmented(id("ap-align"), Rect::new(x, y, w, 34.0), &["Left", "Justified"], &mut align) {
            t.text_alignment = if align == 1 { TextAlignment::Justified } else { TextAlignment::Left };
            changed = true;
        }
        y += 46.0;
        label(ui, &mut y, "Layout");
        let mut mode = if t.reading_mode == ReadingMode::Paged { 0 } else { 1 };
        if ui.segmented(id("ap-mode"), Rect::new(x, y, w, 34.0), &["Pages", "Scroll"], &mut mode) {
            t.reading_mode = if mode == 0 { ReadingMode::Paged } else { ReadingMode::Scroll };
            changed = true;
        }
        y += 42.0;
        if t.reading_mode == ReadingMode::Paged {
            let mut spread = match t.page_layout {
                PageLayoutMode::Auto => 0,
                PageLayoutMode::Single => 1,
                PageLayoutMode::Dual => 2,
            };
            if ui.segmented(id("ap-spread"), Rect::new(x, y, w, 34.0), &["Auto", "Single", "Two pages"], &mut spread) {
                t.page_layout = [PageLayoutMode::Auto, PageLayoutMode::Single, PageLayoutMode::Dual][spread];
                changed = true;
            }
            y += 46.0;
        }

        label(ui, &mut y, "Font");
        let font = reader_font_label(t);
        let fr = Rect::new(x, y, w, 34.0);
        ui.text_in(&font, &TextStyle::ui(14.0), Rect::new(fr.x, fr.y, fr.w - 100.0, fr.h), Align::Left, c.text);
        if ui.button(id("ap-fonts"), Rect::new(fr.right() - 92.0, fr.y, 92.0, fr.h), "Change…") {
            out.push(ReaderAction::OpenTypographySettings);
        }
        if changed {
            out.push(ReaderAction::ConfigChanged);
        }
    }

    fn run_search(&mut self) {
        self.searched = (self.query.clone(), self.search_all);
        self.matches.clear();
        self.current_match = None;
        let q = self.query.trim();
        if q.chars().count() < 2 {
            return;
        }
        let Ok(re) = regex::RegexBuilder::new(&regex::escape(q)).case_insensitive(true).build() else { return };
        // Search from the current chapter onwards and wrap around, so the matches nearest the
        // reader are found even when a common word hits the match cap.
        let n = self.chapter_count();
        let order: Vec<usize> = if self.search_all { (self.chapter..n).chain(0..self.chapter).collect() } else { vec![self.chapter] };
        'outer: for ci in order {
            for (pi, p) in self.work.chapters[ci].paragraphs.iter().enumerate() {
                if is_landmark(pi, p) {
                    continue;
                }
                let Some((_, rich)) = paragraph_rich(p) else { continue };
                for m in re.find_iter(&rich.text) {
                    self.matches.push(Match { chapter: ci, para: pi, start: m.start(), end: m.end() });
                    if self.matches.len() >= MAX_MATCHES {
                        break 'outer;
                    }
                }
            }
        }
        // Start from the first match at or after the reading position.
        let para = self.anchor.floor() as usize;
        let first = self.matches.iter().position(|m| m.chapter != self.chapter || m.para >= para).or(if self.matches.is_empty() { None } else { Some(0) });
        if let Some(k) = first {
            self.goto_match(k);
        }
    }

    fn goto_match(&mut self, k: usize) {
        let Some(m) = self.matches.get(k).copied() else { return };
        self.current_match = Some(k);
        let len = paragraph_rich(&self.work.chapters[m.chapter].paragraphs[m.para]).map_or(1, |(_, r)| r.text.len().max(1));
        let anchor = m.para as f32 + (m.start as f32 / len as f32 * 0.9).min(0.9);
        self.go_chapter(m.chapter, anchor);
    }

    fn search_panel(&mut self, ui: &mut Ui, r: Rect) {
        let c = ui.c;
        Self::panel_frame(ui, r);
        let input = Rect::new(r.x + 14.0, r.y + 14.0, r.w - 28.0, 36.0);
        let sid = id("rd-search-input");
        let resp = ui.text_input(sid, input, &mut self.query, "Find in this work", false);
        if self.focus_pending {
            ui.focus_input(sid);
            self.focus_pending = false;
        }
        if resp.escaped {
            self.close_panel();
            return;
        }
        let y = r.y + 62.0;
        let mut all = self.search_all;
        ui.toggle(id("rd-search-all"), r.x + 16.0, y + 8.0, &mut all);
        self.search_all = all;
        ui.text("All chapters", &TextStyle::ui(13.0), r.x + 64.0, y + 8.0, c.text);
        if self.searched != (self.query.clone(), self.search_all) {
            self.run_search();
        }

        let n = self.matches.len();
        let count = match (self.current_match, n) {
            (_, 0) if self.query.trim().chars().count() >= 2 => "No matches".to_string(),
            (_, 0) => String::new(),
            (Some(k), _) => format!("{} of {}{}", k + 1, n, if n >= MAX_MATCHES { "+" } else { "" }),
            (None, _) => format!("{n} matches"),
        };
        let nav_x = r.right() - 14.0 - 34.0 * 3.0 - 8.0;
        ui.text_in(&count, &TextStyle::ui(13.0), Rect::new(r.x + 170.0, y, nav_x - r.x - 178.0, 34.0), Align::Right, if n == 0 { c.danger } else { c.muted });
        let has = n > 0;
        let next = resp.submitted || ui.button_ex(id("rd-search-next"), Rect::new(nav_x + 38.0, y, 34.0, 34.0), "", Some("\u{E70D}"), ButtonKind::Ghost, has);
        let prev = ui.button_ex(id("rd-search-prev"), Rect::new(nav_x, y, 34.0, 34.0), "", Some("\u{E70E}"), ButtonKind::Ghost, has);
        if has && next {
            let k = self.current_match.map_or(0, |k| (k + 1) % n);
            self.goto_match(k);
        } else if has && prev {
            let k = self.current_match.map_or(0, |k| (k + n - 1) % n);
            self.goto_match(k);
        }
        if ui.button_ex(id("rd-search-close"), Rect::new(nav_x + 76.0, y, 34.0, 34.0), "", Some(icons::CLOSE), ButtonKind::Ghost, true) {
            self.close_panel();
        }
    }

    fn bookmark_panel(&mut self, ui: &mut Ui, r: Rect, out: &mut Vec<ReaderAction>) {
        let c = ui.c;
        Self::panel_frame(ui, r);
        let x = r.x + 16.0;
        let w = r.w - 32.0;
        ui.text("Bookmark this page", &TextStyle::ui(15.0).semibold(), x, r.y + 14.0, c.text);
        let para = (self.anchor.floor() as usize).min(self.work.chapters[self.chapter].paragraphs.len().saturating_sub(1));
        // First real paragraph at or after the reading position.
        let excerpt: String = self.work.chapters[self.chapter]
            .paragraphs
            .iter()
            .enumerate()
            .skip(para)
            .filter(|(i, p)| !is_landmark(*i, p))
            .find_map(|(_, p)| paragraph_rich(p))
            .map(|(_, r)| r.text.chars().take(160).collect())
            .unwrap_or_default();
        let t = ui.measure(&format!("Chapter {} — “{}”", self.chapter + 1, excerpt), &TextStyle::ui(12.5).italic(), TextOpts::wrap(w).max_lines(2));
        ui.g.draw_text(t, x, r.y + 42.0, c.muted);
        let nid = id("rd-bookmark-note");
        let resp = ui.text_input(nid, Rect::new(x, r.y + 92.0, w, 36.0), &mut self.bookmark_note, "Add a note (optional)", false);
        if self.focus_pending {
            ui.focus_input(nid);
            self.focus_pending = false;
        }
        if resp.escaped || ui.button(id("rd-bm-cancel"), Rect::new(r.right() - 16.0 - 230.0, r.bottom() - 52.0, 100.0, 36.0), "Cancel") {
            self.panel = Panel::None;
            return;
        }
        let add = ui.button_ex(id("rd-bm-add"), Rect::new(r.right() - 16.0 - 122.0, r.bottom() - 52.0, 122.0, 36.0), "Add", Some("\u{E734}"), ButtonKind::Primary, true);
        if add || resp.submitted {
            let now = chrono::Utc::now();
            out.push(ReaderAction::AddBookmark(Bookmark {
                id: format!("bm_{}_{}", self.chapter + 1, now.timestamp_millis()),
                chapter_index: self.chapter + 1,
                paragraph_index: para,
                note: self.bookmark_note.trim().to_string(),
                excerpt,
                created_at: now,
            }));
            self.panel = Panel::None;
        }
    }
}
