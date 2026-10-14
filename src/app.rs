use crate::ao3::client::Ao3Client;
use crate::ao3::models::{UserSession, Work};
use crate::ao3::parser::Ao3Parser;
use crate::reader::theme::ReaderColors;
use crate::reader::view::{ReaderAction, ReaderView};
use crate::storage::config::AppConfig;
use crate::storage::export::WorkExporter;
use crate::storage::library::LibraryManager;
use crate::ui::account_view::{AccountAction, AccountView};
use crate::ui::fetch_view::{FetchAction, FetchView};
use crate::ui::library_view::{ExportFormat, LibraryAction, LibraryView};
use crate::ui::settings_view::{SettingsAction, SettingsView};
use crate::updater::{check_for_updates, UpdateInfo, UpdateStatus};
use egui::{
    Color32, CornerRadius, Frame, Margin, RichText, Stroke, Ui,
};
use std::sync::mpsc::{channel, Receiver, Sender};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Library,
    Reader,
    Fetch,
    Account,
    Settings,
}

pub enum AppEvent {
    WorkFetched { work: Work, auto_open: bool },
    FetchFailed(String),
    LoginSuccess(UserSession),
    LoginFailed(String),
    KudosResult(String),
    EpubReady { work_id: String, bytes: Vec<u8> },
    EpubFailed(String),
    UpdateCheckResult(Result<Option<UpdateInfo>, String>),
}

pub struct AO3ReaderApp {
    pub config: AppConfig,
    pub library: LibraryManager,
    pub client: Ao3Client,
    pub active_tab: ActiveTab,
    pub library_view: LibraryView,
    pub fetch_view: FetchView,
    pub account_view: AccountView,
    pub settings_view: SettingsView,
    pub reader_view: Option<ReaderView>,
    pub event_tx: Sender<AppEvent>,
    pub event_rx: Receiver<AppEvent>,
    pub status_toast: Option<(String, chrono::DateTime<chrono::Utc>)>,
    pub last_theme: Option<crate::reader::theme::ReaderThemeType>,
    pub update_status: UpdateStatus,
    pub update_available: bool,
    pub active_font_path: Option<std::path::PathBuf>,
}

