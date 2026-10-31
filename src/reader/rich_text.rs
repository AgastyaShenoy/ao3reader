//! Inline formatting for stored paragraphs.
//!
//! The parser keeps emphasis as lightweight markers (`*italic*`, `**bold**`, `~~struck~~`) so
//! saved works stay plain JSON. This turns a paragraph into display text plus style spans and
//! leaves stray or unmatched markers (`* * *`, `2 * 3`) as literal text.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InlineStyle {
    Italic,
    Bold,
    Strike,
}

/// A styled range of the display text, in byte offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub style: InlineStyle,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichText {
    pub text: String,
    pub spans: Vec<Span>,
}

enum Piece<'a> {
    Text(&'a str),
    Marker { text: &'static str, matched: bool },
}

fn marker_text(style: InlineStyle) -> &'static str {
    match style {
        InlineStyle::Italic => "*",
        InlineStyle::Bold => "**",
        InlineStyle::Strike => "~~",
    }
}

pub fn parse_inline(src: &str) -> RichText {
    let bytes = src.as_bytes();
    let mut pieces: Vec<Piece> = Vec::new();
    // (style, index of the opening marker piece)
    let mut open: Vec<(InlineStyle, usize)> = Vec::new();
    // (open piece, close piece, style)
    let mut pairs: Vec<(usize, usize, InlineStyle)> = Vec::new();
    let mut text_start = 0;
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b != b'*' && b != b'~' {
            i += 1;
            continue;
        }
        let mut run = 1;
        while i + run < bytes.len() && bytes[i + run] == b {
            run += 1;
        }
        if b == b'~' && run < 2 {
            i += run;
            continue;
        }
        if text_start < i {
            pieces.push(Piece::Text(&src[text_start..i]));
        }
        let prev = src[..i].chars().next_back();
        let next = src[i + run..].chars().next();
        let can_close = prev.is_some_and(|c| !c.is_whitespace());
        let can_open = next.is_some_and(|c| !c.is_whitespace());

        let mut left = run;
        // Close innermost-first so `***x***` nests correctly.
        while can_close && left > 0 {
            let Some(&(style, open_idx)) = open.last() else { break };
            let need = marker_text(style).len();
            let fits = match style {
                InlineStyle::Strike => b == b'~' && left >= 2,
                _ => b == b'*' && left >= need,
            };
            if !fits {
                break;
            }
            open.pop();
            pieces.push(Piece::Marker { text: marker_text(style), matched: true });
            if let Piece::Marker { matched, .. } = &mut pieces[open_idx] {
                *matched = true;
            }
            pairs.push((open_idx, pieces.len() - 1, style));
            left -= need;
        }
        while left > 0 {
            let style = if b == b'~' {
                if left < 2 {
                    break;
                }
                InlineStyle::Strike
            } else if left >= 2 {
                InlineStyle::Bold
            } else {
                InlineStyle::Italic
            };
            let len = marker_text(style).len();
            pieces.push(Piece::Marker { text: marker_text(style), matched: false });
            if can_open {
                open.push((style, pieces.len() - 1));
            }
            left -= len;
        }
        if left > 0 {
            pieces.push(Piece::Text(&src[i + run - left..i + run]));
        }
        i += run;
        text_start = i;
    }
    if text_start < bytes.len() {
        pieces.push(Piece::Text(&src[text_start..]));
    }

    let mut text = String::with_capacity(src.len());
    let mut offsets = Vec::with_capacity(pieces.len());
    for p in &pieces {
        offsets.push(text.len());
        match p {
            Piece::Text(t) => text.push_str(t),
            Piece::Marker { text: m, matched: false } => text.push_str(m),
            Piece::Marker { matched: true, .. } => {}
        }
    }
    let mut spans: Vec<Span> = pairs
        .into_iter()
        .map(|(o, c, style)| Span { start: offsets[o], end: offsets[c], style })
        .filter(|s| s.end > s.start)
        .collect();
    spans.sort_by_key(|s| s.start);
    RichText { text, spans }
}

/// How a stored paragraph is displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParagraphKind {
    Text,
    Quote,
    Divider,
    /// A heading inside the text, level 1–6.
    Heading(u8),
    /// A bulleted list item at a nesting depth (1 = top level).
    Bullet(u8),
    /// A numbered list item: depth and number.
    Numbered(u8, u32),
    /// Preformatted text: line breaks and spacing kept, no inline markers.
    Pre,
    /// A centred line or paragraph.
    Center,
    /// A picture; its text is the description (alt text).
    Image,
    /// One row of a table; `true` for a header row.
    TableRow(bool),
}

