use crate::ao3::models::{Chapter, WorkMeta};
use crate::reader::typography::TypographySettings;

#[derive(Debug, Clone, PartialEq)]
pub enum PageItem {
    /// Work preface / cover info (rendered on Page 1 if is_first_chapter)
    CoverPreface,
    /// Chapter Header (title & chapter number)
    ChapterHeader { title: String, index: usize },
    /// Chapter Summary Box
    ChapterSummary(String),
    /// Pre-notes or Post-notes Box
    ChapterNotes { title: String, text: String },
    /// Paragraph of text
    Paragraph { text: String, is_continuation: bool },
    /// Blockquote
    Blockquote(String),
    /// Divider line (---)
    Divider,
    /// Decorative End of Chapter mark (✦ ✦ ✦)
    ChapterEndMark,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub page_index: usize, // 1-based index (e.g. 1, 2, 3...)
    pub total_pages: usize,
    pub items: Vec<PageItem>,
    pub word_count: usize,
}

pub struct Paginator;

impl Paginator {
    /// Paginates a chapter into discrete e-reader pages based on viewport dimensions and typography.
    pub fn paginate(
        chapter: &Chapter,
        is_first_chapter: bool,
        work_meta: &WorkMeta,
        col_width: f32,
        avail_height: f32,
        typography: &TypographySettings,
    ) -> Vec<Page> {
        let line_h = (typography.font_size * typography.line_spacing_multiplier).max(18.0);
        let chars_per_line = ((col_width / (typography.font_size * 0.52)).floor() as usize).max(24);
        // Leave room for top header, bottom footer, and inner page margins
        let max_page_height = (avail_height - 90.0).max(280.0);

        let mut pages: Vec<Page> = Vec::new();
        let mut current_items: Vec<PageItem> = Vec::new();
        let mut current_height: f32 = 0.0;
        let mut current_words: usize = 0;

        // 1. Cover / Title page if Chapter 1
        if is_first_chapter {
            pages.push(Page {
                page_index: 1,
                total_pages: 1,
                items: vec![PageItem::CoverPreface],
                word_count: work_meta.summary_text.split_whitespace().count(),
            });
        }

        let mut flush_page = |items: &mut Vec<PageItem>, height: &mut f32, words: &mut usize| {
            if !items.is_empty() {
                pages.push(Page {
                    page_index: pages.len() + 1,
                    total_pages: 0,
                    items: std::mem::take(items),
                    word_count: *words,
                });
                *height = 0.0;
                *words = 0;
            }
        };

        // 2. Chapter Title Header
        let header_height = typography.font_size * 1.35 * 1.6 + 28.0;
        if current_height + header_height > max_page_height {
            flush_page(&mut current_items, &mut current_height, &mut current_words);
        }
        current_items.push(PageItem::ChapterHeader {
            title: chapter.title.clone(),
            index: chapter.index,
        });
        current_height += header_height;

        // 3. Chapter Summary (if enabled)
        if typography.show_chapter_summary {
            if let Some(summary) = &chapter.summary {
                if !summary.trim().is_empty() {
                    let lines = estimate_lines(summary, chars_per_line);
                    let sum_height = (lines as f32) * line_h + 38.0;

                    if current_height + sum_height > max_page_height {
                        flush_page(&mut current_items, &mut current_height, &mut current_words);
                    }
                    current_items.push(PageItem::ChapterSummary(summary.clone()));
                    current_height += sum_height;
                    current_words += summary.split_whitespace().count();
                }
            }
        }

        // 4. Chapter Pre-notes (if enabled)
        if typography.show_chapter_pre_notes {
            if let Some(notes) = &chapter.pre_notes {
                if !notes.trim().is_empty() {
                    let lines = estimate_lines(notes, chars_per_line);
                    let notes_height = (lines as f32) * line_h + 38.0;

                    if current_height + notes_height > max_page_height {
                        flush_page(&mut current_items, &mut current_height, &mut current_words);
                    }
                    current_items.push(PageItem::ChapterNotes {
                        title: "Chapter Notes".to_string(),
                        text: notes.clone(),
                    });
                    current_height += notes_height;
                    current_words += notes.split_whitespace().count();
                }
            }
        }

        // 5. Paragraphs
        for p in &chapter.paragraphs {
            let p_trimmed = p.trim();
            if p_trimmed.is_empty() {
                continue;
            }

            if p_trimmed == "---" {
                let div_height = 28.0 + typography.paragraph_spacing;
                if current_height + div_height > max_page_height {
                    flush_page(&mut current_items, &mut current_height, &mut current_words);
                }
                current_items.push(PageItem::Divider);
                current_height += div_height;
                continue;
            }

            if p_trimmed.starts_with("> ") {
                let quote_text = p_trimmed.trim_start_matches("> ").trim().to_string();
                let lines = estimate_lines(&quote_text, chars_per_line);
                let q_height = (lines as f32) * line_h + 24.0 + typography.paragraph_spacing;

                if current_height + q_height > max_page_height && !current_items.is_empty() {
                    flush_page(&mut current_items, &mut current_height, &mut current_words);
                }
                current_items.push(PageItem::Blockquote(quote_text.clone()));
                current_height += q_height;
                current_words += quote_text.split_whitespace().count();
                continue;
            }

            // Normal text paragraph
            let mut remaining_text = p_trimmed.to_string();
            let mut is_continuation = false;

            while !remaining_text.is_empty() {
                let lines = estimate_lines(&remaining_text, chars_per_line);
                let p_height = (lines as f32) * line_h + typography.paragraph_spacing;

                let space_left = max_page_height - current_height;

                if p_height <= space_left {
                    // Fits comfortably on current page
                    current_words += remaining_text.split_whitespace().count();
                    current_items.push(PageItem::Paragraph {
                        text: remaining_text,
                        is_continuation,
                    });
                    current_height += p_height;
                    break;
                } else if !current_items.is_empty() && space_left < (line_h * 2.5) {
                    // Very little space left; start a fresh page
                    flush_page(&mut current_items, &mut current_height, &mut current_words);
                } else {
                    // Split the paragraph: take as many lines as fit
                    let usable_space = (max_page_height - current_height).max(line_h * 2.0);
                    let max_lines = (usable_space / line_h).floor() as usize;

                    if max_lines < 2 && !current_items.is_empty() {
                        flush_page(&mut current_items, &mut current_height, &mut current_words);
                        continue;
                    }

                    let target_chars = (max_lines * chars_per_line).min(remaining_text.len());
                    let split_idx = find_best_split_point(&remaining_text, target_chars);

                    let (head, tail) = remaining_text.split_at(split_idx);
                    let head_clean = head.trim().to_string();
                    let tail_clean = tail.trim().to_string();

                    if !head_clean.is_empty() {
                        current_words += head_clean.split_whitespace().count();
                        let head_lines = estimate_lines(&head_clean, chars_per_line);
                        current_items.push(PageItem::Paragraph {
                            text: head_clean,
                            is_continuation,
                        });
                        current_height += (head_lines as f32) * line_h + typography.paragraph_spacing;
                    }

                    // Flush page after placing the first chunk
                    flush_page(&mut current_items, &mut current_height, &mut current_words);
                    remaining_text = tail_clean;
                    is_continuation = true;
                }
            }
        }

        // 6. Chapter Post-notes (if enabled)
        if typography.show_chapter_post_notes {
            if let Some(post) = &chapter.post_notes {
                if !post.trim().is_empty() {
                    let lines = estimate_lines(post, chars_per_line);
                    let notes_height = (lines as f32) * line_h + 38.0;

                    if current_height + notes_height > max_page_height {
                        flush_page(&mut current_items, &mut current_height, &mut current_words);
                    }
                    current_items.push(PageItem::ChapterNotes {
                        title: "Chapter End Notes".to_string(),
                        text: post.clone(),
                    });
                    current_height += notes_height;
                    current_words += post.split_whitespace().count();
                }
            }
        }

        // 7. Decorative End of Chapter Ornament
        let end_mark_h = 36.0;
        if current_height + end_mark_h > max_page_height {
            flush_page(&mut current_items, &mut current_height, &mut current_words);
        }
        current_items.push(PageItem::ChapterEndMark);
        current_height += end_mark_h;

        // Flush any remaining items
        flush_page(&mut current_items, &mut current_height, &mut current_words);

        // Ensure at least 1 page exists
        if pages.is_empty() {
            pages.push(Page {
                page_index: 1,
                total_pages: 1,
                items: vec![PageItem::ChapterHeader {
                    title: chapter.title.clone(),
                    index: chapter.index,
                }],
                word_count: 0,
            });
        }

        // Patch total_pages across all pages
        let total = pages.len();
        for (i, p) in pages.iter_mut().enumerate() {
            p.page_index = i + 1;
            p.total_pages = total;
        }

        pages
    }
}

