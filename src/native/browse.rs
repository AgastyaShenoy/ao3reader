//! Search AO3 (with filters and sorting), open a pasted work link, and Following (authors,
//! tags and collections). AO3 was down for maintenance while these were built; they were tried
//! on sample data only.

use super::gfx::*;
use super::icons;
use super::reader::preface::{card_pills, pills, thousands, Pill};
use super::ui::*;
use crate::ao3::models::{WorkMeta, WorkRating};
use crate::ao3::parser::Ao3Parser;
use std::time::{Duration, Instant};
use crate::ao3::search::{Blurb, Feed, FeedKind, SearchQuery, SortBy, Tri};
use crate::storage::follows::{FollowStore, Mark, Scope, STALE_MINUTES};
use crate::storage::library::LibraryManager;

const CARD_H: f32 = 214.0;
const GAP: f32 = 10.0;
const MAX_W: f32 = 980.0;
/// A link typed by hand is looked up once typing pauses this long (a paste goes at once
/// after it).
const LINK_SETTLE: Duration = Duration::from_millis(400);

/// Which followed feeds an action applies to.
pub enum Target {
    /// Feeds not checked recently.
    Stale,
    Kind(FeedKind),
    One(String),
}

pub enum BrowseAction {
    Search(SearchQuery, u32),
    /// Fetch a work's details for the pasted-link card.
    LookUp(String),
    /// Download AO3's own EPUB of a work and save it as a file.
    Epub(String),
    GoAccount,
    /// Open from the library, or download and open.
    Read(String),
    Save(String),
    Follow(Feed),
    Unfollow(String),
    Check(Target),
    MarkRead(Target),
    FeedPage(Feed, u32),
    OpenUrl(String),
}

const RATINGS: [(Option<WorkRating>, &str); 6] = [
    (None, "Any"),
    (Some(WorkRating::General), "G"),
    (Some(WorkRating::Teen), "T"),
    (Some(WorkRating::Mature), "M"),
    (Some(WorkRating::Explicit), "E"),
    (Some(WorkRating::NotRated), "NR"),
];
const CATEGORIES: [&str; 6] = ["F/F", "F/M", "Gen", "M/M", "Multi", "Other"];
const TRI: [Tri; 3] = [Tri::Any, Tri::Only, Tri::Exclude];

fn tri_index(t: Tri) -> usize {
    TRI.iter().position(|x| *x == t).unwrap_or(0)
}

/// What the work-link card is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkBusy {
    LookUp,
    Download,
    Epub,
}

/// A pasted AO3 work link: the work's details, shown before anything is downloaded.
pub struct WorkLink {
    pub id: String,
    pub meta: Option<WorkMeta>,
    pub busy: Option<LinkBusy>,
    pub error: Option<String>,
    /// Downloaded to the library from this card.
    pub saved: bool,
}

impl WorkLink {
    fn new(id: &str, meta: Option<WorkMeta>) -> Self {
        let busy = if meta.is_none() { Some(LinkBusy::LookUp) } else { None };
        Self { id: id.to_string(), meta, busy, error: None, saved: false }
    }
}

/// The work id when the search box holds a link to an AO3 work (a bare number is searched for).
fn work_link(q: &str) -> Option<String> {
    let q = q.trim();
    if q.contains("works/") { Ao3Parser::extract_work_id(q) } else { None }
}

#[derive(Default)]
pub struct SearchScreen {
    pub query: SearchQuery,
    pub link: Option<WorkLink>,
    /// When the search box last changed.
    typed: Option<Instant>,
    filters_open: bool,
    words_min: String,
    words_max: String,
    pub results: Vec<Blurb>,
    pub total: Option<u64>,
    pub page: u32,
    pub has_next: bool,
    pub busy: bool,
    pub error: Option<String>,
}

fn chip_style(selected: bool) -> TextStyle {
    if selected { TextStyle::ui(13.0).semibold() } else { TextStyle::ui(13.0) }
}

/// Width of a `chip`, for wrapping rows of them.
fn chip_width(ui: &mut Ui, label: &str, new: usize, selected: bool) -> f32 {
    let t = ui.measure(label, &chip_style(selected), TextOpts::default());
    t.w + 26.0 + if new > 0 { ui.badge_width(new) + 6.0 } else { 0.0 }
}

/// A rounded, clickable chip, with a badge for `new` unread works (none for 0). Returns its
/// width and whether it was clicked.
fn chip(ui: &mut Ui, key: Id, x: f32, y: f32, label: &str, new: usize, selected: bool) -> (f32, bool) {
    let c = ui.c;
    let t = ui.measure(label, &chip_style(selected), TextOpts::default());
    let r = Rect::new(x, y, chip_width(ui, label, new, selected), 30.0);
    let resp = ui.interact_as(key, r, Role::Choice(selected), label);
    if selected {
        ui.g.fill_round(r, 15.0, c.selected());
    } else {
        ui.g.stroke_round(r, 15.0, 1.0, c.border);
        if resp.hovered {
            ui.g.fill_round(r, 15.0, c.hover());
        }
    }
    ui.g.draw_text(t, r.x + 13.0, r.y + (30.0 - t.h) / 2.0, c.text);
    ui.badge(r.x + 19.0 + t.w, r.y + 15.0, new, Badge::New, selected);
    (r.w, resp.clicked)
}

fn error_box(ui: &mut Ui, x: f32, y: f32, w: f32, title: &str, msg: &str) -> f32 {
    let c = ui.c;
    let t = ui.measure(msg, &TextStyle::ui(13.5), TextOpts::wrap(w - 58.0).line_height(1.35));
    let r = Rect::new(x, y, w, t.h + 52.0);
    ui.notice(r, title);
    ui.g.draw_text(t, r.x + 40.0, r.y + 36.0, c.text.alpha(0.85));
    r.h
}

impl SearchScreen {
    /// A download or lookup for the link card finished (`meta` on success).
    pub fn link_done(&mut self, id: &str, meta: Result<WorkMeta, String>, saved: bool) -> bool {
        let Some(l) = self.link.as_mut().filter(|l| l.id == id) else { return false };
        l.busy = None;
        match meta {
            Ok(m) => {
                l.meta = Some(m);
                l.error = None;
                l.saved |= saved;
            }
            Err(e) => l.error = Some(e),
        }
        true
    }

    /// The link card's EPUB download finished.
    pub fn epub_done(&mut self, error: Option<String>) {
        if let Some(l) = self.link.as_mut().filter(|l| l.busy == Some(LinkBusy::Epub)) {
            l.busy = None;
            l.error = error;
        }
    }

