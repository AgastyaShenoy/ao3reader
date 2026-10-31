use crate::ao3::models::UserSession;
use crate::reader::pace::ReadingSpeed;
use crate::reader::theme::{ReaderThemeType, Rgb, ThemePalette};
use crate::reader::typography::TypographySettings;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_true() -> bool {
    true
}

fn one() -> f32 {
    1.0
}

fn default_light() -> ReaderThemeType {
    ReaderThemeType::Linen
}

fn default_dark() -> ReaderThemeType {
    ReaderThemeType::Dark
}

fn default_custom_bg() -> u32 {
    0xFAF4E6
}

fn default_custom_text() -> u32 {
    0x282420
}

fn rgb(c: u32) -> Rgb {
    Rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
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
    /// Interface size: 1.0 is the system's own size. Only ever changed by the user.
    #[serde(default = "one")]
    pub ui_scale: f32,
    /// Follow the system's light or dark mode (and its high-contrast setting).
    #[serde(default)]
    pub follow_system: bool,
    /// Themes used for the system's light and dark modes when following it.
    #[serde(default = "default_light")]
    pub light_theme: ReaderThemeType,
    #[serde(default = "default_dark")]
    pub dark_theme: ReaderThemeType,
    /// Page and text colours of the "Your colours" theme (0xRRGGBB).
    #[serde(default = "default_custom_bg")]
    pub custom_bg: u32,
    #[serde(default = "default_custom_text")]
    pub custom_text: u32,
    /// Android: bigger buttons with more room between them (48 dp touch targets).
    #[serde(default)]
    pub larger_targets: bool,
    /// Read-aloud speed: 1.0 is the voice's normal pace.
    #[serde(default = "one")]
    pub speech_rate: f32,
    /// Catalogs the reader added (Project Gutenberg is always there).
    #[serde(default)]
    pub catalogs: Vec<crate::sources::Catalog>,
    /// Sign-ins to the story forums (SpaceBattles, Sufficient Velocity): the forum's own
    /// "remember me" cookies, never a password.
    #[serde(default)]
    pub forum_accounts: Vec<crate::sources::accounts::ForumAccount>,
    /// Windows: where the window was last (left, top, right, bottom in screen pixels) and whether
    /// it was maximised.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<([i32; 4], bool)>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: ReaderThemeType::Linen,
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
            ui_scale: 1.0,
            // New installs match the system (settings saved before this keep their own theme).
            follow_system: true,
            light_theme: default_light(),
            dark_theme: default_dark(),
            custom_bg: default_custom_bg(),
            custom_text: default_custom_text(),
            larger_targets: false,
            speech_rate: 1.0,
            catalogs: Vec::new(),
            forum_accounts: Vec::new(),
            window: None,
        }
    }
}

impl AppConfig {
    /// Colours of the current theme ("Your colours" comes from `custom_bg` and `custom_text`).
    pub fn palette(&self) -> ThemePalette {
        match self.theme {
            ReaderThemeType::Custom => ThemePalette::custom(rgb(self.custom_bg), rgb(self.custom_text)),
            t => t.palette(),
        }
    }

    /// Palette of any theme, with "Your colours" taken from these settings.
    pub fn palette_of(&self, t: ReaderThemeType) -> ThemePalette {
        match t {
            ReaderThemeType::Custom => ThemePalette::custom(rgb(self.custom_bg), rgb(self.custom_text)),
            t => t.palette(),
        }
    }

    /// The theme to use given the system's dark mode and high-contrast setting, when following
    /// the system; otherwise the chosen theme.
    pub fn system_theme(&self, dark: bool, high_contrast: bool) -> ReaderThemeType {
        if !self.follow_system {
            self.theme
        } else if high_contrast {
            ReaderThemeType::HighContrast
        } else if dark {
            self.dark_theme
        } else {
            self.light_theme
        }
    }

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
