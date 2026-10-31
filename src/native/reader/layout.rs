//! Chapter layout: every block of a chapter is measured with DirectWrite once per
//! (chapter, width, typography) and kept as heights and line positions only. Pages are cut
//! at real line boundaries, so a paragraph continues on the next page exactly where it
//! stopped, and positions are stored as paragraph anchors that survive any re-layout.

use super::preface;
use crate::ao3::models::Work;
use crate::native::gfx::*;
use crate::native::ui::Ui;
use crate::reader::rich_text::{image_source, paragraph_display, parse_inline, table_cells, with_prefix, with_word_starts, ParagraphKind, RichText};
use crate::storage::library::LibraryManager;
use std::path::PathBuf;
pub use crate::reader::rich_text::{is_landmark, INDENT};
use crate::reader::typography::{TextAlignment, TypographySettings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Preface,
    Title,
    Summary,
    PreNotes,
    Para,
    Quote,
    Divider,
    Heading,
    ListItem,
    Pre,
    Center,
    Image,
    TableRow,
    EndNotes,
    End,
}

impl Kind {
    /// Blocks that stand for a paragraph of the text (and so carry reading positions).
    pub fn is_body(self) -> bool {
        matches!(self, Kind::Para | Kind::Quote | Kind::Divider | Kind::Heading | Kind::ListItem | Kind::Pre | Kind::Center | Kind::Image | Kind::TableRow)
    }