/// Paragraphs that aren't plain prose start with this private-use character and a short tag:
/// `h2`, `ul1`, `ol1.3`, `pre`, `c`, `img`, `tr`, `th`, then a space and the body. Works stay
/// plain JSON, older versions just show the text, and reading positions (paragraph indexes)
/// are unaffected.
pub const BLOCK: char = '\u{E000}';
/// Separates fields inside a tagged paragraph: table cells, or an image's file and description.
pub const FIELD: char = '\u{E001}';

/// A tagged paragraph, e.g. `tagged("h2", "Part One")`.
pub fn tagged(tag: &str, body: &str) -> String {
    format!("{BLOCK}{tag} {body}")
}

/// (tag, body) of a tagged paragraph.
pub fn tag_of(p: &str) -> Option<(&str, &str)> {
    let rest = p.strip_prefix(BLOCK)?;
    Some(rest.split_once(' ').unwrap_or((rest, "")))
}

/// Display text of a stored paragraph. Search runs over exactly this text, so match offsets
/// line up with what is drawn.
pub fn paragraph_display(p: &str) -> (ParagraphKind, RichText) {
    if let Some((tag, body)) = tag_of(p) {
        let plain = |t: &str| RichText { text: t.to_string(), spans: Vec::new() };
        let num = |s: &str| s.parse::<u32>().ok();
        let kind = match tag {
            "pre" => return (ParagraphKind::Pre, plain(body.trim_end())),
            "img" => return (ParagraphKind::Image, plain(body.split_once(FIELD).map_or("", |(_, alt)| alt).trim())),
            "tr" | "th" => {
                let cells: Vec<String> = table_cells(p).into_iter().map(|c| c.text).collect();
                return (ParagraphKind::TableRow(tag == "th"), plain(&cells.join("   ")));
            }
            "c" => ParagraphKind::Center,
            _ if tag.len() == 2 && tag.starts_with('h') => ParagraphKind::Heading(num(&tag[1..]).unwrap_or(2).clamp(1, 6) as u8),
            _ if tag.starts_with("ul") => ParagraphKind::Bullet(num(&tag[2..]).unwrap_or(1).clamp(1, 8) as u8),
            _ if tag.starts_with("ol") => {
                let (depth, n) = tag[2..].split_once('.').unwrap_or(("1", "1"));
                ParagraphKind::Numbered(num(depth).unwrap_or(1).clamp(1, 8) as u8, num(n).unwrap_or(1))
            }
            _ => ParagraphKind::Text,
        };
        return (kind, parse_inline(body.trim()));
    }
    let p = p.trim();
    if p == "---" || p == "***" || p == "* * *" {
        return (ParagraphKind::Divider, RichText::default());
    }
    if let Some(q) = p.strip_prefix("> ") {
        let text: Vec<&str> = q.split('\n').map(|l| l.strip_prefix("> ").unwrap_or(l)).collect();
        return (ParagraphKind::Quote, parse_inline(&text.join("\n")));
    }
    (ParagraphKind::Text, parse_inline(p))
}

/// The cells of a table-row paragraph, with their inline styles.
pub fn table_cells(p: &str) -> Vec<RichText> {
    match tag_of(p) {
        Some(("tr" | "th", body)) => body.split(FIELD).map(|c| parse_inline(c.trim())).collect(),
        _ => Vec::new(),
    }
}

/// (file name, description) of an image paragraph. The name is one the importer chose, so
/// anything that could reach outside the book's own folder is refused.
pub fn image_source(p: &str) -> Option<(&str, &str)> {
    let (tag, body) = tag_of(p)?;
    if tag != "img" {
        return None;
    }
    let (name, alt) = body.split_once(FIELD).unwrap_or((body, ""));
    let safe = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-') && !name.starts_with('.');
    safe.then_some((name, alt.trim()))
}

/// Words in a stored paragraph as displayed (tags and inline markers aren't words).
pub fn paragraph_words(p: &str) -> usize {
    match paragraph_display(p) {
        (ParagraphKind::Image | ParagraphKind::Divider, _) => 0,
        (_, rich) => word_count(&rich.text),
    }
}

/// Works saved by older versions start chapters with AO3's hidden "Chapter Text" landmark.
pub fn is_landmark(index: usize, p: &str) -> bool {
    index == 0 && matches!(p.trim(), "Chapter Text" | "Work Text")
}

/// Bold spans over the first part of each word (about 40%, at least one letter): the "bold word
/// starts" reading aid. Byte offsets into `text`.
pub fn word_start_spans(text: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut letters: Vec<usize> = Vec::new();
    for (i, c) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
        if i < text.len() && c.is_alphanumeric() {
            if start.is_none() {
                start = Some(i);
                letters.clear();
            }
            letters.push(i);
        } else if let Some(s) = start.take() {
            let n = letters.len();
            let k = if n <= 3 { 1 } else { (n * 2).div_ceil(5) };
            let end = if k >= n { i } else { letters[k] };
            out.push(Span { start: s, end, style: InlineStyle::Bold });
        }
    }
    out
}

