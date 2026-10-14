use crate::ao3::models::Work;
use std::fs;
use std::path::Path;

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

            for p in &chapter.paragraphs {
                if p == "---" {
                    md.push_str("\n---\n\n");
                } else {
                    md.push_str(&format!("{}\n\n", p));
                }
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
h1 { font-size: 2.2em; margin-bottom: 0.2em; color: #990000; }
h2 { font-size: 1.6em; border-bottom: 2px solid #eee; padding-bottom: 0.3em; margin-top: 2em; }
.meta-box { background: #f8f9fa; border: 1px solid #e9ecef; border-radius: 8px; padding: 20px; margin-bottom: 30px; font-size: 0.95em; }
.meta-row { margin-bottom: 8px; }
.meta-label { font-weight: bold; }
.badge { display: inline-block; background: #e2e8f0; color: #334155; padding: 2px 8px; border-radius: 4px; font-size: 0.85em; margin: 2px; }
.notes-box { background: #fdf6e2; border-left: 4px solid #b58900; padding: 12px 16px; margin: 20px 0; border-radius: 0 8px 8px 0; }
p { margin-bottom: 1.2em; text-align: justify; }
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

            for p in &chapter.paragraphs {
                if p == "---" {
                    html.push_str("<hr>\n");
                } else {
                    html.push_str(&format!("<p>{}</p>\n", html_escape(p)));
                }
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

            for p in &chapter.paragraphs {
                txt.push_str(&format!("{}\n\n", p));
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