impl AO3ReaderApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load();
        let colors = config.theme.colors();

        let mut visuals = if colors.is_dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        visuals.window_corner_radius = CornerRadius::same(8);
        visuals.panel_fill = colors.bg;
        visuals.window_fill = colors.surface;
        cc.egui_ctx.set_visuals(visuals);

        crate::reader::font_manager::FontManager::init_fonts(
            &cc.egui_ctx,
            config.typography.custom_font_path.as_deref(),
            config.typography.custom_font_name.as_deref(),
        );
        let active_font_path = config.typography.custom_font_path.clone();

        let library = LibraryManager::new();
        let client = Ao3Client::new(Some(&config.session));

        let (event_tx, event_rx) = channel();

        let mut app = Self {
            last_theme: Some(config.theme),
            config,
            library,
            client,
            active_tab: ActiveTab::Library,
            library_view: LibraryView::new(),
            fetch_view: FetchView::new(),
            account_view: AccountView::new(),
            settings_view: SettingsView::new(),
            reader_view: None,
            event_tx: event_tx.clone(),
            event_rx,
            status_toast: None,
            update_status: UpdateStatus::Idle,
            update_available: false,
            active_font_path,
        };

        if app.config.check_updates_on_startup {
            app.update_status = UpdateStatus::Checking;
            let tx = event_tx;
            tokio::spawn(async move {
                let res = check_for_updates().await;
                let _ = tx.send(AppEvent::UpdateCheckResult(res));
            });
        }

        app
    }

    pub fn set_toast(&mut self, msg: impl Into<String>) {
        self.status_toast = Some((msg.into(), chrono::Utc::now()));
    }

    fn process_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AppEvent::WorkFetched { work, auto_open } => {
                    let title = work.meta.title.clone();
                    let _ = self.library.save_work(&work);
                    self.fetch_view.set_success(work.clone());
                    self.set_toast(format!("Work saved: '{title}'"));

                    if auto_open {
                        self.reader_view = Some(ReaderView::new(work, 1));
                        self.active_tab = ActiveTab::Reader;
                    }
                    ctx.request_repaint();
                }
                AppEvent::FetchFailed(err) => {
                    self.fetch_view.set_error(err);
                    ctx.request_repaint();
                }
                AppEvent::LoginSuccess(session) => {
                    self.config.session = session.clone();
                    let _ = self.config.save();
                    self.client = Ao3Client::new(Some(&session));
                    let uname = session.username.unwrap_or_else(|| "User".to_string());
                    self.account_view.set_success(format!("Successfully signed in as {uname}!"));
                    self.set_toast("Logged in to AO3");
                    ctx.request_repaint();
                }
                AppEvent::LoginFailed(err) => {
                    self.account_view.set_error(err);
                    ctx.request_repaint();
                }
                AppEvent::KudosResult(msg) => {
                    if let Some(reader) = &mut self.reader_view {
                        reader.set_status(&msg);
                    } else {
                        self.set_toast(msg);
                    }
                    ctx.request_repaint();
                }
                AppEvent::EpubReady { work_id, bytes } => {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name(&format!("ao3_{work_id}.epub"))
                        .add_filter("EPUB eBook", &["epub"])
                        .save_file()
                    {
                        if let Err(e) = std::fs::write(&path, bytes) {
                            self.fetch_view.set_error(format!("Failed to save EPUB: {e}"));
                        } else {
                            self.set_toast("EPUB eBook saved successfully!");
                        }
                    }
                    ctx.request_repaint();
                }
                AppEvent::EpubFailed(err) => {
                    self.fetch_view.set_error(err);
                    ctx.request_repaint();
                }
                AppEvent::UpdateCheckResult(res) => {
                    match res {
                        Ok(Some(info)) => {
                            self.set_toast(format!("🚀 New version {} available!", info.latest_version));
                            self.update_available = true;
                            self.update_status = UpdateStatus::UpdateAvailable(Box::new(info));
                        }
                        Ok(None) => {
                            self.update_status = UpdateStatus::UpToDate {
                                checked_version: crate::updater::CURRENT_VERSION.to_string(),
                            };
                        }
                        Err(err) => {
                            self.update_status = UpdateStatus::Error(err);
                        }
                    }
                    ctx.request_repaint();
                }
            }
        }
    }
}

