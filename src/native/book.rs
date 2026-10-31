//! The book open in the reader: text the reader lays out (EPUB, AO3) or a PDF's pages.

use super::pdf::PdfScreen;
use super::reader::{ReaderAction, ReaderScreen};
use super::ui::Ui;
use super::gfx::Rect;
use crate::ao3::models::{Bookmark, Highlight, ReadingProgress};
use crate::storage::config::AppConfig;

pub enum Book {
    Text(ReaderScreen),
    Pdf(PdfScreen),
}

impl Book {
    /// A word's meanings arrived (text books only look words up).
    pub fn defined(&mut self, word: &str, result: Result<Vec<crate::sources::dictionary::Meaning>, String>) {
        if let Book::Text(r) = self {
            r.defined(word, result);
        }
    }

    pub fn comments_loaded(&mut self, key: &str, result: Result<crate::ao3::comments::CommentPage, String>) {
        if let Book::Text(r) = self {
            r.comments_loaded(key, result);
        }
    }

    pub fn comment_posted(&mut self, result: &Result<(), String>) {
        if let Book::Text(r) = self {
            r.comment_posted(result);
        }
    }

    /// Signed in to AO3, so the reader can post comments.
    pub fn set_can_comment(&mut self, can: bool) {
        if let Book::Text(r) = self {
            r.can_comment = can;
        }
    }

    pub fn work_id(&self) -> &str {
        match self {
            Book::Text(r) => r.work_id(),
            Book::Pdf(p) => p.work_id(),
        }
    }

    pub fn progress(&self) -> ReadingProgress {
        match self {
            Book::Text(r) => r.progress(),
            Book::Pdf(p) => p.progress(),
        }
    }

    /// The position moved since it was last saved.
    pub fn unsaved(&self) -> bool {
        match self {
            Book::Text(r) => r.progress_dirty,
            Book::Pdf(p) => p.progress_dirty,
        }
    }

    pub fn mark_saved(&mut self) {
        match self {
            Book::Text(r) => r.progress_dirty = false,
            Book::Pdf(p) => p.progress_dirty = false,
        }
    }

    pub fn fullscreen(&self) -> bool {
        match self {
            Book::Text(r) => r.fullscreen,
            Book::Pdf(p) => p.fullscreen,
        }
    }

    /// Switches full screen; returns whether it's now on.
    pub fn toggle_fullscreen(&mut self) -> bool {
        let f = match self {
            Book::Text(r) => &mut r.fullscreen,
            Book::Pdf(p) => &mut p.fullscreen,
        };
        *f = !*f;
        *f
    }

    pub fn frame(&mut self, ui: &mut Ui, r: Rect, config: &mut AppConfig, bookmarks: &[Bookmark], highlights: &[Highlight]) -> Vec<ReaderAction> {
        match self {
            Book::Text(t) => t.frame(ui, r, config, bookmarks, highlights),
            Book::Pdf(p) => p.frame(ui, r, config),
        }
    }
}
