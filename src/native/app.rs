//! Application state and the window shell (header, navigation, toasts).

use super::gfx::*;
use super::account::{AccountAction, AccountScreen};
use super::fetch::{FetchAction, FetchScreen};
use super::icons;
use super::library::{ExportFormat, LibraryAction, LibraryScreen};
use super::reader::{ReaderAction, ReaderScreen};
use super::settings::{SettingsAction, SettingsCtx, SettingsScreen};
use super::ui::*;
use super::worker::Worker;
use crate::ao3::client::Ao3Client;
use crate::ao3::models::{UserSession, Work};
use crate::ao3::parser::Ao3Parser;
use crate::storage::export::WorkExporter;
use crate::reader::typography::{FontFamilyChoice, TypographySettings};
use crate::storage::config::AppConfig;
use crate::storage::library::LibraryManager;
use crate::updater::{check_for_updates, UpdateInfo, UpdateStatus, CURRENT_VERSION};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;

const HEADER_H: f32 = 56.0;
const TOAST_SECS: u64 = 4;
/// Settings are written to disk this long after the last change, not on every slider step.
const CONFIG_SAVE_DELAY: Duration = Duration::from_millis(600);
/// Reading position is written at most this often while reading (and always on close).
const PROGRESS_SAVE_DELAY: Duration = Duration::from_secs(3);

pub enum AppEvent {
    UpdateChecked(Result<Option<UpdateInfo>, String>),
    Kudos(Result<String, String>),
    Fetched { result: Result<Work, String>, open: bool },
    Epub { name: String, result: Result<Vec<u8>, String> },
    Login(Result<UserSession, String>),
    SessionUser(Result<Option<String>, String>),
}

/// Work the window host runs after the frame, outside any borrow of the app
/// (modal dialogs pump their own messages).
pub enum Deferred {
    PickFontFile,
    ImportFile,
    SaveFile { name: String, filter: &'static str, ext: &'static str, data: Vec<u8> },
    Open(String),
    Fullscreen(bool),
    SetTitle(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Reader,
    Library,
    Fetch,
    Account,
    Settings,
}

pub struct App {
    pub config: AppConfig,
    library: LibraryManager,
    client: Ao3Client,
    tab: Tab,
    settings: SettingsScreen,
    toast: Option<(String, Instant)>,
    update_status: UpdateStatus,
    worker: Option<Worker<AppEvent>>,
    config_dirty_since: Option<Instant>,
    pub deferred: Vec<Deferred>,
    font_families: Option<Vec<String>>,
    reader: Option<ReaderScreen>,
    last_progress_save: Instant,
    library_screen: LibraryScreen,
    fetch: FetchScreen,
    account: AccountScreen,
    /// Cookie being verified with AO3 before it is saved.
    pending_cookie: Option<String>,
}

/// A file name that Windows accepts, from a work title.
fn file_name(title: &str, ext: &str) -> String {
    let clean: String = title
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '_' } else { c })
        .take(120)
        .collect();
    let clean = clean.trim().trim_end_matches('.');
    format!("{}.{ext}", if clean.is_empty() { "work" } else { clean })
}

fn signed_in(s: &UserSession) -> bool {
    s.is_logged_in && (s.session_cookie.is_some() || s.remember_token.is_some())
}

/// The font family the reader uses for the given typography settings.
pub fn reader_family(t: &TypographySettings) -> Family {
    match &t.font_family {
        FontFamilyChoice::SansSerif => Family::Named { name: "Segoe UI".into(), file: None },
        FontFamilyChoice::Serif => Family::Named { name: "Georgia".into(), file: None },
        FontFamilyChoice::Monospace => Family::Mono,
        FontFamilyChoice::Custom(name) => {
            let file = if t.custom_font_name.as_deref() == Some(name.as_str()) { t.custom_font_path.clone() } else { None };
            Family::Named { name: name.clone(), file }
        }
    }
}