/// `rich` with bold word starts added (when `on`).
pub fn with_word_starts(mut rich: RichText, on: bool) -> RichText {
    if on {
        rich.spans.extend(word_start_spans(&rich.text));
        rich.spans.sort_by_key(|s| s.start);
    }
    rich
}

/// First-line indent, as a prefix that keeps inline styles aligned.
pub const INDENT: &str = "\u{2003}\u{2003}";

pub fn with_prefix(prefix: &str, mut rich: RichText) -> RichText {
    rich.text.insert_str(0, prefix);
    for s in &mut rich.spans {
        s.start += prefix.len();
        s.end += prefix.len();
    }
    rich
}

pub fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styled(r: &RichText, style: InlineStyle) -> Vec<&str> {
        r.spans.iter().filter(|s| s.style == style).map(|s| &r.text[s.start..s.end]).collect()
    }

    #[test]
    fn word_starts() {
        let r = with_word_starts(parse_inline("Hello world, a cat."), true);
        assert_eq!(styled(&r, InlineStyle::Bold), ["He", "wo", "a", "c"]);
        let accents = word_start_spans("éclair");
        assert_eq!(&"éclair"[accents[0].start..accents[0].end], "écl");
    }

    #[test]
    fn tagged_blocks() {
        assert_eq!(paragraph_display(&tagged("h2", "Part *One*")).0, ParagraphKind::Heading(2));
        assert_eq!(paragraph_display(&tagged("h2", "Part *One*")).1.text, "Part One");
        assert_eq!(paragraph_display(&tagged("ul2", "milk")).0, ParagraphKind::Bullet(2));
        assert_eq!(paragraph_display(&tagged("ol1.3", "third")).0, ParagraphKind::Numbered(1, 3));
        let pre = paragraph_display(&tagged("pre", "  a  *b*\n    c"));
        assert_eq!((pre.0, pre.1.text.as_str()), (ParagraphKind::Pre, "  a  *b*\n    c"));
        let row = tagged("th", &format!("Name{FIELD}*Age*"));
        assert_eq!(paragraph_display(&row).0, ParagraphKind::TableRow(true));
        assert_eq!(table_cells(&row).iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["Name", "Age"]);
        let img = tagged("img", &format!("3.png{FIELD}A map of the town"));
        assert_eq!(image_source(&img), Some(("3.png", "A map of the town")));
        assert_eq!(paragraph_display(&img).1.text, "A map of the town");
        assert_eq!(paragraph_words(&img), 0);
        assert_eq!(paragraph_words(&tagged("h1", "Two words")), 2);
        // A crafted name can't point outside the book's folder.
        assert_eq!(image_source(&tagged("img", &format!("../../x.png{FIELD}"))), None);
        assert_eq!(image_source(&tagged("img", &format!("C:\\x.png{FIELD}"))), None);
    }

    #[test]
    fn plain_text_is_unchanged() {
        let r = parse_inline("Just words, nothing else.");
        assert_eq!(r.text, "Just words, nothing else.");
        assert!(r.spans.is_empty());
    }

    #[test]
    fn italic_bold_strike() {
        let r = parse_inline("She *really* meant **that** and ~~not this~~.");
        assert_eq!(r.text, "She really meant that and not this.");
        assert_eq!(styled(&r, InlineStyle::Italic), ["really"]);
        assert_eq!(styled(&r, InlineStyle::Bold), ["that"]);
        assert_eq!(styled(&r, InlineStyle::Strike), ["not this"]);
    }

    #[test]
    fn nested_bold_italic() {
        let r = parse_inline("***both***");
        assert_eq!(r.text, "both");
        assert_eq!(styled(&r, InlineStyle::Bold), ["both"]);
        assert_eq!(styled(&r, InlineStyle::Italic), ["both"]);
    }

    #[test]
    fn stray_markers_stay_literal() {
        assert_eq!(parse_inline("* * *").text, "* * *");
        assert_eq!(parse_inline("2 * 3 = 6").text, "2 * 3 = 6");
        assert_eq!(parse_inline("an *unclosed marker").text, "an *unclosed marker");
        assert_eq!(parse_inline("~tilde~").text, "~tilde~");
    }

    #[test]
    fn multibyte_text() {
        let r = parse_inline("日本 *語* ✨");
        assert_eq!(r.text, "日本 語 ✨");
        assert_eq!(styled(&r, InlineStyle::Italic), ["語"]);
    }
}