    pub fn frame(&mut self, ui: &mut Ui, body: Rect, library: &LibraryManager, follows: &FollowStore, logged_in: bool) -> Vec<BrowseAction> {
        let mut out = Vec::new();
        let c = ui.c;
        let w = (body.w - 48.0).min(MAX_W);
        let x = body.x + (body.w - w) / 2.0;
        let t = ui.text("Search AO3", &TextStyle::ui(26.0).semibold(), x, body.y + 22.0, c.text);
        if let Some(total) = self.total.filter(|_| self.link.is_none()) {
            ui.text(&format!("{} works", thousands(total)), &TextStyle::ui(SMALL), x + t.w + 14.0, body.y + 22.0 + t.h - 18.0, c.muted);
        }

        // Search box, then filters, sort and direction as three equal buttons, wide enough for
        // the longest sort name.
        let y = body.y + 22.0 + t.h + 14.0;
        let dir_w = ui.button_width("Descending", Some("\u{E74B}"));
        let tool_w = SortBy::ALL.iter().map(|s| ui.button_width(s.label(), Some("\u{E8CB}"))).fold(dir_w, f32::max).ceil();
        let tool_gap = 8.0;
        let search_w = (w - 3.0 * tool_w - 2.0 * tool_gap - 16.0 - 44.0).max(160.0);
        let input = Rect::new(x, y, search_w, 36.0);
        let resp = ui.text_input(id("search-q"), input, &mut self.query.query, "Search AO3, or paste a link to a work", false);
        if resp.changed {
            self.typed = Some(Instant::now());
        }
        let mut run = resp.submitted;
        if ui.icon_button(id("search-go"), Rect::new(input.right() + 4.0, y, 40.0, 36.0), icons::SEARCH, "Search", false) {
            run = true;
        }
        let n = self.filter_count();
        let flabel = if n > 0 { format!("Filters · {n}") } else { "Filters".into() };
        let fx = x + w - 3.0 * tool_w - 2.0 * tool_gap;
        if ui.button_ex(id("search-filters"), Rect::new(fx, y, tool_w, 36.0), &flabel, Some("\u{E71C}"), if self.filters_open || n > 0 { ButtonKind::Tonal } else { ButtonKind::Secondary }, true) {
            self.filters_open = !self.filters_open;
        }
        let sorts: Vec<(&str, bool)> = SortBy::ALL.iter().map(|s| (s.label(), false)).collect();
        let cur = SortBy::ALL.iter().position(|s| *s == self.query.sort);
        if let Some(i) = ui.menu_button(id("search-sort"), Rect::new(fx + tool_w + tool_gap, y, tool_w, 36.0), self.query.sort.label(), Some("\u{E8CB}"), &sorts, cur) {
            self.query.sort = SortBy::ALL[i];
            run = !self.results.is_empty();
        }
        let dir = if self.query.ascending { "Ascending" } else { "Descending" };
        if ui.button_ex(id("search-dir"), Rect::new(x + w - tool_w, y, tool_w, 36.0), dir, Some(if self.query.ascending { "\u{E74A}" } else { "\u{E74B}" }), ButtonKind::Secondary, true) {
            self.query.ascending = !self.query.ascending;
            run = !self.results.is_empty();
        }

        // A pasted work link shows that work instead of searching.
        match work_link(&self.query.query) {
            None => self.link = None,
            Some(wid) => {
                let fresh = self.link.as_ref().is_none_or(|l| l.id != wid);
                let settled = self.typed.is_none_or(|t| t.elapsed() >= LINK_SETTLE);
                if fresh && (settled || run) {
                    let local = library.get_entry(&wid).map(|e| e.meta.clone());
                    if local.is_none() {
                        out.push(BrowseAction::LookUp(wid.clone()));
                    }
                    self.link = Some(WorkLink::new(&wid, local));
                    self.filters_open = false;
                } else if fresh {
                    ui.wake_after(LINK_SETTLE.as_millis() as u32 + 10);
                } else if run {
                    // Enter on a link that failed: try again.
                    if let Some(l) = self.link.as_mut().filter(|l| l.busy.is_none() && l.error.is_some()) {
                        l.error = None;
                        if l.meta.is_none() {
                            l.busy = Some(LinkBusy::LookUp);
                            out.push(BrowseAction::LookUp(wid.clone()));
                        }
                    }
                }
                run = false;
            }
        }

        let mut top = y + 50.0;
        if self.filters_open {
            top += self.filters(ui, x, top, w, &mut run) + 12.0;
        }
        if run && !self.busy {
            self.words_to_query();
            if self.query.is_empty() {
                self.error = Some("Type something to search for, or set a filter.".into());
            } else {
                self.busy = true;
                self.error = None;
                self.filters_open = false;
                ui.scroll_to_top(id("search-list"));
                out.push(BrowseAction::Search(self.query.clone(), 1));
            }
        }

        let area = Rect::new(body.x, top, body.w, (body.bottom() - top).max(0.0));
        if let Some(link) = &self.link {
            let in_library = library.get_entry(&link.id).is_some();
            ui.scroll_area(id("search-link"), area, |ui, _view, y0| link_card(ui, x, y0 + 2.0, w, link, in_library, logged_in, &mut out) + 26.0);
            if let Some(l) = self.link.as_mut() {
                // Show what the card's buttons started (opening a saved work needs no wait).
                for a in &out {
                    let busy = match a {
                        BrowseAction::Read(_) if in_library => None,
                        BrowseAction::Read(_) | BrowseAction::Save(_) => Some(LinkBusy::Download),
                        BrowseAction::Epub(_) => Some(LinkBusy::Epub),
                        BrowseAction::LookUp(_) => Some(LinkBusy::LookUp),
                        _ => None,
                    };
                    if busy.is_some() {
                        l.busy = busy;
                        l.error = None;
                    }
                }
            }
            return out;
        }
        let busy = self.busy;
        let (error, total, has_next, page) = (self.error.clone(), self.total, self.has_next, self.page);
        let results = &self.results;
        let query = &self.query;
        ui.scroll_area(id("search-list"), area, |ui, view, y0| {
            let mut yy = y0 + 2.0;
            if let Some(e) = &error {
                yy += error_box(ui, x, yy, w, "Search didn't work", e) + 12.0;
            }
            if results.is_empty() {
                if busy {
                    ui.spinner(x + 14.0, yy + 14.0, 7.0);
                    ui.text("Searching AO3…", &TextStyle::ui(13.5), x + 30.0, yy + 5.0, c.muted);
                    yy += 40.0;
                } else if total == Some(0) {
                    ui.text("No works found. Try fewer filters or other words.", &TextStyle::ui(14.0), x + 4.0, yy + 8.0, c.muted);
                    yy += 40.0;
                } else if error.is_none() {
                    yy += tips(ui, x, yy, w);
                }
            }
            for (i, b) in results.iter().enumerate() {
                let r = Rect::new(x, yy + i as f32 * (CARD_H + GAP), w, CARD_H);
                if r.bottom() >= view.y && r.y <= view.bottom() {
                    blurb_card(ui, r, b, library.get_entry(&b.meta.id).is_some(), Mark::None, &[], follows, &mut out);
                }
            }
            yy += results.len() as f32 * (CARD_H + GAP);
            if !results.is_empty() && (has_next || busy) {
                if busy {
                    ui.spinner(x + w / 2.0 - 50.0, yy + 18.0, 7.0);
                    ui.text("Loading more…", &TextStyle::ui(13.5), x + w / 2.0 - 34.0, yy + 9.0, c.muted);
                } else {
                    let bw = 160.0;
                    if ui.button(id("search-more"), Rect::new(x + (w - bw) / 2.0, yy + 2.0, bw, 36.0), "Load more") {
                        out.push(BrowseAction::Search(query.clone(), page + 1));
                    }
                }
                yy += 48.0;
            }
            yy + 24.0 - y0
        });
        if out.iter().any(|a| matches!(a, BrowseAction::Search(_, p) if *p > 1)) {
            self.busy = true;
        }
        out
    }

