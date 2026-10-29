use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SystemFont {
    pub name: String,
    pub path: PathBuf,
}

static SYSTEM_FONTS_CACHE: OnceLock<Vec<SystemFont>> = OnceLock::new();
static CURATED_FONTS_CACHE: OnceLock<Vec<SystemFont>> = OnceLock::new();

pub struct FontManager;

impl FontManager {
    /// Extracts the human-readable font name from TrueType / OpenType font binary data.
    pub fn parse_font_name_from_bytes(bytes: &[u8]) -> Option<String> {
        if bytes.len() < 12 {
            return None;
        }

        // TrueType offset table has numTables at offset 4..6
        let num_tables = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
        let mut name_table_offset = None;

        for i in 0..num_tables {
            let entry_offset = 12 + i * 16;
            if entry_offset + 16 > bytes.len() {
                break;
            }
            let tag = &bytes[entry_offset..entry_offset + 4];
            if tag == b"name" {
                let offset = u32::from_be_bytes([
                    bytes[entry_offset + 8],
                    bytes[entry_offset + 9],
                    bytes[entry_offset + 10],
                    bytes[entry_offset + 11],
                ]) as usize;
                name_table_offset = Some(offset);
                break;
            }
        }

        let name_offset = name_table_offset?;
        if name_offset + 6 > bytes.len() {
            return None;
        }

        let count = u16::from_be_bytes([bytes[name_offset + 2], bytes[name_offset + 3]]) as usize;
        let string_storage_offset =
            name_offset + u16::from_be_bytes([bytes[name_offset + 4], bytes[name_offset + 5]]) as usize;

        let mut candidate_name = None;

        for i in 0..count {
            let rec = name_offset + 6 + i * 12;
            if rec + 12 > bytes.len() {
                break;
            }
            let platform_id = u16::from_be_bytes([bytes[rec], bytes[rec + 1]]);
            let name_id = u16::from_be_bytes([bytes[rec + 6], bytes[rec + 7]]);
            let length = u16::from_be_bytes([bytes[rec + 8], bytes[rec + 9]]) as usize;
            let str_offset =
                string_storage_offset + u16::from_be_bytes([bytes[rec + 10], bytes[rec + 11]]) as usize;

            if str_offset + length > bytes.len() {
                continue;
            }

            // name_id 4 = Full font name, name_id 1 = Font Family
            if name_id == 4 || (name_id == 1 && candidate_name.is_none()) {
                let str_bytes = &bytes[str_offset..str_offset + length];
                let text = if platform_id == 0 || platform_id == 3 {
                    let u16_chars: Vec<u16> = str_bytes
                        .chunks_exact(2)
                        .map(|c| u16::from_be_bytes([c[0], c[1]]))
                        .collect();
                    String::from_utf16_lossy(&u16_chars)
                } else {
                    String::from_utf8_lossy(str_bytes).to_string()
                };

                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    if name_id == 4 {
                        return Some(trimmed.to_string());
                    } else {
                        candidate_name = Some(trimmed.to_string());
                    }
                }
            }
        }