pub fn reader_font_label(t: &TypographySettings) -> String {
    match &t.font_family {
        FontFamilyChoice::SansSerif => "Segoe UI (default)".into(),
        FontFamilyChoice::Serif => "Georgia".into(),
        FontFamilyChoice::Monospace => "Monospace".into(),
        FontFamilyChoice::Custom(name) => name.clone(),
    }
}

impl App {
    pub fn new(config: AppConfig) -> Self {
        let client = Ao3Client::new(Some(&config.session));
        Self {
            config,
            library: LibraryManager::new(),
            client,
            tab: Tab::Library,
            settings: SettingsScreen::default(),
            toast: None,
            update_status: UpdateStatus::Idle,
            worker: None,
            config_dirty_since: None,
            deferred: Vec::new(),
            font_families: None,
            reader: None,
            last_progress_save: Instant::now(),
            library_screen: LibraryScreen::default(),
            fetch: FetchScreen::default(),
            account: AccountScreen::default(),
            pending_cookie: None,
        }
    }

    /// Called once the window exists, so the worker can wake it.
    pub fn attach(&mut self, hwnd: HWND) {
        self.worker = Some(Worker::new(hwnd));
        if self.config.check_updates_on_startup {
            self.check_updates();
        }
    }

    pub fn worker(&self) -> Option<&Worker<AppEvent>> {
        self.worker.as_ref()
    }

    fn check_updates(&mut self) {
        if let Some(w) = &self.worker {
            self.update_status = UpdateStatus::Checking;
            w.run(|| AppEvent::UpdateChecked(check_for_updates()));
        }
    }