    fn filter_count(&self) -> usize {
        let q = &self.query;
        let blank = |s: &String| s.trim().is_empty();
        [!blank(&q.title), !blank(&q.creators), !blank(&q.fandoms), !blank(&q.relationships), !blank(&q.characters), !blank(&q.tags), !blank(&q.language)]
            .iter()
            .filter(|b| **b)
            .count()
            + q.rating.is_some() as usize
            + (!q.categories.is_empty()) as usize
            + (q.complete != Tri::Any) as usize
            + (q.crossovers != Tri::Any) as usize
            + q.single_chapter as usize
            + (!self.words_min.trim().is_empty()) as usize
            + (!self.words_max.trim().is_empty()) as usize
    }

    fn words_to_query(&mut self) {
        self.query.words_min = self.words_min.trim().replace(',', "").parse().ok();
        self.query.words_max = self.words_max.trim().replace(',', "").parse().ok();
    }

    /// The filter card; returns its height.
    fn filters(&mut self, ui: &mut Ui, x: f32, y: f32, w: f32, run: &mut bool) -> f32 {
        let c = ui.c;
        let h = 330.0;
        let card = Rect::new(x, y, w, h);
        ui.card(card);
        let pad = 18.0;
        let cw = (w - 2.0 * pad - 24.0) / 3.0;
        let colx = |i: usize| card.x + pad + i as f32 * (cw + 12.0);
        let label = |ui: &mut Ui, s: &str, x: f32, y: f32| {
            ui.text(s, &TextStyle::ui(12.0).semibold(), x, y, c.muted);
        };
        let q = &mut self.query;
        let fields: [(&str, &str, &mut String); 6] = [
            ("Fandoms", "e.g. Harry Potter - J. K. Rowling", &mut q.fandoms),
            ("Relationships", "e.g. Hermione Granger/Harry Potter", &mut q.relationships),
            ("Characters", "Separate several with commas", &mut q.characters),
            ("Additional tags", "e.g. Fluff, Time Travel", &mut q.tags),
            ("Title", "Words in the title", &mut q.title),
            ("Author", "AO3 user name", &mut q.creators),
        ];
        for (i, (name, hint, value)) in fields.into_iter().enumerate() {
            let fx = colx(i % 3);
            let fy = card.y + 14.0 + (i / 3) as f32 * 64.0;
            label(ui, name, fx, fy);
            if ui.text_input(id2(id("search-f"), i), Rect::new(fx, fy + 20.0, cw, 34.0), value, hint, false).submitted {
                *run = true;
            }
        }
        let ry = card.y + 146.0;
        label(ui, "Rating", colx(0), ry);
        let mut ri = RATINGS.iter().position(|(r, _)| *r == q.rating).unwrap_or(0);
        let labels: Vec<&str> = RATINGS.iter().map(|(_, l)| *l).collect();
        if ui.segmented(id("search-rating"), Rect::new(colx(0), ry + 20.0, cw, 32.0), &labels, &mut ri) {
            q.rating = RATINGS[ri].0;
        }
        label(ui, "Status", colx(1), ry);
        let mut ci = tri_index(q.complete);
        if ui.segmented(id("search-complete"), Rect::new(colx(1), ry + 20.0, cw, 32.0), &["Any", "Complete", "In progress"], &mut ci) {
            q.complete = TRI[ci];
        }
        label(ui, "Crossovers", colx(2), ry);
        let mut xi = tri_index(q.crossovers);
        if ui.segmented(id("search-cross"), Rect::new(colx(2), ry + 20.0, cw, 32.0), &["Include", "Only", "Exclude"], &mut xi) {
            q.crossovers = TRI[xi];
        }

        let ry = card.y + 214.0;
        label(ui, "Categories (any of)", colx(0), ry);
        // Like a segmented control, but several can be on at once.
        let bar = Rect::new(colx(0), ry + 20.0, cw, 32.0);
        ui.g.fill_round(bar, RADIUS, c.bg);
        ui.g.stroke_round(bar, RADIUS, 1.0, c.border);
        let cell_w = bar.w / CATEGORIES.len() as f32;
        for (i, cat) in CATEGORIES.iter().enumerate() {
            let on = q.categories.iter().any(|c| c == cat);
            let cr = Rect::new(bar.x + cell_w * i as f32, bar.y, cell_w, bar.h).shrink(3.0);
            let resp = ui.interact_as(id2(id("search-cat"), i), cr, Role::Toggle(on), cat);
            if resp.clicked {
                if on {
                    q.categories.retain(|c| c != cat);
                } else {
                    q.categories.push(cat.to_string());
                }
            }
            if on {
                ui.g.fill_round(cr, RADIUS - 2.0, c.selected());
            } else if resp.hovered {
                ui.g.fill_round(cr, RADIUS - 2.0, c.hover());
            }
            let style = if on { TextStyle::ui(12.5).semibold() } else { TextStyle::ui(12.5) };
            ui.text_in(cat, &style, cr, Align::Center, if on { c.text } else { c.muted });
        }
        label(ui, "Word count", colx(1), ry);
        let half = (cw - 24.0) / 2.0;
        ui.text_input(id("search-wmin"), Rect::new(colx(1), ry + 20.0, half, 34.0), &mut self.words_min, "Min", false);
        ui.text_in("to", &TextStyle::ui(13.0), Rect::new(colx(1) + half, ry + 20.0, 24.0, 34.0), Align::Center, c.muted);
        ui.text_input(id("search-wmax"), Rect::new(colx(1) + half + 24.0, ry + 20.0, half, 34.0), &mut self.words_max, "Max", false);
        let q = &mut self.query;
        label(ui, "Language code", colx(2), ry);
        ui.text_input(id("search-lang"), Rect::new(colx(2), ry + 20.0, cw, 34.0), &mut q.language, "e.g. en, es, fr; empty for any", false);

        let by = card.bottom() - 48.0;
        ui.toggle(id("search-single"), colx(0), by + 8.0, &mut q.single_chapter);
        ui.text("One-shots only", &TextStyle::ui(13.5), colx(0) + 50.0, by + 8.0, c.text);
        let sw = ui.button_width("Search", Some(icons::SEARCH));
        if ui.button_ex(id("search-apply"), Rect::new(card.right() - pad - sw, by, sw, 34.0), "Search", Some(icons::SEARCH), ButtonKind::Primary, true) {
            *run = true;
        }
        let clw = ui.button_width("Clear filters", None);
        if ui.button_ex(id("search-clear"), Rect::new(card.right() - pad - sw - 8.0 - clw, by, clw, 34.0), "Clear filters", None, ButtonKind::Ghost, true) {
            let keep = (q.query.clone(), q.sort, q.ascending);
            *q = SearchQuery { query: keep.0, sort: keep.1, ascending: keep.2, ..Default::default() };
            self.words_min.clear();
            self.words_max.clear();
        }
        h
    }
}