        candidate_name
    }

    /// Returns standard font directories on Windows, macOS, or Linux.
    pub fn get_system_font_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        #[cfg(target_os = "windows")]
        {
            dirs.push(PathBuf::from("C:\\Windows\\Fonts"));
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                let user_fonts = PathBuf::from(local_app_data)
                    .join("Microsoft")
                    .join("Windows")
                    .join("Fonts");
                if user_fonts.exists() {
                    dirs.push(user_fonts);
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            dirs.push(PathBuf::from("/System/Library/Fonts"));
            dirs.push(PathBuf::from("/Library/Fonts"));
            if let Ok(home) = std::env::var("HOME") {
                dirs.push(PathBuf::from(home).join("Library/Fonts"));
            }
        }

        #[cfg(target_os = "linux")]
        {
            dirs.push(PathBuf::from("/usr/share/fonts"));
            dirs.push(PathBuf::from("/usr/local/share/fonts"));
            if let Ok(home) = std::env::var("HOME") {
                dirs.push(PathBuf::from(home).join(".local/share/fonts"));
                dirs.push(PathBuf::from(home).join(".fonts"));
            }
        }

        dirs
    }

    /// Scans the system for installed TrueType and OpenType fonts, returning a sorted list.
    pub fn scan_installed_fonts() -> Vec<SystemFont> {
        let mut fonts = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        for dir in Self::get_system_font_dirs() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|s| s.to_lowercase())
                        .unwrap_or_default();

                    if ext == "ttf" || ext == "otf" {
                        // Skip heavy/italic sub-variations to keep the font list clean
                        let fname_lower = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .map(|s| s.to_lowercase())
                            .unwrap_or_default();

                        // Only take regular / normal font faces
                        if fname_lower.contains("bold")
                            || fname_lower.contains("italic")
                            || fname_lower.contains("oblique")
                            || fname_lower.contains("black")
                            || fname_lower.contains("heavy")
                            || fname_lower.contains("light")
                            || fname_lower.ends_with("bd")
                            || fname_lower.ends_with("bi")
                            || fname_lower.ends_with('i') && !fname_lower.starts_with("calibri")
                        {
                            continue;
                        }

                        if let Ok(bytes) = std::fs::read(&path) {
                            if let Some(name) = Self::parse_font_name_from_bytes(&bytes) {
                                let clean_name = name
                                    .replace(" Regular", "")
                                    .replace(" Normal", "")
                                    .trim()
                                    .to_string();

                                if !seen_names.contains(&clean_name) {
                                    seen_names.insert(clean_name.clone());
                                    fonts.push(SystemFont {
                                        name: clean_name,
                                        path,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        fonts.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        fonts
    }

    /// Curated popular e-reader and reading fonts if installed on the system.
    pub fn get_curated_reading_fonts() -> Vec<SystemFont> {
        let all_fonts = Self::cached_system_fonts();
        let preferred = [
            "Georgia",
            "Palatino Linotype",
            "Book Antiqua",
            "Garamond",
            "Constantia",
            "Baskerville Old Face",
            "Sitka",
            "Calibri",
            "Segoe UI",
            "Times New Roman",
            "Cambria",
            "Verdana",
            "Consolas",
            "Comic Sans MS",
        ];

        let mut curated = Vec::new();
        for pref in preferred {
            if let Some(found) = all_fonts
                .iter()
                .find(|f| f.name.to_lowercase().starts_with(&pref.to_lowercase()))
            {
                curated.push(found.clone());
            }
        }
        curated
    }

    /// Cached list of installed system fonts (scanned once on demand).
    pub fn cached_system_fonts() -> &'static [SystemFont] {
        SYSTEM_FONTS_CACHE.get_or_init(Self::scan_installed_fonts).as_slice()
    }

    /// Cached list of curated popular reading fonts found on system.
    pub fn cached_curated_fonts() -> &'static [SystemFont] {
        CURATED_FONTS_CACHE.get_or_init(Self::get_curated_reading_fonts).as_slice()
    }

    /// Prompts the user with an OS open file dialog to choose a .ttf or .otf font file.
    pub fn pick_custom_font_file() -> Option<(String, PathBuf)> {
        let picked = rfd::FileDialog::new()
            .set_title("Select Font File")
            .add_filter("Font Files (*.ttf, *.otf)", &["ttf", "otf"])
            .pick_file()?;

        let bytes = std::fs::read(&picked).ok()?;
        let font_name = Self::parse_font_name_from_bytes(&bytes)
            .unwrap_or_else(|| {
                picked
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("CustomFont")
                    .to_string()
            });

        Some((font_name, picked))
    }
}

#[cfg(feature = "legacy-ui")]
impl FontManager {
    /// Constructs FontDefinitions ensuring ReaderBookFont is always bound to avoid epaint panics.
    pub fn get_font_definitions(
        custom_font_path: Option<&Path>,
        custom_font_name: Option<&str>,
    ) -> egui::FontDefinitions {
        let mut font_defs = egui::FontDefinitions::default();
        let mut reader_fonts = Vec::new();

        if let (Some(path), Some(name)) = (custom_font_path, custom_font_name) {
            if let Ok(bytes) = std::fs::read(path) {
                // Validate font format before registering to prevent epaint parse panics
                if ttf_parser::Face::parse(&bytes, 0).is_ok() {
                    font_defs.font_data.insert(
                        name.to_string(),
                        Arc::new(egui::FontData::from_owned(bytes)),
                    );
                    reader_fonts.push(name.to_string());
                }
            }
        }

        // If no custom font was provided, check if Georgia exists on Windows
        #[cfg(target_os = "windows")]
        if reader_fonts.is_empty() {
            let georgia = PathBuf::from("C:\\Windows\\Fonts\\georgia.ttf");
            if let Ok(bytes) = std::fs::read(&georgia) {
                if ttf_parser::Face::parse(&bytes, 0).is_ok() {
                    font_defs.font_data.insert(
                        "Georgia".to_string(),
                        Arc::new(egui::FontData::from_owned(bytes)),
                    );
                    reader_fonts.push("Georgia".to_string());
                }
            }
        }

        // Always guarantee fallbacks to egui's default proportional fonts
        if let Some(prop_fonts) = font_defs.families.get(&egui::FontFamily::Proportional) {
            for f in prop_fonts {
                if !reader_fonts.contains(f) {
                    reader_fonts.push(f.clone());
                }
            }
        }

        // ALWAYS bind ReaderBookFont so epaint NEVER panics!
        font_defs.families.insert(
            egui::FontFamily::Name(crate::reader::typography::FontFamilyChoice::READER_BOOK_FONT.into()),
            reader_fonts,
        );

        font_defs
    }

    /// Initializes fonts on startup, ensuring ReaderBookFont is always safely bound.
    pub fn init_fonts(
        ctx: &egui::Context,
        custom_font_path: Option<&Path>,
        custom_font_name: Option<&str>,
    ) {
        ctx.set_fonts(Self::get_font_definitions(custom_font_path, custom_font_name));
    }

    /// Applies a font file to the current egui Context.
    pub fn apply_font_to_ctx(
        ctx: &egui::Context,
        font_path: &Path,
        font_name: &str,
    ) -> Result<(), String> {
        let _bytes =
            std::fs::read(font_path).map_err(|e| format!("Failed to read font file '{:?}': {e}", font_path))?;

        ctx.set_fonts(Self::get_font_definitions(Some(font_path), Some(font_name)));
        Ok(())
    }

    /// Resets the egui context font definitions back to standard defaults,
    /// while keeping ReaderBookFont safely bound.
    pub fn reset_fonts(ctx: &egui::Context) {
        ctx.set_fonts(Self::get_font_definitions(None, None));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_font_dirs_not_empty() {
        let dirs = FontManager::get_system_font_dirs();
        assert!(!dirs.is_empty(), "Should find at least 1 font directory");
        assert!(dirs[0].exists(), "Default font directory should exist");
    }

    #[test]
    fn test_scan_finds_standard_windows_fonts() {
        let fonts = FontManager::scan_installed_fonts();
        assert!(!fonts.is_empty(), "Should find fonts installed on the system");

        // Verify common font presence on Windows
        let has_georgia = fonts.iter().any(|f| f.name.contains("Georgia"));
        let has_arial = fonts.iter().any(|f| f.name.contains("Arial"));
        assert!(has_georgia || has_arial, "Should find Arial or Georgia on system");
    }
}