impl eframe::App for AO3ReaderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_events(ctx);

        let colors = self.config.theme.colors();

        // Apply dark or light mode ONLY when theme changes!
        if self.last_theme != Some(self.config.theme) {
            let mut visuals = if colors.is_dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            };
            visuals.window_corner_radius = CornerRadius::same(8);
            visuals.panel_fill = colors.bg;
            visuals.window_fill = colors.surface;
            ctx.set_visuals(visuals);
            self.last_theme = Some(self.config.theme);
        }

        // Apply custom font dynamically when setting changes!
        if self.active_font_path != self.config.typography.custom_font_path {
            if let Some(ref path) = self.config.typography.custom_font_path {
                let name = self.config.typography.custom_font_name.as_deref().unwrap_or("CustomFont");
                if let Err(e) = crate::reader::font_manager::FontManager::apply_font_to_ctx(ctx, path, name) {
                    self.set_toast(format!("Failed to load font: {e}"));
                } else {
                    self.set_toast(format!("Applied font '{name}'"));
                }
            } else {
                crate::reader::font_manager::FontManager::reset_fonts(ctx);
                self.set_toast("Reset to standard fonts");
            }
            self.active_font_path = self.config.typography.custom_font_path.clone();
            if let Some(reader) = &mut self.reader_view {
                reader.last_pagination_key = None;
            }
        }

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(colors.bg))
            .show(ctx, |ui| {
                // Main Navigation Bar (unless in reader mode)
                if self.active_tab != ActiveTab::Reader {
                    self.render_navigation_header(ui, &colors);
                    ui.add_space(4.0);
                }

                // Global Toast banner
                if let Some((msg, time)) = &self.status_toast {
                    if (chrono::Utc::now() - *time).num_seconds() < 4 {
                        Frame::NONE
                            .fill(colors.card)
                            .stroke(Stroke::new(1.0, colors.accent))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(Margin::symmetric(14, 8))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.colored_label(colors.accent, "ℹ");
                                    ui.colored_label(colors.text, msg);
                                });
                            });
                        ui.add_space(6.0);
                    } else {
                        self.status_toast = None;
                    }
                }

                // Active Tab Content
                match self.active_tab {
                    ActiveTab::Library => {
                        let action = self.library_view.ui(ui, &mut self.library, &self.config);
                        match action {
                            LibraryAction::None => {}
                            LibraryAction::OpenWork(id) => {
                                if let Some(work) = self.library.load_work(&id) {
                                    let entry = self.library.get_entry(&id);
                                    let ch = entry.map(|e| e.progress.current_chapter).unwrap_or(1);
                                    self.reader_view = Some(ReaderView::new(work, ch));
                                    self.active_tab = ActiveTab::Reader;
                                } else {
                                    self.set_toast(format!("Work content not found for #{id}"));
                                }
                            }
                            LibraryAction::SwitchToFetch => {
                                self.active_tab = ActiveTab::Fetch;
                            }
                            LibraryAction::ExportWork(id, format) => {
                                if let Some(work) = self.library.load_work(&id) {
                                    let (content, ext, desc) = match format {
                                        ExportFormat::Html => (WorkExporter::to_html(&work), "html", "HTML Document"),
                                        ExportFormat::Markdown => (WorkExporter::to_markdown(&work), "md", "Markdown Document"),
                                        ExportFormat::Txt => (WorkExporter::to_txt(&work), "txt", "Plain Text"),
                                    };

                                    let clean_title = work.meta.title.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
                                    if let Some(path) = rfd::FileDialog::new()
                                        .set_file_name(&format!("{clean_title}.{ext}"))
                                        .add_filter(desc, &[ext])
                                        .save_file()
                                    {
                                        if let Err(e) = std::fs::write(&path, content) {
                                            self.set_toast(format!("Export failed: {e}"));
                                        } else {
                                            self.set_toast("Export completed successfully!");
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    ActiveTab::Reader => {
                        if let Some(reader) = &mut self.reader_view {
                            let action = reader.ui(ui, &mut self.config);
                            let work_id = reader.work.meta.id.clone();

                            match action {
                                ReaderAction::None => {}
                                ReaderAction::CloseReader => {
                                    let p = reader.calculate_progress();
                                    self.library.update_progress(&work_id, p);
                                    self.active_tab = ActiveTab::Library;
                                }
                                ReaderAction::UpdateProgress(progress) => {
                                    if self.config.auto_save_progress {
                                        self.library.update_progress(&work_id, progress);
                                    }
                                }
                                ReaderAction::AddBookmark(bm) => {
                                    self.library.add_bookmark(&work_id, bm);
                                }
                                ReaderAction::LeaveKudos(id) => {
                                    let client = self.client.clone();
                                    let tx = self.event_tx.clone();
                                    tokio::spawn(async move {
                                        let res = client.leave_kudos(&id).await;
                                        match res {
                                            Ok(msg) => { let _ = tx.send(AppEvent::KudosResult(msg)); }
                                            Err(e) => { let _ = tx.send(AppEvent::KudosResult(format!("Kudos error: {e}"))); }
                                        }
                                    });
                                }
                                ReaderAction::ChangeChapter(idx) => {
                                    let mut p = reader.calculate_progress();
                                    p.current_chapter = idx;
                                    self.library.update_progress(&work_id, p);
                                }
                            }
                        } else {
                            self.active_tab = ActiveTab::Library;
                        }
                    }
                    ActiveTab::Fetch => {
                        let action = self.fetch_view.ui(ui, &self.config);
                        match action {
                            FetchAction::None => {}
                            FetchAction::StartFetch { input, auto_open } => {
                                self.fetch_view.set_fetching(true);
                                let client = self.client.clone();
                                let tx = self.event_tx.clone();
                                tokio::spawn(async move {
                                    match client.fetch_work(&input).await {
                                        Ok(work) => {
                                            let _ = tx.send(AppEvent::WorkFetched { work, auto_open });
                                        }
                                        Err(err) => {
                                            let _ = tx.send(AppEvent::FetchFailed(err));
                                        }
                                    }
                                });
                            }
                            FetchAction::OpenWork(work) => {
                                let _ = self.library.save_work(&work);
                                self.reader_view = Some(ReaderView::new(work, 1));
                                self.active_tab = ActiveTab::Reader;
                            }
                            FetchAction::DownloadEpub(input) => {
                                let client = self.client.clone();
                                let tx = self.event_tx.clone();
                                if let Some(work_id) = Ao3Parser::extract_work_id(&input) {
                                    tokio::spawn(async move {
                                        match client.download_epub(&work_id).await {
                                            Ok(bytes) => {
                                                let _ = tx.send(AppEvent::EpubReady { work_id, bytes });
                                            }
                                            Err(err) => {
                                                let _ = tx.send(AppEvent::EpubFailed(err));
                                            }
                                        }
                                    });
                                } else {
                                    self.fetch_view.set_error("Invalid AO3 URL or Work ID".to_string());
                                }
                            }
                            FetchAction::SwitchToSettings => {
                                self.active_tab = ActiveTab::Account;
                            }
                            FetchAction::ImportLocalFile => {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("AO3 Work (JSON, HTML)", &["json", "html", "htm"])
                                    .pick_file()
                                {
                                    if let Ok(content) = std::fs::read_to_string(&path) {
                                        let res = if path.extension().map_or(false, |ext| ext == "json") {
                                            serde_json::from_str::<Work>(&content).map_err(|e| e.to_string())
                                        } else {
                                            Ao3Parser::parse_work(&content, "imported")
                                        };

                                        match res {
                                            Ok(work) => {
                                                let _ = self.library.save_work(&work);
                                                self.set_toast(format!("Imported: {}", work.meta.title));
                                                self.reader_view = Some(ReaderView::new(work, 1));
                                                self.active_tab = ActiveTab::Reader;
                                            }
                                            Err(e) => {
                                                self.fetch_view.set_error(format!("Failed to parse file: {e}"));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    ActiveTab::Account => {
                        let action = self.account_view.ui(ui, &self.config);
                        match action {
                            AccountAction::None => {}
                            AccountAction::LoginWithCredentials { username, password } => {
                                self.account_view.set_logging_in(true);
                                let client = self.client.clone();
                                let tx = self.event_tx.clone();
                                tokio::spawn(async move {
                                    match client.login(&username, &password).await {
                                        Ok(sess) => {
                                            let _ = tx.send(AppEvent::LoginSuccess(sess));
                                        }
                                        Err(err) => {
                                            let _ = tx.send(AppEvent::LoginFailed(err));
                                        }
                                    }
                                });
                            }
                            AccountAction::SetCookie(cookie) => {
                                self.client.set_cookie("_otw_remember_me", &cookie);
                                self.config.session.remember_token = Some(cookie);
                                self.config.session.is_logged_in = true;
                                let _ = self.config.save();
                                self.account_view.set_success("Session cookie saved successfully!".to_string());
                                self.set_toast("Session cookie applied");
                            }
                            AccountAction::Logout => {
                                self.config.session = UserSession::default();
                                let _ = self.config.save();
                                self.client = Ao3Client::new(None);
                                self.account_view.set_success("Logged out. Now browsing in Guest mode.".to_string());
                                self.set_toast("Switched to Guest mode");
                            }
                        }
                    }
                    ActiveTab::Settings => {
                        let action = self.settings_view.ui(ui, &mut self.config, &self.update_status);
                        match action {
                            SettingsAction::None => {}
                            SettingsAction::OpenStorageFolder => {
                                let dir = AppConfig::get_app_dir();
                                let _ = webbrowser::open(dir.to_str().unwrap_or("."));
                            }
                            SettingsAction::ClearCache => {
                                let base = AppConfig::get_app_dir();
                                let works_dir = base.join("works");
                                let _ = std::fs::remove_dir_all(&works_dir);
                                let _ = std::fs::create_dir_all(&works_dir);
                                self.library = LibraryManager::new();
                                self.set_toast("Offline cache cleared.");
                            }
                            SettingsAction::CheckForUpdates => {
                                self.update_status = UpdateStatus::Checking;
                                let tx = self.event_tx.clone();
                                tokio::spawn(async move {
                                    let res = check_for_updates().await;
                                    let _ = tx.send(AppEvent::UpdateCheckResult(res));
                                });
                            }
                            SettingsAction::OpenUrl(url) => {
                                let _ = webbrowser::open(&url);
                            }
                        }
                    }
                }
            });
    }
}

impl AO3ReaderApp {
    fn render_navigation_header(&mut self, ui: &mut Ui, colors: &ReaderColors) {
        Frame::NONE
            .fill(colors.surface)
            .stroke(Stroke::new(1.0, colors.border))
            .inner_margin(Margin::symmetric(20, 10))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    // Logo / Brand
                    Frame::NONE
                        .fill(colors.accent)
                        .corner_radius(CornerRadius::same(5))
                        .inner_margin(Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.label(RichText::new("AO3").color(Color32::WHITE).strong().size(13.0));
                        });

                    ui.label(RichText::new("Reader").strong().size(16.0).color(colors.text));

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(12.0);

                    // Nav tabs
                    let lib_count = self.library.entries().len();
                    let lib_tab = self.active_tab == ActiveTab::Library;
                    if ui.selectable_label(lib_tab, format!("📚 My Library ({lib_count})")).clicked() {
                        self.active_tab = ActiveTab::Library;
                    }

                    if self.reader_view.is_some() {
                        let reader_tab = self.active_tab == ActiveTab::Reader;
                        if ui.selectable_label(reader_tab, "📖 Reader").clicked() {
                            self.active_tab = ActiveTab::Reader;
                        }
                    }

                    let fetch_tab = self.active_tab == ActiveTab::Fetch;
                    if ui.selectable_label(fetch_tab, "📥 Fetch Work").clicked() {
                        self.active_tab = ActiveTab::Fetch;
                    }

                    let account_tab = self.active_tab == ActiveTab::Account;
                    let account_label = if self.config.session.is_logged_in {
                        let name = self.config.session.username.as_deref().unwrap_or("User");
                        format!("👤 {name}")
                    } else {
                        "🌐 Guest".to_string()
                    };
                    if ui.selectable_label(account_tab, account_label).clicked() {
                        self.active_tab = ActiveTab::Account;
                    }

                    let settings_tab = self.active_tab == ActiveTab::Settings;
                    let settings_label = if self.update_available {
                        "⚙ Settings 🚀"
                    } else {
                        "⚙ Settings"
                    };
                    if ui.selectable_label(settings_tab, settings_label).clicked() {
                        self.active_tab = ActiveTab::Settings;
                    }

                    if self.update_available {
                        ui.add_space(4.0);
                        if ui.button(RichText::new("🚀 Update Available!").strong().color(Color32::from_rgb(250, 204, 21))).clicked() {
                            self.active_tab = ActiveTab::Settings;
                        }
                    }

                    // Push theme selector right if there is space, otherwise wrap cleanly
                    let avail = ui.available_width();
                    let theme_w = 190.0;
                    if avail > theme_w {
                        ui.add_space(avail - theme_w);
                    }

                    ui.label(RichText::new("Theme:").color(colors.text_muted).size(12.0));
                    let cur_theme = self.config.theme;
                    egui::ComboBox::from_id_salt("header_theme_picker")
                        .selected_text(cur_theme.display_name())
                        .width(115.0)
                        .show_ui(ui, |ui| {
                            for theme in crate::reader::theme::ReaderThemeType::all() {
                                if ui.selectable_value(&mut self.config.theme, *theme, theme.display_name()).clicked() {
                                    let _ = self.config.save();
                                }
                            }
                        });
                });
            });
    }
}
