//! Library screen: search, shelf filters, sorting, and a card per work with progress,
//! favourite, shelf, export and delete.

use super::gfx::*;
use super::icons;
use super::reader::preface::thousands;
use super::ui::*;
use crate::ao3::models::{LibraryEntry, ShelfStatus};
use crate::storage::library::LibraryManager;

const CARD_H: f32 = 192.0;
const GAP: f32 = 10.0;
const MAX_W: f32 = 960.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Html,
    Markdown,
    Text,
}

pub enum LibraryAction {
    Open(String),
    ToggleFavorite(String),
    SetShelf(String, ShelfStatus),
    Export(String, ExportFormat),
    Delete(String),
    GoSearch,
    GoImport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sort {
    Recent,
    Title,
    Added,
    Author,
}

const SORTS: [(Sort, &str); 4] = [(Sort::Recent, "Recently read"), (Sort::Title, "Title"), (Sort::Added, "Recently added"), (Sort::Author, "Author")];
const SHELVES: [(ShelfStatus, &str); 4] = [
    (ShelfStatus::Reading, "Reading"),
    (ShelfStatus::Completed, "Completed"),
    (ShelfStatus::MarkedForLater, "For later"),
    (ShelfStatus::Favorites, "Favorites"),
];

pub struct LibraryScreen {
    query: String,
    filter: usize,
    sort: Sort,
    confirm_delete: Option<String>,
}

impl Default for LibraryScreen {
    fn default() -> Self {
        Self { query: String::new(), filter: 0, sort: Sort::Recent, confirm_delete: None }
    }
}

fn shelf_label(s: ShelfStatus) -> &'static str {
    SHELVES.iter().find(|(x, _)| *x == s).map_or("Reading", |(_, l)| l)
}

/// Filter 0 = all, 1..=3 = shelves, 4 = favourites (the heart or the Favorites shelf).
fn in_filter(e: &LibraryEntry, filter: usize) -> bool {
    match filter {
        0 => true,
        1 => e.shelf == ShelfStatus::Reading,
        2 => e.shelf == ShelfStatus::Completed,
        3 => e.shelf == ShelfStatus::MarkedForLater,
        _ => e.is_favorite || e.shelf == ShelfStatus::Favorites,
    }
}

impl LibraryScreen {
    pub fn frame(&mut self, ui: &mut Ui, body: Rect, library: &LibraryManager) -> Vec<LibraryAction> {
        let mut out = Vec::new();
        let c = ui.c;
        let w = (body.w - 48.0).min(MAX_W);
        let x = body.x + (body.w - w) / 2.0;
        let all: Vec<&LibraryEntry> = library.entries().values().collect();

        let title = ui.text("Library", &TextStyle::ui(26.0).semibold(), x, body.y + 22.0, c.text);
        let search_w = (w * 0.38).clamp(200.0, 320.0);
        let search = Rect::new(x + w - search_w, body.y + 22.0, search_w, 36.0);
        let it = ui.measure(icons::SEARCH, &TextStyle::icons(14.0), TextOpts::default());
        ui.g.draw_text(it, search.x - 24.0, search.y + (36.0 - it.h) / 2.0, c.muted);
        ui.text_input(id("lib-search"), search, &mut self.query, "Search title, author, fandom, tag", false);

        // Filters, each with a count badge (none for an empty shelf), and sorting.
        let y = body.y + 22.0 + title.h + 14.0;
        let names = ["All", "Reading", "Completed", "For later", "Favorites"];
        let items: Vec<(&str, usize)> = names.iter().enumerate().map(|(f, n)| (*n, all.iter().filter(|e| in_filter(e, f)).count())).collect();
        let sort_w = 170.0;
        let seg_w = (w - sort_w - 12.0).min(660.0);
        if ui.segmented_counts(id("lib-filter"), Rect::new(x, y, seg_w, 34.0), &items, &mut self.filter, Badge::Count) {
            ui.scroll_to_top(id("lib-list"));
        }
        let sort_label = SORTS.iter().find(|(s, _)| *s == self.sort).map_or("", |(_, l)| l);
        let items: Vec<(&str, bool)> = SORTS.iter().map(|(_, l)| (*l, false)).collect();
        let current = SORTS.iter().position(|(s, _)| *s == self.sort);
        if let Some(i) = ui.menu_button(id("lib-sort"), Rect::new(x + w - sort_w, y, sort_w, 34.0), sort_label, Some("\u{E8CB}"), &items, current) {
            self.sort = SORTS[i].0;
        }

        let mut list: Vec<&LibraryEntry> = library.query_entries(&self.query, None, false).into_iter().filter(|e| in_filter(e, self.filter)).collect();
        match self.sort {
            Sort::Recent => {}
            Sort::Title => list.sort_by_key(|e| e.meta.title.to_lowercase()),
            Sort::Added => list.sort_by(|a, b| b.downloaded_at.cmp(&a.downloaded_at)),
            Sort::Author => list.sort_by_key(|e| e.meta.authors.first().map(|a| a.to_lowercase()).unwrap_or_default()),
        }

        let top = y + 50.0;
        let area = Rect::new(body.x, top, body.w, (body.bottom() - top).max(0.0));
        if list.is_empty() {
            let r = Rect::new(x, top + 4.0, w, 150.0);
            ui.card(r);
            let (msg, empty) = if all.is_empty() {
                ("Your library is empty. Search AO3 or paste a link to a work to start reading.", true)
            } else {
                ("No works match this search or filter.", false)
            };
            ui.text_in(msg, &TextStyle::ui(14.0), Rect::new(r.x, r.y + 30.0, r.w, 24.0), Align::Center, c.muted);
            if empty {
                let sw = ui.button_width("Search AO3", Some(icons::SEARCH));
                let iw = ui.button_width("Import a file", Some(icons::OPEN_FILE));
                let bx = r.x + (r.w - sw - 8.0 - iw) / 2.0;
                if ui.button_ex(id("lib-empty-search"), Rect::new(bx, r.y + 76.0, sw, 36.0), "Search AO3", Some(icons::SEARCH), ButtonKind::Primary, true) {
                    out.push(LibraryAction::GoSearch);
                }
                if ui.button_ex(id("lib-empty-import"), Rect::new(bx + sw + 8.0, r.y + 76.0, iw, 36.0), "Import a file", Some(icons::OPEN_FILE), ButtonKind::Secondary, true) {
                    out.push(LibraryAction::GoImport);
                }
            }
            return out;
        }

        let confirm = self.confirm_delete.clone();
        let mut set_confirm: Option<Option<String>> = None;
        ui.scroll_area(id("lib-list"), area, |ui, view, y0| {
            for (i, e) in list.iter().enumerate() {
                let r = Rect::new(x, y0 + 2.0 + i as f32 * (CARD_H + GAP), w, CARD_H);
                if r.bottom() < view.y || r.y > view.bottom() {
                    continue;
                }
                let confirming = confirm.as_deref() == Some(e.meta.id.as_str());
                if let Some(c) = card(ui, r, e, confirming, &mut out) {
                    set_confirm = Some(c);
                }
            }
            list.len() as f32 * (CARD_H + GAP) + 24.0
        });
        if let Some(c) = set_confirm {
            self.confirm_delete = c;
        }
        out
    }
}

