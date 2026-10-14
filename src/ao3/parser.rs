use crate::ao3::models::*;
use chrono::Utc;
use regex::Regex;
use scraper::{Html, Selector};

/// Parser for AO3 HTML responses
pub struct Ao3Parser;

impl Ao3Parser {
    /// Extracts Work ID from various formats:
    /// - "123456"
    /// - "https://archiveofourown.org/works/123456"
    /// - "https://archiveofourown.org/works/123456/chapters/789012"
    /// - "archiveofourown.org/works/123456?view_adult=true"
    pub fn extract_work_id(input: &str) -> Option<String> {
        let trimmed = input.trim();
        if trimmed.chars().all(|c| c.is_ascii_digit()) && !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }

        let re = Regex::new(r"works/(\d+)").ok()?;
        if let Some(caps) = re.captures(trimmed) {
            return caps.get(1).map(|m| m.as_str().to_string());
        }

        None
    }

    /// Check if the page indicates that the work is restricted to registered users
    pub fn is_restricted_page(document: &Html) -> bool {
        let flash_sel = Selector::parse("div.flash.error, p.notice, .flash.notice").unwrap();
        for el in document.select(&flash_sel) {
            let text = el.text().collect::<Vec<_>>().join(" ");
            if text.contains("only available to registered users")
                || text.contains("Please log in")
            {
                return true;
            }
        }

        // Also check if login form is present and work skin is absent
        let workskin_sel = Selector::parse("#workskin").unwrap();
        let login_sel = Selector::parse("#signin, form[action*='users/login']").unwrap();

        document.select(&workskin_sel).next().is_none() && document.select(&login_sel).next().is_some()
    }

    /// Check if adult content warning is blocking the page
    pub fn is_adult_warning_page(document: &Html) -> bool {
        let warning_sel = Selector::parse("p.caution, div#tos_prompt").unwrap();
        for el in document.select(&warning_sel) {
            let text = el.text().collect::<Vec<_>>().join(" ");
            if text.contains("adult content") || text.contains("terms of service") {
                return true;
            }
        }
        false
    }

    /// Extract authenticity token (CSRF) from page
    pub fn extract_authenticity_token(document: &Html) -> Option<String> {
        let csrf_meta_sel = Selector::parse("meta[name='csrf-token']").unwrap();
        if let Some(meta) = document.select(&csrf_meta_sel).next() {
            if let Some(token) = meta.value().attr("content") {
                return Some(token.to_string());
            }
        }

        let input_sel = Selector::parse("input[name='authenticity_token']").unwrap();
        if let Some(input) = document.select(&input_sel).next() {
            if let Some(val) = input.value().attr("value") {
                return Some(val.to_string());
            }
        }

        None
    }

    /// Parse full work from HTML
    pub fn parse_work(html_str: &str, fallback_id: &str) -> Result<Work, String> {
        let document = Html::parse_document(html_str);

        if Self::is_restricted_page(&document) {
            return Err("This work is restricted to registered AO3 users. Please log in to AO3 in the Settings tab to read this work.".to_string());
        }

        let mut meta = Self::parse_metadata(&document, fallback_id);

        let chapters = Self::parse_chapters(&document, &meta.title);
        if chapters.is_empty() {
            return Err("Failed to parse any chapters or content from the AO3 page. The work might be private, deleted, or require adult content confirmation.".to_string());
        }

        if meta.stats.words == 0 {
            meta.stats.words = chapters.iter().map(|c| c.word_count).sum();
        }

        if meta.stats.chapters_published == 0 {
            meta.stats.chapters_published = chapters.len() as u32;
        }

        Ok(Work {
            meta,
            chapters,
            fetched_at: Utc::now(),
        })
    }

    /// Parse metadata from the work page
    pub fn parse_metadata(document: &Html, fallback_id: &str) -> WorkMeta {
        let mut meta = WorkMeta::default();
        meta.id = fallback_id.to_string();

        // 1. Title
        let title_sel = Selector::parse("#workskin h2.title, div.preface h2.title, h2.heading").unwrap();
        if let Some(el) = document.select(&title_sel).next() {
            let title_text = el.text().collect::<Vec<_>>().join(" ");
            let clean = title_text.trim();
            if !clean.is_empty() {
                meta.title = clean.to_string();
            }
        }

        // 2. Authors
        let author_sel = Selector::parse("h3.byline a[rel='author'], h3.byline a").unwrap();
        let mut authors: Vec<String> = Vec::new();
        for el in document.select(&author_sel) {
            let name = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
            if !name.is_empty() && !authors.contains(&name) {
                authors.push(name);
            }
        }
        if authors.is_empty() {
            let byline_sel = Selector::parse("h3.byline").unwrap();
            if let Some(el) = document.select(&byline_sel).next() {
                let name = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
                if !name.is_empty() {
                    authors.push(name);
                }
            }
        }
        if !authors.is_empty() {
            meta.authors = authors;
        }

        // 3. Summary
        let summary_sel = Selector::parse("#workskin .preface .summary blockquote.userstuff, div.preface .summary blockquote").unwrap();
        if let Some(el) = document.select(&summary_sel).next() {
            meta.summary_html = el.inner_html();
            meta.summary_text = el.text().collect::<Vec<_>>().join("\n").trim().to_string();
        }

        // 4. Tags in dl.work.meta.group
        // Rating
        let rating_sel = Selector::parse("dd.rating ul.commas li a, dd.rating.tags ul li a").unwrap();
        if let Some(el) = document.select(&rating_sel).next() {
            let r_str = el.text().collect::<Vec<_>>().join(" ");
            meta.rating = WorkRating::from_str_loose(&r_str);
        }

        // Warnings
        let warning_sel = Selector::parse("dd.warning ul.commas li a, dd.warning.tags ul li a").unwrap();
        meta.warnings = document
            .select(&warning_sel)
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Categories
        let category_sel = Selector::parse("dd.category ul.commas li a, dd.category.tags ul li a").unwrap();
        meta.categories = document
            .select(&category_sel)
            .map(|el| {
                let cat_str = el.text().collect::<Vec<_>>().join(" ");
                Category::from_str_loose(&cat_str)
            })
            .collect();

        // Fandoms
        let fandom_sel = Selector::parse("dd.fandom ul.commas li a, dd.fandom.tags ul li a").unwrap();
        meta.fandoms = document
            .select(&fandom_sel)
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Relationships
        let rel_sel = Selector::parse("dd.relationship ul.commas li a, dd.relationship.tags ul li a").unwrap();
        meta.relationships = document
            .select(&rel_sel)
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Characters
        let char_sel = Selector::parse("dd.character ul.commas li a, dd.character.tags ul li a").unwrap();
        meta.characters = document
            .select(&char_sel)
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Freeform / Additional Tags
        let freeform_sel = Selector::parse("dd.freeform ul.commas li a, dd.freeform.tags ul li a").unwrap();
        meta.freeform_tags = document
            .select(&freeform_sel)
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Language
        let lang_sel = Selector::parse("dd.language").unwrap();
        if let Some(el) = document.select(&lang_sel).next() {
            let l_str = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
            if !l_str.is_empty() {
                meta.language = l_str;
            }
        }

        // Series
        let series_sel = Selector::parse("dd.series ul li, dd.series span.position").unwrap();
        for el in document.select(&series_sel) {
            let text = el.text().collect::<Vec<_>>().join(" ");
            let re = Regex::new(r"Part\s+(\d+)\s+of\s+(.+)").unwrap();
            if let Some(caps) = re.captures(&text) {
                let part = caps.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(1);
                let name = caps.get(2).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
                meta.series.push(SeriesInfo {
                    id: None,
                    name,
                    part,
                    previous_work_id: None,
                    next_work_id: None,
                });
            }
        }

        // Stats
        let stats_dl_sel = Selector::parse("dl.stats").unwrap();
        if let Some(stats_dl) = document.select(&stats_dl_sel).next() {
            let pub_sel = Selector::parse("dd.published").unwrap();
            if let Some(el) = stats_dl.select(&pub_sel).next() {
                meta.stats.published_date = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
            }

            let status_sel = Selector::parse("dd.status").unwrap();
            if let Some(el) = stats_dl.select(&status_sel).next() {
                meta.stats.completed_date = Some(el.text().collect::<Vec<_>>().join(" ").trim().to_string());
            }

            let words_sel = Selector::parse("dd.words").unwrap();
            if let Some(el) = stats_dl.select(&words_sel).next() {
                let w = el.text().collect::<Vec<_>>().join("").replace(',', "");
                meta.stats.words = w.trim().parse().unwrap_or(0);
            }

            let chapters_sel = Selector::parse("dd.chapters").unwrap();
            if let Some(el) = stats_dl.select(&chapters_sel).next() {
                let c_text = el.text().collect::<Vec<_>>().join("");
                let parts: Vec<&str> = c_text.split('/').collect();
                if let Some(pub_str) = parts.first() {
                    meta.stats.chapters_published = pub_str.trim().parse().unwrap_or(1);
                }
                if parts.len() > 1 {
                    let total_str = parts[1].trim();
                    if total_str != "?" {
                        meta.stats.chapters_total = total_str.parse().ok();
                        if let Some(tot) = meta.stats.chapters_total {
                            meta.is_complete = tot == meta.stats.chapters_published;
                        }
                    }
                }
            }

            let kudos_sel = Selector::parse("dd.kudos").unwrap();
            if let Some(el) = stats_dl.select(&kudos_sel).next() {
                let k = el.text().collect::<Vec<_>>().join("").replace(',', "");
                meta.stats.kudos = k.trim().parse().unwrap_or(0);
            }

            let bookmarks_sel = Selector::parse("dd.bookmarks").unwrap();
            if let Some(el) = stats_dl.select(&bookmarks_sel).next() {
                let b = el.text().collect::<Vec<_>>().join("").replace(',', "");
                meta.stats.bookmarks = b.trim().parse().unwrap_or(0);
            }

            let hits_sel = Selector::parse("dd.hits").unwrap();
            if let Some(el) = stats_dl.select(&hits_sel).next() {
                let h = el.text().collect::<Vec<_>>().join("").replace(',', "");
                meta.stats.hits = h.trim().parse().unwrap_or(0);
            }

            let comments_sel = Selector::parse("dd.comments").unwrap();
            if let Some(el) = stats_dl.select(&comments_sel).next() {
                let cm = el.text().collect::<Vec<_>>().join("").replace(',', "");
                meta.stats.comments = cm.trim().parse().unwrap_or(0);
            }
        }

        meta
    }

    /// Parse chapters from document
    pub fn parse_chapters(document: &Html, default_title: &str) -> Vec<Chapter> {
        let mut chapters = Vec::new();

        let multi_chapter_sel = Selector::parse("#chapters > div.chapter").unwrap();
        let multi_chapters: Vec<_> = document.select(&multi_chapter_sel).collect();

        if !multi_chapters.is_empty() {
            for (idx, ch_node) in multi_chapters.into_iter().enumerate() {
                let mut chapter = Chapter::new(idx + 1, format!("Chapter {}", idx + 1));

                // Chapter title
                let ch_title_sel = Selector::parse("div.chapter.preface h3.title, .preface.group h3.title").unwrap();
                if let Some(t_el) = ch_node.select(&ch_title_sel).next() {
                    let title_text = t_el.text().collect::<Vec<_>>().join(" ");
                    let clean = title_text.trim();
                    if !clean.is_empty() {
                        chapter.title = clean.to_string();
                    }
                }

                // Summary
                let sum_sel = Selector::parse("div.chapter.preface .summary blockquote, .preface .summary blockquote").unwrap();
                if let Some(s_el) = ch_node.select(&sum_sel).next() {
                    let text = s_el.text().collect::<Vec<_>>().join("\n").trim().to_string();
                    if !text.is_empty() {
                        chapter.summary = Some(text);
                    }
                }

                // Pre-notes
                let pre_notes_sel = Selector::parse("div.chapter.preface .notes blockquote, .preface .notes blockquote").unwrap();
                if let Some(n_el) = ch_node.select(&pre_notes_sel).next() {
                    let text = n_el.text().collect::<Vec<_>>().join("\n").trim().to_string();
                    if !text.is_empty() {
                        chapter.pre_notes = Some(text);
                    }
                }

                // Post-notes
                let post_notes_sel = Selector::parse("div.end.notes blockquote, div.notes.end blockquote").unwrap();
                if let Some(pn_el) = ch_node.select(&post_notes_sel).next() {
                    let text = pn_el.text().collect::<Vec<_>>().join("\n").trim().to_string();
                    if !text.is_empty() {
                        chapter.post_notes = Some(text);
                    }
                }

                // Body content: div[role="article"] or div.userstuff
                let body_sel = Selector::parse("div[role='article'] div.userstuff, div.userstuff.module, div.userstuff").unwrap();
                if let Some(body_el) = ch_node.select(&body_sel).next() {
                    chapter.raw_html = body_el.inner_html();
                    chapter.paragraphs = Self::extract_paragraphs(body_el);
                }

                chapter.word_count = chapter.paragraphs.iter().map(|p| p.split_whitespace().count()).sum();
                chapters.push(chapter);
            }
        } else {
            // Single chapter work
            let mut chapter = Chapter::new(1, default_title.to_string());

            // Pre-notes / Work notes
            let notes_sel = Selector::parse("#workskin > div.preface .notes blockquote, div.preface .notes blockquote").unwrap();
            if let Some(n_el) = document.select(&notes_sel).next() {
                let text = n_el.text().collect::<Vec<_>>().join("\n").trim().to_string();
                if !text.is_empty() {
                    chapter.pre_notes = Some(text);
                }
            }

            // End notes
            let end_notes_sel = Selector::parse("#workskin > div.end.notes blockquote, div.end.notes blockquote").unwrap();
            if let Some(en_el) = document.select(&end_notes_sel).next() {
                let text = en_el.text().collect::<Vec<_>>().join("\n").trim().to_string();
                if !text.is_empty() {
                    chapter.post_notes = Some(text);
                }
            }

            // Body content
            let body_sel = Selector::parse("#chapters div.userstuff, #workskin div.userstuff, div.userstuff").unwrap();
            if let Some(body_el) = document.select(&body_sel).next() {
                chapter.raw_html = body_el.inner_html();
                chapter.paragraphs = Self::extract_paragraphs(body_el);
            }

            chapter.word_count = chapter.paragraphs.iter().map(|p| p.split_whitespace().count()).sum();
            if !chapter.paragraphs.is_empty() {
                chapters.push(chapter);
            }
        }

        chapters
    }

    /// Extract readable paragraphs from a userstuff element
    fn extract_paragraphs(element: scraper::ElementRef) -> Vec<String> {
        let p_sel = Selector::parse("p, blockquote, hr, h1, h2, h3, h4, h5, h6, ul, ol").unwrap();
        let mut paragraphs = Vec::new();

        for child in element.select(&p_sel) {
            let tag_name = child.value().name();
            if tag_name == "hr" {
                paragraphs.push("---".to_string());
                continue;
            }

            let text = Self::clean_element_text(child);
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                if tag_name == "blockquote" {
                    paragraphs.push(format!("> {}", trimmed.replace('\n', "\n> ")));
                } else {
                    paragraphs.push(trimmed.to_string());
                }
            }
        }

        // Fallback: If no paragraph elements found, split by double newlines
        if paragraphs.is_empty() {
            let raw_text = element.text().collect::<Vec<_>>().join(" ");
            for part in raw_text.split("\n\n") {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    paragraphs.push(trimmed.to_string());
                }
            }
        }

        paragraphs
    }

    /// Clean inner text of an element, preserving inline formatting cues if needed
    fn clean_element_text(element: scraper::ElementRef) -> String {
        let mut result = String::new();
        for node in element.children() {
            if let Some(text) = node.value().as_text() {
                result.push_str(text);
            } else if let Some(el) = scraper::ElementRef::wrap(node) {
                let name = el.value().name();
                let inner = Self::clean_element_text(el);
                match name {
                    "em" | "i" => {
                        result.push('*');
                        result.push_str(inner.trim());
                        result.push('*');
                    }
                    "strong" | "b" => {
                        result.push_str("**");
                        result.push_str(inner.trim());
                        result.push_str("**");
                    }
                    "s" | "strike" | "del" => {
                        result.push_str("~~");
                        result.push_str(inner.trim());
                        result.push_str("~~");
                    }
                    "br" => {
                        result.push('\n');
                    }
                    _ => {
                        result.push_str(&inner);
                    }
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_work_id() {
        assert_eq!(Ao3Parser::extract_work_id("12345"), Some("12345".to_string()));
        assert_eq!(
            Ao3Parser::extract_work_id("https://archiveofourown.org/works/98765432"),
            Some("98765432".to_string())
        );
        assert_eq!(
            Ao3Parser::extract_work_id("https://archiveofourown.org/works/555123/chapters/999123"),
            Some("555123".to_string())
        );
        assert_eq!(Ao3Parser::extract_work_id("invalid url"), None);
    }
}
