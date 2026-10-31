use crate::ao3::models::Work;
use crate::reader::rich_text::{image_source, paragraph_display, tag_of, InlineStyle, ParagraphKind, RichText, FIELD};
use crate::storage::library::LibraryManager;
use std::fs;
use std::path::Path;

/// Lists and table rows run together; everything else is its own paragraph.
fn joins(prev: Option<ParagraphKind>, kind: ParagraphKind) -> bool {
    use ParagraphKind::*;
    matches!((prev, kind), (Some(Bullet(_) | Numbered(..)), Bullet(_) | Numbered(..)) | (Some(TableRow(_)), TableRow(_)))
}

/// One stored paragraph as Markdown (inline markers are already Markdown).
fn md_paragraph(p: &str) -> String {
    let Some((_, body)) = tag_of(p) else {
        return if p == "---" { "---".to_string() } else { p.to_string() };
    };
    let body = body.trim_end();
    match paragraph_display(p) {
        (ParagraphKind::Heading(l), _) => format!("{} {}", "#".repeat((l as usize + 2).min(6)), body.trim()),
        (ParagraphKind::Bullet(d), _) => format!("{}- {}", "  ".repeat(d as usize - 1), body.trim()),
        (ParagraphKind::Numbered(d, n), _) => format!("{}{n}. {}", "   ".repeat(d as usize - 1), body.trim()),
        (ParagraphKind::Pre, _) => format!("```\n{body}\n```"),
        (ParagraphKind::Image, alt) if alt.text.is_empty() => String::new(),
        (ParagraphKind::Image, alt) => format!("*[Picture: {}]*", alt.text),
        (ParagraphKind::TableRow(header), _) => {
            let cells: Vec<&str> = body.split(FIELD).map(str::trim).collect();
            let row = format!("| {} |", cells.join(" | "));
            if header { format!("{row}\n|{}", "---|".repeat(cells.len())) } else { row }
        }
        _ => body.trim().to_string(),
    }
}

/// One stored paragraph as plain text.
fn txt_paragraph(p: &str) -> String {
    match paragraph_display(p) {
        (ParagraphKind::Divider, _) => "* * *".to_string(),
        (ParagraphKind::Bullet(d), r) => format!("{}• {}", "  ".repeat(d as usize - 1), r.text),
        (ParagraphKind::Numbered(d, n), r) => format!("{}{n}. {}", "  ".repeat(d as usize - 1), r.text),
        (ParagraphKind::Image, r) if r.text.is_empty() => String::new(),
        (ParagraphKind::Image, r) => format!("[Picture: {}]", r.text),
        (ParagraphKind::TableRow(_), _) => crate::reader::rich_text::table_cells(p).into_iter().map(|c| c.text).collect::<Vec<_>>().join(" | "),
        (_, r) => r.text,
    }
}

