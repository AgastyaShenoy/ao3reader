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

/// A tag pill. AO3's coloured markers (rating, category, warnings) keep their colour as a dot
/// on a quiet pill, so a card with several of them doesn't become a row of bright blocks.
pub enum Pill {
    Marker(String, Color),
    /// A relationship. Romantic ones ("A/B") get a heart on a rose tint, the one bit of colour a
    /// reader looks for first; platonic ones ("A & B") a people sign on a plain outline.
    Ship(String),
    Tag(String),
    Status(String),
}

/// AO3 writes romantic relationships with a slash and platonic ones with an ampersand.
fn romantic(tag: &str) -> bool {
    tag.contains('/')
}

const HEART: &str = "\u{EB52}";
const PEOPLE: &str = "\u{E716}";

/// Rating, categories, warnings (other than "none"), up to `rels` relationships and the
/// complete/in-progress status: the pills on a work card.
pub fn card_pills(m: &WorkMeta, rels: usize) -> Vec<Pill> {
    // Books from files have no AO3 rating or completion status: show only their real tags.
    let ao3 = m.is_ao3();
    let mut out = Vec::new();
    if ao3 {
        out.push(Pill::Marker(m.rating.display_name().to_string(), rating_color(m)));
    }
    for cat in &m.categories {
        out.push(Pill::Marker(cat.display_name().to_string(), category_color(cat)));
    }
    for wn in m.warnings.iter().filter(|w| !w.contains("No Archive Warnings")) {
        out.push(Pill::Marker(wn.clone(), WARNING_RED));
    }
    out.extend(m.relationships.iter().take(rels).map(|r| Pill::Ship(r.clone())));
    if ao3 {
        out.push(Pill::Status(if m.is_complete { "Complete" } else { "In progress" }.to_string()));
    }
    out
}

const WARNING_RED: Color = Color { r: 0.80, g: 0.22, b: 0.22, a: 1.0 };

/// Lays out pills left to right, wrapping at `w`. Returns the height used.
pub fn pills(ui: &mut Ui, x: f32, y: f32, w: f32, items: &[Pill]) -> f32 {
    let c = ui.c;
    let style = TextStyle::ui(12.5);
    let (mut cx, mut cy) = (x, y);
    let h = 24.0;
    for pill in items {
        let (label, lead) = match pill {
            Pill::Marker(l, _) => (l, 22.0),
            Pill::Ship(l) => (l, 27.0),
            Pill::Tag(l) | Pill::Status(l) => (l, 10.0),
        };
        let love = matches!(pill, Pill::Ship(l) if romantic(l));
        let pstyle = if love { style.clone().semibold() } else { style.clone() };
        let t = ui.measure(label, &pstyle, TextOpts::ellipsize(w - lead - 11.0));
        let pw = t.w + lead + 11.0;
        if cx > x && cx + pw > x + w {
            cx = x;
            cy += h + 6.0;
        }
        let r = Rect::new(cx, cy, pw, h);
        match pill {
            Pill::Marker(_, col) => {
                ui.g.fill_round(r, h / 2.0, c.text.alpha(0.07));
                ui.g.fill_circle(r.x + 12.0, r.y + h / 2.0, 3.5, *col);
            }
            Pill::Ship(_) => {
                if love {
                    ui.g.fill_round(r, h / 2.0, c.romance.alpha(0.14));
                    ui.g.stroke_round(r, h / 2.0, 1.0, c.romance.alpha(0.45));
                } else {
                    ui.g.stroke_round(r, h / 2.0, 1.0, c.border);
                }
                let it = ui.measure(if love { HEART } else { PEOPLE }, &TextStyle::icons(11.0), TextOpts::default());
                ui.g.draw_text(it, r.x + 10.0, r.y + (h - it.h) / 2.0, if love { c.romance } else { c.muted });
            }
            _ => ui.g.stroke_round(r, h / 2.0, 1.0, c.border),
        }
        let fg = if matches!(pill, Pill::Status(_)) { c.muted } else { c.text };
        ui.g.draw_text(t, r.x + lead, r.y + (h - t.h) / 2.0, fg);
        cx += pw + 6.0;
    }
    if items.is_empty() { 0.0 } else { cy + h - y }
}

/// Draws the work header and returns its height. With `page_h`, the summary is
/// clamped so the header fits on one page.
pub fn draw(ui: &mut Ui, m: &WorkMeta, x: f32, y: f32, w: f32, page_h: Option<f32>) -> f32 {
    let c = ui.c;
    let mut cy = y + 8.0;

    let t = ui.measure(&m.title, &TextStyle::ui(28.0).semibold(), TextOpts::wrap(w).line_height(1.25));
    ui.g.draw_text(t, x, cy, c.text);
    cy += t.h + 6.0;
    let by = format!("by {}", m.authors.join(", "));
    let t = ui.measure(&by, &TextStyle::ui(15.0), TextOpts::wrap(w));
    ui.g.draw_text(t, x, cy, c.muted);
    cy += t.h + 16.0;

    let mut badges = Vec::new();
    if m.is_ao3() {
        badges.push(Pill::Marker(m.rating.display_name().to_string(), rating_color(m)));
    }
    for cat in &m.categories {
        badges.push(Pill::Marker(cat.display_name().to_string(), category_color(cat)));
    }
    for wn in &m.warnings {
        let col = if wn.contains("No Archive Warnings") { Color::from_u8(34, 160, 84) } else { WARNING_RED };
        badges.push(Pill::Marker(wn.clone(), col));
    }
    if m.is_complete && m.is_ao3() {
        badges.push(Pill::Status("Complete".into()));
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
        let rel: Vec<_> = m.relationships.iter().take(8).map(|r| Pill::Ship(r.clone())).collect();
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
        ui.g.draw_text(label, x + 18.0, cy + 14.0, c.accent_text);
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
