use crate::ao3::models::UserSession;
use crate::reader::pace::ReadingSpeed;
use crate::reader::theme::ReaderThemeType;
use crate::reader::typography::TypographySettings;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub theme: ReaderThemeType,
    pub typography: TypographySettings,
    pub session: UserSession,
    /// Reading speed set by hand; used when `auto_reading_speed` is off or nothing is measured yet.
    pub words_per_minute: u32,
    /// Work out the reading speed from the user's own reading.
    #[serde(default = "default_true")]
    pub auto_reading_speed: bool,
    /// Reading measured so far, for the automatic speed.
    #[serde(default)]
    pub reading_speed: ReadingSpeed,
    pub auto_save_progress: bool,
    pub auto_next_chapter: bool,
    pub confirm_adult_content: bool,
    pub custom_download_dir: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub check_updates_on_startup: bool,
    /// Page-turn and interface animations.
    #[serde(default = "default_true")]
    pub animations: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: ReaderThemeType::Dark,
            typography: TypographySettings::default(),
            session: UserSession::default(),
            words_per_minute: 230,
            auto_reading_speed: true,
            reading_speed: ReadingSpeed::default(),
            auto_save_progress: true,
            auto_next_chapter: true,
            confirm_adult_content: true,
            custom_download_dir: None,
            check_updates_on_startup: true,
            animations: true,
        }
    }
}

impl AppConfig {
    /// The reading speed used for "time left": measured when allowed and known, else the
    /// one set by hand.
    pub fn wpm(&self) -> u32 {
        match self.reading_speed.wpm() {
            Some(w) if self.auto_reading_speed => w,
            _ => self.words_per_minute,
        }
    }

    pub fn get_app_dir() -> PathBuf {
        // A separate data folder (portable installs, demos, testing) without touching the real one.
        if let Some(dir) = std::env::var_os("AO3READER_DATA_DIR").filter(|d| !d.is_empty()) {
            let path = PathBuf::from(dir);
            let _ = fs::create_dir_all(&path);
            return path;
        }
        if let Some(dirs) = ProjectDirs::from("org", "ao3reader", "ao3reader") {
            let path = dirs.data_dir().to_path_buf();
            let _ = fs::create_dir_all(&path);
            path
        } else {
            let path = PathBuf::from("./ao3reader_data");
            let _ = fs::create_dir_all(&path);
            path
        }
    }

    pub fn config_path() -> PathBuf {
        Self::get_app_dir().join("config.json")
    }

    pub fn load() -> Self {
        // A settings file that can't be read is set aside (not overwritten) before the defaults
        // are saved in its place.
        if let Some(config) = crate::storage::safe::load_json(&Self::config_path(), true) {
            return config;
        }
        let default = Self::default();
        let _ = default.save();
        default
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        crate::storage::safe::write_atomic(&path, json.as_bytes()).map_err(|e| e.to_string())
    }
}