/// Estimates number of rendered lines for a given text block based on line wrapping.
pub fn estimate_lines(text: &str, chars_per_line: usize) -> usize {
    if text.is_empty() {
        return 1;
    }
    let mut total = 0;
    for sub in text.split('\n') {
        let count = sub.chars().count();
        if count == 0 {
            total += 1;
        } else {
            total += (count as f32 / chars_per_line as f32).ceil() as usize;
        }
    }
    total.max(1)
}

/// Finds the best boundary (sentence end or word break) near `target_chars`.
fn find_best_split_point(text: &str, target_chars: usize) -> usize {
    let len = text.len();
    if target_chars >= len {
        return len;
    }

    // Try sentence ends (. ! ?) in the range [target_chars * 0.45 .. target_chars]
    let min_search = (target_chars as f32 * 0.45) as usize;
    let search_slice = &text[..target_chars];

    let sentence_ends = ['.', '!', '?'];
    for (i, c) in search_slice.char_indices().rev() {
        if i < min_search {
            break;
        }
        if sentence_ends.contains(&c) {
            let next_pos = i + c.len_utf8();
            if next_pos < text.len() {
                // Ensure followed by whitespace or quote
                let next_char = text[next_pos..].chars().next().unwrap_or(' ');
                if next_char.is_whitespace() || next_char == '"' || next_char == '”' || next_char == '\'' {
                    return next_pos;
                }
            }
        }
    }

    // Fallback: search for last whitespace before target_chars
    for (i, c) in search_slice.char_indices().rev() {
        if c.is_whitespace() {
            return i;
        }
    }

    // Ultimate fallback: hard split on char boundary
    let mut boundary = target_chars;
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    if boundary == 0 {
        text.char_indices().nth(1).map(|(i, _)| i).unwrap_or(len)
    } else {
        boundary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ao3::models::{Chapter, WorkMeta};

    fn sample_work_meta() -> WorkMeta {
        WorkMeta {
            id: "123".to_string(),
            title: "Test Fanfiction".to_string(),
            authors: vec!["AuthorA".to_string()],
            summary_text: "A grand adventure of testing pagination.".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_pagination_splits_pages_when_content_is_long() {
        let meta = sample_work_meta();
        let typography = TypographySettings::default();

        let long_paragraphs: Vec<String> = (0..50)
            .map(|i| format!("Paragraph {i}: The quick brown fox jumps over the lazy dog repeatedly to fill up the page content. It was a dark and stormy night, and the adventure had only just begun."))
            .collect();

        let chapter = Chapter {
            id: Some("1".to_string()),
            index: 1,
            title: "Chapter One: The Beginning".to_string(),
            summary: Some("In which testing commences.".to_string()),
            pre_notes: None,
            paragraphs: long_paragraphs,
            post_notes: None,
            raw_html: String::new(),
            word_count: 800,
        };

        let pages = Paginator::paginate(&chapter, true, &meta, 700.0, 600.0, &typography);

        assert!(pages.len() > 1, "Should paginate into multiple pages");
        assert_eq!(pages[0].items[0], PageItem::CoverPreface, "Page 1 on Chapter 1 should be CoverPreface");
        assert_eq!(pages.last().unwrap().total_pages, pages.len());
    }

    #[test]
    fn test_split_point_finds_sentence_break() {
        let text = "First sentence is here. Second sentence follows right after. Third sentence continues.";
        let split = find_best_split_point(text, 35);
        assert_eq!(&text[..split], "First sentence is here.");
    }
}
