//! Account screen: sign in with username and password or a session cookie, sign out.

use super::gfx::*;
use super::icons;
use super::ui::*;
use crate::ao3::models::UserSession;

const MAX_W: f32 = 720.0;

pub enum AccountAction {
    Login { username: String, password: String },
    ApplyCookie(String),
    Logout,
}

#[derive(Default)]
pub struct AccountScreen {
    username: String,
    password: String,
    cookie: String,
    show_cookie: bool,
    pub busy: bool,
    /// (message, is_error)
    message: Option<(String, bool)>,
}

impl AccountScreen {
    pub fn set_message(&mut self, msg: impl Into<String>, error: bool) {
        self.busy = false;
        self.message = Some((msg.into(), error));
        if !error {
            self.password.clear();
            self.cookie.clear();
        }
    }

    pub fn frame(&mut self, ui: &mut Ui, body: Rect, session: &UserSession) -> Vec<AccountAction> {
        let mut out = Vec::new();
        let c = ui.c;
        let w = (body.w - 48.0).min(MAX_W);
        let x = body.x + (body.w - w) / 2.0;
        let t = ui.text("Account", &TextStyle::ui(26.0).semibold(), x, body.y + 22.0, c.text);
        let top = body.y + 22.0 + t.h + 16.0;
        let area = Rect::new(body.x, top, body.w, (body.bottom() - top).max(0.0));
        ui.scroll_area(id("account-scroll"), area, |ui, _view, y0| {
            let mut col = Col::new(x, y0 + 2.0, w);
            self.content(ui, &mut col, session, &mut out);
            col.y + 24.0 - y0
        });
        out
    }

    fn content(&mut self, ui: &mut Ui, col: &mut Col, session: &UserSession, out: &mut Vec<AccountAction>) {
        let c = ui.c;
        let has_cookie = session.session_cookie.is_some() || session.remember_token.is_some();
        let signed_in = session.is_logged_in && has_cookie;

        // Status.
        let r = col.take(84.0);
        col.gap(12.0);
        ui.card(r);
        ui.g.fill_circle(r.x + 40.0, r.y + 42.0, 22.0, if signed_in { c.accent } else { c.border });
        ui.text_in(icons::CONTACT, &TextStyle::icons(18.0), Rect::new(r.x + 18.0, r.y + 20.0, 44.0, 44.0), Align::Center, if signed_in { c.on_accent } else { c.text });
        let (title, sub) = if signed_in {
            (format!("Signed in as {}", session.username.as_deref().unwrap_or("an AO3 user")), "Works restricted to registered users will download.".to_string())
        } else if session.is_logged_in {
            ("Sign-in expired".to_string(), "This sign-in came from an older version that didn't keep the session. Please sign in again.".to_string())
        } else {
            ("Reading as a guest".to_string(), "Public works download without an account.".to_string())
        };
        ui.text(&title, &TextStyle::ui(16.0).semibold(), r.x + 76.0, r.y + 20.0, c.text);
        ui.text_opts(&sub, &TextStyle::ui(13.0), TextOpts::ellipsize(r.w - 210.0), r.x + 76.0, r.y + 46.0, c.muted);
        if session.is_logged_in && ui.button_ex(id("acct-logout"), Rect::new(r.right() - 120.0, r.y + 24.0, 104.0, 36.0), "Sign out", None, ButtonKind::Secondary, true) {
            out.push(AccountAction::Logout);
        }

        if let Some((msg, err)) = &self.message {
            let t = ui.measure(msg, &TextStyle::ui(13.5), TextOpts::wrap(col.w - 36.0));
            let r = col.take(t.h + 26.0);
            col.gap(12.0);
            let tone = if *err { c.danger } else { c.success };
            ui.g.fill_round(r, 8.0, tone.alpha(0.1));
            ui.g.stroke_round(r, 8.0, 1.0, tone.alpha(0.6));
            ui.g.draw_text(t, r.x + 18.0, r.y + 13.0, c.text);
        }

        if !signed_in {
            let r = col.take(if self.show_cookie { 372.0 } else { 300.0 });
            col.gap(12.0);
            ui.card(r);
            ui.text("Sign in to AO3", &TextStyle::ui(15.0).semibold(), r.x + 18.0, r.y + 16.0, c.text);
            let fw = r.w - 36.0;
            ui.text("Username or email", &TextStyle::ui(12.5), r.x + 18.0, r.y + 50.0, c.muted);
            let u = ui.text_input(id("acct-user"), Rect::new(r.x + 18.0, r.y + 70.0, fw, 36.0), &mut self.username, "", false);
            ui.text("Password", &TextStyle::ui(12.5), r.x + 18.0, r.y + 118.0, c.muted);
            let p = ui.text_input(id("acct-pass"), Rect::new(r.x + 18.0, r.y + 138.0, fw, 36.0), &mut self.password, "", true);
            let can = !self.busy && !self.username.trim().is_empty() && !self.password.is_empty();
            let bw = ui.button_width("Sign in", Some(icons::CONTACT));
            let clicked = ui.button_ex(id("acct-login"), Rect::new(r.x + 18.0, r.y + 190.0, bw.max(110.0), 36.0), "Sign in", Some(icons::CONTACT), ButtonKind::Primary, can);
            if can && (clicked || u.submitted || p.submitted) {
                self.busy = true;
                self.message = None;
                out.push(AccountAction::Login { username: self.username.trim().to_string(), password: self.password.clone() });
            }
            if self.busy {
                ui.spinner(r.x + 160.0, r.y + 208.0, 7.0);
                ui.text("Signing in…", &TextStyle::ui(13.0), r.x + 176.0, r.y + 199.0, c.muted);
            }

            ui.separator(r.x + 18.0, r.y + 244.0, fw);
            let mut show = self.show_cookie;
            ui.toggle(id("acct-cookie-toggle"), r.x + 18.0, r.y + 262.0, &mut show);
            self.show_cookie = show;
            ui.text("Use a session cookie instead", &TextStyle::ui(13.5), r.x + 70.0, r.y + 261.0, c.text);
            if self.show_cookie {
                let cr = Rect::new(r.x + 18.0, r.y + 298.0, fw - 112.0, 36.0);
                let cresp = ui.text_input(id("acct-cookie"), cr, &mut self.cookie, "Value of the _otw_remember_me cookie", true);
                let can_apply = !self.busy && !self.cookie.trim().is_empty();
                let apply = ui.button_ex(id("acct-cookie-apply"), Rect::new(cr.right() + 8.0, cr.y, 104.0, 36.0), "Apply", None, ButtonKind::Secondary, can_apply);
                if can_apply && (apply || cresp.submitted) {
                    self.busy = true;
                    self.message = None;
                    out.push(AccountAction::ApplyCookie(self.cookie.trim().to_string()));
                }
            }
        }

        let privacy = "Your password is sent only to archiveofourown.org and is never saved. After signing in, AO3's session cookie is kept in this app's settings file on this PC so you stay signed in; signing out removes it.";
        let t = ui.measure(privacy, &TextStyle::ui(13.0), TextOpts::wrap(col.w - 36.0).line_height(1.45));
        let r = col.take(t.h + 54.0);
        ui.card(r);
        ui.text("Privacy", &TextStyle::ui(12.5).semibold(), r.x + 18.0, r.y + 14.0, c.muted);
        ui.g.draw_text(t, r.x + 18.0, r.y + 36.0, c.muted);
    }
}