/// Draws one work card. Returns a change to the delete-confirmation state.
fn card(ui: &mut Ui, r: Rect, e: &LibraryEntry, confirming: bool, out: &mut Vec<LibraryAction>) -> Option<Option<String>> {
    let c = ui.c;
    let m = &e.meta;
    let wid = &m.id;
    ui.card(r);
    let mut result = None;

    // Title row with favourite toggle.
    let heart = Rect::new(r.right() - 48.0, r.y + 10.0, 36.0, 34.0);
    let heart_icon = if e.is_favorite { "\u{EB52}" } else { "\u{EB51}" };
    if ui.icon_button(id2(id("lib-fav"), wid), heart, heart_icon, false) {
        out.push(LibraryAction::ToggleFavorite(wid.clone()));
    }
    if e.is_favorite {
        let t = ui.measure("\u{EB52}", &TextStyle::icons(15.0), TextOpts::default());
        ui.g.draw_text(t, heart.x + (heart.w - t.w) / 2.0, heart.y + (heart.h - t.h) / 2.0, Color::from_u8(225, 29, 72));
    }
    let tw = r.w - 92.0;
    let title_r = Rect::new(r.x + 16.0, r.y + 14.0, tw, 28.0);
    let tresp = ui.interact(id2(id("lib-title"), wid), title_r);
    let tt = ui.text_opts(&m.title, &TextStyle::ui(17.0).semibold(), TextOpts::ellipsize(tw), title_r.x, title_r.y, c.text);
    if tresp.hovered {
        ui.g.fill_rect(Rect::new(title_r.x, title_r.y + tt.h - 1.0, tt.w, 1.0), c.text.alpha(0.6));
    }
    if tresp.clicked {
        out.push(LibraryAction::Open(wid.clone()));
    }
    let mut byline = format!("by {}", m.authors.join(", "));
    if !m.fandoms.is_empty() {
        byline.push_str(&format!("  ·  {}", m.fandoms.iter().take(3).cloned().collect::<Vec<_>>().join(", ")));
    }
    ui.text_opts(&byline, &TextStyle::ui(13.0), TextOpts::ellipsize(tw), r.x + 16.0, r.y + 44.0, c.muted);

    // Tag pills on one row, clipped to the card.
    let pills = super::reader::preface::card_pills(m, 3);
    ui.g.push_clip(Rect::new(r.x + 16.0, r.y + 66.0, r.w - 32.0, 26.0));
    super::reader::preface::pills(ui, r.x + 16.0, r.y + 67.0, 10_000.0, &pills);
    ui.g.pop_clip();

    let summary = m.summary_text.split_whitespace().collect::<Vec<_>>().join(" ");
    ui.text_opts(&summary, &TextStyle::ui(13.0), TextOpts::wrap(r.w - 32.0).max_lines(2).line_height(1.35), r.x + 16.0, r.y + 98.0, c.text.alpha(0.82));

    // Bottom row: progress and actions, or the delete confirmation.
    let by = r.bottom() - 46.0;
    if confirming {
        ui.g.fill_round(Rect::new(r.x + 1.0, by - 4.0, r.w - 2.0, 49.0), 0.0, c.danger.alpha(0.08));
        ui.text_in("Remove this work and its downloaded text?", &TextStyle::ui(13.5), Rect::new(r.x + 16.0, by, r.w - 260.0, 36.0), Align::Left, c.text);
        if ui.button(id2(id("lib-del-no"), wid), Rect::new(r.right() - 220.0, by, 96.0, 34.0), "Cancel") {
            result = Some(None);
        }
        if ui.button_ex(id2(id("lib-del-yes"), wid), Rect::new(r.right() - 116.0, by, 100.0, 34.0), "Delete", Some(icons::DELETE), ButtonKind::Danger, true) {
            out.push(LibraryAction::Delete(wid.clone()));
            result = Some(None);
        }
        return result;
    }

    let p = &e.progress;
    let started = p.current_chapter > 1 || p.percentage > 0.0 || p.position > 0.0;
    let label = if p.completed { "Read again" } else if started { "Resume" } else { "Read" };
    let open_w = ui.button_width(label, Some(icons::READING)).max(108.0);
    let more = Rect::new(r.right() - 16.0 - 36.0, by, 36.0, 34.0);
    let shelf_w = 140.0;
    let shelf_r = Rect::new(more.x - 8.0 - shelf_w, by, shelf_w, 34.0);
    let open_r = Rect::new(shelf_r.x - 8.0 - open_w, by, open_w, 34.0);
    if ui.button_ex(id2(id("lib-open"), wid), open_r, label, Some(icons::READING), ButtonKind::Tonal, true) {
        out.push(LibraryAction::Open(wid.clone()));
    }
    let shelf_items: Vec<(&str, bool)> = SHELVES.iter().map(|(_, l)| (*l, false)).collect();
    let current = SHELVES.iter().position(|(s, _)| *s == e.shelf);
    if let Some(i) = ui.menu_button(id2(id("lib-shelf"), wid), shelf_r, shelf_label(e.shelf), Some("\u{E8F1}"), &shelf_items, current) {
        out.push(LibraryAction::SetShelf(wid.clone(), SHELVES[i].0));
    }
    let more_items = [("Export as HTML", false), ("Export as Markdown", false), ("Export as text", false), ("Delete from library", true)];
    match ui.menu_button(id2(id("lib-more"), wid), more, "", Some("\u{E712}"), &more_items, None) {
        Some(0) => out.push(LibraryAction::Export(wid.clone(), ExportFormat::Html)),
        Some(1) => out.push(LibraryAction::Export(wid.clone(), ExportFormat::Markdown)),
        Some(2) => out.push(LibraryAction::Export(wid.clone(), ExportFormat::Text)),
        Some(3) => result = Some(Some(wid.clone())),
        _ => {}
    }

    let total_ch = m.stats.chapters_published.max(1);
    let status = if p.completed {
        "Finished".to_string()
    } else if started {
        format!("Chapter {} of {}  ·  {:.0}%", p.current_chapter, total_ch, p.percentage)
    } else {
        format!("{total_ch} chapters  ·  not started")
    };
    let left_w = (open_r.x - r.x - 32.0).max(0.0);
    let st = ui.text_opts(&status, &TextStyle::ui(12.5), TextOpts::ellipsize(left_w), r.x + 16.0, by + 2.0, c.text.alpha(0.85));
    let mut stats = format!("{} words", thousands(m.stats.words as u64));
    if m.stats.kudos > 0 {
        stats.push_str(&format!("  ·  {} kudos", thousands(m.stats.kudos)));
    }
    ui.text_opts(&stats, &TextStyle::ui(12.0), TextOpts::ellipsize(left_w), r.x + 16.0, by + 22.0, c.muted);
    let bar_x = r.x + 26.0 + st.w;
    let bar_w = (open_r.x - 16.0 - bar_x).min(180.0);
    if bar_w > 30.0 {
        let bar = Rect::new(bar_x, by + 9.0, bar_w, 4.0);
        ui.g.fill_round(bar, 2.0, c.border);
        ui.g.fill_round(Rect::new(bar.x, bar.y, bar.w * (p.percentage / 100.0).clamp(0.0, 1.0), 4.0), 2.0, c.accent_mark);
    }
    result
}