fn tips(ui: &mut Ui, x: f32, y: f32, w: f32) -> f32 {
    let c = ui.c;
    let msg = "Type words from a title, summary or tag, or open Filters to search by fandom, relationship, character, rating, length and more. Sort by kudos, hits or date to find popular or recent works.\n\nHave a link? Paste a link to an AO3 work (or any of its chapters) to see the work and download it.\n\nRead downloads a work and opens it; it's then saved in your Library for offline reading. Use ⋯ on a result to follow its author or tags.";
    let t = ui.measure(msg, &TextStyle::ui(13.5), TextOpts::wrap(w - 36.0).line_height(1.45));
    let r = Rect::new(x, y, w, t.h + 56.0);
    ui.card(r);
    ui.text("Search all of AO3", &TextStyle::ui(15.0).semibold(), r.x + 18.0, r.y + 14.0, c.text);
    ui.g.draw_text(t, r.x + 18.0, r.y + 40.0, c.muted);
    r.h + 12.0
}

/// The work behind a pasted link: its details, and buttons to read, save or get the EPUB.
/// Returns the height used.
fn link_card(ui: &mut Ui, x: f32, y: f32, w: f32, link: &WorkLink, in_library: bool, logged_in: bool, out: &mut Vec<BrowseAction>) -> f32 {
    let c = ui.c;
    let pad = 20.0;
    let iw = w - 2.0 * pad;
    let wid = &link.id;
    let busy_row = |ui: &mut Ui, x: f32, y: f32, msg: &str| {
        ui.spinner(x + 8.0, y + 9.0, 7.0);
        ui.text(msg, &TextStyle::ui(13.5), x + 24.0, y, c.muted);
    };

    let Some(m) = &link.meta else {
        // Still looking it up, or the lookup failed.
        if let Some(err) = &link.error {
            let restricted = err.to_lowercase().contains("restricted") || err.to_lowercase().contains("sign in");
            let t = ui.measure(err, &TextStyle::ui(13.5), TextOpts::wrap(w - 58.0).line_height(1.35));
            let r = Rect::new(x, y, w, t.h + 96.0);
            ui.notice(r, "Couldn't open this link");
            ui.g.draw_text(t, r.x + 40.0, r.y + 36.0, c.text.alpha(0.85));
            let by = r.bottom() - 48.0;
            let rw = ui.button_width("Try again", Some(icons::SYNC));
            if ui.button_ex(id("link-retry"), Rect::new(r.x + 40.0, by, rw, 34.0), "Try again", Some(icons::SYNC), ButtonKind::Secondary, true) {
                out.push(BrowseAction::LookUp(wid.clone()));
            }
            if restricted && !logged_in {
                let sw = ui.button_width("Sign in to AO3", Some(icons::CONTACT));
                if ui.button_ex(id("link-account"), Rect::new(r.x + 48.0 + rw, by, sw, 34.0), "Sign in to AO3", Some(icons::CONTACT), ButtonKind::Secondary, true) {
                    out.push(BrowseAction::GoAccount);
                }
            }
            return r.h;
        }
        let r = Rect::new(x, y, w, 60.0);
        ui.card(r);
        busy_row(ui, r.x + pad, r.y + 21.0, "Looking up the work on AO3…");
        return r.h;
    };

    // Measure everything first so the card can be drawn behind it.
    let title = if m.is_restricted { format!("{}  \u{1F512}", m.title) } else { m.title.clone() };
    let tt = ui.measure(&title, &TextStyle::ui(21.0).semibold(), TextOpts::wrap(iw).max_lines(2).line_height(1.25));
    let by = ui.measure(&format!("by {}", m.authors.join(", ")), &TextStyle::ui(14.0), TextOpts::wrap(iw).max_lines(2));
    let mut tags = card_pills(m, 8);
    let status = tags.pop();
    tags.extend(m.fandoms.iter().take(4).map(|f| Pill::Tag(f.clone())));
    tags.extend(status);
    let more: Vec<String> = m.characters.iter().take(6).chain(m.freeform_tags.iter().take(12)).cloned().collect();
    let mt = (!more.is_empty()).then(|| ui.measure(&more.join(" · "), &TextStyle::ui(13.0), TextOpts::wrap(iw).max_lines(3).line_height(1.4)));
    let summary = m.summary_text.split_whitespace().collect::<Vec<_>>().join(" ");
    let st = (!summary.is_empty()).then(|| ui.measure(&summary, &TextStyle::ui(14.0), TextOpts::wrap(iw - 32.0).max_lines(8).line_height(1.5)));
    let s = &m.stats;
    let mut stats = vec![format!("{} words", thousands(s.words as u64)), format!("{}/{} chapters", s.chapters_published, s.chapters_total.map_or("?".into(), |n| n.to_string()))];
    if s.kudos > 0 {
        stats.push(format!("{} kudos", thousands(s.kudos)));
    }
    if s.hits > 0 {
        stats.push(format!("{} hits", thousands(s.hits)));
    }
    if !s.published_date.is_empty() {
        stats.push(format!("Published {}", s.published_date));
    }
    let stt = ui.measure(&stats.join("  ·  "), &TextStyle::ui(12.5), TextOpts::wrap(iw));

    // Pills wrap; measure their height by laying them out off to the side (clipped away).
    ui.g.push_clip(Rect::new(0.0, 0.0, 0.0, 0.0));
    let pills_h = pills(ui, x + pad, y, iw, &tags);
    ui.g.pop_clip();

    let h = 18.0 + 20.0 + tt.h + 4.0 + by.h + 14.0 + pills_h + mt.map_or(0.0, |t| t.h + 10.0) + st.map_or(0.0, |t| t.h + 44.0) + 14.0 + stt.h + 20.0 + 36.0 + 20.0;
    let r = Rect::new(x, y, w, h);
    ui.card(r);
    let mut cy = r.y + 18.0;
    let (caption, cap_color) = if link.saved {
        ("Saved to your library", c.success)
    } else if in_library {
        ("In your library", c.success)
    } else {
        ("AO3 work", c.muted)
    };
    ui.text(caption, &TextStyle::ui(12.5).semibold(), r.x + pad, cy, cap_color);
    cy += 20.0;
    ui.g.draw_text(tt, r.x + pad, cy, c.text);
    cy += tt.h + 4.0;
    ui.g.draw_text(by, r.x + pad, cy, c.muted);
    cy += by.h + 14.0;
    cy += pills(ui, r.x + pad, cy, iw, &tags);
    if let Some(t) = mt {
        cy += 10.0;
        ui.g.draw_text(t, r.x + pad, cy, c.muted);
        cy += t.h;
    }
    if let Some(t) = st {
        cy += 14.0;
        let sr = Rect::new(r.x + pad, cy, iw, t.h + 30.0);
        ui.g.fill_round(sr, 8.0, c.notes_bg);
        ui.g.draw_text(t, sr.x + 16.0, sr.y + 15.0, c.text.alpha(0.9));
        cy += sr.h;
    }
    cy += 14.0;
    ui.g.draw_text(stt, r.x + pad, cy, c.muted);
    cy += stt.h + 20.0;

    // Actions.
    let busy = link.busy.is_some();
    let mut bx = r.x + pad;
    let mut button = |ui: &mut Ui, key: &str, label: &str, icon: &str, kind: ButtonKind| {
        let bw = ui.button_width(label, Some(icon)).max(if kind == ButtonKind::Primary { 120.0 } else { 0.0 });
        let hit = ui.button_ex(id2(id(key), wid), Rect::new(bx, cy, bw, 36.0), label, Some(icon), kind, !busy);
        bx += bw + 8.0;
        hit
    };
    if in_library {
        if button(ui, "link-open", "Open", icons::READING, ButtonKind::Primary) {
            out.push(BrowseAction::Read(wid.clone()));
        }
        if button(ui, "link-update", "Download again", icons::SYNC, ButtonKind::Secondary) {
            out.push(BrowseAction::Save(wid.clone()));
        }
    } else {
        if button(ui, "link-read", "Read", icons::READING, ButtonKind::Primary) {
            out.push(BrowseAction::Read(wid.clone()));
        }
        if button(ui, "link-save", "Save to library", icons::DOWNLOAD, ButtonKind::Secondary) {
            out.push(BrowseAction::Save(wid.clone()));
        }
    }
    if button(ui, "link-epub", "EPUB file", "\u{E8A5}", ButtonKind::Ghost) {
        out.push(BrowseAction::Epub(wid.clone()));
    }
    let ow = ui.button_width("Open on AO3", Some(icons::GLOBE));
    if bx + ow < r.right() - pad && ui.button_ex(id2(id("link-ao3"), wid), Rect::new(r.right() - pad - ow, cy, ow, 36.0), "Open on AO3", Some(icons::GLOBE), ButtonKind::Ghost, true) {
        out.push(BrowseAction::OpenUrl(format!("https://archiveofourown.org/works/{wid}")));
    }
    let mut total = r.h;
    if let Some(b) = link.busy {
        let msg = match b {
            LinkBusy::Download => "Downloading the work… long works can take a little while.",
            LinkBusy::Epub => "Downloading AO3's EPUB…",
            LinkBusy::LookUp => "Refreshing the details from AO3…",
        };
        busy_row(ui, x + 4.0, r.bottom() + 14.0, msg);
        total += 40.0;
    } else if let Some(err) = &link.error {
        let t = ui.measure(err, &TextStyle::ui(13.5), TextOpts::wrap(w - 58.0).line_height(1.35));
        let er = Rect::new(x, r.bottom() + 12.0, w, t.h + 52.0);
        ui.notice(er, "Couldn't download");
        ui.g.draw_text(t, er.x + 40.0, er.y + 36.0, c.text.alpha(0.85));
        total += er.h + 12.0;
    }
    total
}