/// Inline styles as HTML tags, with the text escaped.
fn rich_html(r: &RichText) -> String {
    // (position, closing?, tag); closings sort before openings at the same place.
    let mut events: Vec<(usize, bool, &str)> = Vec::new();
    for s in &r.spans {
        let tag = match s.style {
            InlineStyle::Italic => "em",
            InlineStyle::Bold => "strong",
            InlineStyle::Strike => "s",
        };
        events.push((s.start, false, tag));
        events.push((s.end, true, tag));
    }
    events.sort_by_key(|(pos, close, _)| (*pos, !*close));
    let mut out = String::new();
    let mut at = 0;
    for (pos, close, tag) in events {
        out.push_str(&html_escape(&r.text[at..pos]));
        out.push_str(&if close { format!("</{tag}>") } else { format!("<{tag}>") });
        at = pos;
    }
    out.push_str(&html_escape(&r.text[at..]));
    out.replace('\n', "<br>")
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A picture of the work as an `<img>` with its data inline, so the exported page is complete.
fn html_image(work_id: &str, p: &str) -> String {
    let Some((name, alt)) = image_source(p) else { return String::new() };
    let mime = match name.rsplit('.').next().unwrap_or("") {
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "image/jpeg",
    };
    match fs::read(LibraryManager::resource_path(work_id, name)) {
        Ok(bytes) => format!("<p class=\"pic\"><img src=\"data:{mime};base64,{}\" alt=\"{}\"></p>", base64(&bytes), html_escape(alt)),
        Err(_) if !alt.is_empty() => format!("<p class=\"pic\">[Picture: {}]</p>", html_escape(alt)),
        Err(_) => String::new(),
    }
}

pub struct WorkExporter;

impl WorkExporter {
    /// Export work as cleanly formatted Markdown
    pub fn to_markdown(work: &Work) -> String {
        let mut md = String::new();

        md.push_str(&format!("# {}\n\n", work.meta.title));
        md.push_str(&format!("**Author:** {}\n\n", work.meta.authors.join(", ")));
        md.push_str(&format!("**Rating:** {}\n\n", work.meta.rating.display_name()));

        if !work.meta.fandoms.is_empty() {
            md.push_str(&format!("**Fandoms:** {}\n\n", work.meta.fandoms.join(", ")));
        }
        if !work.meta.relationships.is_empty() {
            md.push_str(&format!("**Relationships:** {}\n\n", work.meta.relationships.join(", ")));
        }
        if !work.meta.characters.is_empty() {
            md.push_str(&format!("**Characters:** {}\n\n", work.meta.characters.join(", ")));
        }
        if !work.meta.freeform_tags.is_empty() {
            md.push_str(&format!("**Tags:** {}\n\n", work.meta.freeform_tags.join(", ")));
        }
        if !work.meta.summary_text.is_empty() {
            md.push_str(&format!("### Summary\n\n> {}\n\n", work.meta.summary_text.replace('\n', "\n> ")));
        }

        md.push_str("---\n\n");

        for chapter in &work.chapters {
            md.push_str(&format!("## {}\n\n", chapter.title));

            if let Some(summary) = &chapter.summary {
                md.push_str(&format!("*Chapter Summary:* {}\n\n", summary));
            }

            if let Some(pre) = &chapter.pre_notes {
                md.push_str(&format!("*Chapter Notes:*\n> {}\n\n", pre.replace('\n', "\n> ")));
            }

            let mut prev = None;
            for p in &chapter.paragraphs {
                let kind = paragraph_display(p).0;
                let text = md_paragraph(p);
                if text.is_empty() {
                    continue;
                }
                if joins(prev, kind) {
                    // Keep list items and table rows on consecutive lines.
                    md.pop();
                }
                md.push_str(&format!("{text}\n\n"));
                prev = Some(kind);
            }

            if let Some(post) = &chapter.post_notes {
                md.push_str(&format!("*Chapter End Notes:*\n> {}\n\n", post.replace('\n', "\n> ")));
            }

            md.push_str("\n---\n\n");
        }

        md
    }

    /// Export work as standalone readable HTML with embedded CSS styling
    pub fn to_html(work: &Work) -> String {
        let mut html = String::new();
        html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"UTF-8\">\n");
        html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
        html.push_str(&format!("<title>{} - {}</title>\n", work.meta.title, work.meta.authors.join(", ")));
        html.push_str(r#"<style>
body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Georgia, serif;
    line-height: 1.7;
    max-width: 800px;
    margin: 40px auto;
    padding: 0 20px;
    background-color: #fdfdfd;
    color: #1a1a1a;
}
@media (prefers-color-scheme: dark) {
    body { background-color: #18191c; color: #e2e8f0; }
    .meta-box, .notes-box { background: #22242a !important; border-color: #333842 !important; }
    hr { border-color: #333842; }
}
h1 { font-size: 2.2em; margin-bottom: 0.2em; color: #3e5ea8; }
h2 { font-size: 1.6em; border-bottom: 2px solid #eee; padding-bottom: 0.3em; margin-top: 2em; }
.meta-box { background: #f8f9fa; border: 1px solid #e9ecef; border-radius: 8px; padding: 20px; margin-bottom: 30px; font-size: 0.95em; }
.meta-row { margin-bottom: 8px; }
.meta-label { font-weight: bold; }
.badge { display: inline-block; background: #e2e8f0; color: #334155; padding: 2px 8px; border-radius: 4px; font-size: 0.85em; margin: 2px; }
.notes-box { background: #fdf6e2; border-left: 4px solid #b58900; padding: 12px 16px; margin: 20px 0; border-radius: 0 8px 8px 0; }
p { margin-bottom: 1.2em; text-align: justify; }
.center { text-align: center; }
.pic { text-align: center; }
.pic img { max-width: 100%; height: auto; }
pre { background: rgba(127,127,127,0.1); padding: 12px 16px; border-radius: 6px; overflow-x: auto; }
table { border-collapse: collapse; margin: 1.2em 0; }
td, th { border: 1px solid rgba(127,127,127,0.35); padding: 6px 10px; text-align: left; vertical-align: top; }
hr { border: 0; border-top: 1px solid #e2e8f0; margin: 40px 0; }
</style>
</head>
<body>
"#);

        html.push_str(&format!("<h1>{}</h1>\n", html_escape(&work.meta.title)));
        html.push_str(&format!("<p><strong>By:</strong> {}</p>\n", html_escape(&work.meta.authors.join(", "))));

        // Metadata box
        html.push_str("<div class=\"meta-box\">\n");
        html.push_str(&format!("<div class=\"meta-row\"><span class=\"meta-label\">Rating:</span> {}</div>\n", work.meta.rating.display_name()));
        if !work.meta.fandoms.is_empty() {
            html.push_str(&format!("<div class=\"meta-row\"><span class=\"meta-label\">Fandoms:</span> {}</div>\n", html_escape(&work.meta.fandoms.join(", "))));
        }
        if !work.meta.relationships.is_empty() {
            html.push_str(&format!("<div class=\"meta-row\"><span class=\"meta-label\">Relationships:</span> {}</div>\n", html_escape(&work.meta.relationships.join(", "))));
        }
        if !work.meta.freeform_tags.is_empty() {
            html.push_str("<div class=\"meta-row\"><span class=\"meta-label\">Tags:</span> ");
            for tag in &work.meta.freeform_tags {
                html.push_str(&format!("<span class=\"badge\">{}</span> ", html_escape(tag)));
            }
            html.push_str("</div>\n");
        }
        if !work.meta.summary_text.is_empty() {
            html.push_str(&format!("<div class=\"meta-row\"><span class=\"meta-label\">Summary:</span><br>{}</div>\n", html_escape(&work.meta.summary_text).replace('\n', "<br>")));
        }
        html.push_str("</div>\n");

        for chapter in &work.chapters {
            html.push_str(&format!("<h2>{}</h2>\n", html_escape(&chapter.title)));

            if let Some(pre) = &chapter.pre_notes {
                html.push_str(&format!("<div class=\"notes-box\"><strong>Chapter Notes:</strong><br>{}</div>\n", html_escape(pre).replace('\n', "<br>")));
            }

            let mut in_table = false;
            for p in &chapter.paragraphs {
                let (kind, rich) = paragraph_display(p);
                if in_table && !matches!(kind, ParagraphKind::TableRow(_)) {
                    html.push_str("</table>\n");
                    in_table = false;
                }
                let line = match kind {
                    ParagraphKind::Divider => "<hr>".to_string(),
                    ParagraphKind::Quote => format!("<blockquote><p>{}</p></blockquote>", rich_html(&rich)),
                    ParagraphKind::Heading(l) => format!("<h{n}>{}</h{n}>", rich_html(&rich), n = (l as usize + 2).min(6)),
                    ParagraphKind::Bullet(d) => format!("<p style=\"margin:0 0 .3em {}em\">&bull; {}</p>", 1.5 * d as f32, rich_html(&rich)),
                    ParagraphKind::Numbered(d, n) => format!("<p style=\"margin:0 0 .3em {}em\">{n}. {}</p>", 1.5 * d as f32, rich_html(&rich)),
                    ParagraphKind::Pre => format!("<pre>{}</pre>", html_escape(&rich.text)),
                    ParagraphKind::Center => format!("<p class=\"center\">{}</p>", rich_html(&rich)),
                    ParagraphKind::Image => html_image(&work.meta.id, p),
                    ParagraphKind::TableRow(header) => {
                        let cell = if header { "th" } else { "td" };
                        let cells: String = crate::reader::rich_text::table_cells(p).iter().map(|c| format!("<{cell}>{}</{cell}>", rich_html(c))).collect();
                        let open = if in_table { "" } else { "<table>" };
                        in_table = true;
                        format!("{open}<tr>{cells}</tr>")
                    }
                    ParagraphKind::Text => format!("<p>{}</p>", rich_html(&rich)),
                };
                if !line.is_empty() {
                    html.push_str(&line);
                    html.push('\n');
                }
            }
            if in_table {
                html.push_str("</table>\n");
            }

            if let Some(post) = &chapter.post_notes {
                html.push_str(&format!("<div class=\"notes-box\"><strong>Chapter End Notes:</strong><br>{}</div>\n", html_escape(post).replace('\n', "<br>")));
            }

            html.push_str("<hr>\n");
        }

        html.push_str("</body>\n</html>");
        html
    }

    /// Export to plain text file
    pub fn to_txt(work: &Work) -> String {
        let mut txt = String::new();
        txt.push_str(&format!("{}\n", work.meta.title));
        txt.push_str(&format!("By {}\n\n", work.meta.authors.join(", ")));
        txt.push_str(&format!("Rating: {}\n", work.meta.rating.display_name()));
        if !work.meta.fandoms.is_empty() {
            txt.push_str(&format!("Fandoms: {}\n", work.meta.fandoms.join(", ")));
        }
        if !work.meta.summary_text.is_empty() {
            txt.push_str(&format!("\nSummary:\n{}\n\n", work.meta.summary_text));
        }
        txt.push_str("========================================\n\n");

        for chapter in &work.chapters {
            txt.push_str(&format!("--- {} ---\n\n", chapter.title));

            if let Some(pre) = &chapter.pre_notes {
                txt.push_str(&format!("[Author Notes: {}]\n\n", pre));
            }

            let mut prev = None;
            for p in &chapter.paragraphs {
                let kind = paragraph_display(p).0;
                let text = txt_paragraph(p);
                if text.is_empty() {
                    continue;
                }
                if joins(prev, kind) {
                    txt.pop();
                }
                txt.push_str(&format!("{text}\n\n"));
                prev = Some(kind);
            }

            if let Some(post) = &chapter.post_notes {
                txt.push_str(&format!("[End Notes: {}]\n\n", post));
            }
            txt.push_str("\n\n");
        }
        txt
    }

    pub fn save_to_file(path: &Path, content: &str) -> Result<(), String> {
        fs::write(path, content).map_err(|e| e.to_string())
    }
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ao3::models::{Chapter, WorkMeta};
    use crate::reader::rich_text::tagged;

    fn sample() -> Work {
        let mut ch = Chapter::new(1, "One".into());
        ch.paragraphs = vec![
            "Plain *style* here.".into(),
            tagged("h2", "Part Two"),
            tagged("ul1", "milk"),
            tagged("ul1", "bread"),
            tagged("th", &format!("Name{FIELD}Age")),
            tagged("tr", &format!("Ann{FIELD}30")),
            tagged("pre", "  a  b"),
        ];
        Work { meta: WorkMeta { id: "local-x".into(), title: "T".into(), ..Default::default() }, chapters: vec![ch], fetched_at: chrono::Utc::now(), resources: Vec::new() }
    }

    #[test]
    fn exports_render_blocks() {
        let w = sample();
        let md = WorkExporter::to_markdown(&w);
        assert!(md.contains("### Part Two"));
        assert!(md.contains("- milk\n- bread"));
        assert!(md.contains("| Name | Age |\n|---|---|\n| Ann | 30 |"));
        assert!(!md.contains('\u{E000}'));
        let html = WorkExporter::to_html(&w);
        assert!(html.contains("<p>Plain <em>style</em> here.</p>"));
        assert!(html.contains("<table><tr><th>Name</th><th>Age</th></tr>\n<tr><td>Ann</td><td>30</td></tr>\n</table>"));
        assert!(html.contains("<pre>  a  b</pre>"));
        let txt = WorkExporter::to_txt(&w);
        assert!(txt.contains("• milk\n• bread"));
        assert!(txt.contains("Name | Age\nAnn | 30"));
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
