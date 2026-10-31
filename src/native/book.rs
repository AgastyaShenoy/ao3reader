//! The book open in the reader: text the reader lays out (EPUB, AO3) or a PDF's pages.

use super::pdf::PdfScreen;
use super::reader::{ReaderAction, ReaderScreen};
use super::ui::Ui;
use super::gfx::Rect;
use crate::ao3::models::{Bookmark, ReadingProgress};
use crate::storage::config::AppConfig;

pub enum Book {
    Text(ReaderScreen),
    Pdf(PdfScreen),
}

impl Book {
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

    pub fn frame(&mut self, ui: &mut Ui, r: Rect, config: &mut AppConfig, bookmarks: &[Bookmark]) -> Vec<ReaderAction> {
        match self {
            Book::Text(t) => t.frame(ui, r, config, bookmarks),
            Book::Pdf(p) => p.frame(ui, r, config),
        }
    }
}
