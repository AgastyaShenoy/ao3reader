//! Library screen (first cut): saved works, most recently read first, with progress and a
//! button to resume. Search, shelves, favourites, export and delete arrive in phase 4.

use super::gfx::*;
use super::icons;
use super::ui::*;
use crate::ao3::models::LibraryEntry;
use crate::storage::library::LibraryManager;

const ROW_H: f32 = 104.0;
const MAX_W: f32 = 920.0;

pub fn frame(ui: &mut Ui, body: Rect, library: &LibraryManager) -> Option<String> {
    let c = ui.c;
    let w = (body.w - 48.0).min(MAX_W);
    let x = body.x + (body.w - w) / 2.0;
    let mut entries: Vec<&LibraryEntry> = library.entries().values().collect();
    entries.sort_by(|a, b| b.progress.last_read.cmp(&a.progress.last_read));

    let title = ui.text("Library", &TextStyle::ui(26.0).semibold(), x, body.y + 22.0, c.text);
    let count = format!("{} works", entries.len());
    ui.text(&count, &TextStyle::ui(SMALL), x + title.w + 14.0, body.y + 22.0 + title.h - 18.0, c.muted);

    let top = body.y + 22.0 + title.h + 16.0;
    let area = Rect::new(body.x, top, body.w, (body.bottom() - top).max(0.0));
    if entries.is_empty() {
        let r = Rect::new(x, top + 8.0, w, 120.0);
        ui.card(r);
        let msg = "Your library is empty. Works you download appear here.";
        ui.text_in(msg, &TextStyle::ui(14.0), r, Align::Center, c.muted);
        return None;
    }

    let mut open = None;
    ui.scroll_area(id("library-list"), area, |ui, view, y0| {
        for (i, e) in entries.iter().enumerate() {
            let r = Rect::new(x, y0 + 2.0 + i as f32 * (ROW_H + 8.0), w, ROW_H);
            if r.bottom() < view.y || r.y > view.bottom() {
                continue;
            }
            ui.card(r);
            let started = e.progress.current_chapter > 1 || e.progress.percentage > 0.0 || e.progress.position > 0.0;
            let label = if started { "Resume" } else { "Read" };
            let bw = ui.button_width(label, Some(icons::READING)).max(110.0);
            let br = Rect::new(r.right() - 16.0 - bw, r.y + (r.h - 36.0) / 2.0, bw, 36.0);
            if ui.button_ex(id2(id("lib-open"), &e.meta.id), br, label, Some(icons::READING), ButtonKind::Primary, true) {
                open = Some(e.meta.id.clone());
            }
            let tw = br.x - r.x - 32.0;
            let rating = Rect::new(r.x + 16.0, r.y + 18.0, 6.0, 22.0);
            let [cr, cg, cb] = e.meta.rating.badge_color();
            ui.g.fill_round(rating, 3.0, Color::from_u8(cr, cg, cb));
            let title = ui.text_opts(&e.meta.title, &TextStyle::ui(16.0).semibold(), TextOpts::ellipsize(tw - 14.0), r.x + 30.0, r.y + 16.0, c.text);
            let mut byline = format!("by {}", e.meta.authors.join(", "));
            if let Some(f) = e.meta.fandoms.first() {
                byline.push_str(&format!("  ·  {f}"));
            }
            ui.text_opts(&byline, &TextStyle::ui(13.0), TextOpts::ellipsize(tw), r.x + 16.0, r.y + 22.0 + title.h, c.muted);

            let total = e.meta.stats.chapters_published.max(1);
            let pct = e.progress.percentage.clamp(0.0, 100.0);
            let status = if e.progress.completed {
                "Finished".to_string()
            } else if started {
                format!("Chapter {} of {}  ·  {:.0}%", e.progress.current_chapter, total, pct)
            } else {
                format!("{} chapters  ·  not started", total)
            };
            let st = ui.text(&status, &TextStyle::ui(12.5), r.x + 16.0, r.bottom() - 30.0, c.muted);
            let bar = Rect::new(r.x + 28.0 + st.w, r.bottom() - 23.0, (tw - st.w - 12.0).min(220.0).max(0.0), 4.0);
            ui.g.fill_round(bar, 2.0, c.border);
            ui.g.fill_round(Rect::new(bar.x, bar.y, bar.w * pct / 100.0, 4.0), 2.0, c.accent);
        }
        entries.len() as f32 * (ROW_H + 8.0) + 24.0
    });
    open
}
