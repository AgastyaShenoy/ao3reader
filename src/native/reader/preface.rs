//! The work header shown before chapter 1: title, author, tags and summary.

use crate::ao3::models::{Category, WorkMeta};
use crate::native::gfx::*;
use crate::native::ui::Ui;

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn category_color(c: &Category) -> Color {
    match c {
        Category::MM => Color::from_u8(46, 117, 182),
        Category::FM => Color::from_u8(155, 89, 182),
        Category::FF => Color::from_u8(231, 76, 60),
        Category::Gen => Color::from_u8(39, 174, 96),
        Category::Multi => Color::from_u8(230, 126, 34),
        _ => Color::from_u8(110, 119, 129),
    }
}

pub fn rating_color(meta: &WorkMeta) -> Color {
    let [r, g, b] = meta.rating.badge_color();
    Color::from_u8(r, g, b)
}

/// Lays out pills left to right, wrapping at `w`. Returns the height used.
pub fn pills(ui: &mut Ui, x: f32, y: f32, w: f32, items: &[(String, Color, Color, bool)]) -> f32 {
    let style = TextStyle::ui(12.5);
    let (mut cx, mut cy) = (x, y);
    let h = 24.0;
    for (label, fill, fg, outlined) in items {
        let t = ui.measure(label, &style, TextOpts::ellipsize(w - 20.0));
        let pw = t.w + 20.0;
        if cx > x && cx + pw > x + w {
            cx = x;
            cy += h + 6.0;
        }
        let r = Rect::new(cx, cy, pw, h);
        if *outlined {
            ui.g.stroke_round(r, h / 2.0, 1.0, *fill);
        } else {
            ui.g.fill_round(r, h / 2.0, *fill);
        }
        ui.g.draw_text(t, r.x + 10.0, r.y + (h - t.h) / 2.0, *fg);
        cx += pw + 6.0;
    }
    if items.is_empty() { 0.0 } else { cy + h - y }
}

/// Draws the work header and returns its height. With `page_h`, the summary is
/// clamped so the header fits on one page.
pub fn draw(ui: &mut Ui, m: &WorkMeta, x: f32, y: f32, w: f32, page_h: Option<f32>) -> f32 {
    let c = ui.c;
    let white = Color::from_u8(255, 255, 255);
    let mut cy = y + 8.0;

    let t = ui.measure(&m.title, &TextStyle::ui(28.0).semibold(), TextOpts::wrap(w).line_height(1.25));
    ui.g.draw_text(t, x, cy, c.text);
    cy += t.h + 6.0;
    let by = format!("by {}", m.authors.join(", "));
    let t = ui.measure(&by, &TextStyle::ui(15.0), TextOpts::wrap(w));
    ui.g.draw_text(t, x, cy, c.muted);
    cy += t.h + 16.0;

    let mut badges: Vec<(String, Color, Color, bool)> = vec![(m.rating.display_name().to_string(), rating_color(m), white, false)];
    for cat in &m.categories {
        badges.push((cat.display_name().to_string(), category_color(cat), white, false));
    }
    for wn in &m.warnings {
        let green = wn.contains("No Archive Warnings");
        let col = if green { Color::from_u8(34, 139, 34) } else { Color::from_u8(180, 40, 40) };
        badges.push((wn.clone(), col, white, false));
    }
    if m.is_complete {
        badges.push(("Complete".into(), c.border, c.text, true));
    }
    cy += pills(ui, x, cy, w, &badges) + 14.0;

    let line = |ui: &mut Ui, cy: &mut f32, label: &str, items: &[String]| {
        if items.is_empty() {
            return;
        }
        let text = format!("{label}  {}", items.join(" · "));
        let t = ui.measure(&text, &TextStyle::ui(13.5), TextOpts::wrap(w).line_height(1.45).max_lines(3));
        ui.g.draw_text(t, x, *cy, c.muted);
        *cy += t.h + 8.0;
    };
    line(ui, &mut cy, "Fandoms", &m.fandoms);
    if !m.relationships.is_empty() {
        let rel: Vec<_> = m.relationships.iter().take(8).map(|r| (r.clone(), c.accent, c.text, true)).collect();
        cy += pills(ui, x, cy, w, &rel) + 10.0;
    }
    line(ui, &mut cy, "Characters", &m.characters);
    line(ui, &mut cy, "Tags", &m.freeform_tags);

    if !m.summary_text.trim().is_empty() {
        cy += 8.0;
        let label = ui.measure("Summary", &TextStyle::ui(12.5).semibold(), TextOpts::default());
        let max_lines = page_h.map(|ph| (((ph - (cy - y) - 140.0) / (15.0 * 1.5)).floor().max(3.0)) as u32);
        let mut opts = TextOpts::wrap(w - 36.0).line_height(1.5);
        opts.max_lines = max_lines;
        let t = ui.measure(m.summary_text.trim(), &TextStyle::ui(15.0), opts);
        let card = Rect::new(x, cy, w, t.h + label.h + 44.0);
        ui.g.fill_round(card, 8.0, c.notes_bg);
        ui.g.stroke_round(card, 8.0, 1.0, c.border);
        ui.g.draw_text(label, x + 18.0, cy + 14.0, c.accent);
        ui.g.draw_text(t, x + 18.0, cy + 22.0 + label.h, c.text);
        cy += card.h + 14.0;
    }

    let s = &m.stats;
    let chapters = format!("{}/{}", s.chapters_published, s.chapters_total.map_or("?".into(), |n| n.to_string()));
    let mut stats = vec![format!("{} words", thousands(s.words as u64)), format!("Chapters {chapters}")];
    if s.kudos > 0 {
        stats.push(format!("{} kudos", thousands(s.kudos)));
    }
    if s.bookmarks > 0 {
        stats.push(format!("{} bookmarks", thousands(s.bookmarks)));
    }
    if s.hits > 0 {
        stats.push(format!("{} hits", thousands(s.hits)));
    }
    if !s.published_date.is_empty() {
        stats.push(format!("Published {}", s.published_date));
    }
    let t = ui.measure(&stats.join("  ·  "), &TextStyle::ui(12.5), TextOpts::wrap(w));
    ui.g.draw_text(t, x, cy, c.muted);
    cy += t.h + 18.0;
    ui.separator(x, cy, w);
    cy + 1.0 - y
}

#[cfg(test)]
mod tests {
    use super::thousands;

    #[test]
    fn thousands_separators() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1234567), "1,234,567");
    }
}