    pub fn on_event(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::UpdateChecked(res) => {
                self.update_status = match res {
                    Ok(Some(info)) => {
                        self.set_toast(format!("Version {} is available", info.latest_version));
                        UpdateStatus::UpdateAvailable(Box::new(info))
                    }
                    Ok(None) => UpdateStatus::UpToDate { checked_version: CURRENT_VERSION.to_string() },
                    Err(e) => UpdateStatus::Error(e),
                };
            }
            AppEvent::Kudos(Ok(msg)) => self.set_toast(msg),
            AppEvent::Kudos(Err(e)) => self.set_toast(format!("Couldn't leave kudos: {e}")),
            AppEvent::Fetched { result: Ok(work), open } => {
                if let Err(e) = self.library.save_work(&work) {
                    self.fetch.set_error(format!("Downloaded, but couldn't save it: {e}"));
                    return;
                }
                self.fetch.set_done(Some(work.meta.clone()));
                self.set_toast(format!("Saved “{}”", work.meta.title));
                if open {
                    self.open_loaded(work);
                }
            }
            AppEvent::Fetched { result: Err(e), .. } => self.fetch.set_error(e),
            AppEvent::Epub { name, result: Ok(data) } => {
                self.fetch.set_done(None);
                self.deferred.push(Deferred::SaveFile { name, filter: "EPUB e-book", ext: "epub", data });
            }
            AppEvent::Epub { result: Err(e), .. } => self.fetch.set_error(e),
            AppEvent::Login(Ok(session)) => {
                let name = session.username.clone().unwrap_or_default();
                self.config.session = session;
                let _ = self.config.save();
                self.account.set_message(format!("Signed in as {name}."), false);
                self.set_toast("Signed in to AO3");
            }
            AppEvent::Login(Err(e)) => self.account.set_message(e, true),
            AppEvent::SessionUser(result) => {
                let cookie = self.pending_cookie.take();
                match (result, cookie) {
                    (Ok(Some(name)), Some(cookie)) => {
                        self.config.session = UserSession { username: Some(name.clone()), session_cookie: None, remember_token: Some(cookie), is_logged_in: true };
                        let _ = self.config.save();
                        self.account.set_message(format!("Signed in as {name}."), false);
                        self.set_toast("Signed in to AO3");
                    }
                    (Ok(_), _) => {
                        self.client = Ao3Client::new(Some(&self.config.session));
                        self.account.set_message("AO3 didn't recognise that cookie as signed in. Copy it again from a browser where you're signed in.", true);
                    }
                    (Err(e), _) => {
                        self.client = Ao3Client::new(Some(&self.config.session));
                        self.account.set_message(e, true);
                    }
                }
            }
        }
    }

    fn run_job(&self, job: impl FnOnce() -> AppEvent + Send + 'static) {
        if let Some(w) = &self.worker {
            w.run(job);
        }
    }

    fn start_fetch(&mut self, work_id: String, open: bool) {
        let client = self.client.clone();
        self.run_job(move || AppEvent::Fetched { result: client.fetch_work(&work_id), open });
    }

    /// Reads and parses a saved .json or AO3 .html download off the UI thread.
    pub fn import_file(&mut self, path: std::path::PathBuf) {
        self.fetch.start_import();
        self.tab = Tab::Fetch;
        self.run_job(move || {
            let result = std::fs::read(&path)
                .map_err(|e| format!("Couldn't read the file: {e}"))
                .and_then(|bytes| crate::ao3::epub::import_bytes(&bytes));
            AppEvent::Fetched { result, open: true }
        });
    }

    pub fn on_saved(&mut self, result: Result<std::path::PathBuf, String>) {
        match result {
            Ok(path) => self.set_toast(format!("Saved {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())),
            Err(e) => self.set_toast(format!("Couldn't save the file: {e}")),
        }
    }

    fn open_loaded(&mut self, work: Work) {
        self.close_reader();
        let id = work.meta.id.clone();
        let progress = self.library.get_entry(&id).map(|e| e.progress.clone());
        let title = work.meta.title.clone();
        self.reader = Some(ReaderScreen::new(work, progress.as_ref()));
        self.library.touch_history(&id);
        self.tab = Tab::Reader;
        self.deferred.push(Deferred::SetTitle(format!("{title} — AO3 Reader")));
    }

    pub fn open_work(&mut self, work_id: &str) {
        match self.library.load_work(work_id) {
            Some(work) => self.open_loaded(work),
            None if work_id.chars().all(|c| c.is_ascii_digit()) => {
                // The text was cleared from disk; the library entry still knows the work.
                self.set_toast("Downloading this work again…");
                self.tab = Tab::Fetch;
                self.fetch.busy = Some(super::fetch::Busy::Work);
                self.start_fetch(work_id.to_string(), true);
            }
            None => self.set_toast("This work's text isn't on disk any more."),
        }
    }

    fn save_progress(&mut self) {
        if let Some(r) = &mut self.reader {
            if r.progress_dirty {
                r.progress_dirty = false;
                let id = r.work_id().to_string();
                self.library.update_progress(&id, r.progress());
            }
        }
        self.last_progress_save = Instant::now();
    }

    fn close_reader(&mut self) {
        self.save_progress();
        if let Some(r) = self.reader.take() {
            if r.fullscreen {
                self.deferred.push(Deferred::Fullscreen(false));
            }
            self.deferred.push(Deferred::SetTitle("AO3 Reader".into()));
        }
        if self.tab == Tab::Reader {
            self.tab = Tab::Library;
        }
    }

    fn reader_frame(&mut self, ui: &mut Ui, r: Rect) {
        let Some(reader) = &mut self.reader else {
            self.tab = Tab::Library;
            return;
        };
        let bookmarks = self.library.get_entry(reader.work_id()).map(|e| e.bookmarks.clone()).unwrap_or_default();
        let actions = reader.frame(ui, r, &mut self.config, &bookmarks);
        let work_id = reader.work_id().to_string();
        for a in actions {
            match a {
                ReaderAction::Close => {
                    self.close_reader();
                    return;
                }
                ReaderAction::Kudos => {
                    if let Some(w) = &self.worker {
                        let client = self.client.clone();
                        let id = work_id.clone();
                        w.run(move || AppEvent::Kudos(client.leave_kudos(&id)));
                        self.set_toast("Sending kudos…");
                    }
                }
                ReaderAction::AddBookmark(b) => {
                    self.library.add_bookmark(&work_id, b);
                    self.set_toast("Bookmark added");
                }
                ReaderAction::RemoveBookmark(bid) => self.library.remove_bookmark(&work_id, &bid),
                ReaderAction::ConfigChanged => self.mark_config_dirty(),
                ReaderAction::ToggleFullscreen => {
                    if let Some(r) = &mut self.reader {
                        r.fullscreen = !r.fullscreen;
                        let on = r.fullscreen;
                        self.deferred.push(Deferred::Fullscreen(on));
                        if on {
                            self.set_toast("Full screen: press Esc or F11 to leave");
                        }
                    }
                }
                ReaderAction::OpenTypographySettings => {
                    self.tab = Tab::Settings;
                    self.settings.show_typography();
                }
            }
        }
        if self.reader.as_ref().is_some_and(|r| r.progress_dirty) {
            let since = self.last_progress_save.elapsed();
            if since >= PROGRESS_SAVE_DELAY {
                self.save_progress();
            } else {
                ui.wake_after((PROGRESS_SAVE_DELAY - since).as_millis() as u32 + 1);
            }
        }
    }

    pub fn set_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    pub fn mark_config_dirty(&mut self) {
        self.config_dirty_since = Some(Instant::now());
    }

    pub fn on_close(&mut self) {
        self.save_progress();
        if self.config_dirty_since.take().is_some() {
            let _ = self.config.save();
        }
    }

    /// A font file chosen in the picker; `family` is None if DirectWrite could not load it.
    pub fn on_font_file(&mut self, path: PathBuf, family: Option<String>) {
        match family {
            Some(name) => {
                let t = &mut self.config.typography;
                t.font_family = FontFamilyChoice::Custom(name.clone());
                t.custom_font_name = Some(name.clone());
                t.custom_font_path = Some(path);
                self.mark_config_dirty();
                self.set_toast(format!("Reading font set to {name}"));
            }
            None => self.set_toast("That file could not be loaded as a font"),
        }
    }

    pub fn frame(&mut self, ui: &mut Ui) {
        let (w, h) = ui.size;
        let c = ui.c;
        ui.g.clear(c.bg);

        if self.tab == Tab::Reader {
            self.reader_frame(ui, Rect::new(0.0, 0.0, w, h));
        }
        if self.tab != Tab::Reader {
            let body = Rect::new(0.0, HEADER_H, w, (h - HEADER_H).max(0.0));
            match self.tab {
                Tab::Settings => self.settings_frame(ui, body),
                Tab::Library => self.library_frame(ui, body),
                Tab::Fetch => self.fetch_frame(ui, body),
                Tab::Account => self.account_frame(ui, body),
                Tab::Reader => {}
            }
            // Drawn last so it sits on top of the scrolled content.
            self.header(ui, Rect::new(0.0, 0.0, w, HEADER_H));
        }
        self.toast_overlay(ui);

        if let Some(since) = self.config_dirty_since {
            let elapsed = since.elapsed();
            if elapsed >= CONFIG_SAVE_DELAY {
                let _ = self.config.save();
                self.config_dirty_since = None;
            } else {
                ui.wake_after((CONFIG_SAVE_DELAY - elapsed).as_millis() as u32 + 1);
            }
        }
    }

    fn header(&mut self, ui: &mut Ui, r: Rect) {
        let c = ui.c;
        ui.g.fill_rect(r, c.surface);
        ui.separator(r.x, r.bottom() - 1.0, r.w);

        let badge = Rect::new(16.0, 14.0, 46.0, 28.0);
        ui.g.fill_round(badge, 6.0, c.accent);
        ui.text_in("AO3", &TextStyle::ui(14.0).bold(), badge, Align::Center, c.on_accent);
        ui.text_in("Reader", &TextStyle::ui(16.0).semibold(), Rect::new(70.0, r.y, 70.0, r.h), Align::Left, c.text);

        let lib_label = format!("Library ({})", self.library.entries().len());
        let account_label = if signed_in(&self.config.session) { self.config.session.username.clone().unwrap_or_else(|| "Account".into()) } else { "Guest".into() };
        let mut items = vec![
            (icons::LIBRARY, lib_label.as_str()),
            (icons::DOWNLOAD, "Fetch"),
            (icons::CONTACT, account_label.as_str()),
            (icons::SETTINGS, "Settings"),
        ];
        let mut tabs = vec![Tab::Library, Tab::Fetch, Tab::Account, Tab::Settings];
        if self.reader.is_some() {
            items.insert(0, (icons::READING, "Reading"));
            tabs.insert(0, Tab::Reader);
        }
        let mut sel = tabs.iter().position(|t| *t == self.tab).unwrap_or(0);

        let has_update = matches!(self.update_status, UpdateStatus::UpdateAvailable(_));
        let update_w = if has_update { if r.w > 860.0 { ui.button_width("Update available", Some(icons::UPDATE)) } else { 40.0 } } else { 0.0 };
        let tabs_w = r.w - 156.0 - update_w - 16.0;
        let (_, changed) = ui.tab_strip(id("nav"), 156.0, 11.0, tabs_w, &items, &mut sel);
        if changed {
            self.tab = tabs[sel];
        }

        if has_update {
            let label = if r.w > 860.0 { "Update available" } else { "" };
            let br = Rect::new(r.right() - 16.0 - update_w, 12.0, update_w, 32.0);
            if ui.button_ex(id("nav-update"), br, label, Some(icons::UPDATE), ButtonKind::Primary, true) {
                self.tab = Tab::Settings;
                self.settings.show_updates();
            }
        }
    }

    fn library_frame(&mut self, ui: &mut Ui, body: Rect) {
        for a in self.library_screen.frame(ui, body, &self.library) {
            match a {
                LibraryAction::Open(id) => self.open_work(&id),
                LibraryAction::ToggleFavorite(id) => self.library.toggle_favorite(&id),
                LibraryAction::SetShelf(id, shelf) => self.library.set_shelf(&id, shelf),
                LibraryAction::GoFetch => self.tab = Tab::Fetch,
                LibraryAction::Delete(id) => {
                    if self.reader.as_ref().is_some_and(|r| r.work_id() == id) {
                        self.reader = None;
                        self.deferred.push(Deferred::SetTitle("AO3 Reader".into()));
                    }
                    let title = self.library.get_entry(&id).map(|e| e.meta.title.clone()).unwrap_or_default();
                    self.library.delete_work(&id);
                    self.set_toast(format!("Removed “{title}”"));
                }
                LibraryAction::Export(id, format) => match self.library.load_work(&id) {
                    Some(work) => {
                        let (text, filter, ext) = match format {
                            ExportFormat::Html => (WorkExporter::to_html(&work), "Web page", "html"),
                            ExportFormat::Markdown => (WorkExporter::to_markdown(&work), "Markdown", "md"),
                            ExportFormat::Text => (WorkExporter::to_txt(&work), "Plain text", "txt"),
                        };
                        let name = file_name(&work.meta.title, ext);
                        self.deferred.push(Deferred::SaveFile { name, filter, ext, data: text.into_bytes() });
                    }
                    None => self.set_toast("This work's text isn't on disk. Open it to download it again first."),
                },
            }
        }
    }

    fn fetch_frame(&mut self, ui: &mut Ui, body: Rect) {
        let logged_in = signed_in(&self.config.session);
        for a in self.fetch.frame(ui, body, logged_in) {
            match a {
                FetchAction::Fetch { id, open } => self.start_fetch(id, open),
                FetchAction::Epub(work_id) => {
                    let client = self.client.clone();
                    let title = self.library.get_entry(&work_id).map(|e| e.meta.title.clone()).unwrap_or_else(|| format!("ao3_{work_id}"));
                    let name = file_name(&title, "epub");
                    self.run_job(move || AppEvent::Epub { name, result: client.download_epub(&work_id) });
                }
                FetchAction::Import => self.deferred.push(Deferred::ImportFile),
                FetchAction::Open(id) => self.open_work(&id),
                FetchAction::GoAccount => self.tab = Tab::Account,
            }
        }
    }

    fn account_frame(&mut self, ui: &mut Ui, body: Rect) {
        let session = self.config.session.clone();
        for a in self.account.frame(ui, body, &session) {
            match a {
                AccountAction::Login { username, password } => {
                    let client = self.client.clone();
                    self.run_job(move || AppEvent::Login(client.login(&username, &password)));
                }
                AccountAction::ApplyCookie(cookie) => {
                    self.client.set_cookie("_otw_remember_me", &cookie);
                    self.pending_cookie = Some(cookie);
                    let client = self.client.clone();
                    self.run_job(move || AppEvent::SessionUser(client.current_user()));
                }
                AccountAction::Logout => {
                    self.config.session = UserSession::default();
                    let _ = self.config.save();
                    self.client = Ao3Client::new(None);
                    self.account.set_message("Signed out. You're reading as a guest.", false);
                    self.set_toast("Signed out");
                }
            }
        }
    }

    fn settings_frame(&mut self, ui: &mut Ui, body: Rect) {
        if self.font_families.is_none() && self.settings.needs_fonts() {
            self.font_families = Some(ui.g.system_font_families());
        }
        let storage_dir = AppConfig::get_app_dir();
        let mut ctx = SettingsCtx {
            config: &mut self.config,
            update: &self.update_status,
            fonts: self.font_families.as_deref().unwrap_or(&[]),
            storage_dir: storage_dir.to_string_lossy().into_owned(),
        };
        let actions = self.settings.frame(ui, body, &mut ctx);
        for a in actions {
            match a {
                SettingsAction::ConfigChanged => self.mark_config_dirty(),
                SettingsAction::CheckUpdates => self.check_updates(),
                SettingsAction::PickFontFile => self.deferred.push(Deferred::PickFontFile),
                SettingsAction::Open(target) => self.deferred.push(Deferred::Open(target)),
                SettingsAction::ClearCache => {
                    let works = storage_dir.join("works");
                    let _ = std::fs::remove_dir_all(&works);
                    let _ = std::fs::create_dir_all(&works);
                    self.set_toast("Downloaded works removed");
                }
            }
        }
    }

    fn toast_overlay(&mut self, ui: &mut Ui) {
        let Some((msg, at)) = &self.toast else { return };
        let left = Duration::from_secs(TOAST_SECS).saturating_sub(at.elapsed());
        if left.is_zero() {
            self.toast = None;
            ui.request_repaint();
            return;
        }
        ui.wake_after(left.as_millis() as u32 + 1);
        let c = ui.c;
        let (w, h) = ui.size;
        let style = TextStyle::ui(13.5);
        let t = ui.measure(msg, &style, TextOpts::wrap((w - 120.0).min(520.0)));
        let r = Rect::new((w - t.w - 56.0) / 2.0, h - t.h - 48.0, t.w + 56.0, t.h + 22.0);
        ui.g.fill_round(r, 8.0, c.surface);
        ui.g.stroke_round(r, 8.0, 1.0, c.border);
        ui.g.fill_round(Rect::new(r.x, r.y, 4.0, r.h), 2.0, c.accent);
        let it = ui.measure(icons::INFO, &TextStyle::icons(14.0), TextOpts::default());
        ui.g.draw_text(it, r.x + 16.0, r.y + (r.h - it.h) / 2.0, c.accent);
        ui.g.draw_text(t, r.x + 40.0, r.y + 11.0, c.text);
    }
}
