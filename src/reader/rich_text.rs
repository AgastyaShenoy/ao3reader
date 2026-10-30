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
}

/// Display text of a stored paragraph. Search runs over exactly this text, so match offsets
/// line up with what is drawn.
pub fn paragraph_display(p: &str) -> (ParagraphKind, RichText) {
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

/// Works saved by older versions start chapters with AO3's hidden "Chapter Text" landmark.
pub fn is_landmark(index: usize, p: &str) -> bool {
    index == 0 && matches!(p.trim(), "Chapter Text" | "Work Text")
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
