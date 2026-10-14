use crate::ao3::models::{Bookmark, LibraryEntry, ReadingProgress, ShelfStatus, Work};
use crate::storage::config::AppConfig;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LibraryData {
    pub entries: HashMap<String, LibraryEntry>,
    pub recent_history: Vec<String>, // list of work IDs ordered by most recently opened
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

        let library_file = base_dir.join("library.json");
        let data = if library_file.exists() {
            fs::read_to_string(&library_file)
                .ok()
                .and_then(|content| serde_json::from_str(&content).ok())
                .unwrap_or_default()
        } else {
            LibraryData::default()
        };

        Self { data, base_dir }
    }

    fn works_dir(&self) -> PathBuf {
        self.base_dir.join("works")
    }

    fn work_file_path(&self, work_id: &str) -> PathBuf {
        self.works_dir().join(format!("{work_id}.json"))
    }

    pub fn save(&self) -> Result<(), String> {
        let path = self.base_dir.join("library.json");
        let json = serde_json::to_string_pretty(&self.data).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Add a freshly downloaded or imported work to the library
    pub fn save_work(&mut self, work: &Work) -> Result<(), String> {
        let work_id = work.meta.id.clone();
        let file_path = self.work_file_path(&work_id);

        let json = serde_json::to_string_pretty(work).map_err(|e| e.to_string())?;
        fs::write(&file_path, json).map_err(|e| e.to_string())?;

        // Update or insert library entry
        if let Some(entry) = self.data.entries.get_mut(&work_id) {
            entry.meta = work.meta.clone();
            entry.local_file_exists = true;
        } else {
            let entry = LibraryEntry {
                meta: work.meta.clone(),
                shelf: ShelfStatus::Reading,
                is_favorite: false,
                progress: ReadingProgress::default(),
                bookmarks: Vec::new(),
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
            entry.progress = progress;
            if entry.progress.percentage >= 99.0 || entry.progress.completed {
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
        let file_path = self.work_file_path(work_id);
        let _ = fs::remove_file(file_path);
        let _ = self.save();
    }

    /// Get all library entries
    pub fn entries(&self) -> &HashMap<String, LibraryEntry> {
        &self.data.entries
    }

    /// Get entry for a work
    pub fn get_entry(&self, work_id: &str) -> Option<&LibraryEntry> {
        self.data.entries.get(work_id)
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
