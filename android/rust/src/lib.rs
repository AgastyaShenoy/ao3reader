//! Bridge from the Android app to the AO3 Reader core.
//!
//! Kotlin calls one JNI function, `Core.call(method, json)`, and gets JSON back:
//! `{"ok": ...}` or `{"err": "message"}`. A single coarse entry point keeps the native
//! surface tiny and needs no binding generator or JNA at runtime. Network methods
//! (`fetch`, `epub`, `login`, `cookie`, `kudos`) block, so Kotlin calls them off the UI thread;
//! they do not hold the state lock while waiting on AO3.

use ao3reader::ao3::client::Ao3Client;
use ao3reader::ao3::models::{Bookmark, Category, LibraryEntry, ReadingProgress, ShelfStatus, UserSession, Work};
use ao3reader::reader::rich_text::{is_landmark, paragraph_display, parse_inline, word_count, ParagraphKind, RichText};
use ao3reader::reader::theme::{ReaderThemeType, Rgb};
use ao3reader::storage::config::AppConfig;
use ao3reader::storage::export::WorkExporter;
use ao3reader::storage::library::LibraryManager;
use serde_json::{json, Value};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::Mutex;

/// The system allocator plus a running count of the bytes the Rust core holds, so the memory
/// readout in Settings can tell the core apart from Android's own native memory.
struct Counting;

static RUST_LIVE: AtomicUsize = AtomicUsize::new(0);
static RUST_PEAK: AtomicUsize = AtomicUsize::new(0);