/// One AO3 work (search result or feed entry).
fn blurb_card(ui: &mut Ui, r: Rect, b: &Blurb, in_library: bool, mark: Mark, from: &[String], follows: &FollowStore, out: &mut Vec<BrowseAction>) {
    let c = ui.c;
    let m = &b.meta;
    let wid = &m.id;
    ui.card(r);

    // Title row; the ⋯ menu follows (or unfollows) the author or tags.
    let more = Rect::new(r.right() - 48.0, r.y + 10.0, 36.0, 34.0);
    let mut menu: Vec<(String, BrowseAction)> = Vec::new();
    let mut offer = |kind: FeedKind, name: &String, what: &str| {
        let feed = Feed { kind, name: name.clone() };
        if follows.is_following(kind, name) {
            menu.push((format!("Unfollow {what}{name}"), BrowseAction::Unfollow(feed.id())));
        } else {
            menu.push((format!("Follow {what}{name}"), BrowseAction::Follow(feed)));
        }
    };
    for a in m.authors.iter().filter(|a| *a != "Anonymous" && !a.contains("orphan_account")) {
        offer(FeedKind::Author, a, "author ");
    }
    for t in m.relationships.iter().take(3).chain(m.fandoms.iter().take(2)) {
        offer(FeedKind::Tag, t, "");
    }
    menu.push(("Open on AO3".into(), BrowseAction::OpenUrl(format!("https://archiveofourown.org/works/{wid}"))));
    let items: Vec<(&str, bool)> = menu.iter().map(|(l, _)| (l.as_str(), false)).collect();
    if let Some(i) = ui.menu_button(id2(id("blurb-more"), wid), more, "", Some("\u{E712}"), &items, None) {
        out.push(menu.swap_remove(i).1);
    }
    ui.tip(id2(id("blurb-more"), wid), more, "More: follow the author or tags, open on AO3");
    let tw = r.w - 92.0;
    let mut tx = r.x + 16.0;
    if mark != Mark::None {
        // New works get the accent; updates to ones already seen stay neutral.
        let nt = ui.measure(mark.label(), &TextStyle::ui(12.0).semibold(), TextOpts::default());
        let nr = Rect::new(tx, r.y + 17.0, nt.w + 16.0, 22.0);
        let (fill, fg) = if mark == Mark::New { (c.accent_text.alpha(0.16), c.accent_text) } else { (c.selected(), c.text) };
        ui.g.fill_round(nr, 11.0, fill);
        ui.g.draw_text(nt, nr.x + 8.0, nr.y + (22.0 - nt.h) / 2.0, fg);
        tx += nr.w + 8.0;
    }
    let title = if m.is_restricted { format!("{}  \u{1F512}", m.title) } else { m.title.clone() };
    let title_r = Rect::new(tx, r.y + 14.0, tw - (tx - r.x - 16.0), 28.0);
    let tresp = ui.interact_as(id2(id("blurb-title"), wid), title_r, Role::Button, &m.title);
    let tt = ui.text_opts(&title, &TextStyle::ui(17.0).semibold(), TextOpts::ellipsize(title_r.w), title_r.x, title_r.y, c.text);
    if tresp.hovered {
        ui.g.fill_rect(Rect::new(title_r.x, title_r.y + tt.h - 1.0, tt.w, 1.0), c.text.alpha(0.6));
    }
    if tresp.clicked {
        out.push(BrowseAction::Read(wid.clone()));
    }
    let mut byline = format!("by {}", m.authors.join(", "));
    if !m.fandoms.is_empty() {
        byline.push_str(&format!("  ·  {}", m.fandoms.iter().take(3).cloned().collect::<Vec<_>>().join(", ")));
    }
    ui.text_opts(&byline, &TextStyle::ui(13.0), TextOpts::ellipsize(tw), r.x + 16.0, r.y + 44.0, c.muted);

    let tags = card_pills(m, 3);
    ui.g.push_clip(Rect::new(r.x + 16.0, r.y + 66.0, r.w - 32.0, 26.0));
    pills(ui, r.x + 16.0, r.y + 67.0, 10_000.0, &tags);
    ui.g.pop_clip();

    let more_tags: Vec<String> = m.characters.iter().take(3).chain(m.freeform_tags.iter().take(6)).cloned().collect();
    ui.text_opts(&more_tags.join(" · "), &TextStyle::ui(12.5), TextOpts::ellipsize(r.w - 32.0), r.x + 16.0, r.y + 98.0, c.muted);
    let summary: String = m.summary_text.replace("**", "").replace("~~", "").replace('*', "").split_whitespace().collect::<Vec<_>>().join(" ");
    ui.text_opts(&summary, &TextStyle::ui(13.0), TextOpts::wrap(r.w - 32.0).max_lines(2).line_height(1.35), r.x + 16.0, r.y + 120.0, c.text.alpha(0.82));

    let by = r.bottom() - 46.0;
    let label = if in_library { "Open" } else { "Read" };
    let rw = ui.button_width(label, Some(icons::READING)).max(96.0);
    let read_r = Rect::new(r.right() - 16.0 - rw, by, rw, 34.0);
    if ui.button_ex(id2(id("blurb-read"), wid), read_r, label, Some(icons::READING), ButtonKind::Tonal, true) {
        out.push(BrowseAction::Read(wid.clone()));
    }
    let mut left_end = read_r.x - 16.0;
    if !in_library {
        let sw = ui.button_width("Save", Some(icons::DOWNLOAD));
        let sr = Rect::new(read_r.x - 8.0 - sw, by, sw, 34.0);
        if ui.button_ex(id2(id("blurb-save"), wid), sr, "Save", Some(icons::DOWNLOAD), ButtonKind::Secondary, true) {
            out.push(BrowseAction::Save(wid.clone()));
        }
        left_end = sr.x - 16.0;
    }
    let total = m.stats.chapters_total.map_or("?".to_string(), |t| t.to_string());
    let mut stats = vec![format!("{} words", thousands(m.stats.words as u64)), format!("{}/{} chapters", m.stats.chapters_published, total)];
    if m.stats.kudos > 0 {
        stats.push(format!("{} kudos", thousands(m.stats.kudos)));
    }
    if m.stats.hits > 0 {
        stats.push(format!("{} hits", thousands(m.stats.hits)));
    }
    if !b.updated.is_empty() {
        stats.push(b.updated.clone());
    }
    let lw = (left_end - r.x - 16.0).max(0.0);
    ui.text_opts(&stats.join("  ·  "), &TextStyle::ui(12.5), TextOpts::ellipsize(lw), r.x + 16.0, by + 2.0, c.muted);
    if !from.is_empty() {
        ui.text_opts(&format!("From {}", from.join(", ")), &TextStyle::ui(12.0), TextOpts::ellipsize(lw), r.x + 16.0, by + 22.0, c.text.alpha(0.65));
    }
}

