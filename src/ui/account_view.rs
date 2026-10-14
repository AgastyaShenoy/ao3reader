use crate::ao3::models::UserSession;
use crate::reader::theme::ReaderColors;
use crate::storage::config::AppConfig;
use egui::{Button, Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke, Ui};

pub enum AccountAction {
    None,
    LoginWithCredentials { username: String, password: String },
    SetCookie(String),
    Logout,
}

pub struct AccountView {
    pub username_input: String,
    pub password_input: String,
    pub cookie_input: String,
    pub is_logging_in: bool,
    pub error_message: Option<String>,
    pub success_message: Option<String>,
    pub show_cookie_input: bool,
}

impl AccountView {
    pub fn new() -> Self {
        Self {
            username_input: String::new(),
            password_input: String::new(),
            cookie_input: String::new(),
            is_logging_in: false,
            error_message: None,
            success_message: None,
            show_cookie_input: false,
        }
    }

    pub fn set_logging_in(&mut self, logging_in: bool) {
        self.is_logging_in = logging_in;
        if logging_in {
            self.error_message = None;
            self.success_message = None;
        }
    }

    pub fn set_error(&mut self, err: String) {
        self.is_logging_in = false;
        self.error_message = Some(err);
        self.success_message = None;
    }

    pub fn set_success(&mut self, msg: String) {
        self.is_logging_in = false;
        self.error_message = None;
        self.success_message = Some(msg);
        self.password_input.clear();
    }

