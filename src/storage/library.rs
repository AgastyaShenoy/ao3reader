use crate::ao3::models::{Bookmark, Highlight, LibraryEntry, ReadingProgress, ShelfStatus, Work, WorkMeta};
use crate::storage::config::AppConfig;
use crate::storage::safe::{load_json, write_atomic};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LibraryData {
    pub entries: HashMap<String, LibraryEntry>,
    pub recent_history: Vec<String>, // list of work IDs ordered by most recently opened
    /// The reader's own collections, in the order they were made.
    #[serde(default)]
    pub collections: Vec<Collection>,
}

/// A collection the reader made, like a folder of books. A book can be in several, and
/// deleting a collection leaves its books in the library.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Collection {
    pub id: String,
    pub name: String,
    /// Work ids, in the order they were added.
    #[serde(default)]
    pub works: Vec<String>,
}

pub struct LibraryManager {
    data: LibraryData,
    base_dir: PathBuf,
}

impl LibraryManager {
    pub fn new() -> Self {
        let base_dir = AppConfig::get_app_dir();
        let works_dir = base_dir.join("works");
        let _ = fs::create_dir_all(&works_dir);

        let data = load_json(&base_dir.join("library.json"), true).unwrap_or_default();

        Self { data, base_dir }
    }

    fn works_dir(&self) -> PathBuf {
        self.base_dir.join("works")
    }

    fn work_file_path(&self, work_id: &str) -> PathBuf {
        self.works_dir().join(format!("{work_id}.json"))
    }

    /// Folder holding a work's pictures (EPUB illustrations, covers).
    fn resource_dir(&self, work_id: &str) -> PathBuf {
        self.works_dir().join(format!("{work_id}.res"))
    }

    /// Where a work's picture `name` is kept. `name` comes from `rich_text::image_source`,
    /// which only lets plain file names through.
    pub fn resource_path(work_id: &str, name: &str) -> PathBuf {
        AppConfig::get_app_dir().join("works").join(format!("{work_id}.res")).join(name)
    }

    pub fn save(&self) -> Result<(), String> {
        let path = self.base_dir.join("library.json");
        let json = serde_json::to_string_pretty(&self.data).map_err(|e| e.to_string())?;
        write_atomic(&path, json.as_bytes()).map_err(|e| e.to_string())
    }

    /// Add a freshly downloaded or imported work to the library
    pub fn save_work(&mut self, work: &Work) -> Result<(), String> {
        let work_id = work.meta.id.clone();
        let file_path = self.work_file_path(&work_id);

        // Compact JSON: work files are read by the app only, and can be several MB.
        let json = serde_json::to_string(work).map_err(|e| e.to_string())?;
        write_atomic(&file_path, json.as_bytes()).map_err(|e| e.to_string())?;
        if !work.resources.is_empty() {
            let dir = self.resource_dir(&work_id);
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            for (name, bytes) in &work.resources {
                write_atomic(&dir.join(name), bytes).map_err(|e| e.to_string())?;
            }
        }

        // Update or insert library entry
        if let Some(entry) = self.data.entries.get_mut(&work_id) {
            // A finished story with new chapters isn't finished any more.
            let (was, now) = (entry.meta.stats.chapters_published, work.meta.stats.chapters_published);
            if was > 0 && now > was && entry.progress.completed {
                entry.progress.completed = false;
                if entry.shelf == ShelfStatus::Completed {
                    entry.shelf = ShelfStatus::Reading;
                }
            }
            entry.meta = work.meta.clone();
            entry.local_file_exists = true;
        } else {
            let entry = LibraryEntry {
                meta: work.meta.clone(),
                shelf: ShelfStatus::Reading,
                is_favorite: false,
                progress: ReadingProgress::default(),
                bookmarks: Vec::new(),
                highlights: Vec::new(),
                downloaded_at: Utc::now(),
                local_file_exists: true,
                personal_notes: String::new(),
            };
            self.data.entries.insert(work_id.clone(), entry);
        }

        self.touch_history(&work_id);
        self.save()
    }

    /// Load a work's full content from local disk
    pub fn load_work(&self, work_id: &str) -> Option<Work> {
        let file_path = self.work_file_path(work_id);
        if file_path.exists() {
            let content = fs::read_to_string(file_path).ok()?;
            serde_json::from_str(&content).ok()
        } else {
            None
        }
    }

