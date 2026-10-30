//! Crash-safe saving for the JSON files that hold the library, follows and settings.
//!
//! A save writes a temporary file next to the real one, flushes it to disk, then renames it over
//! the original: the file on disk is always the old version or the new one, never half of each.
//! Loading falls back to an interrupted save (`.tmp`) and then to the backup taken when the app
//! last started (`.bak`). A file that can't be read is moved aside, never overwritten, so a crash,
//! a bad disk or an older app version can't quietly replace a library with an empty one.

use serde::de::DeserializeOwned;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// `library.json` + `.tmp` → `library.json.tmp`.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// Writes `data` to `path` all or nothing.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = sibling(path, ".tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

/// Reads the JSON file at `path`, or None if there is nothing readable. A damaged file is renamed
/// to `.damaged-<time>` and the last good copy is used instead (an interrupted save, then the
/// backup). With `backup`, a good file is copied to `.bak` as this session's recovery point.
pub fn load_json<T: DeserializeOwned>(path: &Path, backup: bool) -> Option<T> {
    let parse = |p: &Path| fs::read(p).ok().and_then(|b| serde_json::from_slice::<T>(&b).ok());
    if path.exists() {
        if let Some(v) = parse(path) {
            if backup {
                let _ = fs::copy(path, sibling(path, ".bak"));
            }
            return Some(v);
        }
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let _ = fs::rename(path, sibling(path, &format!(".damaged-{stamp}")));
    }
    for suffix in [".tmp", ".bak"] {
        let p = sibling(path, suffix);
        if let Some(v) = parse(&p) {
            // Put the good copy back in place, keeping the backup itself.
            let _ = if suffix == ".tmp" { fs::rename(&p, path) } else { fs::copy(&p, path).map(|_| ()) };
            return Some(v);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ao3-safe-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn files(d: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    #[test]
    fn saves_and_keeps_a_backup() {
        let d = dir("roundtrip");
        let p = d.join("library.json");
        write_atomic(&p, b"[1,2,3]").unwrap();
        assert_eq!(load_json::<Vec<u32>>(&p, true), Some(vec![1, 2, 3]));
        assert_eq!(files(&d), ["library.json", "library.json.bak"]);
        write_atomic(&p, b"[4]").unwrap();
        assert_eq!(load_json::<Vec<u32>>(&p, false), Some(vec![4]));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn damaged_file_is_set_aside_and_the_backup_used() {
        let d = dir("damaged");
        let p = d.join("library.json");
        write_atomic(&p, b"[1,2,3]").unwrap();
        load_json::<Vec<u32>>(&p, true);
        // Cut off mid-write by something other than this code (an old version, a bad disk).
        fs::write(&p, b"[1,2").unwrap();
        assert_eq!(load_json::<Vec<u32>>(&p, true), Some(vec![1, 2, 3]));
        let names = files(&d);
        assert!(names.iter().any(|n| n.starts_with("library.json.damaged-")), "{names:?}");
        assert_eq!(load_json::<Vec<u32>>(&p, false), Some(vec![1, 2, 3]));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn interrupted_save_is_recovered() {
        let d = dir("interrupted");
        let p = d.join("follows.json");
        // The new version was written in full, but the rename never happened.
        fs::write(sibling(&p, ".tmp"), b"[7]").unwrap();
        assert_eq!(load_json::<Vec<u32>>(&p, false), Some(vec![7]));
        assert_eq!(files(&d), ["follows.json"]);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn nothing_readable_keeps_the_damaged_file() {
        let d = dir("garbage");
        let p = d.join("config.json");
        fs::write(&p, b"not json").unwrap();
        assert_eq!(load_json::<Vec<u32>>(&p, true), None);
        let names = files(&d);
        assert_eq!(names.len(), 1);
        assert!(names[0].starts_with("config.json.damaged-"));
        assert_eq!(fs::read(d.join(&names[0])).unwrap(), b"not json");
        let _ = fs::remove_dir_all(&d);
    }
}
