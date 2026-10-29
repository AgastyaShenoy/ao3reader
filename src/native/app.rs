//! Application state and the window shell (header, navigation, toasts).

use super::gfx::*;
use super::icons;
use super::settings::{SettingsAction, SettingsCtx, SettingsScreen};
use super::ui::*;
use super::worker::Worker;
use crate::ao3::client::Ao3Client;
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

pub enum AppEvent {
    UpdateChecked(Result<Option<UpdateInfo>, String>),
}

/// Work the window host runs after the frame, outside any borrow of the app
/// (modal dialogs pump their own messages).
pub enum Deferred {
    PickFontFile,
    Open(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
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
            tab: Tab::Settings,
            settings: SettingsScreen::default(),
            toast: None,
            update_status: UpdateStatus::Idle,
            worker: None,
            config_dirty_since: None,
            deferred: Vec::new(),
            font_families: None,
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
        }
    }

    pub fn set_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    pub fn mark_config_dirty(&mut self) {
        self.config_dirty_since = Some(Instant::now());
    }

    pub fn on_close(&mut self) {
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

        let body = Rect::new(0.0, HEADER_H, w, (h - HEADER_H).max(0.0));
        match self.tab {
            Tab::Settings => self.settings_frame(ui, body),
            Tab::Library => self.placeholder(ui, body, icons::LIBRARY, "Library"),
            Tab::Fetch => self.placeholder(ui, body, icons::DOWNLOAD, "Fetch"),
            Tab::Account => self.placeholder(ui, body, icons::CONTACT, "Account"),
        }

        // Drawn last so it sits on top of the scrolled content.
        self.header(ui, Rect::new(0.0, 0.0, w, HEADER_H));
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
        let account_label = self.config.session.username.clone().unwrap_or_else(|| "Guest".into());
        let items = [
            (icons::LIBRARY, lib_label.as_str()),
            (icons::DOWNLOAD, "Fetch"),
            (icons::CONTACT, account_label.as_str()),
            (icons::SETTINGS, "Settings"),
        ];
        let tabs = [Tab::Library, Tab::Fetch, Tab::Account, Tab::Settings];
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

    fn placeholder(&mut self, ui: &mut Ui, body: Rect, icon: &str, name: &str) {
        let c = ui.c;
        let w = body.w.min(520.0) - 32.0;
        let r = Rect::new(body.x + (body.w - w) / 2.0, body.y + 64.0, w, 190.0);
        ui.card(r);
        let it = ui.measure(icon, &TextStyle::icons(30.0), TextOpts::default());
        ui.g.draw_text(it, r.x + (r.w - it.w) / 2.0, r.y + 28.0, c.accent);
        let title = format!("{name} is moving to the new engine");
        let t = ui.measure(&title, &TextStyle::ui(17.0).semibold(), TextOpts::wrap(r.w - 48.0).align(Align::Center));
        ui.g.draw_text(t, r.x + 24.0, r.y + 76.0, c.text);
        let msg = "This screen is being rebuilt on the native renderer. Until it lands, use the classic build (ao3reader.exe) for it.";
        let m = ui.measure(msg, &TextStyle::ui(13.0), TextOpts::wrap(r.w - 48.0).align(Align::Center));
        ui.g.draw_text(m, r.x + 24.0, r.y + 84.0 + t.h, c.muted);
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