    /// Update reading progress for a work
    pub fn update_progress(&mut self, work_id: &str, progress: ReadingProgress) {
        if let Some(entry) = self.data.entries.get_mut(work_id) {
            if entry.progress.current_chapter == progress.current_chapter
                && (entry.progress.percentage - progress.percentage).abs() < 0.1
                && (entry.progress.position - progress.position).abs() < 0.01
                && entry.progress.completed == progress.completed
            {
                return;
            }
            entry.progress = progress;
            // Finishing moves a book off Reading; a shelf the reader chose (For later,
            // Favourites) stays.
            if (entry.progress.percentage >= 99.0 || entry.progress.completed) && entry.shelf == ShelfStatus::Reading {
                entry.shelf = ShelfStatus::Completed;
            }
        }
        self.touch_history(work_id);
        let _ = self.save();
    }

    /// Mark work as recently read
    pub fn touch_history(&mut self, work_id: &str) {
        self.data.recent_history.retain(|id| id != work_id);
        self.data.recent_history.insert(0, work_id.to_string());
        if self.data.recent_history.len() > 100 {
            self.data.recent_history.truncate(100);
        }
    }

    /// Add a bookmark to a work
    pub fn add_bookmark(&mut self, work_id: &str, bookmark: Bookmark) {
        if let Some(entry) = self.data.entries.get_mut(work_id) {
            entry.bookmarks.retain(|b| b.id != bookmark.id);
            entry.bookmarks.push(bookmark);
            let _ = self.save();
        }
    }

    /// Adds a highlight, or replaces the one with its id (a new colour or note). A new one over
    /// older ones takes their place, and their notes if it has none.
    pub fn save_highlight(&mut self, work_id: &str, mut h: Highlight) {
        if let Some(entry) = self.data.entries.get_mut(work_id) {
            match entry.highlights.iter_mut().find(|x| x.id == h.id) {
                Some(old) => *old = h,
                None => {
                    let (chapter, start, end) = (h.chapter, h.start, h.end);
                    let overlaps = |x: &Highlight| x.chapter == chapter && x.start < end && start < x.end;
                    if h.note.is_empty() {
                        h.note = entry.highlights.iter().filter(|x| overlaps(x) && !x.note.is_empty()).map(|x| x.note.as_str()).collect::<Vec<_>>().join("\n\n");
                    }
                    entry.highlights.retain(|x| !overlaps(x));
                    entry.highlights.push(h);
                }
            }
            let _ = self.save();
        }
    }

    pub fn remove_highlight(&mut self, work_id: &str, id: &str) {
        if let Some(entry) = self.data.entries.get_mut(work_id) {
            entry.highlights.retain(|h| h.id != id);
            let _ = self.save();
        }
    }

    /// Remove a bookmark from a work
    pub fn remove_bookmark(&mut self, work_id: &str, bookmark_id: &str) {
        if let Some(entry) = self.data.entries.get_mut(work_id) {
            entry.bookmarks.retain(|b| b.id != bookmark_id);
            let _ = self.save();
        }
    }

    /// Toggle favorite status
    pub fn toggle_favorite(&mut self, work_id: &str) {
        if let Some(entry) = self.data.entries.get_mut(work_id) {
            entry.is_favorite = !entry.is_favorite;
            let _ = self.save();
        }
    }

    /// Set shelf status
    pub fn set_shelf(&mut self, work_id: &str, shelf: ShelfStatus) {
        if let Some(entry) = self.data.entries.get_mut(work_id) {
            entry.shelf = shelf;
            let _ = self.save();
        }
    }

    /// Delete a work from library and remove local file
    pub fn delete_work(&mut self, work_id: &str) {
        self.data.entries.remove(work_id);
        self.data.recent_history.retain(|id| id != work_id);
        for c in &mut self.data.collections {
            c.works.retain(|id| id != work_id);
        }
        let file_path = self.work_file_path(work_id);
        let _ = fs::remove_file(file_path);
        let _ = fs::remove_dir_all(self.resource_dir(work_id));
        let _ = self.save();
    }

    /// Frees disk space by deleting the text of works that can be downloaded again when opened
    /// (AO3 works and forum stories). Books from files and catalogues are kept: there's nowhere
    /// to get them back from. Their entries, progress, highlights and notes all stay. Returns how
    /// many were cleared.
    pub fn clear_downloads(&mut self) -> usize {
        let ids: Vec<String> = self
            .data
            .entries
            .iter()
            .filter(|(id, e)| e.local_file_exists && (e.meta.is_ao3() || crate::sources::forum::url_of_id(id).is_some()))
            .map(|(id, _)| id.clone())
            .collect();
        for id in &ids {
            let _ = fs::remove_file(self.work_file_path(id));
            let _ = fs::remove_dir_all(self.resource_dir(id));
            if let Some(e) = self.data.entries.get_mut(id) {
                e.local_file_exists = false;
            }
        }
        let _ = self.save();
        ids.len()
    }