// ---- Following ---------------------------------------------------------------------------

const KINDS: [(FeedKind, &str, &str, &str); 3] = [
    (FeedKind::Author, "Authors", "AO3 user name, or a link to their page", "authors"),
    (FeedKind::Tag, "Tags", "Fandom, ship or tag, e.g. Hermione Granger/Harry Potter", "tags"),
    (FeedKind::Collection, "Collections", "Collection name, or a link to it", "collections"),
];

pub struct FollowingScreen {
    /// Index into KINDS: which kind of feed the screen shows.
    tab: usize,
    /// One feed shown on its own (within the tab), or None for all of the tab's feeds.
    selected: Option<String>,
    pub checking: bool,
    pub error: Option<String>,
    adding: bool,
    add_name: String,
    /// Older works of the selected feed, loaded on request.
    pub more: Vec<Blurb>,
    pub more_page: u32,
    pub more_next: bool,
    pub more_busy: bool,
    auto_checked: bool,
}

impl Default for FollowingScreen {
    fn default() -> Self {
        Self { tab: 0, selected: None, checking: false, error: None, adding: false, add_name: String::new(), more: Vec::new(), more_page: 1, more_next: true, more_busy: false, auto_checked: false }
    }
}

fn ago(t: chrono::DateTime<chrono::Utc>) -> String {
    let m = (chrono::Utc::now() - t).num_minutes();
    match m {
        ..=0 => "just now".into(),
        1..=59 => format!("{m} min ago"),
        60..=1439 => format!("{} h ago", m / 60),
        _ => format!("{} d ago", m / 1440),
    }
}

/// Height of a wrapped row of suggestion chips, measured the same way they are drawn.
fn chips_height(ui: &mut Ui, w: f32, names: &[String]) -> f32 {
    let mut cx = 0.0;
    let mut rows = 1.0;
    for n in names {
        let tw = ui.measure(&format!("+ {n}"), &TextStyle::ui(13.0), TextOpts::default()).w + 26.0;
        if cx > 0.0 && cx + tw > w {
            cx = 0.0;
            rows += 1.0;
        }
        cx += tw + 6.0;
    }
    rows * 36.0 - 6.0
}