fn grew(by: usize) {
    let now = RUST_LIVE.fetch_add(by, Relaxed) + by;
    RUST_PEAK.fetch_max(now, Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = System.alloc(l);
        if !p.is_null() {
            grew(l.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = System.alloc_zeroed(l);
        if !p.is_null() {
            grew(l.size());
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l);
        RUST_LIVE.fetch_sub(l.size(), Relaxed);
    }

    unsafe fn realloc(&self, p: *mut u8, l: Layout, new_size: usize) -> *mut u8 {
        let q = System.realloc(p, l, new_size);
        if !q.is_null() {
            if new_size >= l.size() {
                grew(new_size - l.size());
            } else {
                RUST_LIVE.fetch_sub(l.size() - new_size, Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

const MAX_MATCHES: usize = 2000;

struct State {
    config: AppConfig,
    library: LibraryManager,
    client: Ao3Client,
    /// The work open in the reader.
    work: Option<Work>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

type R = Result<Value, String>;

fn with_state<T>(f: impl FnOnce(&mut State) -> Result<T, String>) -> Result<T, String> {
    let mut guard = STATE.lock().map_err(|_| "state poisoned".to_string())?;
    let st = guard.as_mut().ok_or("not initialised")?;
    f(st)
}

fn arg_str<'a>(a: &'a Value, key: &str) -> Result<&'a str, String> {
    a.get(key).and_then(Value::as_str).ok_or_else(|| format!("missing '{key}'"))
}

fn arg_u64(a: &Value, key: &str) -> Result<u64, String> {
    a.get(key).and_then(Value::as_u64).ok_or_else(|| format!("missing '{key}'"))
}

fn hex(c: Rgb) -> u32 {
    ((c.0 as u32) << 16) | ((c.1 as u32) << 8) | c.2 as u32
}

/// Byte offset in `s` to a UTF-16 index, which is what Android text APIs use.
fn u16_index(s: &str, byte: usize) -> usize {
    s[..byte.min(s.len())].encode_utf16().count()
}

fn rich_json(r: &RichText) -> Value {
    let spans: Vec<Value> = r
        .spans
        .iter()
        .map(|s| {
            let style = match s.style {
                ao3reader::reader::rich_text::InlineStyle::Italic => "i",
                ao3reader::reader::rich_text::InlineStyle::Bold => "b",
                ao3reader::reader::rich_text::InlineStyle::Strike => "s",
            };
            json!([u16_index(&r.text, s.start), u16_index(&r.text, s.end), style])
        })
        .collect();
    json!({ "text": r.text, "spans": spans })
}

fn category_color(c: &Category) -> u32 {
    match c {
        Category::MM => 0x2E75B6,
        Category::FM => 0x9B59B6,
        Category::FF => 0xE74C3C,
        Category::Gen => 0x27AE60,
        Category::Multi => 0xE67E22,
        _ => 0x6E7781,
    }
}

fn meta_json(m: &ao3reader::ao3::models::WorkMeta) -> Value {
    let [r, g, b] = m.rating.badge_color();
    json!({
        "id": m.id,
        "title": m.title,
        "authors": m.authors,
        "summary": m.summary_text,
        "rating": m.rating.display_name(),
        "rating_color": ((r as u32) << 16) | ((g as u32) << 8) | b as u32,
        "categories": m.categories.iter().map(|c| json!({"name": c.display_name(), "color": category_color(c)})).collect::<Vec<_>>(),
        "warnings": m.warnings,
        "fandoms": m.fandoms,
        "relationships": m.relationships,
        "characters": m.characters,
        "tags": m.freeform_tags,
        "words": m.stats.words,
        "chapters": m.stats.chapters_published,
        "chapters_total": m.stats.chapters_total,
        "kudos": m.stats.kudos,
        "bookmarks": m.stats.bookmarks,
        "hits": m.stats.hits,
        "published": m.stats.published_date,
        "complete": m.is_complete,
    })
}

fn shelf_name(s: ShelfStatus) -> &'static str {
    match s {
        ShelfStatus::Reading => "reading",
        ShelfStatus::Completed => "completed",
        ShelfStatus::MarkedForLater => "later",
        ShelfStatus::Favorites => "favorites",
    }
}

fn entry_json(e: &LibraryEntry) -> Value {
    json!({
        "meta": meta_json(&e.meta),
        "shelf": shelf_name(e.shelf),
        "favorite": e.is_favorite,
        "chapter": e.progress.current_chapter,
        "position": e.progress.position,
        "percentage": e.progress.percentage,
        "completed": e.progress.completed,
        "last_read": e.progress.last_read.timestamp(),
        "added": e.downloaded_at.timestamp(),
    })
}

fn bookmarks_json(e: Option<&LibraryEntry>) -> Value {
    let mut list: Vec<&Bookmark> = e.map(|e| e.bookmarks.iter().collect()).unwrap_or_default();
    list.sort_by_key(|b| (b.chapter_index, b.paragraph_index));
    Value::Array(
        list.iter()
            .map(|b| json!({"id": b.id, "chapter": b.chapter_index, "para": b.paragraph_index, "note": b.note, "excerpt": b.excerpt}))
            .collect(),
    )
}

fn paragraph_words(work: &Work, ch: usize) -> Vec<usize> {
    work.chapters.get(ch).map(|c| c.paragraphs.iter().map(|p| word_count(p)).collect()).unwrap_or_default()
}

/// (percentage, words left in work, words left in chapter) for a reading position.
fn progress_stats(work: &Work, ch: usize, position: f64) -> (f64, usize, usize) {
    let chapter_words: Vec<usize> = (0..work.chapters.len()).map(|i| paragraph_words(work, i).iter().sum()).collect();
    let total: usize = chapter_words.iter().sum();
    let paras = paragraph_words(work, ch);
    let idx = position.max(0.0).floor() as usize;
    let mut in_ch: f64 = paras.iter().take(idx).sum::<usize>() as f64;
    if let Some(w) = paras.get(idx) {
        in_ch += *w as f64 * (position - position.floor());
    }
    let read = chapter_words[..ch.min(chapter_words.len())].iter().sum::<usize>() as f64 + in_ch;
    let pct = (read / total.max(1) as f64 * 100.0).clamp(0.0, 100.0);
    let ch_words = chapter_words.get(ch).copied().unwrap_or(0) as f64;
    (pct, (total as f64 - read).max(0.0) as usize, (ch_words - in_ch).max(0.0) as usize)
}

fn themes_json() -> Value {
    Value::Array(
        ReaderThemeType::all()
            .iter()
            .map(|t| {
                let p = t.palette();
                json!({
                    "id": serde_json::to_value(t).unwrap_or(Value::Null),
                    "name": t.display_name(),
                    "dark": p.is_dark,
                    "bg": hex(p.bg), "surface": hex(p.surface), "card": hex(p.card), "text": hex(p.text),
                    "muted": hex(p.text_muted), "accent": hex(p.accent), "border": hex(p.border),
                    "quote_bg": hex(p.quote_bg), "notes_bg": hex(p.notes_bg),
                })
            })
            .collect(),
    )
}

fn signed_in(s: &UserSession) -> bool {
    s.is_logged_in && (s.session_cookie.is_some() || s.remember_token.is_some())
}

fn session_json(s: &UserSession) -> Value {
    json!({ "signed_in": signed_in(s), "stale": s.is_logged_in && !signed_in(s), "username": s.username })
}

pub fn handle(method: &str, arg: &str) -> String {
    let a: Value = serde_json::from_str(arg).unwrap_or(Value::Null);
    let res = dispatch(method, &a);
    match res {
        Ok(v) => json!({ "ok": v }).to_string(),
        Err(e) => json!({ "err": e }).to_string(),
    }
}

fn dispatch(method: &str, a: &Value) -> R {
    match method {
        "init" => {
            let dir = arg_str(a, "dir")?;
            // The core resolves its data folder from this variable (set before any file access).
            std::env::set_var("AO3READER_DATA_DIR", dir);
            let config = AppConfig::load();
            let client = Ao3Client::new(Some(&config.session));
            let library = LibraryManager::new();
            let mut guard = STATE.lock().map_err(|_| "state poisoned")?;
            *guard = Some(State { config, library, client, work: None });
            Ok(json!({ "themes": themes_json() }))
        }
        "config.get" => with_state(|s| serde_json::to_value(&s.config).map_err(|e| e.to_string())),
        "config.set" => with_state(|s| {
            let mut cfg: AppConfig = serde_json::from_value(a.get("config").cloned().unwrap_or(Value::Null)).map_err(|e| e.to_string())?;
            // The session is only changed by sign-in and sign-out.
            cfg.session = s.config.session.clone();
            s.config = cfg;
            s.config.save()?;
            Ok(Value::Null)
        }),
        "session" => with_state(|s| Ok(session_json(&s.config.session))),
        "library" => with_state(|s| {
            let mut list: Vec<&LibraryEntry> = s.library.entries().values().collect();
            list.sort_by(|x, y| y.progress.last_read.cmp(&x.progress.last_read));
            Ok(Value::Array(list.into_iter().map(entry_json).collect()))
        }),
        "library.favorite" => with_state(|s| {
            s.library.toggle_favorite(arg_str(a, "id")?);
            Ok(Value::Null)
        }),
        "library.shelf" => with_state(|s| {
            let shelf = match arg_str(a, "shelf")? {
                "completed" => ShelfStatus::Completed,
                "later" => ShelfStatus::MarkedForLater,
                "favorites" => ShelfStatus::Favorites,
                _ => ShelfStatus::Reading,
            };
            s.library.set_shelf(arg_str(a, "id")?, shelf);
            Ok(Value::Null)
        }),
        "library.delete" => with_state(|s| {
            let id = arg_str(a, "id")?;
            if s.work.as_ref().is_some_and(|w| w.meta.id == id) {
                s.work = None;
            }
            s.library.delete_work(id);
            Ok(Value::Null)
        }),
        "work.open" => with_state(|s| {
            let id = arg_str(a, "id")?;
            let mut work = s.library.load_work(id).ok_or("missing")?;
            for c in &mut work.chapters {
                c.raw_html = String::new();
            }
            s.library.touch_history(id);
            let entry = s.library.get_entry(id);
            let chapters: Vec<Value> = (0..work.chapters.len())
                .map(|i| json!({"title": work.chapters[i].title, "words": paragraph_words(&work, i).iter().sum::<usize>()}))
                .collect();
            let out = json!({
                "meta": meta_json(&work.meta),
                "chapters": chapters,
                "chapter": entry.map_or(1, |e| e.progress.current_chapter),
                "position": entry.map_or(0.0, |e| e.progress.position),
                "bookmarks": bookmarks_json(entry),
            });
            s.work = Some(work);
            Ok(out)
        }),
        "work.chapter" => with_state(|s| {
            let work = s.work.as_ref().ok_or("no work open")?;
            let idx = arg_u64(a, "index")? as usize;
            let ch = work.chapters.get(idx).ok_or("no such chapter")?;
            let mut blocks = Vec::new();
            let mut push = |kind: &str, para: usize, r: &RichText| {
                let mut v = rich_json(r);
                v["kind"] = json!(kind);
                v["para"] = json!(para);
                blocks.push(v);
            };
            if let Some(t) = ch.summary.as_deref().filter(|t| !t.trim().is_empty()) {
                push("summary", 0, &parse_inline(t.trim()));
            }
            if let Some(t) = ch.pre_notes.as_deref().filter(|t| !t.trim().is_empty()) {
                push("notes", 0, &parse_inline(t.trim()));
            }
            for (i, p) in ch.paragraphs.iter().enumerate() {
                if is_landmark(i, p) {
                    continue;
                }
                match paragraph_display(p) {
                    (ParagraphKind::Divider, r) => push("divider", i, &r),
                    (ParagraphKind::Quote, r) => push("quote", i, &r),
                    (ParagraphKind::Text, r) => push("para", i, &r),
                }
            }
            if let Some(t) = ch.post_notes.as_deref().filter(|t| !t.trim().is_empty()) {
                push("endnotes", ch.paragraphs.len(), &parse_inline(t.trim()));
            }
            Ok(json!({ "title": ch.title, "paragraphs": ch.paragraphs.len(), "blocks": blocks }))
        }),
        "progress.stats" => with_state(|s| {
            let work = s.work.as_ref().ok_or("no work open")?;
            let ch = arg_u64(a, "chapter")? as usize;
            let pos = a.get("position").and_then(Value::as_f64).unwrap_or(0.0);
            let (pct, left_work, left_ch) = progress_stats(work, ch, pos);
            Ok(json!({ "percentage": pct, "words_left": left_work, "chapter_words_left": left_ch }))
        }),
        "progress.save" => with_state(|s| {
            let work = s.work.as_ref().ok_or("no work open")?;
            let id = work.meta.id.clone();
            let ch = arg_u64(a, "chapter")? as usize;
            let pos = a.get("position").and_then(Value::as_f64).unwrap_or(0.0);
            let completed = a.get("completed").and_then(Value::as_bool).unwrap_or(false);
            let (pct, _, _) = progress_stats(work, ch, pos);
            s.library.update_progress(
                &id,
                ReadingProgress {
                    current_chapter: ch + 1,
                    scroll_offset: 0.0,
                    position: pos as f32,
                    percentage: if completed { 100.0 } else { pct as f32 },
                    last_read: chrono::Utc::now(),
                    completed,
                },
            );
            Ok(Value::Null)
        }),
        "bookmark.add" => with_state(|s| {
            let id = arg_str(a, "id")?.to_string();
            let ch = arg_u64(a, "chapter")? as usize;
            let para = arg_u64(a, "para")? as usize;
            let now = chrono::Utc::now();
            let excerpt = s
                .work
                .as_ref()
                .and_then(|w| w.chapters.get(ch.saturating_sub(1)))
                .and_then(|c| c.paragraphs.iter().enumerate().skip(para).find(|(i, p)| !is_landmark(*i, p)))
                .map(|(_, p)| paragraph_display(p).1.text.chars().take(160).collect())
                .unwrap_or_default();
            s.library.add_bookmark(
                &id,
                Bookmark {
                    id: format!("bm_{}_{}", ch, now.timestamp_millis()),
                    chapter_index: ch,
                    paragraph_index: para,
                    note: a.get("note").and_then(Value::as_str).unwrap_or("").trim().to_string(),
                    excerpt,
                    created_at: now,
                },
            );
            Ok(bookmarks_json(s.library.get_entry(&id)))
        }),
        "bookmark.remove" => with_state(|s| {
            let id = arg_str(a, "id")?;
            s.library.remove_bookmark(id, arg_str(a, "bookmark")?);
            Ok(bookmarks_json(s.library.get_entry(id)))
        }),
        "search" => with_state(|s| {
            let work = s.work.as_ref().ok_or("no work open")?;
            let q = arg_str(a, "query")?.trim();
            if q.chars().count() < 2 {
                return Ok(json!([]));
            }
            let re = regex::RegexBuilder::new(&regex::escape(q)).case_insensitive(true).build().map_err(|e| e.to_string())?;
            let cur = a.get("chapter").and_then(Value::as_u64).unwrap_or(0) as usize;
            let n = work.chapters.len();
            let order: Vec<usize> = if a.get("all").and_then(Value::as_bool).unwrap_or(true) { (cur..n).chain(0..cur).collect() } else { vec![cur] };
            let mut out = Vec::new();
            'outer: for ci in order {
                for (pi, p) in work.chapters[ci].paragraphs.iter().enumerate() {
                    if is_landmark(pi, p) {
                        continue;
                    }
                    let (_, r) = paragraph_display(p);
                    for m in re.find_iter(&r.text) {
                        out.push(json!([ci, pi, u16_index(&r.text, m.start()), u16_index(&r.text, m.end())]));
                        if out.len() >= MAX_MATCHES {
                            break 'outer;
                        }
                    }
                }
            }
            Ok(Value::Array(out))
        }),
        "export" => with_state(|s| {
            let work = s.library.load_work(arg_str(a, "id")?).ok_or("This work's text isn't downloaded.")?;
            let text = match arg_str(a, "format")? {
                "md" => WorkExporter::to_markdown(&work),
                "txt" => WorkExporter::to_txt(&work),
                _ => WorkExporter::to_html(&work),
            };
            Ok(json!({ "title": work.meta.title, "text": text }))
        }),
        "storage" => Ok(json!({ "dir": AppConfig::get_app_dir().to_string_lossy() })),
        "memory" => Ok(json!({ "live": RUST_LIVE.load(Relaxed), "peak": RUST_PEAK.load(Relaxed) })),
        "clear_cache" => with_state(|s| {
            let works = AppConfig::get_app_dir().join("works");
            let _ = std::fs::remove_dir_all(&works);
            let _ = std::fs::create_dir_all(&works);
            s.work = None;
            Ok(Value::Null)
        }),
        "logout" => with_state(|s| {
            s.config.session = UserSession::default();
            s.config.save()?;
            s.client = Ao3Client::new(None);
            Ok(session_json(&s.config.session))
        }),

        // ---- network: the lock is released while waiting on AO3 ----
        "fetch" => {
            let input = arg_str(a, "input")?.to_string();
            let client = with_state(|s| Ok(s.client.clone()))?;
            let work = client.fetch_work(&input)?;
            with_state(|s| {
                s.library.save_work(&work)?;
                Ok(json!({ "id": work.meta.id, "meta": meta_json(&work.meta) }))
            })
        }
        "import" => {
            // Recognised by content: Android's picker often hides the real file name.
            let path = std::path::PathBuf::from(arg_str(a, "path")?);
            let bytes = std::fs::read(&path).map_err(|e| format!("Couldn't read the file: {e}"))?;
            let work = ao3reader::ao3::epub::import_bytes(&bytes)?;
            with_state(|s| {
                s.library.save_work(&work)?;
                Ok(json!({ "id": work.meta.id, "meta": meta_json(&work.meta) }))
            })
        }
        "epub" => {
            let id = arg_str(a, "id")?.to_string();
            let path = arg_str(a, "path")?.to_string();
            let client = with_state(|s| Ok(s.client.clone()))?;
            let bytes = client.download_epub(&id)?;
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "kudos" => {
            let id = arg_str(a, "id")?.to_string();
            let client = with_state(|s| Ok(s.client.clone()))?;
            Ok(json!(client.leave_kudos(&id)?))
        }
        "login" => {
            let (user, pass) = (arg_str(a, "user")?.to_string(), arg_str(a, "pass")?.to_string());
            let client = with_state(|s| Ok(s.client.clone()))?;
            let session = client.login(&user, &pass)?;
            with_state(|s| {
                s.config.session = session;
                s.config.save()?;
                Ok(session_json(&s.config.session))
            })
        }
        "cookie" => {
            let value = arg_str(a, "value")?.trim().to_string();
            let client = with_state(|s| {
                s.client.set_cookie("_otw_remember_me", &value);
                Ok(s.client.clone())
            })?;
            match client.current_user() {
                Ok(Some(name)) => with_state(|s| {
                    s.config.session = UserSession { username: Some(name), session_cookie: None, remember_token: Some(value), is_logged_in: true };
                    s.config.save()?;
                    Ok(session_json(&s.config.session))
                }),
                other => {
                    with_state(|s| {
                        s.client = Ao3Client::new(Some(&s.config.session));
                        Ok(())
                    })?;
                    match other {
                        Err(e) => Err(e),
                        _ => Err("AO3 didn't recognise that cookie as signed in.".into()),
                    }
                }
            }
        }
        _ => Err(format!("unknown method '{method}'")),
    }
}

#[cfg(target_os = "android")]
mod jni_entry {
    use jni::objects::{JClass, JString};
    use jni::sys::jstring;
    use jni::JNIEnv;

    #[no_mangle]
    pub extern "system" fn Java_org_ao3reader_Core_call<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, method: JString<'l>, arg: JString<'l>) -> jstring {
        let method: String = env.get_string(&method).map(Into::into).unwrap_or_default();
        let arg: String = env.get_string(&arg).map(Into::into).unwrap_or_default();
        let out = super::handle(&method, &arg);
        env.new_string(out).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_indices_for_android_spans() {
        let r = parse_inline("日本 *語* ✨ *x*");
        let v = rich_json(&r);
        assert_eq!(v["text"], "日本 語 ✨ x");
        // 語 is one UTF-16 unit at index 3; ✨ is one unit, so x is at 7.
        assert_eq!(v["spans"][0], json!([3, 4, "i"]));
        assert_eq!(v["spans"][1], json!([7, 8, "i"]));
    }

    #[test]
    fn library_round_trip_in_a_temp_folder() {
        let dir = std::env::temp_dir().join(format!("ao3core-test-{}", std::process::id()));
        let init: Value = serde_json::from_str(&handle("init", &json!({"dir": dir.to_string_lossy()}).to_string())).unwrap();
        assert_eq!(init["ok"]["themes"].as_array().map(|t| t.len()), Some(6));
        let lib: Value = serde_json::from_str(&handle("library", "{}")).unwrap();
        assert_eq!(lib["ok"], json!([]));
        let bad: Value = serde_json::from_str(&handle("nope", "{}")).unwrap();
        assert!(bad["err"].is_string());
        let _ = std::fs::remove_dir_all(dir);
    }
}
