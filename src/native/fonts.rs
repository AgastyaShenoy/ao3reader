//! Fonts that ship with the app, for readers who find them easier: Atkinson Hyperlegible (Braille
//! Institute) and OpenDyslexic, both under the SIL Open Font License (licences in
//! `assets/fonts`). DirectWrite loads fonts from files, so each family is written once to the
//! data folder and read from there.

use crate::storage::config::AppConfig;
use crate::storage::safe::write_atomic;
use std::path::PathBuf;
use std::sync::OnceLock;

type Files = &'static [(&'static str, &'static [u8])];

const ATKINSON: Files = &[
    ("AtkinsonHyperlegible-Regular.ttf", include_bytes!("../../assets/fonts/atkinson/AtkinsonHyperlegible-Regular.ttf")),
    ("AtkinsonHyperlegible-Bold.ttf", include_bytes!("../../assets/fonts/atkinson/AtkinsonHyperlegible-Bold.ttf")),
    ("AtkinsonHyperlegible-Italic.ttf", include_bytes!("../../assets/fonts/atkinson/AtkinsonHyperlegible-Italic.ttf")),
    ("AtkinsonHyperlegible-BoldItalic.ttf", include_bytes!("../../assets/fonts/atkinson/AtkinsonHyperlegible-BoldItalic.ttf")),
];

const OPEN_DYSLEXIC: Files = &[
    ("OpenDyslexic-Regular.otf", include_bytes!("../../assets/fonts/opendyslexic/OpenDyslexic-Regular.otf")),
    ("OpenDyslexic-Bold.otf", include_bytes!("../../assets/fonts/opendyslexic/OpenDyslexic-Bold.otf")),
    ("OpenDyslexic-Italic.otf", include_bytes!("../../assets/fonts/opendyslexic/OpenDyslexic-Italic.otf")),
    ("OpenDyslexic-Bold-Italic.otf", include_bytes!("../../assets/fonts/opendyslexic/OpenDyslexic-Bold-Italic.otf")),
];

/// Writes a family's files to `fonts/<name>` (unless they're already there) and returns the folder.
fn folder(name: &str, files: Files) -> Option<PathBuf> {
    let dir = AppConfig::get_app_dir().join("fonts").join(name);
    std::fs::create_dir_all(&dir).ok()?;
    for (file, bytes) in files {
        let path = dir.join(file);
        if std::fs::metadata(&path).map(|m| m.len() as usize).ok() != Some(bytes.len()) {
            write_atomic(&path, bytes).ok()?;
        }
    }
    Some(dir)
}

/// Folder holding Atkinson Hyperlegible's four styles.
pub fn atkinson() -> Option<PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| folder("atkinson", ATKINSON)).clone()
}

/// Folder holding OpenDyslexic's four styles.
pub fn open_dyslexic() -> Option<PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| folder("opendyslexic", OPEN_DYSLEXIC)).clone()
}