/// "+ name" chips that follow on click (only when the user clicks: nothing is followed for them).
fn suggestion_chips(ui: &mut Ui, x: f32, y: f32, w: f32, kind: FeedKind, names: &[String], out: &mut Vec<BrowseAction>) -> f32 {
    let (mut cx, mut cy) = (x, y);
    for (i, n) in names.iter().enumerate() {
        let label = format!("+ {n}");
        let tw = ui.measure(&label, &TextStyle::ui(13.0), TextOpts::default()).w + 26.0;
        if cx > x && cx + tw > x + w {
            cx = x;
            cy += 36.0;
        }
        let (cw, clicked) = chip(ui, id2(id("fol-suggest"), (kind.label(), i)), cx, cy, &label, 0, false);
        if clicked {
            out.push(BrowseAction::Follow(Feed { kind, name: n.clone() }));
        }
        cx += cw + 6.0;
    }
    cy + 30.0 - y
}

impl FollowingScreen {
    pub fn frame(&mut self, ui: &mut Ui, body: Rect, follows: &FollowStore, library: &LibraryManager) -> Vec<BrowseAction> {
        let mut out = Vec::new();
        let c = ui.c;
        let w = (body.w - 48.0).min(MAX_W);
        let x = body.x + (body.w - w) / 2.0;
        let (kind, kind_label, hint, plural) = KINDS[self.tab];
        if self.selected.as_deref().is_some_and(|s| follows.get(s).is_none_or(|f| f.feed.kind != kind)) {
            self.selected = None;
        }
        let selected = self.selected.clone();
        let scope = match &selected {
            Some(s) => Scope::One(s.as_str()),
            None => Scope::Kind(kind),
        };
        let t = ui.text("Following", &TextStyle::ui(26.0).semibold(), x, body.y + 22.0, c.text);

        // Actions on the right of the title.
        let mine: Vec<_> = follows.list().iter().filter(|f| f.feed.kind == kind).collect();
        let scoped_unread = follows.unread_count(scope);
        let mut bx = x + w;
        let mut button = |ui: &mut Ui, key: &str, label: &str, icon: &str, kind: ButtonKind, enabled: bool| {
            let bw = ui.button_width(label, Some(icon));
            bx -= bw;
            let hit = ui.button_ex(id(key), Rect::new(bx, body.y + 22.0, bw, 36.0), label, Some(icon), kind, enabled);
            bx -= 8.0;
            hit
        };
        if button(ui, "fol-add", "Follow", "\u{E710}", ButtonKind::Primary, true) {
            self.adding = !self.adding;
        }
        if !mine.is_empty() && button(ui, "fol-check", if self.checking { "Checking…" } else { "Refresh" }, icons::SYNC, ButtonKind::Secondary, !self.checking) {
            self.checking = true;
            self.error = None;
            out.push(BrowseAction::Check(self.target(kind)));
        }
        if scoped_unread > 0 && button(ui, "fol-read", "Mark read", icons::CHECK, ButtonKind::Ghost, true) {
            out.push(BrowseAction::MarkRead(self.target(kind)));
        }

        // Check feeds not looked at (or tried) for half an hour, once per visit.
        if !self.auto_checked && !self.checking && !follows.stale(STALE_MINUTES).is_empty() {
            self.auto_checked = true;
            self.checking = true;
            out.push(BrowseAction::Check(Target::Stale));
        }

        // Authors / Tags (/ Collections, once there are any), each with a badge for its new works.
        let mut y = body.y + 22.0 + t.h + 14.0;
        let show_collections = self.tab == 2 || follows.list().iter().any(|f| f.feed.kind == FeedKind::Collection);
        let tabs: Vec<(&str, usize)> = KINDS
            .iter()
            .take(if show_collections { 3 } else { 2 })
            .map(|(k, label, _, _)| (*label, follows.unread_count(Scope::Kind(*k))))
            .collect();
        let mut tab = self.tab;
        if ui.segmented_counts(id("fol-tabs"), Rect::new(x, y, 150.0 * tabs.len() as f32, 34.0), &tabs, &mut tab, Badge::New) {
            self.tab = tab;
            self.select(None, ui);
            return out;
        }
        y += 48.0;

        let suggestions = follows.suggestions(library.entries().values().map(|e| &e.meta), kind, 8);
        if self.adding {
            let extra = if suggestions.is_empty() { 0.0 } else { 36.0 + chips_height(ui, w - 36.0, &suggestions) };
            let card = Rect::new(x, y, w, 112.0 + extra);
            ui.card(card);
            ui.text(&format!("Follow {} on AO3", if kind == FeedKind::Author { "an author" } else if kind == FeedKind::Tag { "a tag" } else { "a collection" }), &TextStyle::ui(14.0).semibold(), card.x + 18.0, card.y + 14.0, c.text);
            let fw = ui.button_width("Follow", Some(icons::CHECK));
            let input = Rect::new(card.x + 18.0, card.y + 42.0, card.w - 36.0 - fw - 8.0, 34.0);
            let resp = ui.text_input(id("fol-name"), input, &mut self.add_name, hint, false);
            let go = ui.button_ex(id("fol-go"), Rect::new(input.right() + 8.0, input.y, fw, 34.0), "Follow", Some(icons::CHECK), ButtonKind::Primary, !self.add_name.trim().is_empty());
            let note = if kind == FeedKind::Tag { "Tag names must match AO3's exactly. A link copied from AO3 always works." } else { "A link copied from AO3 always works." };
            ui.text(note, &TextStyle::ui(12.5), card.x + 18.0, card.y + 84.0, c.muted);
            if (go || resp.submitted) && !self.add_name.trim().is_empty() {
                if let Some(feed) = Feed::parse(&self.add_name, kind) {
                    if feed.kind != kind {
                        // A pasted link of another kind: show it under its own tab.
                        self.tab = KINDS.iter().position(|k| k.0 == feed.kind).unwrap_or(self.tab);
                    }
                    out.push(BrowseAction::Follow(feed));
                    self.add_name.clear();
                    self.adding = false;
                }
            }
            if !suggestions.is_empty() {
                ui.text("From your library", &TextStyle::ui(12.0).semibold(), card.x + 18.0, card.y + 112.0, c.muted);
                suggestion_chips(ui, card.x + 18.0, card.y + 134.0, card.w - 36.0, kind, &suggestions, &mut out);
            }
            y += card.h + 12.0;
        }

        if mine.is_empty() {
            let msg = match kind {
                FeedKind::Author => "Follow AO3 users to see their new works and updates here. Use Follow above, or ⋯ on any search result.",
                FeedKind::Tag => "Follow any AO3 tag: a fandom, a character, or a relationship such as Hermione Granger/Harry Potter. Use Follow above, or ⋯ on any search result.",
                FeedKind::Collection => "Follow an AO3 collection (a challenge, fest or community) by its name or link.",
            };
            let tt = ui.measure(msg, &TextStyle::ui(13.5), TextOpts::wrap(w - 36.0).line_height(1.45));
            let sug_h = if suggestions.is_empty() || self.adding { 0.0 } else { 40.0 + chips_height(ui, w - 36.0, &suggestions) };
            let r = Rect::new(x, y, w, tt.h + 64.0 + sug_h);
            ui.card(r);
            ui.text(&format!("No {plural} followed yet"), &TextStyle::ui(15.0).semibold(), r.x + 18.0, r.y + 14.0, c.text);
            ui.g.draw_text(tt, r.x + 18.0, r.y + 40.0, c.muted);
            if sug_h > 0.0 {
                let sy = r.y + 52.0 + tt.h;
                ui.text("From your library (click to follow)", &TextStyle::ui(12.0).semibold(), r.x + 18.0, sy, c.muted);
                suggestion_chips(ui, r.x + 18.0, sy + 22.0, r.w - 36.0, kind, &suggestions, &mut out);
            }
            return out;
        }

        // This tab's feeds.
        let mut cx = x;
        let tab_unread = follows.unread_count(Scope::Kind(kind));
        let (cw, clicked) = chip(ui, id("fol-all"), cx, y, &format!("All {plural}"), tab_unread, self.selected.is_none());
        if clicked {
            self.select(None, ui);
        }
        cx += cw + 6.0;
        for f in &mine {
            let fid = f.id();
            let on = self.selected.as_deref() == Some(fid.as_str());
            if cx + chip_width(ui, &f.feed.name, f.unread.len(), on) > x + w {
                cx = x;
                y += 36.0;
            }
            let (cw, clicked) = chip(ui, id2(id("fol-chip"), &fid), cx, y, &f.feed.name, f.unread.len(), on);
            if clicked {
                self.select(Some(fid), ui);
            }
            cx += cw + 6.0;
        }
        y += 42.0;

        let sel = self.selected.as_deref().and_then(|s| follows.get(s));
        if let Some(f) = sel {
            let mut info = format!("{} · {}", kind_label.trim_end_matches('s'), f.feed.name);
            if let Some(t) = f.total {
                info.push_str(&format!("  ·  {} works", thousands(t)));
            }
            info.push_str(&format!("  ·  {}", f.checked.map_or("not checked yet".into(), |t| format!("checked {}", ago(t)))));
            ui.text_opts(&info, &TextStyle::ui(13.0), TextOpts::ellipsize(w - 260.0), x + 2.0, y + 8.0, c.text);
            let uw = ui.button_width("Unfollow", Some(icons::CLOSE));
            if ui.button_ex(id("fol-unfollow"), Rect::new(x + w - uw, y, uw, 32.0), "Unfollow", Some(icons::CLOSE), ButtonKind::Secondary, true) {
                out.push(BrowseAction::Unfollow(f.id()));
            }
            let ow = ui.button_width("Open on AO3", Some(icons::GLOBE));
            if ui.button_ex(id("fol-ao3"), Rect::new(x + w - uw - 8.0 - ow, y, ow, 32.0), "Open on AO3", Some(icons::GLOBE), ButtonKind::Ghost, true) {
                out.push(BrowseAction::OpenUrl(f.feed.url(1)));
            }
            y += 44.0;
        }

        let timeline = follows.timeline(scope);
        let extra: Vec<&Blurb> = self.more.iter().filter(|m| !timeline.iter().any(|(b, _, _)| b.meta.id == m.meta.id)).collect();
        let area = Rect::new(body.x, y, body.w, (body.bottom() - y).max(0.0));
        let (checking, error) = (self.checking, self.error.clone());
        let sel_error = sel.and_then(|f| f.error.clone());
        let show_from = self.selected.is_none();
        let (more_next, more_busy, more_page) = (self.more_next, self.more_busy, self.more_page);
        let sel_feed = sel.map(|f| f.feed.clone());
        let mut load_more = false;
        ui.scroll_area(id("fol-list"), area, |ui, view, y0| {
            let mut yy = y0 + 2.0;
            if checking {
                ui.spinner(x + 14.0, yy + 14.0, 7.0);
                ui.text("Checking AO3 for new works…", &TextStyle::ui(13.5), x + 30.0, yy + 5.0, c.muted);
                yy += 38.0;
            }
            if let Some(e) = sel_error.as_ref().or(error.as_ref()) {
                yy += error_box(ui, x, yy, w, "Couldn't check for new works", e) + 12.0;
            }
            if timeline.is_empty() && !checking {
                ui.text("No works yet. Use Refresh to load the newest works from AO3.", &TextStyle::ui(14.0), x + 4.0, yy + 8.0, c.muted);
                yy += 40.0;
            }
            let n = timeline.len() + extra.len();
            for i in 0..n {
                let r = Rect::new(x, yy + i as f32 * (CARD_H + GAP), w, CARD_H);
                if r.bottom() < view.y || r.y > view.bottom() {
                    continue;
                }
                if let Some((b, from, mark)) = timeline.get(i) {
                    let names: Vec<String> = if show_from { from.iter().map(|f| f.feed.name.clone()).collect() } else { Vec::new() };
                    blurb_card(ui, r, b, library.get_entry(&b.meta.id).is_some(), *mark, &names, follows, &mut out);
                } else if let Some(b) = extra.get(i - timeline.len()) {
                    blurb_card(ui, r, b, library.get_entry(&b.meta.id).is_some(), Mark::None, &[], follows, &mut out);
                }
            }
            yy += n as f32 * (CARD_H + GAP);
            if sel_feed.is_some() && n > 0 && more_next {
                if more_busy {
                    ui.spinner(x + w / 2.0 - 70.0, yy + 18.0, 7.0);
                    ui.text("Loading older works…", &TextStyle::ui(13.5), x + w / 2.0 - 54.0, yy + 9.0, c.muted);
                } else if ui.button(id("fol-more"), Rect::new(x + (w - 180.0) / 2.0, yy + 2.0, 180.0, 36.0), "Load older works") {
                    load_more = true;
                }
                yy += 48.0;
            }
            yy + 24.0 - y0
        });
        if load_more {
            if let Some(feed) = sel_feed {
                self.more_busy = true;
                out.push(BrowseAction::FeedPage(feed, more_page + 1));
            }
        }
        out
    }

    fn target(&self, kind: FeedKind) -> Target {
        self.selected.clone().map_or(Target::Kind(kind), Target::One)
    }

    fn select(&mut self, id_: Option<String>, ui: &mut Ui) {
        self.selected = id_;
        self.more.clear();
        self.more_page = 1;
        self.more_next = true;
        ui.scroll_to_top(id("fol-list"));
    }

    /// Shows the tab for a kind of feed (e.g. after following one).
    pub fn show_kind(&mut self, kind: FeedKind) {
        if let Some(i) = KINDS.iter().position(|k| k.0 == kind) {
            self.tab = i;
        }
    }

    /// Leaving the screen allows another automatic check on the next visit.
    pub fn left(&mut self) {
        self.auto_checked = false;
    }
}