    /// Blocks whose text search highlights are drawn on.
    pub fn searchable(self) -> bool {
        matches!(self, Kind::Para | Kind::Quote | Kind::Heading | Kind::ListItem | Kind::Pre | Kind::Center)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleKind {
    Body,
    Small,
    Title,
    /// Heading levels 1, 2 and 3+ (index 0–2).
    Heading(u8),
    Mono,
    Center,
}

/// What a block draws besides its text.
pub enum Extra {
    None,
    /// List marker ("•", "3.") drawn left of the text.
    Marker(String),
    /// A picture file; None when it's missing or unreadable (its description is shown).
    Image(Option<PathBuf>),
    /// A table row: its cells, the column width, and whether it's a header row.
    Row(Vec<RichText>, f32, bool),
}

pub struct Block {
    pub kind: Kind,
    /// Paragraph this block belongs to (blocks before the first paragraph use 0,
    /// blocks after the last use the paragraph count).
    pub para: usize,
    pub rich: RichText,
    pub style: StyleKind,
    /// Top of the block in the chapter's continuous column.
    pub y: f32,
    pub h: f32,
    /// Text origin relative to the block's top-left.
    pub text_x: f32,
    pub text_y: f32,
    pub text_w: f32,
    pub text_h: f32,
    /// Bottom of each text line, in block coordinates (empty for unsplittable blocks).
    pub lines: Vec<f32>,
    pub extra: Extra,
}

#[derive(Debug, Clone, Copy)]
pub struct Slice {
    pub block: usize,
    pub from: f32,
    pub to: f32,
}

pub struct Page {
    pub slices: Vec<Slice>,
    /// Continuous-column y where the page starts.
    pub y: f32,
}

pub struct Styles {
    pub body: TextStyle,
    pub body_opts: TextOpts,
    pub small: TextStyle,
    pub small_opts: TextOpts,
    pub title: TextStyle,
    pub title_opts: TextOpts,
    pub headings: [TextStyle; 3],
    pub heading_opts: TextOpts,
    pub mono: TextStyle,
    pub mono_opts: TextOpts,
    pub center_opts: TextOpts,
}

pub struct ChapterLayout {
    pub key: u64,
    pub page_h: Option<f32>,
    pub chapter: usize,
    pub width: f32,
    pub blocks: Vec<Block>,
    pub total_h: f32,
    pub pages: Vec<Page>,
    pub styles: Styles,
    pub para_count: usize,
}

/// Display text of a stored paragraph: `None` for scene dividers and undescribed pictures.
pub fn paragraph_rich(p: &str) -> Option<(Kind, RichText)> {
    let (kind, rich) = paragraph_display(p);
    let kind = match kind {
        ParagraphKind::Divider => return None,
        ParagraphKind::Image if rich.text.is_empty() => return None,
        ParagraphKind::Quote => Kind::Quote,
        ParagraphKind::Heading(_) => Kind::Heading,
        ParagraphKind::Bullet(_) | ParagraphKind::Numbered(..) => Kind::ListItem,
        ParagraphKind::Pre => Kind::Pre,
        ParagraphKind::Center => Kind::Center,
        ParagraphKind::Image => Kind::Image,
        ParagraphKind::TableRow(_) => Kind::TableRow,
        ParagraphKind::Text => Kind::Para,
    };
    Some((kind, rich))
}

/// Words per paragraph of a chapter, after inline markers are removed.
pub fn paragraph_words(work: &Work, chapter: usize) -> Vec<usize> {
    work.chapters.get(chapter).map(|c| c.paragraphs.iter().map(|p| crate::reader::rich_text::paragraph_words(p)).collect()).unwrap_or_default()
}

pub fn styles(t: &TypographySettings, width: f32, family: Family) -> Styles {
    let body = TextStyle { family: family.clone(), ..TextStyle::ui(t.font_size) };
    let align = if t.text_alignment == TextAlignment::Justified { Align::Justify } else { Align::Left };
    // Letter and word spacing are fractions of the text size.
    let (letter, word) = (t.letter_spacing.clamp(0.0, 0.5) * t.font_size, t.word_spacing.clamp(0.0, 2.0) * t.font_size);
    let body_opts = TextOpts::wrap(width).align(align).line_height(t.line_spacing_multiplier).spacing(letter, word);
    let small = TextStyle { family: family.clone(), ..TextStyle::ui((t.font_size * 0.9).max(12.0)) };
    let small_opts = TextOpts::wrap(width - 36.0).line_height(t.line_spacing_multiplier.min(1.7)).spacing(letter * 0.9, word * 0.9);
    let title = TextStyle { family: family.clone(), ..TextStyle::ui(t.font_size * 1.5) }.semibold();
    let title_opts = TextOpts::wrap(width).align(Align::Center).line_height(1.3);
    let heading = |scale: f32| TextStyle { family: family.clone(), ..TextStyle::ui(t.font_size * scale) }.semibold();
    let headings = [heading(1.4), heading(1.22), heading(1.08)];
    let heading_opts = TextOpts::wrap(width).line_height(1.3).spacing(letter, word);
    let mono = TextStyle { family: Family::Mono, ..TextStyle::ui((t.font_size * 0.86).max(12.0)) };
    let mono_opts = TextOpts::wrap(width).line_height(1.4);
    let center_opts = TextOpts { align: Align::Center, ..body_opts };
    Styles { body, body_opts, small, small_opts, title, title_opts, headings, heading_opts, mono, mono_opts, center_opts }
}

impl Styles {
    pub fn get(&self, k: StyleKind) -> (&TextStyle, TextOpts) {
        match k {
            StyleKind::Body => (&self.body, self.body_opts),
            StyleKind::Small => (&self.small, self.small_opts),
            StyleKind::Title => (&self.title, self.title_opts),
            StyleKind::Heading(i) => (&self.headings[(i as usize).min(2)], self.heading_opts),
            StyleKind::Mono => (&self.mono, self.mono_opts),
            StyleKind::Center => (&self.body, self.center_opts),
        }
    }
}

/// Note-like cards (summary, notes, block quotes) indent their text by this much.
pub const CARD_PAD: f32 = 18.0;
pub const LABEL_H: f32 = 26.0;
pub const END_H: f32 = 132.0;

impl ChapterLayout {
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        ui: &mut Ui,
        key: u64,
        work: &Work,
        chapter: usize,
        t: &TypographySettings,
        family: Family,
        width: f32,
        page_h: Option<f32>,
    ) -> Self {
        let styles = styles(t, width, family);
        let mut blocks: Vec<Block> = Vec::new();
        let ch = &work.chapters[chapter];
        let para_count = ch.paragraphs.len();

        // `inset` is (left, right).
        let push_text = |ui: &mut Ui, blocks: &mut Vec<Block>, kind: Kind, para: usize, rich: RichText, style: StyleKind, inset: (f32, f32), top: f32, bottom: f32, extra: Extra| {
            let (ts, opts) = styles.get(style);
            let text_w = (width - inset.0 - inset.1).max(40.0);
            let opts = TextOpts { wrap: Some(text_w), ..opts };
            let (text_h, lines) = ui.g.measure_lines(&rich.text, &rich.spans, ts, opts);
            let text_y = top;
            let h = top + text_h + bottom;
            blocks.push(Block {
                kind,
                para,
                rich,
                style,
                y: 0.0,
                h,
                text_x: inset.0,
                text_y,
                text_w,
                text_h,
                lines: lines.into_iter().map(|l| l + text_y).collect(),
                extra,
            });
        };

        if chapter == 0 {
            let h = ui.measure_only(|ui| preface::draw(ui, &work.meta, 0.0, 0.0, width, page_h));
            blocks.push(Block {
                kind: Kind::Preface,
                para: 0,
                rich: RichText::default(),
                style: StyleKind::Body,
                y: 0.0,
                h: h + 36.0,
                text_x: 0.0,
                text_y: 0.0,
                text_w: width,
                text_h: h,
                lines: Vec::new(),
                extra: Extra::None,
            });
        }
        // A page that's only pictures (a cover, a plate) shows them without a chapter title.
        let pictures_only = !ch.paragraphs.is_empty() && ch.paragraphs.iter().all(|p| image_source(p).is_some());
        if work.chapters.len() > 1 && !ch.title.trim().is_empty() && !pictures_only {
            push_text(ui, &mut blocks, Kind::Title, 0, RichText { text: ch.title.trim().to_string(), spans: Vec::new() }, StyleKind::Title, (0.0, 0.0), 8.0, 28.0, Extra::None);
        }
        let card = |label_top: bool| if label_top { CARD_PAD + LABEL_H } else { CARD_PAD };
        if t.show_chapter_summary {
            if let Some(s) = ch.summary.as_deref().filter(|s| !s.trim().is_empty()) {
                push_text(ui, &mut blocks, Kind::Summary, 0, parse_inline(s.trim()), StyleKind::Small, (CARD_PAD, CARD_PAD), card(true), CARD_PAD + 20.0, Extra::None);
            }
        }
        if t.show_chapter_pre_notes {
            if let Some(s) = ch.pre_notes.as_deref().filter(|s| !s.trim().is_empty()) {
                push_text(ui, &mut blocks, Kind::PreNotes, 0, parse_inline(s.trim()), StyleKind::Small, (CARD_PAD, CARD_PAD), card(true), CARD_PAD + 20.0, Extra::None);
            }
        }
        let indent = if t.indent_paragraphs { INDENT } else { "" };
        let gap = t.paragraph_spacing;
        let shown: Vec<(ParagraphKind, RichText)> = ch.paragraphs.iter().map(|p| paragraph_display(p)).collect();
        // Pictures are never taller than a page (or, when scrolling, a tall screenful).
        let max_image_h = page_h.map_or(width * 1.4, |ph| ph - 8.0).max(80.0);
        let mut table_cols = 0usize;
        for (i, p) in ch.paragraphs.iter().enumerate() {
            if is_landmark(i, p) {
                continue;
            }
            let (kind, rich) = shown[i].clone();
            let next = shown.get(i + 1).map(|(k, _)| *k);
            match kind {
                ParagraphKind::Divider => {
                    let h = t.font_size * 2.6 + gap;
                    blocks.push(Block {
                        kind: Kind::Divider,
                        para: i,
                        rich: RichText::default(),
                        style: StyleKind::Body,
                        y: 0.0,
                        h,
                        text_x: 0.0,
                        text_y: 0.0,
                        text_w: width,
                        text_h: h - gap,
                        lines: Vec::new(),
                        extra: Extra::None,
                    });
                }
                ParagraphKind::Quote => push_text(ui, &mut blocks, Kind::Quote, i, with_word_starts(rich, t.bold_word_starts), StyleKind::Body, (26.0, 26.0), 12.0, 12.0 + gap, Extra::None),
                ParagraphKind::Heading(level) => {
                    let first = blocks.last().is_none_or(|b| !b.kind.is_body());
                    let top = if first { 0.0 } else { gap * 0.6 + t.font_size * 0.6 };
                    push_text(ui, &mut blocks, Kind::Heading, i, rich, StyleKind::Heading(level.saturating_sub(1)), (0.0, 0.0), top, gap * 0.7, Extra::None);
                }
                ParagraphKind::Bullet(depth) | ParagraphKind::Numbered(depth, _) => {
                    let marker = match kind {
                        ParagraphKind::Numbered(_, n) => format!("{n}."),
                        _ => ["\u{2022}", "\u{25E6}", "\u{25AA}"][(depth as usize - 1).min(2)].to_string(),
                    };
                    let more = matches!(next, Some(ParagraphKind::Bullet(_) | ParagraphKind::Numbered(..)));
                    let left = 30.0 * depth as f32;
                    push_text(ui, &mut blocks, Kind::ListItem, i, with_word_starts(rich, t.bold_word_starts), StyleKind::Body, (left, 0.0), 0.0, if more { gap * 0.35 } else { gap }, Extra::Marker(marker));
                }
                ParagraphKind::Pre => push_text(ui, &mut blocks, Kind::Pre, i, rich, StyleKind::Mono, (16.0, 16.0), 12.0, 12.0 + gap, Extra::None),
                ParagraphKind::Center => push_text(ui, &mut blocks, Kind::Center, i, with_word_starts(rich, t.bold_word_starts), StyleKind::Center, (0.0, 0.0), 0.0, gap, Extra::None),
                ParagraphKind::Image => {
                    let path = image_source(p).map(|(name, _)| LibraryManager::resource_path(&work.meta.id, name));
                    match path.as_deref().and_then(|f| ui.g.image_size(f)) {
                        Some((iw, ih)) => {
                            // Small pictures keep their size; large ones shrink to the column
                            // (and to the page height).
                            let mut w = (iw as f32).min(width);
                            let mut h = ih as f32 * w / iw as f32;
                            if h > max_image_h {
                                w *= max_image_h / h;
                                h = max_image_h;
                            }
                            blocks.push(Block {
                                kind: Kind::Image,
                                para: i,
                                rich,
                                style: StyleKind::Body,
                                y: 0.0,
                                h: h + gap,
                                text_x: (width - w) / 2.0,
                                text_y: 0.0,
                                text_w: w,
                                text_h: h,
                                lines: Vec::new(),
                                extra: Extra::Image(path),
                            });
                        }
                        None => {
                            let label = if rich.text.is_empty() { "[Picture]".to_string() } else { format!("[Picture: {}]", rich.text) };
                            push_text(ui, &mut blocks, Kind::Image, i, RichText { text: label, spans: Vec::new() }, StyleKind::Center, (0.0, 0.0), 0.0, gap, Extra::Image(None));
                        }
                    }
                }
                ParagraphKind::TableRow(header) => {
                    // Columns are shared by the whole table: as many as its widest row.
                    if !blocks.last().is_some_and(|b| b.kind == Kind::TableRow) {
                        table_cols = shown[i..].iter().take_while(|(k, _)| matches!(k, ParagraphKind::TableRow(_))).zip(&ch.paragraphs[i..]).map(|(_, p)| table_cells(p).len()).max().unwrap_or(1).max(1);
                    }
                    let cells = table_cells(p);
                    let col_w = width / table_cols as f32;
                    let (ts, opts) = styles.get(StyleKind::Small);
                    let cell_opts = TextOpts { wrap: Some((col_w - 20.0).max(20.0)), ..opts };
                    let text_h = cells.iter().map(|c| ui.g.measure_lines(&c.text, &c.spans, ts, cell_opts).0).fold(0.0, f32::max);
                    let last = !matches!(next, Some(ParagraphKind::TableRow(_)));
                    blocks.push(Block {
                        kind: Kind::TableRow,
                        para: i,
                        rich,
                        style: StyleKind::Small,
                        y: 0.0,
                        h: text_h + 16.0 + if last { gap } else { 0.0 },
                        text_x: 10.0,
                        text_y: 8.0,
                        text_w: col_w - 20.0,
                        text_h,
                        lines: Vec::new(),
                        extra: Extra::Row(cells, col_w, header),
                    });
                }
                ParagraphKind::Text => {
                    let rich = with_word_starts(rich, t.bold_word_starts);
                    let rich = if indent.is_empty() { rich } else { with_prefix(indent, rich) };
                    push_text(ui, &mut blocks, Kind::Para, i, rich, StyleKind::Body, (0.0, 0.0), 0.0, gap, Extra::None);
                }
            }
        }
        if t.show_chapter_post_notes {
            if let Some(s) = ch.post_notes.as_deref().filter(|s| !s.trim().is_empty()) {
                push_text(ui, &mut blocks, Kind::EndNotes, para_count, parse_inline(s.trim()), StyleKind::Small, (CARD_PAD, CARD_PAD), 16.0 + card(true), CARD_PAD, Extra::None);
            }
        }
        blocks.push(Block {
            kind: Kind::End,
            para: para_count,
            rich: RichText::default(),
            style: StyleKind::Body,
            y: 0.0,
            h: END_H,
            text_x: 0.0,
            text_y: 0.0,
            text_w: width,
            text_h: END_H,
            lines: Vec::new(),
            extra: Extra::None,
        });

        let mut y = 0.0;
        for b in &mut blocks {
            b.y = y;
            y += b.h;
        }
        let pages = page_h.map(|ph| paginate(&blocks, ph)).unwrap_or_default();
        Self { key, page_h, chapter, width, blocks, total_h: y, pages, styles, para_count }
    }

    /// Paragraph anchor (index + fraction) for a y position in the continuous column.
    pub fn anchor_at(&self, y: f32) -> f32 {
        let Some(b) = self.blocks.iter().rev().find(|b| b.y <= y + 0.5) else { return 0.0 };
        match b.kind {
            k if k.is_body() => b.para as f32 + ((y - b.y) / b.h.max(1.0)).clamp(0.0, 0.999),
            Kind::EndNotes | Kind::End => self.para_count as f32,
            _ => 0.0,
        }
    }

    /// y position in the continuous column for a paragraph anchor.
    pub fn y_for_anchor(&self, anchor: f32) -> f32 {
        if anchor <= 0.0 {
            return 0.0;
        }
        let idx = anchor.floor() as usize;
        let frac = anchor - anchor.floor();
        if idx >= self.para_count {
            return self.blocks.iter().find(|b| matches!(b.kind, Kind::EndNotes | Kind::End)).map_or(self.total_h, |b| b.y);
        }
        match self.blocks.iter().find(|b| b.para == idx && b.kind.is_body()) {
            Some(b) => b.y + b.h * frac,
            None => 0.0,
        }
    }

    /// Index of the page containing continuous-column position `y`.
    pub fn page_for_y(&self, y: f32) -> usize {
        self.pages.iter().rposition(|p| p.y <= y + 0.5).unwrap_or(0)
    }
}

/// Fills pages top to bottom, splitting text blocks at line boundaries.
fn paginate(blocks: &[Block], page_h: f32) -> Vec<Page> {
    let page_h = page_h.max(80.0);
    let mut pages = Vec::new();
    let mut cur = Page { slices: Vec::new(), y: 0.0 };
    let mut used = 0.0f32;

    for (bi, b) in blocks.iter().enumerate() {
        let mut from = 0.0f32;
        loop {
            let remaining = page_h - used;
            let rest = b.h - from;
            if rest <= remaining + 0.5 {
                cur.slices.push(Slice { block: bi, from, to: b.h });
                used += rest;
                break;
            }
            // Trailing spacing below the text never forces a page break.
            let text_end = b.text_y + b.text_h;
            if text_end - from <= remaining + 0.5 && !b.lines.is_empty() {
                cur.slices.push(Slice { block: bi, from, to: b.h.min(from + remaining) });
                pages.push(std::mem::replace(&mut cur, Page { slices: Vec::new(), y: b.y + b.h }));
                used = 0.0;
                break;
            }
            let cut = b.lines.iter().copied().filter(|&l| l > from + 0.5 && l - from <= remaining + 0.5).last();
            match cut {
                Some(c) => {
                    cur.slices.push(Slice { block: bi, from, to: c });
                    pages.push(std::mem::replace(&mut cur, Page { slices: Vec::new(), y: b.y + c }));
                    used = 0.0;
                    from = c;
                }
                None if used > 0.0 => {
                    pages.push(std::mem::replace(&mut cur, Page { slices: Vec::new(), y: b.y + from }));
                    used = 0.0;
                }
                None => {
                    // Taller than a page and cannot be split (e.g. a long preface): clip it.
                    let to = (from + page_h).min(b.h);
                    cur.slices.push(Slice { block: bi, from, to });
                    pages.push(std::mem::replace(&mut cur, Page { slices: Vec::new(), y: b.y + to }));
                    from = to;
                    if from >= b.h - 0.5 {
                        break;
                    }
                }
            }
        }
    }
    if !cur.slices.is_empty() {
        pages.push(cur);
    }
    if pages.is_empty() {
        pages.push(Page { slices: Vec::new(), y: 0.0 });
    }
    pages
}