    /// Get all library entries
    pub fn entries(&self) -> &HashMap<String, LibraryEntry> {
        &self.data.entries
    }

    /// Get entry for a work
    pub fn get_entry(&self, work_id: &str) -> Option<&LibraryEntry> {
        self.data.entries.get(work_id)
    }

    /// Brings in works the reader keeps on a site (AO3 bookmarks, watched forum threads) without
    /// downloading them: opening one downloads it. New ones go on `shelf`; all are marked as
    /// favourites if `favorite`, and put in the collection named `collection` (made if need be).
    /// Returns (new to the library, all on the list).
    pub fn import_known(&mut self, metas: &[WorkMeta], collection: &str, shelf: ShelfStatus, favorite: bool) -> (usize, usize) {
        let mut added = 0;
        for m in metas.iter().filter(|m| !m.id.is_empty()) {
            let entry = self.data.entries.entry(m.id.clone()).or_insert_with(|| {
                added += 1;
                LibraryEntry {
                    meta: m.clone(),
                    shelf,
                    is_favorite: false,
                    progress: ReadingProgress::default(),
                    bookmarks: Vec::new(),
                    highlights: Vec::new(),
                    downloaded_at: Utc::now(),
                    local_file_exists: false,
                    personal_notes: String::new(),
                }
            });
            entry.is_favorite |= favorite;
        }
        if let Some(cid) = self.create_collection(collection) {
            if let Some(c) = self.data.collections.iter_mut().find(|c| c.id == cid) {
                for m in metas.iter().filter(|m| !m.id.is_empty()) {
                    if !c.works.contains(&m.id) {
                        c.works.push(m.id.clone());
                    }
                }
            }
        }
        let _ = self.save();
        (added, metas.len())
    }

    pub fn collections(&self) -> &[Collection] {
        &self.data.collections
    }

    pub fn collection(&self, id: &str) -> Option<&Collection> {
        self.data.collections.iter().find(|c| c.id == id)
    }

    /// Makes a collection and returns its id; a blank name makes none. A name that's already
    /// taken (in any case) gives that collection instead of a second one.
    pub fn create_collection(&mut self, name: &str) -> Option<String> {
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        if name.is_empty() {
            return None;
        }
        if let Some(c) = self.data.collections.iter().find(|c| c.name.to_lowercase() == name.to_lowercase()) {
            return Some(c.id.clone());
        }
        let mut n = Utc::now().timestamp_millis();
        while self.collection(&format!("c{n}")).is_some() {
            n += 1;
        }
        let id = format!("c{n}");
        self.data.collections.push(Collection { id: id.clone(), name, works: Vec::new() });
        let _ = self.save();
        Some(id)
    }

    pub fn rename_collection(&mut self, id: &str, name: &str) {
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        if let Some(c) = self.data.collections.iter_mut().find(|c| c.id == id).filter(|_| !name.is_empty()) {
            c.name = name;
            let _ = self.save();
        }
    }

    /// Removes the collection only: its books stay in the library.
    pub fn delete_collection(&mut self, id: &str) {
        self.data.collections.retain(|c| c.id != id);
        let _ = self.save();
    }

    /// Puts a work in a collection, or takes it out.
    pub fn set_in_collection(&mut self, id: &str, work_id: &str, on: bool) {
        if let Some(c) = self.data.collections.iter_mut().find(|c| c.id == id) {
            let has = c.works.iter().any(|w| w == work_id);
            if on && !has && self.data.entries.contains_key(work_id) {
                c.works.push(work_id.to_string());
            } else if !on && has {
                c.works.retain(|w| w != work_id);
            } else {
                return;
            }
            let _ = self.save();
        }
    }

    pub fn in_collection(&self, id: &str, work_id: &str) -> bool {
        self.collection(id).is_some_and(|c| c.works.iter().any(|w| w == work_id))
    }