    pub fn ui(&mut self, ui: &mut Ui, config: &AppConfig) -> AccountAction {
        let mut action = AccountAction::None;
        let colors = config.theme.colors();

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(20.0);

            // Centered layout
            ui.horizontal(|ui| {
                let avail_width = ui.available_width();
                let max_w = 680.0f32.min(avail_width - 32.0);
                let pad = ((avail_width - max_w) / 2.0).max(10.0);

                ui.add_space(pad);

                ui.vertical(|ui| {
                    ui.set_max_width(max_w);

                    // Current Session Status Card
                    self.render_status_card(ui, &colors, &config.session, &mut action);

                    ui.add_space(20.0);

                    // Notifications
                    if let Some(err) = &self.error_message {
                        Frame::NONE
                            .fill(colors.card)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(239, 68, 68)))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::same(14))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("❌").size(16.0));
                                    ui.label(RichText::new(err).color(Color32::from_rgb(239, 68, 68)));
                                });
                            });
                        ui.add_space(16.0);
                    }

                    if let Some(msg) = &self.success_message {
                        Frame::NONE
                            .fill(colors.card)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(34, 197, 94)))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::same(14))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("✔").size(16.0));
                                    ui.label(RichText::new(msg).color(Color32::from_rgb(34, 197, 94)).strong());
                                });
                            });
                        ui.add_space(16.0);
                    }

                    // Login Card (if not logged in)
                    if !config.session.is_logged_in {
                        self.render_login_form(ui, &colors, &mut action);
                        ui.add_space(20.0);
                    }

                    // Information Card about AO3 Guest vs Account
                    self.render_guest_info_card(ui, &colors);

                    ui.add_space(40.0);
                });

                ui.add_space(pad);
            });
        });

        action
    }

    fn render_status_card(
        &mut self,
        ui: &mut Ui,
        colors: &ReaderColors,
        session: &UserSession,
        action: &mut AccountAction,
    ) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(24))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if session.is_logged_in {
                        ui.label(RichText::new("👤").size(28.0));
                        ui.vertical(|ui| {
                            let username = session.username.as_deref().unwrap_or("AO3 User");
                            ui.label(RichText::new(format!("Signed in as {username}")).size(18.0).strong().color(colors.text));
                            ui.label(RichText::new("✔ Access to all public and restricted/locked works enabled").size(12.5).color(Color32::from_rgb(34, 197, 94)));
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(Button::new(RichText::new("Log Out").color(colors.text))).clicked() {
                                *action = AccountAction::Logout;
                            }
                        });
                    } else {
                        ui.label(RichText::new("🌐").size(28.0));
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Reading as Guest (No Login)").size(18.0).strong().color(colors.text));
                            ui.label(RichText::new("Public works can be read freely without an account.").size(12.5).color(colors.text_muted));
                        });
                    }
                });
            });
    }

    fn render_login_form(
        &mut self,
        ui: &mut Ui,
        colors: &ReaderColors,
        action: &mut AccountAction,
    ) {
        Frame::NONE
            .fill(colors.card)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(24))
            .show(ui, |ui| {
                ui.heading(RichText::new("Sign In to Archive of Our Own").size(18.0).color(colors.text));
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Logging in allows you to read works that authors have restricted to registered users only, and leave kudos under your username.")
                        .size(12.5)
                        .color(colors.text_muted),
                );

                ui.add_space(16.0);

                // Username field
                ui.label(RichText::new("AO3 User Name or Email:").strong().color(colors.text).size(13.0));
                ui.add_enabled(
                    !self.is_logging_in,
                    egui::TextEdit::singleline(&mut self.username_input)
                        .desired_width(f32::INFINITY)
                        .hint_text("Enter your AO3 username"),
                );

                ui.add_space(12.0);

                // Password field
                ui.label(RichText::new("AO3 Password:").strong().color(colors.text).size(13.0));
                let pass_edit = ui.add_enabled(
                    !self.is_logging_in,
                    egui::TextEdit::singleline(&mut self.password_input)
                        .password(true)
                        .desired_width(f32::INFINITY)
                        .hint_text("Enter your AO3 password"),
                );

                if pass_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if !self.username_input.trim().is_empty() && !self.password_input.trim().is_empty() {
                        *action = AccountAction::LoginWithCredentials {
                            username: self.username_input.trim().to_string(),
                            password: self.password_input.clone(),
                        };
                    }
                }

                ui.add_space(18.0);

                ui.horizontal(|ui| {
                    let can_login = !self.username_input.trim().is_empty()
                        && !self.password_input.trim().is_empty()
                        && !self.is_logging_in;

                    let login_btn = Button::new(RichText::new("Log In to AO3").color(Color32::WHITE).strong().size(14.0))
                        .fill(colors.accent);

                    if ui.add_enabled(can_login, login_btn).clicked() {
                        *action = AccountAction::LoginWithCredentials {
                            username: self.username_input.trim().to_string(),
                            password: self.password_input.clone(),
                        };
                    }

                    if self.is_logging_in {
                        ui.add_space(8.0);
                        ui.spinner();
                        ui.label(RichText::new("Authenticating with AO3...").italics().color(colors.text_muted));
                    }
                });

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                // Alternative: Cookie Login
                ui.collapsing("Alternative: Sign in with Session Cookie (_otw_remember_me)", |ui| {
                    ui.label(
                        RichText::new("If you have two-factor authentication or prefer not entering your password, you can copy your '_otw_remember_me' cookie value from your web browser.")
                            .size(12.0)
                            .color(colors.text_muted),
                    );
                    ui.add_space(6.0);

                    ui.add(
                        egui::TextEdit::singleline(&mut self.cookie_input)
                            .desired_width(f32::INFINITY)
                            .hint_text("Paste _otw_remember_me or _otw_session value..."),
                    );

                    ui.add_space(6.0);
                    if ui.button("Apply Session Cookie").clicked() {
                        let trimmed = self.cookie_input.trim().to_string();
                        if !trimmed.is_empty() {
                            *action = AccountAction::SetCookie(trimmed);
                        }
                    }
                });
            });
    }

    fn render_guest_info_card(&self, ui: &mut Ui, colors: &ReaderColors) {
        Frame::NONE
            .fill(colors.surface)
            .stroke(Stroke::new(1.0, colors.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.label(RichText::new("🔒 Privacy & AO3 Access").strong().color(colors.text).size(13.5));
                ui.add_space(6.0);
                ui.label(
                    RichText::new("• You are free to use this reader entirely as a Guest with zero account.\n• The vast majority of works on AO3 do not require login.\n• Your credentials are stored locally on your device in your user directory and are only sent directly via HTTPS to archiveofourown.org.\n• No third-party servers, tracking, or telemetry are ever involved.")
                        .size(12.5)
                        .color(colors.text_muted),
                );
            });
    }
}