    /// Filter and sort entries
    pub fn query_entries(
        &self,
        search_query: &str,
        shelf_filter: Option<ShelfStatus>,
        only_favorites: bool,
    ) -> Vec<&LibraryEntry> {
        let q = search_query.trim().to_lowercase();
        let mut list: Vec<&LibraryEntry> = self
            .data
            .entries
            .values()
            .filter(|entry| {
                if only_favorites && !entry.is_favorite {
                    return false;
                }
                if let Some(shelf) = shelf_filter {
                    if entry.shelf != shelf {
                        return false;
                    }
                }
                if q.is_empty() {
                    return true;
                }

                // Check title, authors, fandoms, tags
                entry.meta.title.to_lowercase().contains(&q)
                    || entry.meta.authors.iter().any(|a| a.to_lowercase().contains(&q))
                    || entry.meta.fandoms.iter().any(|f| f.to_lowercase().contains(&q))
                    || entry.meta.characters.iter().any(|c| c.to_lowercase().contains(&q))
                    || entry.meta.relationships.iter().any(|r| r.to_lowercase().contains(&q))
                    || entry.meta.freeform_tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect();

        // Sort by most recently read
        list.sort_by(|a, b| b.progress.last_read.cmp(&a.progress.last_read));
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(name: &str, works: &[&str]) -> LibraryManager {
        let dir = std::env::temp_dir().join(format!("ao3-library-{name}-{}", std::process::id()));
        let _ = fs::create_dir_all(dir.join("works"));
        let mut lib = LibraryManager { data: LibraryData::default(), base_dir: dir };
        for id in works {
            let entry = LibraryEntry {
                meta: WorkMeta { id: id.to_string(), ..WorkMeta::default() },
                shelf: ShelfStatus::Reading,
                is_favorite: false,
                progress: ReadingProgress::default(),
                bookmarks: Vec::new(),
                highlights: Vec::new(),
                downloaded_at: Utc::now(),
                local_file_exists: true,
                personal_notes: String::new(),
            };
            lib.data.entries.insert(id.to_string(), entry);
        }
        lib
    }

    #[test]
    fn collections_hold_books_until_they_go() {
        let mut lib = library("collections", &["a", "b"]);
        assert_eq!(lib.create_collection("   "), None);
        let austen = lib.create_collection("  Austen  novels ").unwrap();
        assert_eq!(lib.collection(&austen).unwrap().name, "Austen novels");
        assert_eq!(lib.create_collection("austen NOVELS"), Some(austen.clone()), "same name, same collection");
        let summer = lib.create_collection("Summer").unwrap();
        assert_ne!(austen, summer);

        lib.set_in_collection(&austen, "a", true);
        lib.set_in_collection(&austen, "a", true);
        lib.set_in_collection(&austen, "b", true);
        lib.set_in_collection(&austen, "missing", true);
        lib.set_in_collection(&summer, "a", true);
        assert_eq!(lib.collection(&austen).unwrap().works, ["a", "b"]);
        lib.set_in_collection(&austen, "b", false);
        assert!(!lib.in_collection(&austen, "b"));

        lib.rename_collection(&summer, "");
        lib.rename_collection(&summer, "Beach reads");
        assert_eq!(lib.collection(&summer).unwrap().name, "Beach reads");

        // Deleting a book takes it out of every collection; deleting a collection keeps its books.
        lib.delete_work("a");
        assert!(lib.collections().iter().all(|c| c.works.is_empty()));
        lib.set_in_collection(&summer, "b", true);
        lib.delete_collection(&summer);
        assert_eq!(lib.collections().len(), 1);
        assert!(lib.get_entry("b").is_some());

        // Saved and read back, including libraries saved before collections existed.
        let saved = serde_json::to_string(&lib.data).unwrap();
        let back: LibraryData = serde_json::from_str(&saved).unwrap();
        assert_eq!(back.collections, lib.data.collections);
        let old: LibraryData = serde_json::from_str(r#"{"entries":{},"recent_history":[]}"#).unwrap();
        assert!(old.collections.is_empty());
        let _ = fs::remove_dir_all(&lib.base_dir);
    }

    #[test]
    fn imported_lists_join_without_downloading() {
        let mut lib = library("import", &["a"]);
        let meta = |id: &str| WorkMeta { id: id.into(), title: format!("Work {id}"), ..WorkMeta::default() };
        let (added, total) = lib.import_known(&[meta("a"), meta("b"), meta("")], "AO3 bookmarks", ShelfStatus::MarkedForLater, true);
        assert_eq!((added, total), (1, 3));
        let b = lib.get_entry("b").unwrap();
        assert!(!b.local_file_exists && b.is_favorite && b.shelf == ShelfStatus::MarkedForLater);
        assert_eq!(lib.get_entry("a").unwrap().shelf, ShelfStatus::Reading, "a book already there keeps its shelf");
        assert!(lib.get_entry("a").unwrap().is_favorite);
        let c = lib.collections().iter().find(|c| c.name == "AO3 bookmarks").unwrap();
        assert_eq!(c.works, ["a", "b"]);
        // Importing again adds nothing new.
        assert_eq!(lib.import_known(&[meta("b")], "AO3 bookmarks", ShelfStatus::MarkedForLater, true), (0, 1));
        assert_eq!(lib.collections().len(), 1);
        let _ = fs::remove_dir_all(&lib.base_dir);
    }
}

